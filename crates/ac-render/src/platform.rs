//! Which windowing system the window host uses: on Linux, X11 or Wayland,
//! decided once before the event loop exists.

/// The operating system, for the decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    /// Windows (Win32 windows).
    Windows,
    /// macOS (no target yet; the decision is fixed).
    MacOs,
    /// Linux and other Unix desktops.
    Linux,
}

impl HostOs {
    /// The system this program was built for.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The windowing system asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayProtocol {
    /// Win32.
    Windows,
    /// Cocoa.
    Cocoa,
    /// X11.
    X11,
    /// Wayland.
    Wayland,
    /// Whatever the platform layer finds.
    #[default]
    Automatic,
}

/// The decision and why, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplaySelection {
    /// What is asked for.
    pub protocol: DisplayProtocol,
    /// Why.
    pub reason: String,
}

/// The environment variable that forces X11 or Wayland on Linux.
pub const DISPLAY_PROTOCOL_VAR: &str = "OPENAC_DISPLAY_PROTOCOL";

/// Decide the windowing system. On Linux `OPENAC_DISPLAY_PROTOCOL`
/// (`auto`, `x11`, `wayland`, any case) wins; otherwise a Wayland session
/// with a Wayland display uses Wayland, an X display uses X11, a Wayland
/// display alone uses Wayland, and with neither the platform layer
/// chooses. Any other value of the variable is an error naming the choices.
pub fn resolve_display(
    os: HostOs,
    env: impl Fn(&str) -> Option<String>,
) -> Result<DisplaySelection, String> {
    let selection = |protocol, reason: &str| DisplaySelection {
        protocol,
        reason: reason.to_owned(),
    };
    match os {
        HostOs::Windows => {
            return Ok(selection(
                DisplayProtocol::Windows,
                "Windows graphical host",
            ));
        }
        HostOs::MacOs => return Ok(selection(DisplayProtocol::Cocoa, "macOS graphical host")),
        HostOs::Linux => {}
    }
    let set = |name: &str| env(name).filter(|v| !v.trim().is_empty());
    if let Some(configured) = set(DISPLAY_PROTOCOL_VAR) {
        let reason = format!(
            "{DISPLAY_PROTOCOL_VAR}={}",
            configured.trim().to_lowercase()
        );
        return match configured.trim().to_lowercase().as_str() {
            "auto" => Ok(selection(DisplayProtocol::Automatic, &reason)),
            "x11" => Ok(selection(DisplayProtocol::X11, &reason)),
            "wayland" => Ok(selection(DisplayProtocol::Wayland, &reason)),
            _ => Err(format!(
                "{DISPLAY_PROTOCOL_VAR} must be auto, x11, or wayland; received '{configured}'."
            )),
        };
    }
    let wayland_display = set("WAYLAND_DISPLAY").is_some();
    if set("XDG_SESSION_TYPE").is_some_and(|t| t.eq_ignore_ascii_case("wayland")) && wayland_display
    {
        return Ok(selection(
            DisplayProtocol::Wayland,
            "XDG_SESSION_TYPE=wayland and WAYLAND_DISPLAY is set",
        ));
    }
    if set("DISPLAY").is_some() {
        return Ok(selection(DisplayProtocol::X11, "DISPLAY is set"));
    }
    if wayland_display {
        return Ok(selection(
            DisplayProtocol::Wayland,
            "WAYLAND_DISPLAY is set",
        ));
    }
    Ok(selection(
        DisplayProtocol::Automatic,
        "no X11 or Wayland environment was detected",
    ))
}

/// Apply the decision to the event loop builder (Linux only; elsewhere
/// there is one windowing system).
pub(crate) fn apply<T>(
    builder: &mut winit::event_loop::EventLoopBuilder<T>,
    protocol: DisplayProtocol,
) {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use winit::platform::wayland::EventLoopBuilderExtWayland;
        use winit::platform::x11::EventLoopBuilderExtX11;
        match protocol {
            DisplayProtocol::X11 => {
                builder.with_x11();
            }
            DisplayProtocol::Wayland => {
                builder.with_wayland();
            }
            _ => {}
        }
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = (builder, protocol);
    }
}

/// The operating system's localized name of a key by scan code (the low
/// seven bits, and whether it is an extended key), for the key
/// configuration screen when the dat names none (OpenAC
/// `PlatformKeyNameProvider.ForCurrentProcess`: Windows only).
#[must_use]
pub fn key_name_provider() -> Option<Box<ac_ui::input::key_names::OsKeyName>> {
    #[cfg(windows)]
    {
        Some(Box::new(windows_key_name))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// `GetKeyNameTextW` with the scan code in bits 16..23 and the extended
/// flag in bit 24 of its key parameter.
#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_key_name(scan: u8, extended: bool) -> Option<String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextW;
    let mut buffer = [0u16; 64];
    let parameter = (i32::from(scan) << 16) | if extended { 1 << 24 } else { 0 };
    // SAFETY: the buffer is a live, writable array of 64 UTF-16 units and
    // its length is passed as the limit; the call writes at most that many
    // units and returns how many it wrote.
    let length = unsafe { GetKeyNameTextW(parameter, buffer.as_mut_ptr(), 64) };
    let length = usize::try_from(length).ok().filter(|&n| n > 0)?;
    Some(String::from_utf16_lossy(
        &buffer[..length.min(buffer.len())],
    ))
}

#[cfg(test)]
mod tests {
    //! Cases from OpenAC `tests/AcDream.App.Tests/Platform/GraphicalWindowBackendSelectionTests.cs`.
    use super::*;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn windows_always_selects_win32() {
        let s =
            resolve_display(HostOs::Windows, env(&[(DISPLAY_PROTOCOL_VAR, "wayland")])).unwrap();
        assert_eq!(s.protocol, DisplayProtocol::Windows);
    }

    #[test]
    fn an_explicit_linux_protocol_wins() {
        for (configured, expected) in [
            ("auto", DisplayProtocol::Automatic),
            ("x11", DisplayProtocol::X11),
            ("wayland", DisplayProtocol::Wayland),
            (" WAYLAND ", DisplayProtocol::Wayland),
        ] {
            let s = resolve_display(
                HostOs::Linux,
                env(&[
                    (DISPLAY_PROTOCOL_VAR, configured),
                    ("XDG_SESSION_TYPE", "x11"),
                    ("DISPLAY", ":0"),
                ]),
            )
            .unwrap();
            assert_eq!(s.protocol, expected, "{configured}");
        }
    }

    #[test]
    fn an_invalid_explicit_protocol_is_actionable() {
        let error =
            resolve_display(HostOs::Linux, env(&[(DISPLAY_PROTOCOL_VAR, "mir")])).unwrap_err();
        assert!(error.contains("auto, x11, or wayland"));
        assert!(error.contains("mir"));
    }

    #[test]
    fn the_linux_environment_selects_the_expected_protocol() {
        let cases: [(&[(&str, &str)], DisplayProtocol); 4] = [
            (
                &[
                    ("XDG_SESSION_TYPE", "wayland"),
                    ("WAYLAND_DISPLAY", "wayland-0"),
                    ("DISPLAY", ":0"),
                ],
                DisplayProtocol::Wayland,
            ),
            (
                &[
                    ("XDG_SESSION_TYPE", "x11"),
                    ("WAYLAND_DISPLAY", "wayland-0"),
                    ("DISPLAY", ":0"),
                ],
                DisplayProtocol::X11,
            ),
            (
                &[("WAYLAND_DISPLAY", "wayland-0")],
                DisplayProtocol::Wayland,
            ),
            (&[], DisplayProtocol::Automatic),
        ];
        for (vars, expected) in cases {
            assert_eq!(
                resolve_display(HostOs::Linux, env(vars)).unwrap().protocol,
                expected
            );
        }
    }
}
