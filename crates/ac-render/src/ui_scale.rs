//! The UI's scale: how many framebuffer pixels one UI unit covers.
//!
//! On macOS the UI is laid out in points, as OpenAC does there: a Retina
//! display's framebuffer has two pixels a point, and the retail UI drawn
//! one unit a pixel would come out at half its size. Elsewhere one unit is
//! one pixel. The world, picking and screenshots stay in pixels.

use crate::platform::HostOs;

/// Framebuffer pixels per UI unit (1 or more).
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
    /// in pixels (its 3-D viewports).
    pub(crate) fn pixels_per_unit(self) -> f32 {
        self.0 as f32
    }

    /// A window position in pixels, in UI units.
    pub(crate) fn ui_point(self, x: f64, y: f64) -> (f64, f64) {
        (x / self.0, y / self.0)
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
}
