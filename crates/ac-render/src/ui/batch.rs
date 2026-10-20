//! A UI draw list turned into triangles: one vertex stream and the runs of
//! it that share a texture, in paint order. No GPU here, so the rules are
//! tested on their own.
//!
//! The rules are OpenAC's `TextRenderer` (the UI's sprite batcher):
//! - every command is two triangles per rectangle, or a fan from the first
//!   corner for a polygon, in screen pixels with the texture coordinates
//!   as given (past 1 they repeat: the samplers wrap);
//! - consecutive commands on the same texture share one run, so the order
//!   of painting is kept and a texture change starts a new run;
//! - the main layer is painted before the overlay (popups, tooltips, the
//!   drag ghost);
//! - with a fixed canvas every position is multiplied by the canvas scale,
//!   and the glyph atlases, sampled nearest at 1 : 1, are sampled linear
//!   (OpenAC's "linear twin" of a nearest texture);
//! - a solid fill or an untextured polygon samples a 1 x 1 white texture,
//!   so everything goes through one pipeline;
//! - a render surface id of 0 names nothing and draws nothing (never a
//!   placeholder: a 0 resolved to the magenta texture is the 1-pixel
//!   magenta edge bug).

use ac_ui::draw::{Color, DrawCmd, DrawList, Filter, Quad, TextureRef, Vertex};

/// One corner as the GPU reads it: screen pixels, texture coordinate,
/// colour (straight alpha).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    /// Screen x, y in pixels (origin top-left).
    pub position: [f32; 2],
    /// Texture u, v.
    pub uv: [f32; 2],
    /// Tint, multiplied into the texel.
    pub color: [f32; 4],
}

/// What a run samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiTexture {
    /// The 1 x 1 white texture (fills, untextured polygons).
    White,
    /// A texture the draw list named.
    Named(TextureRef),
}

/// A texture and how it is sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiTextureKey {
    /// The texture.
    pub texture: UiTexture,
    /// Nearest or linear (both repeat).
    pub filter: Filter,
}

/// A run of vertices drawn with one texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiRun {
    /// What it samples.
    pub key: UiTextureKey,
    /// First vertex.
    pub first: u32,
    /// Vertex count (a multiple of 3).
    pub count: u32,
}

/// A frame of UI triangles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiBatch {
    /// Every vertex, in paint order.
    pub vertices: Vec<UiVertex>,
    /// The runs over them, in paint order.
    pub runs: Vec<UiRun>,
    /// Commands that named render surface 0 and were dropped.
    pub skipped_zero_ids: u32,
}

impl UiBatch {
    /// Turn `list` into triangles.
    #[must_use]
    pub fn build(list: &DrawList) -> Self {
        let mut batch = Self::default();
        let (sx, sy) = list.canvas_scale;
        let scale = if sx.is_finite() && sy.is_finite() && sx > 0.0 && sy > 0.0 {
            (sx, sy)
        } else {
            (1.0, 1.0)
        };
        let scaled = scale != (1.0, 1.0);
        for cmd in list.main.iter().chain(list.overlay.iter()) {
            batch.push(cmd, scale, scaled);
        }
        batch
    }

    /// True when nothing is drawn.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    fn push(&mut self, cmd: &DrawCmd, scale: (f32, f32), scaled: bool) {
        match cmd {
            DrawCmd::Fill { x, y, w, h, color } => {
                let quad = Quad {
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
                    ..Quad::default()
                };
                self.quad(white(), quad, *color, scale);
            }
            DrawCmd::Sprite {
                texture,
                filter,
                quad,
                tint,
            } => {
                let Some(texture) = named(*texture) else {
                    self.skipped_zero_ids += 1;
                    return;
                };
                let key = UiTextureKey {
                    texture,
                    filter: sampled(*filter, scaled),
                };
                self.quad(key, *quad, *tint, scale);
            }
            DrawCmd::Polygon {
                texture,
                vertices,
                color,
            } => {
                let key = match texture {
                    None => white(),
                    Some(t) => match named(*t) {
                        Some(texture) => UiTextureKey {
                            texture,
                            filter: Filter::Linear,
                        },
                        None => {
                            self.skipped_zero_ids += 1;
                            return;
                        }
                    },
                };
                self.fan(key, vertices, *color, scale);
            }
            DrawCmd::Glyphs {
                texture,
                tint,
                quads,
            } => {
                let Some(texture) = named(*texture) else {
                    self.skipped_zero_ids += 1;
                    return;
                };
                let key = UiTextureKey {
                    texture,
                    filter: sampled(Filter::Nearest, scaled),
                };
                for quad in quads {
                    self.quad(key, *quad, *tint, scale);
                }
            }
        }
    }

    /// The run `key` continues, or a new one.
    fn run(&mut self, key: UiTextureKey, added: u32) {
        let first = u32::try_from(self.vertices.len()).unwrap_or(u32::MAX) - added;
        match self.runs.last_mut() {
            Some(run) if run.key == key => run.count += added,
            _ => self.runs.push(UiRun {
                key,
                first,
                count: added,
            }),
        }
    }

    fn quad(&mut self, key: UiTextureKey, q: Quad, color: Color, (sx, sy): (f32, f32)) {
        if q.w <= 0.0 || q.h <= 0.0 {
            return;
        }
        let (x0, y0, x1, y1) = (q.x * sx, q.y * sy, (q.x + q.w) * sx, (q.y + q.h) * sy);
        let c = [color.r, color.g, color.b, color.a];
        let v = |x: f32, y: f32, u: f32, v: f32| UiVertex {
            position: [x, y],
            uv: [u, v],
            color: c,
        };
        // OpenAC's winding: (tl, br, tr), (tl, bl, br).
        self.vertices.extend_from_slice(&[
            v(x0, y0, q.u0, q.v0),
            v(x1, y1, q.u1, q.v1),
            v(x1, y0, q.u1, q.v0),
            v(x0, y0, q.u0, q.v0),
            v(x0, y1, q.u0, q.v1),
            v(x1, y1, q.u1, q.v1),
        ]);
        self.run(key, 6);
    }

    fn fan(&mut self, key: UiTextureKey, corners: &[Vertex], color: Color, (sx, sy): (f32, f32)) {
        if corners.len() < 3 {
            return;
        }
        let c = [color.r, color.g, color.b, color.a];
        let v = |p: &Vertex| UiVertex {
            position: [p.x * sx, p.y * sy],
            uv: [p.u, p.v],
            color: c,
        };
        let mut added = 0;
        for i in 1..corners.len() - 1 {
            self.vertices
                .extend_from_slice(&[v(&corners[0]), v(&corners[i]), v(&corners[i + 1])]);
            added += 3;
        }
        self.run(key, added);
    }
}

fn white() -> UiTextureKey {
    UiTextureKey {
        texture: UiTexture::White,
        filter: Filter::Nearest,
    }
}

/// A named texture, or `None` for surface 0 (nothing).
fn named(texture: TextureRef) -> Option<UiTexture> {
    match texture {
        TextureRef::Surface(0) => None,
        other => Some(UiTexture::Named(other)),
    }
}

/// Nearest sampling becomes linear when the canvas is scaled.
fn sampled(filter: Filter, scaled: bool) -> Filter {
    if scaled { Filter::Linear } else { filter }
}

#[cfg(test)]
mod tests {
    //! OpenAC `tests/AcDream.App.Tests/Rendering/TextRendererLinearTwinTests.cs`
    //! (the linear twin only when the canvas is scaled, identity for other
    //! textures) and `TextRenderer.NextSpriteSeg` / `DrawConvexPolygon`
    //! (runs per texture in order, fans), plus the zero-id guard (a zero
    //! texture id must never resolve to the magenta placeholder).
    use super::*;

    fn sprite(id: u32, filter: Filter, x: f32) -> DrawCmd {
        DrawCmd::Sprite {
            texture: TextureRef::Surface(id),
            filter,
            quad: Quad {
                x,
                y: 0.0,
                w: 10.0,
                h: 10.0,
                u0: 0.0,
                v0: 0.0,
                u1: 1.0,
                v1: 1.0,
            },
            tint: Color::WHITE,
        }
    }

    fn list(main: Vec<DrawCmd>, overlay: Vec<DrawCmd>, scale: (f32, f32)) -> DrawList {
        DrawList {
            main,
            overlay,
            canvas_scale: scale,
        }
    }

    fn keys(batch: &UiBatch) -> Vec<(UiTexture, Filter, u32)> {
        batch
            .runs
            .iter()
            .map(|r| (r.key.texture, r.key.filter, r.count))
            .collect()
    }

    #[test]
    fn a_sprite_is_two_triangles_with_its_texture_coordinates() {
        let batch = UiBatch::build(&list(
            vec![sprite(0x0600_0001, Filter::Linear, 5.0)],
            vec![],
            (1.0, 1.0),
        ));
        assert_eq!(batch.vertices.len(), 6);
        let corners: Vec<([f32; 2], [f32; 2])> =
            batch.vertices.iter().map(|v| (v.position, v.uv)).collect();
        assert_eq!(
            corners,
            [
                ([5.0, 0.0], [0.0, 0.0]),
                ([15.0, 10.0], [1.0, 1.0]),
                ([15.0, 0.0], [1.0, 0.0]),
                ([5.0, 0.0], [0.0, 0.0]),
                ([5.0, 10.0], [0.0, 1.0]),
                ([15.0, 10.0], [1.0, 1.0]),
            ]
        );
    }

    #[test]
    fn consecutive_commands_on_one_texture_share_a_run_and_order_is_kept() {
        let a = 0x0600_000A;
        let b = 0x0600_000B;
        let batch = UiBatch::build(&list(
            vec![
                sprite(a, Filter::Linear, 0.0),
                sprite(a, Filter::Linear, 10.0),
                sprite(b, Filter::Linear, 20.0),
                sprite(a, Filter::Linear, 30.0),
            ],
            vec![],
            (1.0, 1.0),
        ));
        let named = |id| UiTexture::Named(TextureRef::Surface(id));
        assert_eq!(
            keys(&batch),
            [
                (named(a), Filter::Linear, 12),
                (named(b), Filter::Linear, 6),
                (named(a), Filter::Linear, 6),
            ]
        );
        assert_eq!(
            batch.runs.iter().map(|r| r.first).collect::<Vec<_>>(),
            [0, 12, 18]
        );
    }

    #[test]
    fn the_overlay_is_painted_after_the_main_layer() {
        let batch = UiBatch::build(&list(
            vec![sprite(1, Filter::Linear, 0.0)],
            vec![sprite(2, Filter::Linear, 0.0)],
            (1.0, 1.0),
        ));
        let order: Vec<UiTexture> = batch.runs.iter().map(|r| r.key.texture).collect();
        assert_eq!(
            order,
            [
                UiTexture::Named(TextureRef::Surface(1)),
                UiTexture::Named(TextureRef::Surface(2))
            ]
        );
    }

    #[test]
    fn a_fixed_canvas_scales_positions_and_samples_glyphs_linear() {
        let glyphs = DrawCmd::Glyphs {
            texture: TextureRef::Surface(7),
            tint: Color::WHITE,
            quads: vec![Quad {
                x: 10.0,
                y: 10.0,
                w: 4.0,
                h: 8.0,
                u0: 0.0,
                v0: 0.0,
                u1: 0.5,
                v1: 0.5,
            }],
        };
        // CanvasScale One: the nearest atlas stays nearest.
        let one = UiBatch::build(&list(vec![glyphs.clone()], vec![], (1.0, 1.0)));
        assert_eq!(one.runs[0].key.filter, Filter::Nearest);
        assert_eq!(one.vertices[1].position, [14.0, 18.0]);
        // CanvasScale not One: positions scale, nearest becomes linear, the
        // texture is the same.
        let scaled = UiBatch::build(&list(vec![glyphs], vec![], (2.4, 1.8)));
        assert_eq!(scaled.runs[0].key.filter, Filter::Linear);
        assert_eq!(
            scaled.runs[0].key.texture,
            UiTexture::Named(TextureRef::Surface(7))
        );
        let p = scaled.vertices[1].position;
        assert!((p[0] - 14.0 * 2.4).abs() < 1e-4 && (p[1] - 18.0 * 1.8).abs() < 1e-4);
        // A linear sprite is unaffected by the twin rule.
        let sprite = UiBatch::build(&list(
            vec![sprite(5, Filter::Linear, 0.0)],
            vec![],
            (2.4, 1.8),
        ));
        assert_eq!(sprite.runs[0].key.filter, Filter::Linear);
    }

    #[test]
    fn a_polygon_is_a_fan_from_its_first_corner() {
        let corners: Vec<Vertex> = (0..5)
            .map(|i| Vertex::new(i as f32, 0.0, 0.0, 0.0))
            .collect();
        let batch = UiBatch::build(&list(
            vec![DrawCmd::Polygon {
                texture: None,
                vertices: corners,
                color: Color::BLACK,
            }],
            vec![],
            (1.0, 1.0),
        ));
        let xs: Vec<f32> = batch.vertices.iter().map(|v| v.position[0]).collect();
        assert_eq!(xs, [0.0, 1.0, 2.0, 0.0, 2.0, 3.0, 0.0, 3.0, 4.0]);
        assert_eq!(batch.runs[0].key, white());
    }

    #[test]
    fn fills_sample_white_and_join_untextured_polygons() {
        let fill = DrawCmd::Fill {
            x: 0.0,
            y: 0.0,
            w: 2.0,
            h: 2.0,
            color: Color::rgba(1.0, 0.0, 0.0, 0.5),
        };
        let poly = DrawCmd::Polygon {
            texture: None,
            vertices: vec![
                Vertex::new(0.0, 0.0, 0.0, 0.0),
                Vertex::new(1.0, 0.0, 0.0, 0.0),
                Vertex::new(1.0, 1.0, 0.0, 0.0),
            ],
            color: Color::WHITE,
        };
        let batch = UiBatch::build(&list(vec![fill, poly], vec![], (1.0, 1.0)));
        assert_eq!(keys(&batch), [(UiTexture::White, Filter::Nearest, 9)]);
        assert_eq!(batch.vertices[0].color, [1.0, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn surface_zero_draws_nothing() {
        let batch = UiBatch::build(&list(
            vec![
                sprite(0, Filter::Linear, 0.0),
                DrawCmd::Glyphs {
                    texture: TextureRef::Surface(0),
                    tint: Color::WHITE,
                    quads: vec![Quad::default()],
                },
            ],
            vec![],
            (1.0, 1.0),
        ));
        assert!(batch.is_empty() && batch.runs.is_empty());
        assert_eq!(batch.skipped_zero_ids, 2);
    }

    #[test]
    fn empty_rectangles_add_nothing() {
        let mut s = sprite(3, Filter::Linear, 0.0);
        if let DrawCmd::Sprite { quad, .. } = &mut s {
            quad.w = 0.0;
        }
        let batch = UiBatch::build(&list(vec![s], vec![], (1.0, 1.0)));
        assert!(batch.is_empty() && batch.runs.is_empty());
    }
}
