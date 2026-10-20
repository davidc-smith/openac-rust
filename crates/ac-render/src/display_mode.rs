//! Applying the display settings to the window: fullscreen is a real mode
//! switch to the chosen resolution (when the monitor offers it), leaving
//! fullscreen restores a window of the chosen size, and a windowed pick
//! resizes the window (restoring it first when maximized).

use winit::window::{Fullscreen, Window};

use ac_host::settings::DisplaySettings;

/// The window's size and maximized state.
pub trait WindowSizeSurface {
    /// Inner size, physical pixels.
    fn size(&self) -> (u32, u32);
    /// Ask for an inner size.
    fn set_size(&mut self, width: u32, height: u32);
    /// The window is maximized.
    fn is_maximized(&self) -> bool;
    /// Un-maximize.
    fn restore(&mut self);
}

/// The monitor's mode switching.
pub trait DisplayModeSwitcher {
    /// The window is fullscreen on a monitor.
    fn is_fullscreen(&self) -> bool;
    /// The fullscreen mode's size, when fullscreen.
    fn current_fullscreen_mode(&self) -> Option<(u32, u32)>;
    /// Switch the monitor to `width` x `height` with the window fullscreen
    /// on it.
    ///
    /// # Errors
    /// Why the switch did not happen.
    fn try_enter_fullscreen(&mut self, width: u32, height: u32) -> Result<(), String>;
    /// Leave fullscreen for a window of `width` x `height`.
    ///
    /// # Errors
    /// Why the window is still fullscreen.
    fn try_leave_fullscreen(&mut self, width: u32, height: u32) -> Result<(), String>;
}

/// `"<w>x<h>"` with both positive.
#[must_use]
pub fn parse_resolution(spec: &str) -> Option<(u32, u32)> {
    let (w, h) = spec.trim().split_once('x')?;
    let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// Apply `display`'s fullscreen and resolution; `is_offered` says whether
/// a fullscreen mode is one the monitor offers. Lines for the log go to
/// `log`. Returns whether the window is fullscreen afterwards.
pub fn apply_display(
    display: &DisplaySettings,
    surface: &mut dyn WindowSizeSurface,
    switcher: &mut dyn DisplayModeSwitcher,
    is_offered: impl Fn(&str) -> bool,
    log: &mut Vec<String>,
) -> bool {
    let resolution = parse_resolution(&display.resolution);
    if display.fullscreen {
        let Some((w, h)) = resolution else {
            log.push(format!(
                "display: fullscreen refused — unparseable resolution '{}'",
                display.resolution
            ));
            return switcher.is_fullscreen();
        };
        if switcher.current_fullscreen_mode() == Some((w, h)) {
            return switcher.is_fullscreen();
        }
        if !is_offered(&format!("{w}x{h}")) {
            log.push(format!(
                "display: fullscreen {w}x{h} refused — not an offered mode"
            ));
            return switcher.is_fullscreen();
        }
        if let Err(error) = switcher.try_enter_fullscreen(w, h) {
            log.push(format!(
                "display: fullscreen {w}x{h} failed ({error}) — window state unchanged"
            ));
        }
        return switcher.is_fullscreen();
    }
    if switcher.is_fullscreen() {
        let (w, h) = resolution.unwrap_or_else(|| surface.size());
        if let Err(error) = switcher.try_leave_fullscreen(w, h) {
            log.push(format!("display: leaving fullscreen failed ({error})"));
        }
        return switcher.is_fullscreen();
    }
    if let Some((w, h)) = resolution
        && surface.size() != (w, h)
    {
        if surface.is_maximized() {
            surface.restore();
        }
        let (ow, oh) = surface.size();
        log.push(format!(
            "display: resolution pick {w}x{h} (window was {ow}x{oh})"
        ));
        surface.set_size(w, h);
    }
    switcher.is_fullscreen()
}

/// The window as both the size surface and the mode switcher.
#[derive(Debug)]
pub struct WinitDisplay<'a>(pub &'a Window);

impl WindowSizeSurface for WinitDisplay<'_> {
    fn size(&self) -> (u32, u32) {
        let s = self.0.inner_size();
        (s.width, s.height)
    }

    fn set_size(&mut self, width: u32, height: u32) {
        let _ = self
            .0
            .request_inner_size(winit::dpi::PhysicalSize::new(width, height));
    }

    fn is_maximized(&self) -> bool {
        self.0.is_maximized()
    }

    fn restore(&mut self) {
        self.0.set_maximized(false);
    }
}

impl DisplayModeSwitcher for WinitDisplay<'_> {
    fn is_fullscreen(&self) -> bool {
        self.0.fullscreen().is_some()
    }

    fn current_fullscreen_mode(&self) -> Option<(u32, u32)> {
        match self.0.fullscreen()? {
            Fullscreen::Exclusive(mode) => {
                let s = mode.size();
                Some((s.width, s.height))
            }
            Fullscreen::Borderless(_) => None,
        }
    }

    fn try_enter_fullscreen(&mut self, width: u32, height: u32) -> Result<(), String> {
        let monitor = self
            .0
            .current_monitor()
            .or_else(|| self.0.primary_monitor())
            .ok_or("no monitor")?;
        // The highest refresh rate the monitor has at that size.
        let mode = monitor
            .video_modes()
            .filter(|m| {
                let s = m.size();
                (s.width, s.height) == (width, height)
            })
            .max_by_key(winit::monitor::VideoModeHandle::refresh_rate_millihertz)
            .ok_or_else(|| format!("mode {width}x{height} is not in the monitor's mode list"))?;
        self.0.set_fullscreen(Some(Fullscreen::Exclusive(mode)));
        if self.0.fullscreen().is_none() {
            return Err("the mode switch did not take".into());
        }
        Ok(())
    }

    fn try_leave_fullscreen(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.0.fullscreen().is_none() {
            return Ok(());
        }
        self.0.set_fullscreen(None);
        let _ = self
            .0
            .request_inner_size(winit::dpi::PhysicalSize::new(width, height));
        if self.0.fullscreen().is_some() {
            return Err("the window is still fullscreen".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Ported from OpenAC `tests/AcDream.App.Tests/Settings/RuntimeSettingsControllerTests.cs`
    //! (`DisplayApply_*`, `SilkRuntimeDisplayWindowTarget`).

    use super::*;

    #[derive(Default)]
    struct Surface {
        size: (u32, u32),
        maximized: bool,
        writes: usize,
        restores: usize,
    }

    impl WindowSizeSurface for Surface {
        fn size(&self) -> (u32, u32) {
            self.size
        }
        fn set_size(&mut self, width: u32, height: u32) {
            self.writes += 1;
            self.size = (width, height);
        }
        fn is_maximized(&self) -> bool {
            self.maximized
        }
        fn restore(&mut self) {
            self.restores += 1;
            self.maximized = false;
        }
    }

    struct Switcher {
        fullscreen: bool,
        mode: Option<(u32, u32)>,
        enter_succeeds: bool,
        calls: Vec<String>,
    }

    impl Default for Switcher {
        fn default() -> Self {
            Self {
                fullscreen: false,
                mode: None,
                enter_succeeds: true,
                calls: Vec::new(),
            }
        }
    }

    impl DisplayModeSwitcher for Switcher {
        fn is_fullscreen(&self) -> bool {
            self.fullscreen
        }
        fn current_fullscreen_mode(&self) -> Option<(u32, u32)> {
            self.mode
        }
        fn try_enter_fullscreen(&mut self, w: u32, h: u32) -> Result<(), String> {
            self.calls.push(format!("enter:{w}x{h}"));
            if !self.enter_succeeds {
                return Err("refused".into());
            }
            self.fullscreen = true;
            self.mode = Some((w, h));
            Ok(())
        }
        fn try_leave_fullscreen(&mut self, w: u32, h: u32) -> Result<(), String> {
            self.calls.push(format!("leave:{w}x{h}"));
            self.fullscreen = false;
            self.mode = None;
            Ok(())
        }
    }

    fn display(fullscreen: bool, resolution: &str) -> DisplaySettings {
        DisplaySettings {
            fullscreen,
            resolution: resolution.into(),
            ..DisplaySettings::default()
        }
    }

    fn apply(
        d: &DisplaySettings,
        surface: &mut Surface,
        switcher: &mut Switcher,
        offered: impl Fn(&str) -> bool,
    ) -> bool {
        apply_display(d, surface, switcher, offered, &mut Vec::new())
    }

    #[test]
    fn the_same_fullscreen_mode_does_nothing() {
        let (mut s, mut w) = (
            Surface::default(),
            Switcher {
                fullscreen: true,
                mode: Some((1920, 1080)),
                ..Switcher::default()
            },
        );
        apply(&display(true, "1920x1080"), &mut s, &mut w, |_| true);
        assert!(w.calls.is_empty());
        assert_eq!(s.writes, 0);
    }

    #[test]
    fn an_unreadable_resolution_is_refused_without_calls() {
        let (mut s, mut w) = (Surface::default(), Switcher::default());
        apply(&display(true, "garbage"), &mut s, &mut w, |_| true);
        assert!(w.calls.is_empty());
        assert_eq!(s.writes, 0);
    }

    #[test]
    fn a_maximized_windowed_pick_restores_before_the_size_write() {
        let mut s = Surface {
            maximized: true,
            size: (1280, 720),
            ..Surface::default()
        };
        let mut w = Switcher::default();
        apply(&display(false, "1600x900"), &mut s, &mut w, |_| true);
        assert_eq!((s.restores, s.writes), (1, 1));
        assert!(!s.maximized);
    }

    #[test]
    fn a_fullscreen_pick_is_a_validated_mode_switch_never_a_size_write() {
        let (mut s, mut w) = (Surface::default(), Switcher::default());
        apply(&display(true, "1920x1080"), &mut s, &mut w, |m| {
            m == "1920x1080"
        });
        assert_eq!(w.calls, ["enter:1920x1080"]);
        assert_eq!(s.writes, 0);
    }

    #[test]
    fn an_unoffered_fullscreen_mode_is_refused_not_attempted() {
        let (mut s, mut w) = (Surface::default(), Switcher::default());
        assert!(!apply(&display(true, "1234x777"), &mut s, &mut w, |_| {
            false
        }));
        assert!(w.calls.is_empty());
        assert_eq!(s.writes, 0);
    }

    #[test]
    fn a_failed_mode_switch_leaves_the_window_usable() {
        let mut s = Surface::default();
        let mut w = Switcher {
            enter_succeeds: false,
            ..Switcher::default()
        };
        assert!(!apply(&display(true, "1920x1080"), &mut s, &mut w, |_| {
            true
        }));
        assert!(!w.fullscreen);
        assert_eq!(s.writes, 0);
    }

    #[test]
    fn windowed_while_fullscreen_leaves_through_the_switcher() {
        let mut s = Surface::default();
        let mut w = Switcher {
            fullscreen: true,
            ..Switcher::default()
        };
        apply(&display(false, "1600x900"), &mut s, &mut w, |_| true);
        assert_eq!(w.calls, ["leave:1600x900"]);
        assert_eq!(s.writes, 0, "leaving sets the size itself");
    }

    #[test]
    fn a_plain_windowed_pick_writes_the_size() {
        let mut s = Surface {
            size: (1280, 720),
            ..Surface::default()
        };
        let mut w = Switcher::default();
        apply(&display(false, "1600x900"), &mut s, &mut w, |_| true);
        assert!(w.calls.is_empty());
        assert_eq!(s.writes, 1);
        assert_eq!(s.size, (1600, 900));
    }
}
