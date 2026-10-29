//! Applying the display settings to the window: fullscreen is a real mode
//! switch to the chosen resolution (when the monitor offers it), leaving
//! fullscreen restores a window of the chosen size, and a windowed pick
//! resizes the window (restoring it first when maximized).
//!
//! On Wayland a client cannot switch the monitor's mode: fullscreen there
//! asks the compositor to show the window over the whole monitor, which
//! stays at its own mode, and the window takes the monitor's size. The
//! compositor answers a request later, and a size asked for while the
//! window is still shown fullscreen is not honoured, so what a request
//! still owes is kept ([`Awaiting`]) and finished when the compositor
//! answers ([`follow_compositor`]): the chosen size once the window is shown
//! windowed again, and a leave once an unanswered fullscreen is shown.

use winit::window::{Fullscreen, Window};

use ac_host::settings::DisplaySettings;

use crate::platform::DisplayProtocol;

/// How the windowing system makes a window fullscreen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenKind {
    /// The monitor switches to the chosen mode and the window covers it
    /// (Windows, X11). The switch has happened, or not, when the request
    /// returns.
    ModeSwitch,
    /// The compositor shows the window over the whole monitor at the
    /// monitor's own mode (Wayland). The compositor answers the request
    /// later (the window is resized when it does), so a request counts as
    /// made once it is sent.
    Compositor,
}

/// The fullscreen a window on `protocol` gets.
#[must_use]
pub const fn fullscreen_kind(protocol: DisplayProtocol) -> FullscreenKind {
    match protocol {
        DisplayProtocol::Wayland => FullscreenKind::Compositor,
        _ => FullscreenKind::ModeSwitch,
    }
}

/// Whether a request to enter (`entering`) or leave fullscreen took, from
/// whether the window reports itself fullscreen right after the request.
/// A compositor answers later, so its window's report right after the
/// request says nothing yet.
#[must_use]
pub const fn request_took(kind: FullscreenKind, entering: bool, reported_fullscreen: bool) -> bool {
    match kind {
        FullscreenKind::ModeSwitch => reported_fullscreen == entering,
        FullscreenKind::Compositor => true,
    }
}

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
    /// How this window is made fullscreen.
    fn kind(&self) -> FullscreenKind;
    /// The window is fullscreen on a monitor.
    fn is_fullscreen(&self) -> bool;
    /// The fullscreen mode's size, when fullscreen.
    fn current_fullscreen_mode(&self) -> Option<(u32, u32)>;
    /// Switch the monitor to `width` x `height` with the window fullscreen
    /// on it (under a compositor: fullscreen at the monitor's own mode,
    /// whatever the size).
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

/// What a request to the compositor still waits for (Wayland): the window
/// reports its old state until the compositor answers, and a size asked
/// for while it is shown fullscreen is dropped. Always [`Awaiting::Nothing`]
/// for a mode switch, which is done when the request returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Awaiting {
    /// Nothing asked is unanswered.
    #[default]
    Nothing,
    /// Fullscreen is asked for and the window is not shown fullscreen yet.
    Fullscreen,
    /// Fullscreen is asked for and not shown yet, and the settings went
    /// back to windowed: once the window is shown fullscreen it leaves, for
    /// a window of this size. (Asking to leave before the compositor has
    /// answered would leave the outcome to the order of its answers.)
    FullscreenThenWindowed {
        /// Inner width, physical pixels.
        width: u32,
        /// Inner height, physical pixels.
        height: u32,
    },
    /// Leaving is asked for; once the window is shown windowed it is given
    /// this size.
    Windowed {
        /// Inner width, physical pixels.
        width: u32,
        /// Inner height, physical pixels.
        height: u32,
    },
}

/// Finish what `awaiting` still owes now that the window reports the
/// compositor's answer (call on every resize: the compositor's answer
/// resizes the window). The chosen size goes to a window shown windowed
/// again, and a window shown fullscreen after the settings went back to
/// windowed leaves. Lines for the log go to `log`.
pub fn follow_compositor(
    awaiting: &mut Awaiting,
    surface: &mut dyn WindowSizeSurface,
    switcher: &mut dyn DisplayModeSwitcher,
    log: &mut Vec<String>,
) {
    match *awaiting {
        Awaiting::Nothing => {}
        Awaiting::Fullscreen => {
            if switcher.is_fullscreen() {
                *awaiting = Awaiting::Nothing;
            }
        }
        Awaiting::FullscreenThenWindowed { width, height } => {
            if switcher.is_fullscreen() {
                *awaiting = match switcher.try_leave_fullscreen(width, height) {
                    Ok(()) => Awaiting::Windowed { width, height },
                    Err(error) => {
                        log.push(format!("display: leaving fullscreen failed ({error})"));
                        Awaiting::Nothing
                    }
                };
            }
        }
        Awaiting::Windowed { width, height } => {
            if !switcher.is_fullscreen() {
                *awaiting = Awaiting::Nothing;
                if surface.size() != (width, height) {
                    log.push(format!(
                        "display: windowed again, resolution {width}x{height}"
                    ));
                    surface.set_size(width, height);
                }
            }
        }
    }
}

/// Apply `display`'s fullscreen and resolution; `is_offered` says whether
/// a fullscreen mode is one the monitor offers (for a mode switch; under a
/// compositor the size is not used). `awaiting` is what earlier requests
/// to a compositor still wait for, kept by the window between calls. Lines
/// for the log go to `log`. Returns whether the window is fullscreen
/// afterwards.
pub fn apply_display(
    display: &DisplaySettings,
    surface: &mut dyn WindowSizeSurface,
    switcher: &mut dyn DisplayModeSwitcher,
    awaiting: &mut Awaiting,
    is_offered: impl Fn(&str) -> bool,
    log: &mut Vec<String>,
) -> bool {
    let resolution = parse_resolution(&display.resolution);
    if switcher.kind() == FullscreenKind::Compositor {
        apply_compositor(display, resolution, surface, switcher, awaiting, log);
        return switcher.is_fullscreen();
    }
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
    resize_windowed(resolution, surface, log);
    switcher.is_fullscreen()
}

/// A windowed pick: the window takes the chosen size, restored first when
/// maximized.
fn resize_windowed(
    resolution: Option<(u32, u32)>,
    surface: &mut dyn WindowSizeSurface,
    log: &mut Vec<String>,
) {
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
}

/// [`apply_display`] under a compositor. Its fullscreen is at the
/// monitor's own mode, so the chosen size is not checked for it; the
/// settings' state is reached whatever the compositor has answered so far.
fn apply_compositor(
    display: &DisplaySettings,
    resolution: Option<(u32, u32)>,
    surface: &mut dyn WindowSizeSurface,
    switcher: &mut dyn DisplayModeSwitcher,
    awaiting: &mut Awaiting,
    log: &mut Vec<String>,
) {
    // Settle what the compositor has answered already.
    follow_compositor(awaiting, surface, switcher, log);
    let (w, h) = resolution.unwrap_or_else(|| surface.size());
    if display.fullscreen {
        if *awaiting == Awaiting::Nothing && switcher.is_fullscreen() {
            return;
        }
        // Asked again even when a fullscreen is still unanswered (the
        // compositor may have refused it): a leave planned after it, or a
        // leave still unanswered, is overtaken, as the compositor takes
        // requests in order and the last one decides.
        match switcher.try_enter_fullscreen(w, h) {
            Ok(()) => *awaiting = Awaiting::Fullscreen,
            Err(error) => log.push(format!(
                "display: fullscreen failed ({error}) — window state unchanged"
            )),
        }
        return;
    }
    match *awaiting {
        // Not shown fullscreen yet, so the window is still windowed and
        // takes the size now; it leaves once the compositor shows it
        // fullscreen.
        Awaiting::Fullscreen | Awaiting::FullscreenThenWindowed { .. } => {
            resize_windowed(resolution, surface, log);
            *awaiting = Awaiting::FullscreenThenWindowed {
                width: w,
                height: h,
            };
        }
        // The leave is asked for already; the size it ends at changes.
        Awaiting::Windowed { .. } => {
            *awaiting = Awaiting::Windowed {
                width: w,
                height: h,
            };
        }
        Awaiting::Nothing if switcher.is_fullscreen() => {
            match switcher.try_leave_fullscreen(w, h) {
                Ok(()) => {
                    *awaiting = Awaiting::Windowed {
                        width: w,
                        height: h,
                    };
                }
                Err(error) => log.push(format!("display: leaving fullscreen failed ({error})")),
            }
        }
        Awaiting::Nothing => resize_windowed(resolution, surface, log),
    }
}

/// The monitor a fullscreen request goes to: the window's own, else the
/// primary. Under a compositor a window is on no monitor until it is first
/// shown and there is no primary monitor, so the first monitor is used.
fn fullscreen_monitor<M>(
    kind: FullscreenKind,
    current: Option<M>,
    primary: Option<M>,
    first: impl FnOnce() -> Option<M>,
) -> Option<M> {
    current.or(primary).or_else(|| match kind {
        FullscreenKind::Compositor => first(),
        FullscreenKind::ModeSwitch => None,
    })
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
    fn kind(&self) -> FullscreenKind {
        fullscreen_kind(crate::platform::window_protocol(self.0))
    }

    fn is_fullscreen(&self) -> bool {
        self.0.fullscreen().is_some()
    }

    fn current_fullscreen_mode(&self) -> Option<(u32, u32)> {
        match self.0.fullscreen()? {
            Fullscreen::Exclusive(mode) => {
                let s = mode.size();
                Some((s.width, s.height))
            }
            // The monitor keeps its own mode, and that is the window's size.
            Fullscreen::Borderless(monitor) => {
                let s = monitor.or_else(|| self.0.current_monitor())?.size();
                Some((s.width, s.height))
            }
        }
    }

    fn try_enter_fullscreen(&mut self, width: u32, height: u32) -> Result<(), String> {
        let kind = self.kind();
        let monitor = fullscreen_monitor(
            kind,
            self.0.current_monitor(),
            self.0.primary_monitor(),
            || self.0.available_monitors().next(),
        )
        .ok_or("no monitor")?;
        let request = match kind {
            FullscreenKind::ModeSwitch => {
                // The highest refresh rate the monitor has at that size.
                let mode = monitor
                    .video_modes()
                    .filter(|m| {
                        let s = m.size();
                        (s.width, s.height) == (width, height)
                    })
                    .max_by_key(winit::monitor::VideoModeHandle::refresh_rate_millihertz)
                    .ok_or_else(|| {
                        format!("mode {width}x{height} is not in the monitor's mode list")
                    })?;
                Fullscreen::Exclusive(mode)
            }
            // The monitor keeps its own mode; the size has no effect.
            FullscreenKind::Compositor => Fullscreen::Borderless(Some(monitor)),
        };
        self.0.set_fullscreen(Some(request));
        if !request_took(kind, true, self.0.fullscreen().is_some()) {
            return Err("the mode switch did not take".into());
        }
        Ok(())
    }

    fn try_leave_fullscreen(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.0.fullscreen().is_none() {
            return Ok(());
        }
        let kind = self.kind();
        self.0.set_fullscreen(None);
        // Under a compositor the window stays fullscreen until it answers,
        // and a size asked for meanwhile is dropped: the size follows the
        // answer (`follow_compositor`).
        if kind == FullscreenKind::ModeSwitch {
            let _ = self
                .0
                .request_inner_size(winit::dpi::PhysicalSize::new(width, height));
        }
        if !request_took(kind, false, self.0.fullscreen().is_some()) {
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
        kind: FullscreenKind,
        fullscreen: bool,
        mode: Option<(u32, u32)>,
        enter_succeeds: bool,
        calls: Vec<String>,
    }

    impl Default for Switcher {
        fn default() -> Self {
            Self {
                kind: FullscreenKind::ModeSwitch,
                fullscreen: false,
                mode: None,
                enter_succeeds: true,
                calls: Vec::new(),
            }
        }
    }

    impl DisplayModeSwitcher for Switcher {
        fn kind(&self) -> FullscreenKind {
            self.kind
        }
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
        apply_display(
            d,
            surface,
            switcher,
            &mut Awaiting::Nothing,
            offered,
            &mut Vec::new(),
        )
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
    fn wayland_fullscreen_is_the_compositors_and_the_rest_switch_modes() {
        assert_eq!(
            fullscreen_kind(DisplayProtocol::Wayland),
            FullscreenKind::Compositor
        );
        for protocol in [
            DisplayProtocol::Windows,
            DisplayProtocol::X11,
            DisplayProtocol::Cocoa,
            DisplayProtocol::Automatic,
        ] {
            assert_eq!(
                fullscreen_kind(protocol),
                FullscreenKind::ModeSwitch,
                "{protocol:?}"
            );
        }
    }

    fn compositor() -> Switcher {
        Switcher {
            kind: FullscreenKind::Compositor,
            ..Switcher::default()
        }
    }

    /// The compositor keeps the monitor at its own mode, so the chosen
    /// size is not checked: the default 1280x720 is rarely in a Wayland
    /// monitor's list, which usually holds only the current mode.
    #[test]
    fn compositor_fullscreen_does_not_need_an_offered_size() {
        let (mut s, mut w) = (Surface::default(), compositor());
        assert!(apply(&display(true, "1280x720"), &mut s, &mut w, |_| {
            false
        }));
        assert_eq!(w.calls, ["enter:1280x720"]);
        assert_eq!(s.writes, 0);
    }

    #[test]
    fn compositor_fullscreen_does_not_need_a_readable_size() {
        let mut s = Surface {
            size: (1600, 900),
            ..Surface::default()
        };
        let mut w = compositor();
        assert!(apply(&display(true, "garbage"), &mut s, &mut w, |_| true));
        assert_eq!(w.calls, ["enter:1600x900"]);
    }

    /// Any chosen size is the compositor's fullscreen at the monitor's
    /// mode, so a window already fullscreen there is left alone.
    #[test]
    fn compositor_fullscreen_already_fullscreen_does_nothing() {
        let (mut s, mut w) = (
            Surface::default(),
            Switcher {
                fullscreen: true,
                mode: Some((2560, 1440)),
                ..compositor()
            },
        );
        apply(&display(true, "1920x1080"), &mut s, &mut w, |_| true);
        assert!(w.calls.is_empty());
    }

    /// Before the first frame a Wayland window is on no monitor yet and
    /// there is no primary monitor, so the first monitor is used (as the
    /// primary monitor is the first one there in OpenAC's window library).
    #[test]
    fn compositor_fullscreen_falls_back_to_the_first_monitor() {
        assert_eq!(
            fullscreen_monitor(FullscreenKind::Compositor, None, None, || Some(3)),
            Some(3)
        );
        assert_eq!(
            fullscreen_monitor(FullscreenKind::Compositor, Some(1), Some(2), || Some(3)),
            Some(1)
        );
        assert_eq!(
            fullscreen_monitor(FullscreenKind::Compositor, None, Some(2), || Some(3)),
            Some(2)
        );
    }

    #[test]
    fn a_mode_switch_uses_the_windows_monitor_else_the_primary_only() {
        let k = FullscreenKind::ModeSwitch;
        assert_eq!(fullscreen_monitor(k, Some(1), Some(2), || Some(3)), Some(1));
        assert_eq!(fullscreen_monitor(k, None, Some(2), || Some(3)), Some(2));
        assert_eq!(fullscreen_monitor(k, None, None, || Some(3)), None);
    }

    #[test]
    fn a_mode_switch_took_only_when_the_window_reports_it() {
        let k = FullscreenKind::ModeSwitch;
        assert!(request_took(k, true, true));
        assert!(!request_took(k, true, false));
        assert!(request_took(k, false, false));
        assert!(!request_took(k, false, true));
    }

    /// The window reports its old state until the compositor answers, so
    /// an unanswered request is not a failure.
    #[test]
    fn a_compositor_request_took_before_the_compositor_answers() {
        let k = FullscreenKind::Compositor;
        assert!(request_took(k, true, false));
        assert!(request_took(k, false, true));
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

    /// A compositor that answers requests only when told to, oldest first,
    /// as a Wayland compositor does some time after the request.
    #[derive(Default)]
    struct LateCompositor {
        shown_fullscreen: bool,
        unanswered: Vec<bool>,
        calls: Vec<String>,
    }

    impl LateCompositor {
        /// The compositor answers the oldest request; a window shown
        /// windowed again gets the compositor's own size for it.
        fn answer(&mut self, surface: &mut Surface) {
            if self.unanswered.is_empty() {
                return;
            }
            self.shown_fullscreen = self.unanswered.remove(0);
            surface.size = if self.shown_fullscreen {
                (2560, 1440)
            } else {
                (1280, 720)
            };
        }
    }

    impl DisplayModeSwitcher for LateCompositor {
        fn kind(&self) -> FullscreenKind {
            FullscreenKind::Compositor
        }
        fn is_fullscreen(&self) -> bool {
            self.shown_fullscreen
        }
        fn current_fullscreen_mode(&self) -> Option<(u32, u32)> {
            self.shown_fullscreen.then_some((2560, 1440))
        }
        fn try_enter_fullscreen(&mut self, w: u32, h: u32) -> Result<(), String> {
            self.calls.push(format!("enter:{w}x{h}"));
            self.unanswered.push(true);
            Ok(())
        }
        fn try_leave_fullscreen(&mut self, w: u32, h: u32) -> Result<(), String> {
            self.calls.push(format!("leave:{w}x{h}"));
            self.unanswered.push(false);
            Ok(())
        }
    }

    fn apply_late(
        d: &DisplaySettings,
        surface: &mut Surface,
        compositor: &mut LateCompositor,
        awaiting: &mut Awaiting,
    ) -> bool {
        apply_display(d, surface, compositor, awaiting, |_| false, &mut Vec::new())
    }

    fn follow(awaiting: &mut Awaiting, surface: &mut Surface, compositor: &mut LateCompositor) {
        follow_compositor(awaiting, surface, compositor, &mut Vec::new());
    }

    /// The size asked for while still shown fullscreen would be dropped,
    /// so it is given once the compositor shows the window windowed; a
    /// later resize by the player is left alone.
    #[test]
    fn leaving_compositor_fullscreen_gives_the_chosen_size_once_shown_windowed() {
        let mut s = Surface {
            size: (2560, 1440),
            ..Surface::default()
        };
        let mut c = LateCompositor {
            shown_fullscreen: true,
            ..LateCompositor::default()
        };
        let mut aw = Awaiting::Nothing;
        apply_late(&display(false, "1600x900"), &mut s, &mut c, &mut aw);
        assert_eq!(c.calls, ["leave:1600x900"]);
        assert_eq!(s.writes, 0, "no size while still shown fullscreen");
        assert_eq!(
            aw,
            Awaiting::Windowed {
                width: 1600,
                height: 900
            }
        );
        follow(&mut aw, &mut s, &mut c);
        assert_eq!(s.writes, 0, "the compositor has not answered");
        c.answer(&mut s);
        follow(&mut aw, &mut s, &mut c);
        assert_eq!((s.writes, s.size), (1, (1600, 900)));
        assert_eq!(aw, Awaiting::Nothing);
        s.size = (1700, 1000);
        follow(&mut aw, &mut s, &mut c);
        assert_eq!(s.writes, 1, "the player's own resize stays");
    }

    /// Fullscreen turned off before the compositor showed it: the window
    /// is still windowed and takes the size at once, then leaves once the
    /// compositor shows it fullscreen, and ends windowed at the size.
    #[test]
    fn compositor_fullscreen_turned_off_before_it_is_shown_ends_windowed() {
        let mut s = Surface {
            size: (1280, 720),
            ..Surface::default()
        };
        let mut c = LateCompositor::default();
        let mut aw = Awaiting::Nothing;
        apply_late(&display(true, "1280x720"), &mut s, &mut c, &mut aw);
        assert_eq!(aw, Awaiting::Fullscreen);
        apply_late(&display(false, "1600x900"), &mut s, &mut c, &mut aw);
        assert_eq!(c.calls, ["enter:1280x720"], "no leave before the answer");
        assert_eq!((s.writes, s.size), (1, (1600, 900)));
        c.answer(&mut s);
        follow(&mut aw, &mut s, &mut c);
        assert_eq!(c.calls, ["enter:1280x720", "leave:1600x900"]);
        c.answer(&mut s);
        follow(&mut aw, &mut s, &mut c);
        assert!(!c.shown_fullscreen);
        assert_eq!(s.size, (1600, 900));
        assert_eq!(aw, Awaiting::Nothing);
        assert!(c.unanswered.is_empty());
    }

    /// Fullscreen turned on again before the compositor answered the leave:
    /// the last request decides, and the window ends fullscreen without a
    /// size written.
    #[test]
    fn compositor_fullscreen_turned_on_before_the_leave_is_shown_ends_fullscreen() {
        let mut s = Surface {
            size: (2560, 1440),
            ..Surface::default()
        };
        let mut c = LateCompositor {
            shown_fullscreen: true,
            ..LateCompositor::default()
        };
        let mut aw = Awaiting::Nothing;
        apply_late(&display(false, "1600x900"), &mut s, &mut c, &mut aw);
        apply_late(&display(true, "1600x900"), &mut s, &mut c, &mut aw);
        assert_eq!(c.calls, ["leave:1600x900", "enter:1600x900"]);
        assert_eq!(aw, Awaiting::Fullscreen);
        c.answer(&mut s);
        follow(&mut aw, &mut s, &mut c);
        assert_eq!(aw, Awaiting::Fullscreen, "shown windowed for a moment");
        c.answer(&mut s);
        follow(&mut aw, &mut s, &mut c);
        assert!(c.shown_fullscreen);
        assert_eq!(aw, Awaiting::Nothing);
        assert_eq!(s.writes, 0);
    }

    /// A second windowed pick while the leave is unanswered changes the
    /// size the window ends at, without a second leave.
    #[test]
    fn a_windowed_pick_during_a_compositor_leave_changes_its_size() {
        let mut s = Surface {
            size: (2560, 1440),
            ..Surface::default()
        };
        let mut c = LateCompositor {
            shown_fullscreen: true,
            ..LateCompositor::default()
        };
        let mut aw = Awaiting::Nothing;
        apply_late(&display(false, "1600x900"), &mut s, &mut c, &mut aw);
        apply_late(&display(false, "1920x1080"), &mut s, &mut c, &mut aw);
        assert_eq!(c.calls, ["leave:1600x900"]);
        c.answer(&mut s);
        follow(&mut aw, &mut s, &mut c);
        assert_eq!(s.size, (1920, 1080));
    }

    /// A compositor that never shows the window fullscreen (it refused):
    /// the windowed pick still gives the size at once, and turning
    /// fullscreen on again asks again.
    #[test]
    fn a_refused_compositor_fullscreen_leaves_the_window_usable() {
        let mut s = Surface {
            size: (1280, 720),
            ..Surface::default()
        };
        let mut c = LateCompositor::default();
        let mut aw = Awaiting::Nothing;
        apply_late(&display(true, "1280x720"), &mut s, &mut c, &mut aw);
        c.unanswered.clear();
        apply_late(&display(false, "1600x900"), &mut s, &mut c, &mut aw);
        assert_eq!(s.size, (1600, 900));
        assert!(!c.shown_fullscreen);
        apply_late(&display(true, "1600x900"), &mut s, &mut c, &mut aw);
        assert_eq!(c.calls, ["enter:1280x720", "enter:1600x900"]);
        assert_eq!(aw, Awaiting::Fullscreen);
    }

    /// A mode switch is done when the request returns: nothing is awaited.
    #[test]
    fn a_mode_switch_awaits_nothing() {
        let (mut s, mut w) = (Surface::default(), Switcher::default());
        let mut aw = Awaiting::Nothing;
        apply_display(
            &display(true, "1920x1080"),
            &mut s,
            &mut w,
            &mut aw,
            |_| true,
            &mut Vec::new(),
        );
        assert_eq!(aw, Awaiting::Nothing);
        apply_display(
            &display(false, "1600x900"),
            &mut s,
            &mut w,
            &mut aw,
            |_| true,
            &mut Vec::new(),
        );
        assert_eq!(aw, Awaiting::Nothing);
        assert_eq!(w.calls, ["enter:1920x1080", "leave:1600x900"]);
    }
}
