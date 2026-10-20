//! The plugin appearance the player picks (OpenAC `PluginUiTheme`,
//! `PluginUiThemeSettings`, `PluginUiPalette`): Classic (the client's own
//! look) or one of two dark themes that windows opting in with
//! `theme="plugin"` take. The window keeps the player's choice in its
//! settings and tells the session; plugins read it to paint their canvases
//! in the same colours.

use crate::model::Color8;

/// A theme the Appearance picker offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ThemeKind {
    /// The client's own black and gold; themed windows draw unthemed.
    #[default]
    Classic,
    /// Charcoal with moss-green accents.
    Moss,
    /// Warm graphite with brass accents.
    Brass,
}

/// The seven colours a theme draws themed windows in (OpenAC
/// `PluginUiPalette`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    /// The window's face.
    pub background: Color8,
    /// Inputs, lists and other sunken areas.
    pub field: Color8,
    /// Window and control edges.
    pub border: Color8,
    /// Body text.
    pub text: Color8,
    /// Captions, hints and disabled labels.
    pub muted: Color8,
    /// Highlights: a switch that is on, focus, links.
    pub accent: Color8,
    /// The fill behind a selected row or tab.
    pub selected: Color8,
}

const fn rgb(packed: u32) -> Color8 {
    Color8::new(
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
        0xFF,
    )
}

impl ThemePalette {
    /// Charcoal + moss.
    pub const MOSS: Self = Self {
        background: rgb(0x17_1D1B),
        field: rgb(0x10_1613),
        border: rgb(0x35_443B),
        text: rgb(0xE0_E8E2),
        muted: rgb(0x9A_A99E),
        accent: rgb(0x97_BE81),
        selected: rgb(0x34_4B37),
    };

    /// Warm graphite + brass.
    pub const BRASS: Self = Self {
        background: rgb(0x22_1F1B),
        field: rgb(0x18_1612),
        border: rgb(0x4C_4335),
        text: rgb(0xE9_E2D5),
        muted: rgb(0xB2_A58E),
        accent: rgb(0xC9_A665),
        selected: rgb(0x51_442D),
    };
}

impl ThemeKind {
    /// Every theme in picker order.
    pub const ALL: [Self; 3] = [Self::Classic, Self::Moss, Self::Brass];

    /// What the Appearance picker calls it (OpenAC
    /// `PluginUiThemeSettings.ThemeNames`).
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Moss => "Charcoal + moss",
            Self::Brass => "Warm graphite + brass",
        }
    }

    /// The name the settings file keeps (the enum member's name).
    #[must_use]
    pub fn saved_name(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Moss => "Moss",
            Self::Brass => "Brass",
        }
    }

    /// The theme a saved name stands for (OpenAC `Enum.TryParse` with
    /// `Enum.IsDefined`: a member's name, exactly, or its number); anything
    /// else is Classic.
    #[must_use]
    pub fn from_saved(name: &str) -> Self {
        let by_number = name.trim().parse::<i64>().ok().and_then(|n| match n {
            0 => Some(Self::Classic),
            1 => Some(Self::Moss),
            2 => Some(Self::Brass),
            _ => None,
        });
        by_number
            .or_else(|| {
                Self::ALL
                    .into_iter()
                    .find(|t| t.saved_name() == name.trim())
            })
            .unwrap_or_default()
    }

    /// The theme a picker name stands for.
    #[must_use]
    pub fn from_display_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.display_name() == name)
    }

    /// Its colours; none under Classic, where themed windows draw unthemed.
    #[must_use]
    pub fn palette(self) -> Option<ThemePalette> {
        match self {
            Self::Classic => None,
            Self::Moss => Some(ThemePalette::MOSS),
            Self::Brass => Some(ThemePalette::BRASS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_names_read_back_and_anything_else_is_classic() {
        for theme in ThemeKind::ALL {
            assert_eq!(ThemeKind::from_saved(theme.saved_name()), theme);
        }
        assert_eq!(ThemeKind::from_saved("2"), ThemeKind::Brass);
        assert_eq!(ThemeKind::from_saved("7"), ThemeKind::Classic);
        assert_eq!(ThemeKind::from_saved("moss"), ThemeKind::Classic);
        assert_eq!(ThemeKind::from_saved(""), ThemeKind::Classic);
    }

    #[test]
    fn the_palettes_are_openacs_bytes() {
        // PluginUiPalette.Moss / .Brass, as `ToPluginPalette` rounds them.
        assert_eq!(
            ThemePalette::MOSS.background,
            Color8::new(0x17, 0x1D, 0x1B, 255)
        );
        assert_eq!(
            ThemePalette::MOSS.accent,
            Color8::new(0x97, 0xBE, 0x81, 255)
        );
        assert_eq!(
            ThemePalette::BRASS.selected,
            Color8::new(0x51, 0x44, 0x2D, 255)
        );
        assert_eq!(ThemeKind::Classic.palette(), None);
        assert_eq!(
            ThemeKind::from_display_name("Charcoal + moss"),
            Some(ThemeKind::Moss)
        );
    }
}
