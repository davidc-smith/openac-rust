//! Shapes as untextured triangles with a colour at every corner, anti-aliased
//! on the CPU: each edge gets a band one device pixel wide whose outer
//! corners are transparent, centred on the true edge, so the rasteriser's
//! blending ramps coverage across it. Half the band lies inside the shape and
//! half outside, which keeps the covered area equal to the shape's own. No
//! shader is involved.
//!
//! A shape is built from rings: its outline pushed out and pulled in by the
//! soft edge (and, for a stroke, by half the stroke's width), each ring with
//! the same number of points so neighbouring rings join point for point.
//! Everything here is pure: sizes come in UI units, along with how many units
//! one device pixel is, and triangles go out as a list, three corners each.
//!
//! Strokes grow both radii of an ellipse alike, so a strongly eccentric
//! ellipse's stroke is thinner along its flat sides.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::draw::{Color, ColorVertex};

/// The most points a polygon may have.
pub const MAX_POLYGON_POINTS: usize = 64;

/// How far, in device pixels, a chord may stray inside the true curve.
pub const ARC_TOLERANCE: f32 = 0.1;

/// The most chords one rounded corner is cut into.
pub const MAX_SEGMENTS_PER_CORNER: u32 = 32;

/// The fewest chords an ellipse is cut into.
pub const MIN_ELLIPSE_SEGMENTS: u32 = 8;

/// The most chords an ellipse is cut into.
pub const MAX_ELLIPSE_SEGMENTS: u32 = 256;

/// How far out a polygon corner's soft edge may reach, as a multiple of half
/// the edge. A very sharp corner's true mitre would reach much further.
const MAX_MITER: f64 = 10.0;

/// A rectangle's left, top, width and height.
pub type ShapeRect = (f32, f32, f32, f32);

/// What tessellating a polygon came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeOutcome {
    /// Triangles were written.
    Drawn,
    /// The polygon has no area; nothing was written and nothing is wrong.
    Nothing,
    /// Fewer than three points.
    TooFewPoints,
    /// More than [`MAX_POLYGON_POINTS`] points.
    TooManyPoints,
    /// An outline that turns both ways, or winds round more than once.
    NotConvex,
}

/// The four corner radii of a rounded rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CornerRadii {
    /// Top left.
    pub top_left: f32,
    /// Top right.
    pub top_right: f32,
    /// Bottom right.
    pub bottom_right: f32,
    /// Bottom left.
    pub bottom_left: f32,
}

impl CornerRadii {
    /// Each corner's radius.
    #[must_use]
    pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    /// One radius for every corner.
    #[must_use]
    pub const fn all(radius: f32) -> Self {
        Self::new(radius, radius, radius, radius)
    }

    fn map(self, f: impl Fn(f32) -> f32) -> Self {
        Self::new(
            f(self.top_left),
            f(self.top_right),
            f(self.bottom_right),
            f(self.bottom_left),
        )
    }
}

/// How many chords an arc of `radius` units sweeping `sweep` radians needs so
/// that no chord strays more than [`ARC_TOLERANCE`] device pixels from it,
/// with `pixel` units to a device pixel.
#[must_use]
pub fn arc_segments(radius: f32, sweep: f32, pixel: f32) -> u32 {
    let in_pixels = radius / pixel;
    if !positive(in_pixels) {
        return 0;
    }
    if in_pixels <= ARC_TOLERANCE {
        return 1;
    }
    let step = 2.0 * (1.0 - ARC_TOLERANCE / in_pixels).acos();
    ((sweep / step).ceil() as u32).max(1)
}

/// Scales every radius down by one factor when two neighbours would not fit
/// along the side they share, the way CSS does, so a corner never overruns
/// its side and the shape keeps its proportions.
#[must_use]
pub fn clamp_radii(radii: CornerRadii, width: f32, height: f32) -> CornerRadii {
    let fit = |factor: f32, side: f32, sum: f32| {
        if sum > side {
            factor.min(side / sum)
        } else {
            factor
        }
    };
    let mut factor = 1.0;
    factor = fit(factor, width, radii.top_left + radii.top_right);
    factor = fit(factor, width, radii.bottom_left + radii.bottom_right);
    factor = fit(factor, height, radii.top_left + radii.bottom_left);
    factor = fit(factor, height, radii.top_right + radii.bottom_right);
    if factor < 1.0 {
        radii.map(|r| r * factor)
    } else {
        radii
    }
}

/// A convex polygon with a colour at each point, blended across it. Repeated
/// neighbouring points are merged; a polygon with no area draws nothing.
/// Either winding is accepted.
pub fn fill_convex_polygon(
    points: &[ColorVertex],
    pixel: f32,
    out: &mut Vec<ColorVertex>,
) -> ShapeOutcome {
    if points.len() < 3 {
        return ShapeOutcome::TooFewPoints;
    }
    if points.len() > MAX_POLYGON_POINTS {
        return ShapeOutcome::TooManyPoints;
    }
    let mut corners: Vec<ColorVertex> = Vec::with_capacity(points.len());
    for &point in points {
        if corners.last().is_none_or(|last| !same_point(*last, point)) {
            corners.push(point);
        }
    }
    while corners.len() > 1 && same_point(corners[corners.len() - 1], corners[0]) {
        corners.pop();
    }
    if corners.len() < 3 {
        return ShapeOutcome::Nothing;
    }
    let count = corners.len();

    // Convex means every turn goes the same way and the turns add up to one
    // full circle; a star turns one way throughout but twice round. An
    // outline that doubles straight back on itself, as round a spike with no
    // width, is not convex either, though its half turns can cancel out; a
    // polygon that is all one line still has no area.
    let mut turn = 0;
    let mut reverses = false;
    let mut turning = 0.0_f64;
    for i in 0..count {
        let (a, b, c) = (
            corners[i],
            corners[(i + 1) % count],
            corners[(i + 2) % count],
        );
        let into = (b.x - a.x, b.y - a.y);
        let out_of = (c.x - b.x, c.y - b.y);
        let cross = into.0 * out_of.1 - into.1 * out_of.0;
        let dot = into.0 * out_of.0 + into.1 * out_of.1;
        if cross.abs() > 1e-6 * length(into) * length(out_of) {
            let sign = if cross > 0.0 { 1 } else { -1 };
            if turn == 0 {
                turn = sign;
            } else if sign != turn {
                return ShapeOutcome::NotConvex;
            }
        } else if dot < 0.0 {
            reverses = true;
        }
        turning += f64::from(cross).atan2(f64::from(dot));
    }
    if turn == 0 {
        return ShapeOutcome::Nothing;
    }
    if reverses || (turning.abs() - std::f64::consts::TAU).abs() > 1e-3 {
        return ShapeOutcome::NotConvex;
    }
    fill_convex_core(&corners, turn, pixel, out);
    ShapeOutcome::Drawn
}

/// Softens a convex polygon already merged and checked: the outer ring is the
/// outline pushed out half a device pixel (bevelled where a mitre would reach
/// past [`MAX_MITER`]), the inner ring the outline pulled in half a device
/// pixel. The pull-in moves every edge's line inward; an edge that shrinks to
/// nothing on the way drops out and its neighbours meet directly, so the inner
/// ring never folds. A polygon too thin to pull in that far stops where it
/// collapses to a point or a line, and is then mostly soft edge, so its alpha
/// is lowered until it covers its own area.
fn fill_convex_core(corners: &[ColorVertex], turn: i32, pixel: f32, out: &mut Vec<ColorVertex>) {
    let count = corners.len();
    let half = f64::from(pixel) * 0.5;

    // Outward unit normal of each edge, corner i to corner i + 1.
    let normals: Vec<(f64, f64)> = (0..count)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % count]);
            let (ex, ey) = (
                f64::from(b.x) - f64::from(a.x),
                f64::from(b.y) - f64::from(a.y),
            );
            let edge = (ex * ex + ey * ey).sqrt();
            let (ex, ey) = (ex / edge, ey / edge);
            if turn > 0 { (ey, -ex) } else { (-ey, ex) }
        })
        .collect();

    // The polygon being pulled in: its corners, the edge leaving each, and
    // which of them each original corner has become.
    let mut x: Vec<f64> = corners.iter().map(|c| f64::from(c.x)).collect();
    let mut y: Vec<f64> = corners.iter().map(|c| f64::from(c.y)).collect();
    let mut leaving: Vec<usize> = (0..count).collect();
    let mut becomes: Vec<usize> = (0..count).collect();
    let mut miter = vec![(0.0_f64, 0.0_f64); count];
    let mut shrink = vec![0.0_f64; count];
    let mut vanishes = vec![false; count];

    let mut active = count;
    let mut pulled = 0.0_f64;
    let mut collapsed = false;
    loop {
        // Each corner moves along its mitre, and each end of an edge eats
        // tan(half the turn) of it for every pixel pulled in.
        for j in 0..active {
            let into = leaving[(j + active - 1) % active];
            let out_of = leaving[j];
            let ax = (normals[into].0 + normals[out_of].0) * 0.5;
            let ay = (normals[into].1 + normals[out_of].1) * 0.5;
            let length_squared = (ax * ax + ay * ay).max(1e-18);
            miter[j] = (ax / length_squared, ay / length_squared);
            shrink[j] = ((1.0 - length_squared).max(0.0) / length_squared).sqrt();
        }

        let side = |x: &[f64], y: &[f64], j: usize, next: usize| {
            let (dx, dy) = (x[next] - x[j], y[next] - y[j]);
            (dx * dx + dy * dy).sqrt()
        };
        let mut soonest = f64::MAX;
        let mut first = None;
        for j in 0..active {
            let next = (j + 1) % active;
            let rate = shrink[j] + shrink[next];
            if rate <= 0.0 {
                continue;
            }
            let time = side(&x, &y, j, next) / rate;
            if time < soonest {
                soonest = time;
                first = Some(j);
            }
        }

        let reaches = soonest >= half - pulled;
        let step = if reaches { half - pulled } else { soonest };
        for j in 0..active {
            x[j] -= miter[j].0 * step;
            y[j] -= miter[j].1 * step;
        }
        pulled += step;
        if reaches {
            break;
        }

        // Drop every edge that has just shrunk to nothing.
        let mut dropping = 0;
        for j in 0..active {
            let next = (j + 1) % active;
            let rate = shrink[j] + shrink[next];
            let left = side(&x, &y, j, next);
            // The edge that set the step has vanished whatever rounding left
            // of it.
            vanishes[j] =
                Some(j) == first || (rate > 0.0 && left <= 1e-9 * (1.0 + x[j].abs() + y[j].abs()));
            if vanishes[j] {
                dropping += 1;
            }
        }
        // Numerically stuck: stop where it is.
        if dropping == 0 {
            break;
        }
        // Pulled in to a point or a line.
        if active - dropping < 3 {
            collapsed = true;
            break;
        }
        while dropping > 0 {
            let mut j = 0;
            while !vanishes[j] {
                j += 1;
            }
            // Corner j and the next become one, which keeps the edge leaving
            // the next.
            let next = (j + 1) % active;
            x[j] = (x[j] + x[next]) * 0.5;
            y[j] = (y[j] + y[next]) * 0.5;
            leaving[j] = leaving[next];
            vanishes[j] = vanishes[next];
            for b in becomes.iter_mut() {
                if *b == next {
                    *b = j;
                }
            }
            for k in next..active - 1 {
                x[k] = x[k + 1];
                y[k] = y[k + 1];
                leaving[k] = leaving[k + 1];
                vanishes[k] = vanishes[k + 1];
            }
            for b in becomes.iter_mut() {
                if *b > next {
                    *b -= 1;
                }
            }
            active -= 1;
            dropping -= 1;
        }

        // Thinner now than what is left to pull in: going on would shrink it
        // along its length rather than across it. Stop and let the alpha make
        // up for the soft edge.
        let mut active_area = 0.0;
        let mut perimeter = 0.0;
        for j in 0..active {
            let next = (j + 1) % active;
            active_area += (x[j] - x[0]) * (y[next] - y[0]) - (x[next] - x[0]) * (y[j] - y[0]);
            perimeter += side(&x, &y, j, next);
        }
        if active_area.abs() * 0.5 < perimeter * (half - pulled) {
            collapsed = true;
            break;
        }
    }

    // The rings, one or two points per original corner: a corner whose mitre
    // would reach too far is bevelled on the outside and repeated on the
    // inside, so the rings still pair point for point.
    let mut inner = Vec::with_capacity(count * 2);
    let mut outer = Vec::with_capacity(count * 2);
    for i in 0..count {
        let corner = corners[i];
        let into = normals[(i + count - 1) % count];
        let out_of = normals[i];
        let inside = ColorVertex::new(x[becomes[i]] as f32, y[becomes[i]] as f32, corner.color);
        let (cx, cy) = (f64::from(corner.x), f64::from(corner.y));
        let clear = transparent(corner.color);
        let at = |dx: f64, dy: f64| {
            ColorVertex::new((cx + dx * half) as f32, (cy + dy * half) as f32, clear)
        };
        let (ax, ay) = ((into.0 + out_of.0) * 0.5, (into.1 + out_of.1) * 0.5);
        let length_squared = ax * ax + ay * ay;
        if length_squared * MAX_MITER * MAX_MITER >= 1.0 {
            inner.push(inside);
            outer.push(at(ax / length_squared, ay / length_squared));
        } else {
            inner.push(inside);
            outer.push(at(into.0, into.1));
            inner.push(inside);
            outer.push(at(out_of.0, out_of.1));
        }
    }

    let start = out.len();
    fan(&inner, out);
    band(&inner, &outer, out);
    if collapsed {
        let first = corners[0];
        let mut area = 0.0;
        for i in 1..count - 1 {
            let (b, c) = (corners[i], corners[i + 1]);
            let (ux, uy) = (
                f64::from(b.x) - f64::from(first.x),
                f64::from(b.y) - f64::from(first.y),
            );
            let (vx, vy) = (
                f64::from(c.x) - f64::from(first.x),
                f64::from(c.y) - f64::from(first.y),
            );
            area += ux * vy - uy * vx;
        }
        scale_to_area(out, start, area.abs() * 0.5);
    }
}

/// A rectangle with rounded corners; zero radii give square corners.
pub fn fill_rounded_rect(
    rect: ShapeRect,
    radii: CornerRadii,
    color: Color,
    pixel: f32,
    out: &mut Vec<ColorVertex>,
) {
    let (_, _, width, height) = rect;
    if !(positive(width) && positive(height)) {
        return;
    }
    let radii = clamp_radii(radii, width, height);
    let half = pixel * 0.5;
    let segments = corner_segments(radii, half, pixel);
    let inner = rounded_rect_ring(rect, radii, -half, segments);
    let outer = rounded_rect_ring(rect, radii, half, segments);
    let start = out.len();
    fill_ring(&inner, &outer, color, out);
    if width <= pixel || height <= pixel {
        let squared = |r: f32| f64::from(r) * f64::from(r);
        let corners = squared(radii.top_left)
            + squared(radii.top_right)
            + squared(radii.bottom_right)
            + squared(radii.bottom_left);
        let lost = (4.0 - std::f64::consts::PI) / 4.0 * corners;
        scale_to_area(out, start, f64::from(width) * f64::from(height) - lost);
    }
}

/// A rounded rectangle's outline, `thickness` wide, centred on the outline.
/// Outside a square corner the stroke stays square. A stroke that leaves no
/// hole is drawn as the fill of the rectangle grown by half its width, so its
/// middle is not left thin.
pub fn stroke_rounded_rect(
    rect: ShapeRect,
    radii: CornerRadii,
    color: Color,
    thickness: f32,
    pixel: f32,
    out: &mut Vec<ColorVertex>,
) {
    let (x, y, width, height) = rect;
    if !(positive(width) && positive(height) && positive(thickness)) {
        return;
    }
    let radii = clamp_radii(radii, width, height);
    let (half_width, ink) = stroke_profile(thickness, color, pixel);
    if width <= 2.0 * half_width || height <= 2.0 * half_width {
        // No hole left: the stroke is the rectangle grown by half its width,
        // each round corner grown alike; square corners stay square.
        let grown = radii.map(|r| if r > 0.0 { r + half_width } else { r });
        let rect = (
            x - half_width,
            y - half_width,
            width + 2.0 * half_width,
            height + 2.0 * half_width,
        );
        fill_rounded_rect(rect, grown, ink, pixel, out);
        return;
    }
    let fringe = pixel * 0.5;
    let segments = corner_segments(radii, half_width + fringe, pixel);
    let ring = |grow: f32| rounded_rect_ring(rect, radii, grow, segments);
    stroke_rings(
        [
            ring(half_width + fringe),
            ring(half_width - fringe),
            ring(fringe - half_width),
            ring(-half_width - fringe),
        ],
        ink,
        half_width > fringe,
        out,
    );
}

/// The soft shadow round a rounded rectangle, outside it only: `layers` bands,
/// each `spread` / layers wide, as dark as that many fills of `layer` would be
/// if each were grown a band further than the last and all were stacked.
/// Neighbouring bands share their edge, so none shows a seam; the outermost
/// fades out over a device pixel.
pub fn shadow_bands(
    rect: ShapeRect,
    radii: CornerRadii,
    (spread, layers): (f32, u32),
    layer: Color,
    pixel: f32,
    out: &mut Vec<ColorVertex>,
) {
    let (_, _, width, height) = rect;
    if !(positive(width) && positive(height) && positive(spread)) || layers < 1 {
        return;
    }
    let radii = clamp_radii(radii, width, height);
    let half = pixel * 0.5;
    let segments = corner_segments(radii, spread + half, pixel);
    let mut inner = rounded_rect_ring(rect, radii, 0.0, segments);
    let mut ink = layer;
    for band_at in 1..=layers {
        let grow = if band_at == layers {
            (spread - half).max(0.0)
        } else {
            spread * band_at as f32 / layers as f32
        };
        let outer = rounded_rect_ring(rect, radii, grow, segments);
        ink = stacked(layer, layers - band_at + 1);
        band(&painted(&inner, ink), &painted(&outer, ink), out);
        inner = outer;
    }
    let outer = rounded_rect_ring(rect, radii, spread + half, segments);
    band(
        &painted(&inner, ink),
        &painted(&outer, transparent(layer)),
        out,
    );
}

/// The inside of the rectangle [`shadow_bands`] surrounds, as dark as all its
/// layers stacked, with an edge that meets the first band's exactly.
pub fn shadow_core(
    rect: ShapeRect,
    radii: CornerRadii,
    (spread, layers): (f32, u32),
    layer: Color,
    pixel: f32,
    out: &mut Vec<ColorVertex>,
) {
    let (_, _, width, height) = rect;
    if !(positive(width) && positive(height) && positive(spread)) || layers < 1 {
        return;
    }
    let radii = clamp_radii(radii, width, height);
    let segments = corner_segments(radii, spread + pixel * 0.5, pixel);
    let ring = rounded_rect_ring(rect, radii, 0.0, segments);
    fan(&painted(&ring, stacked(layer, layers)), out);
}

/// An ellipse filling a rectangle.
pub fn fill_ellipse(rect: ShapeRect, color: Color, pixel: f32, out: &mut Vec<ColorVertex>) {
    let (x, y, width, height) = rect;
    if !(positive(width) && positive(height)) {
        return;
    }
    let centre = (x + width * 0.5, y + height * 0.5);
    let radii = (width * 0.5, height * 0.5);
    let half = pixel * 0.5;
    let segments = ellipse_segments(radii, half, pixel);
    let inner = ellipse_ring(centre, radii, -half, segments);
    let outer = ellipse_ring(centre, radii, half, segments);
    let start = out.len();
    fill_ring(&inner, &outer, color, out);
    if radii.0 <= half || radii.1 <= half {
        let area = std::f64::consts::PI * f64::from(radii.0) * f64::from(radii.1);
        scale_to_area(out, start, area);
    }
}

/// An ellipse's outline, `thickness` wide, centred on the outline. Its rings
/// grow both radii alike, which is exact for a circle and close for an
/// ellipse.
pub fn stroke_ellipse(
    rect: ShapeRect,
    color: Color,
    thickness: f32,
    pixel: f32,
    out: &mut Vec<ColorVertex>,
) {
    let (x, y, width, height) = rect;
    if !(positive(width) && positive(height) && positive(thickness)) {
        return;
    }
    let centre = (x + width * 0.5, y + height * 0.5);
    let radii = (width * 0.5, height * 0.5);
    let (half_width, ink) = stroke_profile(thickness, color, pixel);
    let fringe = pixel * 0.5;
    let segments = ellipse_segments(radii, half_width + fringe, pixel);
    let ring = |grow: f32| ellipse_ring(centre, radii, grow, segments);
    stroke_rings(
        [
            ring(half_width + fringe),
            ring(half_width - fringe),
            ring(fringe - half_width),
            ring(-half_width - fringe),
        ],
        ink,
        half_width > fringe,
        out,
    );
}

/// False for zero, a negative or not a number.
fn positive(value: f32) -> bool {
    value > 0.0
}

/// How wide a stroke is drawn and in what colour: as asked, or, thinner than a
/// device pixel, one device pixel wide with the alpha lowered in proportion,
/// so it covers what the thin stroke would have.
fn stroke_profile(thickness: f32, color: Color, pixel: f32) -> (f32, Color) {
    if thickness >= pixel {
        (thickness * 0.5, color)
    } else {
        let a = color.a * (thickness / pixel);
        (pixel * 0.5, Color { a, ..color })
    }
}

/// Four rings from outside in (outer fade, outer edge, inner edge, inner fade)
/// joined into the two soft edges and, when the stroke is wider than them, the
/// solid core between.
fn stroke_rings(
    [outer_fade, outer_edge, inner_edge, inner_fade]: [Vec<(f32, f32)>; 4],
    ink: Color,
    has_core: bool,
    out: &mut Vec<ColorVertex>,
) {
    let clear = transparent(ink);
    let outer_edge = painted(&outer_edge, ink);
    let inner_edge = painted(&inner_edge, ink);
    band(&outer_edge, &painted(&outer_fade, clear), out);
    if has_core {
        band(&inner_edge, &outer_edge, out);
    }
    band(&painted(&inner_fade, clear), &inner_edge, out);
}

/// A convex shape given as its inner (solid) and outer (clear) rings.
fn fill_ring(inner: &[(f32, f32)], outer: &[(f32, f32)], color: Color, out: &mut Vec<ColorVertex>) {
    let solid = painted(inner, color);
    fan(&solid, out);
    band(&solid, &painted(outer, transparent(color)), out);
}

/// A ring's points, all one colour.
fn painted(ring: &[(f32, f32)], color: Color) -> Vec<ColorVertex> {
    ring.iter()
        .map(|&(x, y)| ColorVertex::new(x, y, color))
        .collect()
}

/// A convex ring filled as a fan from its first point.
fn fan(ring: &[ColorVertex], out: &mut Vec<ColorVertex>) {
    for i in 1..ring.len().saturating_sub(1) {
        out.extend_from_slice(&[ring[0], ring[i], ring[i + 1]]);
    }
}

/// Two rings of the same length joined point for point, all the way round.
fn band(a: &[ColorVertex], b: &[ColorVertex], out: &mut Vec<ColorVertex>) {
    let count = a.len();
    for i in 0..count {
        let next = (i + 1) % count;
        out.extend_from_slice(&[a[i], a[next], b[next], a[i], b[next], b[i]]);
    }
}

/// Chords per corner (top left, top right, bottom right, bottom left), from
/// the largest radius any ring reaches there. A square corner is one point in
/// every ring.
fn corner_segments(radii: CornerRadii, grow: f32, pixel: f32) -> [u32; 4] {
    let corner = |radius: f32| {
        if radius > 0.0 {
            arc_segments(radius + grow, FRAC_PI_2, pixel).clamp(1, MAX_SEGMENTS_PER_CORNER)
        } else {
            0
        }
    };
    [
        corner(radii.top_left),
        corner(radii.top_right),
        corner(radii.bottom_right),
        corner(radii.bottom_left),
    ]
}

/// The outline of the rectangle grown by `grow` on every side (negative
/// shrinks it, no further than to its middle), each rounded corner's radius
/// grown alike and never below zero, clockwise on screen from the top-left
/// corner. A square corner stays square.
fn rounded_rect_ring(
    (x, y, width, height): ShapeRect,
    radii: CornerRadii,
    grow: f32,
    segments: [u32; 4],
) -> Vec<(f32, f32)> {
    let (mut left, mut top) = (x - grow, y - grow);
    let (mut w, mut h) = (width + 2.0 * grow, height + 2.0 * grow);
    if w < 0.0 {
        left = x + width * 0.5;
        w = 0.0;
    }
    if h < 0.0 {
        top = y + height * 0.5;
        h = 0.0;
    }
    let grown = clamp_radii(
        radii.map(|r| if r > 0.0 { (r + grow).max(0.0) } else { 0.0 }),
        w,
        h,
    );
    let points = segments.iter().sum::<u32>() as usize + 4;
    let mut ring = Vec::with_capacity(points);
    let r = grown.top_left;
    arc((left + r, top + r), r, PI, segments[0], &mut ring);
    let r = grown.top_right;
    arc((left + w - r, top + r), r, PI * 1.5, segments[1], &mut ring);
    let r = grown.bottom_right;
    arc((left + w - r, top + h - r), r, 0.0, segments[2], &mut ring);
    let r = grown.bottom_left;
    arc(
        (left + r, top + h - r),
        r,
        FRAC_PI_2,
        segments[3],
        &mut ring,
    );
    ring
}

/// A quarter turn clockwise on screen from `start`, as `segments` + 1 points.
fn arc((cx, cy): (f32, f32), radius: f32, start: f32, segments: u32, ring: &mut Vec<(f32, f32)>) {
    for i in 0..=segments {
        let angle = if segments == 0 {
            start
        } else {
            start + FRAC_PI_2 * i as f32 / segments as f32
        };
        ring.push((cx + radius * angle.cos(), cy + radius * angle.sin()));
    }
}

fn ellipse_segments(radii: (f32, f32), grow: f32, pixel: f32) -> u32 {
    let segments = arc_segments(radii.0.max(radii.1) + grow, TAU, pixel)
        .clamp(MIN_ELLIPSE_SEGMENTS, MAX_ELLIPSE_SEGMENTS);
    segments.div_ceil(4) * 4
}

/// The ellipse with both radii grown by `grow` (never below zero).
fn ellipse_ring(
    (cx, cy): (f32, f32),
    radii: (f32, f32),
    grow: f32,
    segments: u32,
) -> Vec<(f32, f32)> {
    let rx = (radii.0 + grow).max(0.0);
    let ry = (radii.1 + grow).max(0.0);
    (0..segments)
        .map(|i| {
            let angle = TAU * i as f32 / segments as f32;
            (cx + rx * angle.cos(), cy + ry * angle.sin())
        })
        .collect()
}

/// Weakens the triangles written from `start` on, all by one factor, until
/// what they cover weighted by alpha is `area`. A fill narrower than its soft
/// edge has no solid core, and its two soft edges alone would cover more than
/// the shape does. Never raises alpha.
fn scale_to_area(out: &mut [ColorVertex], start: usize, area: f64) {
    let covered = covered_area(&out[start..]);
    if covered <= area || covered <= 0.0 {
        return;
    }
    let factor = (area / covered) as f32;
    for v in &mut out[start..] {
        v.color.a *= factor;
    }
}

/// The area triangles cover, each weighted by its mean alpha: exact for
/// linear blending.
fn covered_area(triangles: &[ColorVertex]) -> f64 {
    triangles
        .chunks_exact(3)
        .map(|t| {
            let (ax, ay) = (f64::from(t[0].x), f64::from(t[0].y));
            let (bx, by) = (f64::from(t[1].x) - ax, f64::from(t[1].y) - ay);
            let (cx, cy) = (f64::from(t[2].x) - ax, f64::from(t[2].y) - ay);
            let alpha = f64::from(t[0].color.a) + f64::from(t[1].color.a) + f64::from(t[2].color.a);
            (bx * cy - cx * by).abs() / 2.0 * alpha / 3.0
        })
        .sum()
}

/// `layer` drawn `count` times over itself.
fn stacked(layer: Color, count: u32) -> Color {
    let a = 1.0 - (1.0 - layer.a).powi(i32::try_from(count).unwrap_or(i32::MAX));
    Color { a, ..layer }
}

fn same_point(a: ColorVertex, b: ColorVertex) -> bool {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy <= 1e-8
}

fn length((x, y): (f32, f32)) -> f32 {
    (x * x + y * y).sqrt()
}

fn transparent(color: Color) -> Color {
    Color { a: 0.0, ..color }
}

#[cfg(test)]
#[path = "shapes/tests.rs"]
mod tests;
