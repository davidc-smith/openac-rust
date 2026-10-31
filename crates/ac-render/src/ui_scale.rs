//! The UI's scale: how many framebuffer pixels one UI unit covers.
//!
//! Two things set it. On macOS the UI is laid out in points, as OpenAC
//! does there: a Retina display's framebuffer has two pixels a point, and
//! the retail UI drawn one unit a pixel would come out at half its size.
//! Elsewhere one unit is one pixel. On top of that the player's UI Scale
//! option (50% to 300%) multiplies it, in the game only (the pre-game
//! screens fill the window whatever the scale).
//!
//! The UI is laid out and hit-tested in units; the world, picking and
//! screenshots stay in pixels. Every crossing between the two goes through
//! this type: a pointer in pixels into units ([`UiScale::ui_point`]), a UI
//! position into pixels ([`UiScale::window_point`]), the world's projection
//! into units ([`UiScale::overlay_viewport`]), and what the UI draws into
//! pixels ([`UiScale::pixels_per_unit`], snapped to whole pixels by
//! `crate::ui::batch`).

use crate::platform::HostOs;

/// Framebuffer pixels per UI unit (more than 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiScale(f64);

impl UiScale {
    /// One unit a pixel.
    pub(crate) const ONE: Self = Self(1.0);

    /// The scale for a window whose monitor reports `scale_factor` (winit's
    /// pixels per point) on `os`.
    /// A factor below one, or not a number, counts as one.
    pub(crate) fn for_window(os: HostOs, scale_factor: f64) -> Self {
        if os == HostOs::MacOs && scale_factor.is_finite() && scale_factor > 1.0 {
            Self(scale_factor)
        } else {
            Self::ONE
        }
    }

    /// This scale with the player's UI Scale option of `percent` on top
    /// (read through `ac_host::settings::ui_scale_percent`, so within 50 to
    /// 300).
    pub(crate) fn with_user_percent(self, percent: i32) -> Self {
        let percent = ac_host::settings::ui_scale_percent(percent);
        Self(self.0 * f64::from(percent) / 100.0)
    }

    /// The UI's size, in units, of a framebuffer of `pixels`: whole units,
    /// at least one along a side that has pixels.
    pub(crate) fn ui_size(self, pixels: (u32, u32)) -> (u32, u32) {
        let side = |p: u32| {
            if p == 0 {
                0
            } else {
                ((f64::from(p) / self.0).round() as u32).max(1)
            }
        };
        (side(pixels.0), side(pixels.1))
    }

    /// Framebuffer pixels per UI unit, for what the UI shows that is drawn
    /// in pixels (its 3-D viewports) and for drawing the UI itself.
    pub(crate) fn pixels_per_unit(self) -> f32 {
        self.0 as f32
    }

    /// A window position in pixels, in UI units.
    pub(crate) fn ui_point(self, x: f64, y: f64) -> (f64, f64) {
        (x / self.0, y / self.0)
    }

    /// A position the UI reports in its units, in window pixels: the
    /// world picks in pixels, so what the UI hands it (where an item was
    /// let go, where a drag over the world began) is scaled back first.
    pub(crate) fn window_point(self, x: i32, y: i32) -> (f32, f32) {
        (
            (f64::from(x) * self.0) as f32,
            (f64::from(y) * self.0) as f32,
        )
    }

    /// The screen the overlays project the world onto, in UI units, for a
    /// framebuffer of `pixels`: the world fills the framebuffer, so a point
    /// the camera puts at pixel `p` sits at unit `p / scale`, where the UI
    /// draws it back at pixel `p`.
    pub(crate) fn overlay_viewport(self, pixels: (u32, u32)) -> (f32, f32) {
        (
            (f64::from(pixels.0) / self.0) as f32,
            (f64::from(pixels.1) / self.0) as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retina_mac_lays_the_ui_out_in_points() {
        let scale = UiScale::for_window(HostOs::MacOs, 2.0);
        assert_eq!(scale.ui_size((3024, 1842)), (1512, 921));
        assert_eq!(scale.ui_point(800.0, 401.0), (400.0, 200.5));
    }

    /// An item let go over the world at (400, 300) points on a Retina
    /// screen lands on what is drawn at (800, 600) pixels, not on what is
    /// at half the pointer's position.
    #[test]
    fn a_ui_position_picks_the_world_at_its_pixel() {
        let scale = UiScale::for_window(HostOs::MacOs, 2.0);
        assert_eq!(scale.window_point(400, 300), (800.0, 600.0));
        // The pointer the UI saw, back where the window had it.
        let (x, y) = scale.ui_point(800.0, 600.0);
        assert_eq!(scale.window_point(x as i32, y as i32), (800.0, 600.0));
    }

    #[test]
    fn one_unit_a_pixel_picks_where_the_ui_says() {
        for (x, y) in [(0, 0), (301, 7), (-5, 1_000_000), (i32::MAX, i32::MIN)] {
            assert_eq!(
                UiScale::ONE.window_point(x, y),
                (x as f32, y as f32),
                "{x},{y}"
            );
        }
    }

    #[test]
    fn a_one_x_mac_display_keeps_one_unit_a_pixel() {
        let scale = UiScale::for_window(HostOs::MacOs, 1.0);
        assert_eq!(scale, UiScale::ONE);
        assert_eq!(scale.ui_size((2560, 1440)), (2560, 1440));
    }

    #[test]
    fn windows_and_linux_keep_one_unit_a_pixel_whatever_the_monitor_says() {
        for os in [HostOs::Windows, HostOs::Linux] {
            let scale = UiScale::for_window(os, 1.5);
            assert_eq!(scale, UiScale::ONE, "{os:?}");
            assert_eq!(scale.ui_point(301.0, 7.0), (301.0, 7.0), "{os:?}");
        }
    }

    #[test]
    fn a_factor_that_is_not_a_scale_counts_as_one() {
        for factor in [0.0, -2.0, 0.5, f64::NAN, f64::INFINITY] {
            assert_eq!(
                UiScale::for_window(HostOs::MacOs, factor),
                UiScale::ONE,
                "{factor}"
            );
        }
    }

    #[test]
    fn an_odd_pixel_size_rounds_to_whole_units_and_never_to_nothing() {
        let scale = UiScale::for_window(HostOs::MacOs, 2.0);
        assert_eq!(scale.ui_size((1001, 1)), (501, 1));
        assert_eq!(scale.ui_size((0, 0)), (0, 0));
    }

    #[test]
    fn one_hundred_percent_changes_nothing() {
        assert_eq!(UiScale::ONE.with_user_percent(100), UiScale::ONE);
        let retina = UiScale::for_window(HostOs::MacOs, 2.0);
        assert_eq!(retina.with_user_percent(100), retina);
    }

    /// The player's scale multiplies the platform's: 150% on a Retina Mac
    /// is three pixels a unit; 50% elsewhere is two units a pixel.
    #[test]
    fn the_players_scale_multiplies_the_platforms() {
        let retina = UiScale::for_window(HostOs::MacOs, 2.0).with_user_percent(150);
        assert_eq!(retina.pixels_per_unit(), 3.0);
        assert_eq!(retina.ui_size((3000, 1800)), (1000, 600));
        let half = UiScale::ONE.with_user_percent(50);
        assert_eq!(half.pixels_per_unit(), 0.5);
        assert_eq!(half.ui_size((1920, 1080)), (3840, 2160));
        assert_eq!(half.ui_point(100.0, 50.0), (200.0, 100.0));
        assert_eq!(half.window_point(200, 100), (100.0, 50.0));
    }

    /// The UI at 150% on 1920 x 1080 pixels is 1280 x 720 units; a click
    /// at pixel (960, 540) is unit (640, 360), and that unit picks the
    /// world at the same pixel again.
    #[test]
    fn at_one_hundred_fifty_percent_a_click_and_its_pick_meet_at_one_pixel() {
        let scale = UiScale::ONE.with_user_percent(150);
        assert_eq!(scale.ui_size((1920, 1080)), (1280, 720));
        assert_eq!(scale.ui_point(960.0, 540.0), (640.0, 360.0));
        assert_eq!(scale.window_point(640, 360), (960.0, 540.0));
        assert_eq!(scale.overlay_viewport((1920, 1080)), (1280.0, 720.0));
    }

    #[test]
    fn a_scale_outside_the_offered_range_is_brought_into_it() {
        assert_eq!(UiScale::ONE.with_user_percent(10).pixels_per_unit(), 0.5);
        assert_eq!(UiScale::ONE.with_user_percent(900).pixels_per_unit(), 3.0);
        assert_eq!(UiScale::ONE.with_user_percent(154).pixels_per_unit(), 1.5);
    }

    /// The vivid target bracket at 150%: the world is drawn at full
    /// resolution, so the selected creature's sphere is at a pixel square;
    /// the bracket is placed in units on the overlay viewport, and the UI
    /// draws units at 1.5 pixels each, so it frames the same pixel square.
    /// Projected onto the framebuffer's pixels instead, the bracket would
    /// be drawn half as far again from the top-left corner (the reported
    /// "worse towards the bottom right" drift).
    #[test]
    fn the_target_bracket_frames_the_creature_at_one_hundred_fifty_percent() {
        use ac_ui::overlay::{OverlayCamera, TargetProjection, compute_projection, target_layout};
        use glam::{Mat4, Vec2, Vec3, Vec4};
        let framebuffer = (1920, 1080);
        // At the origin looking down -z, y up; a 1-radian vertical field of
        // view at 16:9 (depth 0.1 to 1000 m).
        let view = Mat4::IDENTITY;
        let f = 1.0 / 0.5f32.tan();
        let (near, far) = (0.1f32, 1000.0f32);
        let projection = Mat4::from_cols(
            Vec4::new(f / (1920.0 / 1080.0), 0.0, 0.0, 0.0),
            Vec4::new(0.0, f, 0.0, 0.0),
            Vec4::new(0.0, 0.0, far / (near - far), -1.0),
            Vec4::new(0.0, 0.0, near * far / (near - far), 0.0),
        );
        // Down and to the right of the screen's centre, 10 m away.
        let (center, radius) = (Vec3::new(3.0, -1.5, -10.0), 0.5);
        let square = |viewport: (f32, f32)| {
            let camera = OverlayCamera {
                view,
                projection,
                viewport,
            };
            match compute_projection(center, radius, &camera) {
                TargetProjection::OnScreen { min, max } => (min, max),
                other => panic!("{other:?}"),
            }
        };
        let (pixel_min, pixel_max) = square((1920.0, 1080.0));
        let scale = UiScale::ONE.with_user_percent(150);
        let viewport = scale.overlay_viewport(framebuffer);
        let (min, max) = square(viewport);
        let ppu = scale.pixels_per_unit();
        let near = |a: Vec2, b: Vec2| (a - b).length() < 1e-2;
        assert!(near(min * ppu, pixel_min), "{min:?} {pixel_min:?}");
        assert!(near(max * ppu, pixel_max), "{max:?} {pixel_max:?}");
        // The bracket's corners sit just outside that square.
        let corner = Vec2::new(12.0, 12.0);
        let layout = target_layout(min, max, Vec2::new(viewport.0, viewport.1), [corner; 4]);
        assert!(near((layout.position + corner) * ppu, pixel_min));
        assert!(near(
            (layout.position + layout.size - corner) * ppu,
            pixel_max
        ));
        // Placed on the pixel viewport and then drawn at 150%, it misses.
        assert!(!near(pixel_min * ppu, pixel_min));
    }

    /// A drop-down at 50% on a 1280 x 720 window: the UI is 2560 x 1440
    /// units; a popup too wide for the room right of its button shifts
    /// left to end at the screen's right edge in units, is drawn inside the
    /// window's pixels, and a click at a row's pixel picks that row.
    #[test]
    fn a_drop_down_at_fifty_percent_opens_on_screen_and_picks_where_clicked() {
        use crate::ui::batch::UiBatch;
        use ac_ui::draw::{DrawList, NoSprites};
        use ac_ui::widgets::{Menu, MenuItem};
        use ac_ui::{Element, ElementMessage, MouseButton, UiOutput, UiRoot};

        let framebuffer = (1280, 720);
        let scale = UiScale::ONE.with_user_percent(50);
        let (w, h) = scale.ui_size(framebuffer);
        assert_eq!((w, h), (2560, 1440));
        let mut ui = UiRoot::new();
        ui.set_size(w as f32, h as f32);
        let mut m = Menu::default();
        m.column_width = 100.0;
        m.fits_long_labels = true;
        m.retail_button_art = false;
        m.open_upward = false;
        m.scrollable = true;
        m.popup_scrollbar_hide_when_disabled = true;
        m.rows_per_column = 4;
        m.row_height = 18.0;
        m.text_indent = 6.0;
        // 40 characters at 7 units each: wider than the 160 units left of
        // the screen's edge.
        let long = "x".repeat(40);
        m.items = ["A", "B", "C", long.as_str()]
            .iter()
            .enumerate()
            .map(|(i, l)| MenuItem::new(*l, i as u64))
            .collect();
        let mut e = Element::at(2400.0, 1300.0, 100.0, 20.0);
        Menu::configure(&mut e);
        let root = ui.root();
        let menu = ui.add(root, e, m);
        let click = |ui: &mut UiRoot, px: f64, py: f64| {
            let (x, y) = scale.ui_point(px, py);
            ui.on_mouse_down(MouseButton::Left, x as i32, y as i32, 0);
            ui.on_mouse_up(MouseButton::Left, x as i32, y as i32, 0);
        };
        // The button is pixels (1200, 650) to (1250, 660).
        click(&mut ui, 1210.0, 655.0);
        let opened = ui.widget::<Menu>(menu).unwrap();
        assert!(opened.is_open());
        let right = 2400.0 + opened.popup_left() + opened.popup_outer_width();
        assert_eq!(right, 2560.0);
        let list = ui.draw((w as f32, h as f32), &NoSprites);
        let popup = UiBatch::build_scaled(
            &DrawList {
                main: Vec::new(),
                overlay: list.overlay,
                canvas_scale: list.canvas_scale,
            },
            scale.pixels_per_unit(),
        );
        assert!(!popup.is_empty());
        for v in &popup.vertices {
            let [x, y] = v.position;
            assert!(
                (0.0..=1280.0).contains(&x) && (0.0..=720.0).contains(&y),
                "{x},{y}"
            );
        }
        ui.drain_output();
        // The third row is units 1361 to 1379 high (under the 20-unit
        // button and the 5-unit border, 18 units a row), left of the button:
        // pixels (1160, 685).
        click(&mut ui, 1160.0, 685.0);
        let picked: Vec<u64> = ui
            .drain_output()
            .into_iter()
            .filter_map(|o| match o {
                UiOutput::Element {
                    message: ElementMessage::MenuSelected(p),
                    ..
                } => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(picked, vec![2]);
    }

    /// The overlays' screen is the framebuffer in units, not the UI's
    /// rounded size: at 110% a 1001-pixel-wide window is 910 units, and
    /// the world's right edge is at unit 910 exactly.
    #[test]
    fn the_overlay_viewport_is_the_framebuffer_in_units() {
        let scale = UiScale::ONE.with_user_percent(110);
        let (w, h) = scale.overlay_viewport((1001, 600));
        assert!((w - 910.0).abs() < 1e-3, "{w}");
        assert!((h - 545.454_5).abs() < 1e-3, "{h}");
        assert_eq!(
            UiScale::ONE.overlay_viewport((1920, 1080)),
            (1920.0, 1080.0)
        );
    }
}
