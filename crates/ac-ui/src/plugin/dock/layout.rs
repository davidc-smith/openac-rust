//! Where everything in the plugin dock goes, worked out from its mode,
//! whether it is minimized, which plugin owns each slot and how long it may
//! be. It is pure: the dock applies the result to its children.
//!
//! It works along the dock's main axis, the way its slots run, and across
//! it, so a horizontal dock is the vertical one turned on its side. Along the
//! main axis: the tile, a divider, the slots (a divider between two plugins, a
//! gap between one plugin's windows), a divider, the gear slot and padding.
//! When the slots do not fit, only those that do from the first one are laid
//! out; the tile and the gear stay. Closed, the dock is only the tile, and the
//! tile's art is in the same place either way.

use crate::geometry::Rect;

/// The padding round a floating dock's slots at the default size.
pub const PADDING: f32 = 6.0;
/// A floating dock's slot, each way, at the default size.
pub const SLOT: f32 = 36.0;
/// Between a floating dock's slots of one plugin at the default size.
pub const GAP: f32 = 4.0;
/// A slot's art, each way, at the default size.
pub const ART: f32 = 24.0;
/// A floating dock's thickness across its main axis at the default size.
pub const FLOATING_THICKNESS: f32 = PADDING + SLOT + PADDING;
/// The space each side of a divider.
pub const DIVIDER_SPACE: f32 = 4.0;
/// How long a fade over cut-off slots is.
pub const FADE_LENGTH: f32 = 12.0;
/// A rail's thickness across its main axis at the default size.
pub const RAIL_THICKNESS: f32 = 46.0;
/// A rail's slot along its main axis at the default size.
pub const RAIL_SLOT: f32 = 38.0;
/// Between a rail's slots of one plugin.
pub const RAIL_GAP: f32 = 2.0;
/// How far a floating dock's longest stays from each end of the screen, so
/// that one which fills the screen is not touching an edge wherever it is
/// dragged.
pub const FLOATING_MARGIN: f32 = 24.0;

/// A dragged dock's edge snaps to a screen edge this close.
pub const SNAP_IN: f32 = 8.0;
/// A rail floats again once it is pulled this far from its screen edge.
pub const SNAP_OUT: f32 = 32.0;

const DIVIDER_BLOCK: f32 = DIVIDER_SPACE * 2.0 + 1.0;

/// How big the dock is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DockSize {
    /// Smaller than the default.
    Smaller,
    /// The default.
    #[default]
    Default,
    /// Bigger than the default. A dat icon's 32 pixels are its art's own size
    /// on a standard screen.
    Bigger,
}

impl DockSize {
    /// Every size, in the order the Appearance window lists them.
    pub const ALL: [Self; 3] = [Self::Smaller, Self::Default, Self::Bigger];

    /// The name it is saved under.
    #[must_use]
    pub const fn saved_name(self) -> &'static str {
        match self {
            Self::Smaller => "smaller",
            Self::Default => "default",
            Self::Bigger => "bigger",
        }
    }

    /// The size saved as `name`, in any case; an unknown name is the default.
    #[must_use]
    pub fn from_saved(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|s| s.saved_name().eq_ignore_ascii_case(name))
            .unwrap_or_default()
    }

    /// Its name in the Appearance window.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Smaller => "Smaller",
            Self::Default => "Default",
            Self::Bigger => "Bigger",
        }
    }

    /// The size with this name in the Appearance window.
    #[must_use]
    pub fn from_display_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.display_name() == name)
    }

    /// Its measurements.
    #[must_use]
    pub const fn metrics(self) -> Metrics {
        match self {
            Self::Smaller => Metrics {
                padding: 5.0,
                slot: 28.0,
                gap: 3.0,
                art: 18.0,
                rail_thickness: 36.0,
                rail_slot: 30.0,
                radius: 12.0,
                rail_radius: 10.0,
                well_radius: 7.0,
            },
            Self::Default => Metrics {
                padding: PADDING,
                slot: SLOT,
                gap: GAP,
                art: ART,
                rail_thickness: RAIL_THICKNESS,
                rail_slot: RAIL_SLOT,
                radius: 14.0,
                rail_radius: 12.0,
                well_radius: 9.0,
            },
            Self::Bigger => Metrics {
                padding: 7.0,
                slot: 44.0,
                gap: 5.0,
                art: 32.0,
                rail_thickness: 56.0,
                rail_slot: 46.0,
                radius: 16.0,
                rail_radius: 14.0,
                well_radius: 11.0,
            },
        }
    }
}

/// The measurements of a dock of one size, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// The padding round a floating dock's slots.
    pub padding: f32,
    /// A floating dock's slot, each way.
    pub slot: f32,
    /// Between a floating dock's slots of one plugin.
    pub gap: f32,
    /// A slot's art, each way.
    pub art: f32,
    /// A rail's thickness across its main axis.
    pub rail_thickness: f32,
    /// A rail's slot along its main axis.
    pub rail_slot: f32,
    /// A floating dock's corner radius.
    pub radius: f32,
    /// A rail's inner corner radius.
    pub rail_radius: f32,
    /// A slot's hover well's corner radius.
    pub well_radius: f32,
}

impl Metrics {
    /// A floating dock's thickness across its main axis.
    #[must_use]
    pub const fn floating_thickness(&self) -> f32 {
        self.padding + self.slot + self.padding
    }
}

/// Where the dock is and which way it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DockMode {
    /// Free on the screen, running top to bottom.
    #[default]
    Floating,
    /// Free on the screen, running left to right.
    FloatingHorizontal,
    /// A rail on the left edge.
    Left,
    /// A rail on the right edge.
    Right,
    /// A rail on the top edge.
    Top,
    /// A rail on the bottom edge.
    Bottom,
}

impl DockMode {
    /// Every mode, in the order the Appearance window lists them.
    pub const ALL: [Self; 6] = [
        Self::Floating,
        Self::FloatingHorizontal,
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
    ];

    /// The name it is saved under.
    #[must_use]
    pub const fn saved_name(self) -> &'static str {
        match self {
            Self::Floating => "floating",
            Self::FloatingHorizontal => "floating-horizontal",
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    /// The mode saved as `name`, in any case; an unknown name is floating.
    #[must_use]
    pub fn from_saved(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|m| m.saved_name().eq_ignore_ascii_case(name))
            .unwrap_or_default()
    }

    /// Its name in the Appearance window.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Floating => "Floating",
            Self::FloatingHorizontal => "Floating, horizontal",
            Self::Left => "Left edge",
            Self::Right => "Right edge",
            Self::Top => "Top edge",
            Self::Bottom => "Bottom edge",
        }
    }

    /// The mode with this name in the Appearance window.
    #[must_use]
    pub fn from_display_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.display_name() == name)
    }

    /// Whether its slots run left to right.
    #[must_use]
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::FloatingHorizontal | Self::Top | Self::Bottom)
    }

    /// Whether it is a rail on a screen edge.
    #[must_use]
    pub const fn is_rail(self) -> bool {
        !matches!(self, Self::Floating | Self::FloatingHorizontal)
    }
}

/// The longest a dock in `mode` may be on a screen of `width` by `height`. It
/// comes from the screen alone, not from where the dock is: a dock limited by
/// the room left below it would always end level with the bottom of the
/// screen, and so count as touching that edge wherever it was dragged.
#[must_use]
pub fn max_length(mode: DockMode, width: f32, height: f32) -> f32 {
    let screen = if mode.is_horizontal() { width } else { height };
    if mode.is_rail() {
        screen
    } else {
        screen - 2.0 * FLOATING_MARGIN
    }
}

/// The pointer's distance from a rail's screen edge; none for a floating dock.
#[must_use]
pub fn pointer_from_edge(
    mode: DockMode,
    (screen_w, screen_h): (f32, f32),
    (x, y): (f32, f32),
) -> Option<f32> {
    match mode {
        DockMode::Left => Some(x),
        DockMode::Right => Some(screen_w - x),
        DockMode::Top => Some(y),
        DockMode::Bottom => Some(screen_h - y),
        DockMode::Floating | DockMode::FloatingHorizontal => None,
    }
}

/// The mode a dock dragged to `at` takes, with the pointer at `pointer`. A
/// floating dock becomes a rail when one of its edges is within [`SNAP_IN`] of
/// a screen edge and that is the screen edge nearest the pointer, so that a
/// long dock whose far end touches an edge does not snap there while it is
/// being carried past.
///
/// A rail stays until the pointer is [`SNAP_OUT`] further from its edge than
/// `closest`, the closest it has been in this drag, and then floats running the
/// way it ran, so moving to another edge means floating first. It goes by the
/// pointer because a dock that snaps by its far end may turn into a rail well
/// away from the pointer.
#[must_use]
pub fn dragged_mode(
    mode: DockMode,
    at: Rect,
    screen: (f32, f32),
    pointer: (f32, f32),
    closest: f32,
) -> DockMode {
    let (screen_w, screen_h) = screen;
    let (x, y) = pointer;
    if let Some(away) = pointer_from_edge(mode, screen, pointer) {
        return match mode {
            _ if away <= closest + SNAP_OUT => mode,
            DockMode::Left | DockMode::Right => DockMode::Floating,
            _ => DockMode::FloatingHorizontal,
        };
    }
    let edges = [
        (DockMode::Left, x, at.x),
        (DockMode::Right, screen_w - x, screen_w - (at.x + at.w)),
        (DockMode::Top, y, at.y),
        (DockMode::Bottom, screen_h - y, screen_h - (at.y + at.h)),
    ];
    let mut nearest = edges[0];
    for edge in &edges[1..] {
        if edge.1 < nearest.1 {
            nearest = *edge;
        }
    }
    if nearest.2 <= SNAP_IN {
        nearest.0
    } else {
        mode
    }
}

/// Where the dock's parts are, in its own space.
#[derive(Debug, Clone, PartialEq)]
pub struct DockLayout {
    /// The dock's width.
    pub width: f32,
    /// The dock's height.
    pub height: f32,
    /// Each slot's place; none for a slot that is not laid out.
    pub slots: Vec<Option<Rect>>,
    /// Where each one-point divider starts along the main axis.
    pub dividers: Vec<f32>,
    /// The gear's place; empty on a closed dock.
    pub gear: Rect,
    /// The tile, which moves the dock and opens and closes it.
    pub tile: Rect,
    /// The first slot laid out.
    pub first: usize,
    /// How many slots are laid out.
    pub visible: usize,
    /// The furthest the slots can scroll.
    pub max_first: usize,
    /// Slots are cut off before the list.
    pub fade_start: bool,
    /// Slots are cut off after the list.
    pub fade_end: bool,
    /// Where the list starts along the main axis.
    pub list_start: f32,
    /// Where the list ends along the main axis.
    pub list_end: f32,
}

/// Places things by main and cross axis in a dock running either way.
#[derive(Debug, Clone, Copy)]
struct Axes {
    horizontal: bool,
}

impl Axes {
    fn rect(self, main: f32, cross: f32, main_length: f32, cross_length: f32) -> Rect {
        if self.horizontal {
            Rect::new(main, cross, main_length, cross_length)
        } else {
            Rect::new(cross, main, cross_length, main_length)
        }
    }

    fn size(self, main_length: f32, cross_length: f32) -> (f32, f32) {
        if self.horizontal {
            (main_length, cross_length)
        } else {
            (cross_length, main_length)
        }
    }
}

/// The layout of a dock in `mode` at `size`, minimized or not, with one slot per entry
/// of `owners` (each slot's plugin), at most `max_length` long, scrolled to
/// start at slot `first`.
#[must_use]
pub fn compute(
    mode: DockMode,
    size: DockSize,
    minimized: bool,
    owners: &[&str],
    max_length: f32,
    first: usize,
) -> DockLayout {
    let axes = Axes {
        horizontal: mode.is_horizontal(),
    };
    let rail = mode.is_rail();
    let count = owners.len();
    let mut slots = vec![None; count];

    let m = size.metrics();
    let thickness = if rail {
        m.rail_thickness
    } else {
        m.floating_thickness()
    };
    if minimized {
        return DockLayout {
            width: thickness,
            height: thickness,
            slots,
            dividers: Vec::new(),
            gear: Rect::new(0.0, 0.0, 0.0, 0.0),
            tile: Rect::new(0.0, 0.0, thickness, thickness),
            first: 0,
            visible: 0,
            max_first: 0,
            fade_start: false,
            fade_end: false,
            list_start: 0.0,
            list_end: 0.0,
        };
    }

    // A rail's tile is centred in the square a closed rail is, so its art stays
    // where it was.
    let (tile_start, slot_cross_at, slot_across, slot_along, gap) = if rail {
        (
            (m.rail_thickness - m.rail_slot) / 2.0,
            0.0,
            m.rail_thickness,
            m.rail_slot,
            RAIL_GAP,
        )
    } else {
        (m.padding, m.padding, m.slot, m.slot, m.gap)
    };
    let tile = axes.rect(tile_start, slot_cross_at, slot_along, slot_across);
    let list_start = tile_start + slot_along + DIVIDER_BLOCK;
    let spacing = |i: usize| {
        if owners[i] == owners[i - 1] {
            gap
        } else {
            DIVIDER_BLOCK
        }
    };
    // Everything that is not the slot list: the tile and divider before it, and
    // the divider, gear and padding after it.
    let divider = if count > 0 { DIVIDER_BLOCK } else { 0.0 };
    let room = max_length - (list_start + divider + slot_along + m.padding);

    // At least one slot, however little room there is.
    let mut fit_backward = 0;
    let mut used = 0.0;
    for i in (0..count).rev() {
        let next =
            used + if fit_backward > 0 {
                spacing(i + 1)
            } else {
                0.0
            } + slot_along;
        if next > room && fit_backward > 0 {
            break;
        }
        used = next;
        fit_backward += 1;
    }
    let max_first = count - fit_backward;
    let first = first.min(max_first);
    let mut visible = 0;
    let mut used = 0.0;
    for i in first..count {
        let next = used + if visible > 0 { spacing(i) } else { 0.0 } + slot_along;
        if next > room && visible > 0 {
            break;
        }
        used = next;
        visible += 1;
    }

    let mut dividers = vec![tile_start + slot_along + DIVIDER_SPACE + 0.5];
    let mut at = list_start;
    for (i, slot) in slots.iter_mut().enumerate().skip(first).take(visible) {
        if i > first {
            let space = spacing(i);
            if owners[i] != owners[i - 1] {
                dividers.push(at + DIVIDER_SPACE + 0.5);
            }
            at += space;
        }
        *slot = Some(axes.rect(at, slot_cross_at, slot_along, slot_across));
        at += slot_along;
    }
    let list_end = at;
    if count > 0 {
        dividers.push(at + DIVIDER_SPACE + 0.5);
        at += DIVIDER_BLOCK;
    }
    let gear = axes.rect(at, slot_cross_at, slot_along, slot_across);
    let (width, height) = axes.size(at + slot_along + m.padding, thickness);
    DockLayout {
        width,
        height,
        slots,
        dividers,
        gear,
        tile,
        first,
        visible,
        max_first,
        fade_start: first > 0,
        fade_end: first + visible < count,
        list_start,
        list_end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At the default size.
    fn compute(
        mode: DockMode,
        minimized: bool,
        owners: &[&str],
        max: f32,
        first: usize,
    ) -> DockLayout {
        super::compute(mode, DockSize::Default, minimized, owners, max, first)
    }

    fn floating(owners: &[&str], max: f32, first: usize) -> DockLayout {
        compute(DockMode::Floating, false, owners, max, first)
    }

    fn twelve() -> Vec<String> {
        (0..12).map(|i| format!("p{i}")).collect()
    }

    fn refs(owners: &[String]) -> Vec<&str> {
        owners.iter().map(String::as_str).collect()
    }

    #[test]
    fn one_slot_is_the_tile_a_divider_a_slot_a_divider_the_gear_and_padding() {
        let l = floating(&["a"], f32::INFINITY, 0);
        assert_eq!(l.width, 48.0);
        // 6 + 36 (tile) + 9 (divider) + 36 (slot) + 9 (divider) + 36 (gear) + 6
        assert_eq!(l.height, 138.0);
        assert_eq!(l.tile, Rect::new(6.0, 6.0, 36.0, 36.0));
        assert_eq!(l.slots[0], Some(Rect::new(6.0, 51.0, 36.0, 36.0)));
        assert_eq!(l.dividers, [46.5, 91.5]);
        assert_eq!(l.gear, Rect::new(6.0, 96.0, 36.0, 36.0));
    }

    #[test]
    fn slots_of_one_plugin_have_a_gap_and_another_plugin_a_divider() {
        let l = floating(&["moss", "moss", "goarrow"], f32::INFINITY, 0);
        let main = |i: usize| l.slots[i].unwrap().y;
        assert_eq!(main(0), 51.0);
        assert_eq!(main(1), 91.0); // 51 + 36 + 4
        assert_eq!(main(2), 136.0); // 91 + 36 + 9
        assert_eq!(l.dividers, [46.5, 131.5, 176.5]);
        assert_eq!(l.gear.y, 181.0);
        assert_eq!(l.height, 223.0);
    }

    #[test]
    fn overflow_shows_what_fits_and_scrolling_stops_at_the_last_full_page() {
        let owners = twelve();
        let owners = refs(&owners);
        // Room for 51 + 3 slots (36 + 9 + 36 + 9 + 36) + 9 + 36 + 6 = 228.
        let top = floating(&owners, 230.0, 0);
        assert_eq!((top.visible, top.first, top.max_first), (3, 0, 9));
        assert!(!top.fade_start && top.fade_end);
        assert!(top.height <= 230.0);
        assert!(top.slots[2].is_some());
        assert!(top.slots[3].is_none());

        let end = floating(&owners, 230.0, 50);
        assert_eq!(end.first, 9);
        assert!(end.fade_start && !end.fade_end);
        assert!(end.slots[8].is_none());
        assert_eq!(end.slots[9].unwrap().y, 51.0);
        assert!(end.slots[11].is_some());
    }

    #[test]
    fn too_little_room_still_shows_one_slot() {
        let l = floating(&["a", "b"], 10.0, 0);
        assert_eq!(l.visible, 1);
        assert!(l.slots[0].is_some());
    }

    #[test]
    fn a_rail_has_full_width_slots_at_the_same_pitch_as_floating() {
        for mode in [DockMode::Left, DockMode::Right] {
            let rail = compute(mode, false, &["a", "a"], f32::INFINITY, 0);
            assert_eq!(rail.width, 46.0);
            assert_eq!(rail.tile, Rect::new(0.0, 4.0, 46.0, 38.0));
            assert_eq!(rail.slots[0], Some(Rect::new(0.0, 51.0, 46.0, 38.0)));
            assert_eq!(rail.slots[1].unwrap().y, 91.0); // 51 + 38 + 2
        }
    }

    #[test]
    fn closed_is_only_the_tile_a_square_as_thick_as_the_dock() {
        for (mode, side) in [
            (DockMode::Floating, 48.0),
            (DockMode::FloatingHorizontal, 48.0),
            (DockMode::Left, 46.0),
            (DockMode::Right, 46.0),
            (DockMode::Top, 46.0),
            (DockMode::Bottom, 46.0),
        ] {
            let l = compute(mode, true, &["a"], 500.0, 0);
            assert_eq!((l.width, l.height), (side, side));
            assert_eq!(l.tile, Rect::new(0.0, 0.0, side, side));
            assert!(l.slots.iter().all(Option::is_none));
            assert!(l.dividers.is_empty());
        }
    }

    #[test]
    fn the_tiles_art_does_not_move_between_closed_and_open() {
        for mode in DockMode::ALL {
            let centre = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
            let closed = compute(mode, true, &["a"], 500.0, 0);
            let open = compute(mode, false, &["a"], 500.0, 0);
            assert_eq!(centre(closed.tile), centre(open.tile), "{mode:?}");
        }
    }

    #[test]
    fn the_three_sizes_measure_as_agreed_and_the_default_is_unchanged() {
        let owners = ["a", "b", "c", "d"];
        for (size, slot, art, thick, rail, height) in [
            (DockSize::Smaller, 28.0, 18.0, 38.0, 36.0, 223.0),
            (DockSize::Default, 36.0, 24.0, 48.0, 46.0, 273.0),
            (DockSize::Bigger, 44.0, 32.0, 58.0, 56.0, 323.0),
        ] {
            let m = size.metrics();
            assert_eq!((m.slot, m.art), (slot, art), "{size:?}");
            let free = super::compute(DockMode::Floating, size, false, &owners, 2000.0, 0);
            assert_eq!(free.width, thick, "{size:?}");
            assert_eq!(free.height, height, "{size:?}");
            assert_eq!(free.tile.w, slot, "{size:?}");
            let edge = super::compute(DockMode::Left, size, false, &owners, 2000.0, 0);
            assert_eq!(edge.width, rail, "{size:?}");
            let closed = super::compute(DockMode::Floating, size, true, &owners, 2000.0, 0);
            assert_eq!((closed.width, closed.height), (thick, thick), "{size:?}");
            for mode in DockMode::ALL {
                let centre = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
                let shut = super::compute(mode, size, true, &owners, 2000.0, 0);
                let open = super::compute(mode, size, false, &owners, 2000.0, 0);
                assert_eq!(centre(shut.tile), centre(open.tile), "{size:?} {mode:?}");
            }
        }
    }

    #[test]
    fn size_names_round_trip_and_an_unknown_one_is_the_default() {
        for size in DockSize::ALL {
            assert_eq!(DockSize::from_saved(size.saved_name()), size);
            assert_eq!(DockSize::from_display_name(size.display_name()), Some(size));
        }
        assert_eq!(DockSize::from_saved("BIGGER"), DockSize::Bigger);
        assert_eq!(DockSize::from_saved("huge"), DockSize::Default);
        assert_eq!(DockSize::from_saved(""), DockSize::Default);
    }

    #[test]
    fn no_entries_have_no_divider_before_the_gear() {
        let l = floating(&[], f32::INFINITY, 0);
        assert_eq!(l.dividers, [46.5]);
        assert_eq!(l.gear.y, 51.0);
    }

    // ── Running the other way ───────────────────────────────────────────

    /// A rectangle turned on its side.
    fn turned(r: Rect) -> Rect {
        Rect::new(r.y, r.x, r.h, r.w)
    }

    #[test]
    fn a_horizontal_dock_is_the_vertical_one_turned_on_its_side() {
        let owners = ["moss", "moss", "goarrow"];
        for (vertical, horizontal) in [
            (DockMode::Floating, DockMode::FloatingHorizontal),
            (DockMode::Left, DockMode::Top),
            (DockMode::Right, DockMode::Bottom),
        ] {
            for minimized in [false, true] {
                let v = compute(vertical, minimized, &owners, 260.0, 0);
                let h = compute(horizontal, minimized, &owners, 260.0, 0);
                assert_eq!((h.width, h.height), (v.height, v.width));
                assert_eq!(h.tile, turned(v.tile));
                assert_eq!(h.gear, turned(v.gear));
                assert_eq!(h.dividers, v.dividers);
                assert_eq!(
                    h.slots,
                    v.slots.iter().map(|s| s.map(turned)).collect::<Vec<_>>()
                );
                assert_eq!(
                    (h.first, h.visible, h.max_first),
                    (v.first, v.visible, v.max_first)
                );
            }
        }
    }

    #[test]
    fn a_horizontal_dock_lays_out_along_x() {
        let l = compute(
            DockMode::FloatingHorizontal,
            false,
            &["a"],
            f32::INFINITY,
            0,
        );
        assert_eq!((l.width, l.height), (138.0, 48.0));
        assert_eq!(l.tile, Rect::new(6.0, 6.0, 36.0, 36.0));
        assert_eq!(l.slots[0], Some(Rect::new(51.0, 6.0, 36.0, 36.0)));
        assert_eq!(l.gear, Rect::new(96.0, 6.0, 36.0, 36.0));
    }

    #[test]
    fn a_horizontal_dock_overflows_along_its_length() {
        let owners = twelve();
        let owners = refs(&owners);
        let l = compute(DockMode::Top, false, &owners, 400.0, 2);
        assert_eq!(l.height, RAIL_THICKNESS);
        assert!(l.width <= 400.0);
        assert!(l.fade_start && l.fade_end);
        assert!(l.slots[1].is_none());
        assert_eq!(l.slots[2].unwrap().x, l.list_start);
    }

    #[test]
    fn saved_names_round_trip_and_an_unknown_one_is_floating() {
        for mode in DockMode::ALL {
            assert_eq!(DockMode::from_saved(mode.saved_name()), mode);
            assert_eq!(DockMode::from_display_name(mode.display_name()), Some(mode));
        }
        assert_eq!(DockMode::from_saved("RIGHT"), DockMode::Right);
        assert_eq!(DockMode::from_saved(""), DockMode::Floating);
        assert_eq!(DockMode::from_saved("sideways"), DockMode::Floating);
        assert_eq!(DockMode::from_display_name("Sideways"), None);
    }

    const SCREEN: (f32, f32) = (800.0, 600.0);

    fn floated(at: Rect, pointer: (f32, f32)) -> DockMode {
        dragged_mode(DockMode::Floating, at, SCREEN, pointer, 0.0)
    }

    #[test]
    fn a_floating_dock_snaps_to_the_edge_nearest_the_pointer() {
        let tall = |x, y| Rect::new(x, y, 48.0, 100.0);
        assert_eq!(floated(tall(6.0, 200.0), (16.0, 210.0)), DockMode::Left);
        assert_eq!(floated(tall(746.0, 200.0), (756.0, 210.0)), DockMode::Right);
        assert_eq!(floated(tall(300.0, 5.0), (310.0, 10.0)), DockMode::Top);
        assert_eq!(
            floated(tall(300.0, 496.0), (310.0, 590.0)),
            DockMode::Bottom
        );
        assert_eq!(
            floated(tall(300.0, 200.0), (310.0, 210.0)),
            DockMode::Floating
        );
    }

    #[test]
    fn a_dock_touching_an_edge_far_from_the_pointer_does_not_snap_there() {
        // A tall dock carried across the screen: its bottom is on the edge,
        // the pointer on its handle near the top.
        let at = Rect::new(300.0, 100.0, 48.0, 496.0);
        assert_eq!(floated(at, (310.0, 110.0)), DockMode::Floating);
        // Carried on down, past where it can go, the pointer comes near.
        assert_eq!(floated(at, (310.0, 590.0)), DockMode::Bottom);
    }

    #[test]
    fn a_dock_as_long_as_the_screen_goes_to_the_end_the_pointer_is_near() {
        let at = Rect::new(300.0, 0.0, 48.0, 600.0);
        assert_eq!(floated(at, (310.0, 5.0)), DockMode::Top);
        assert_eq!(floated(at, (310.0, 595.0)), DockMode::Bottom);
        // Nearer a side it does not touch, it carries on floating.
        assert_eq!(floated(at, (100.0, 300.0)), DockMode::Floating);
    }

    #[test]
    fn in_a_corner_the_pointers_edge_wins() {
        let at = Rect::new(2.0, 2.0, 48.0, 100.0);
        assert_eq!(floated(at, (5.0, 9.0)), DockMode::Left);
        assert_eq!(floated(at, (9.0, 5.0)), DockMode::Top);
    }

    #[test]
    fn a_horizontal_floating_dock_snaps_like_a_vertical_one() {
        let at = Rect::new(200.0, 496.0, 200.0, 100.0);
        assert_eq!(
            dragged_mode(
                DockMode::FloatingHorizontal,
                at,
                SCREEN,
                (205.0, 590.0),
                0.0
            ),
            DockMode::Bottom
        );
    }

    #[test]
    fn a_rail_stays_until_the_pointer_is_pulled_away_and_floats_running_as_it_ran() {
        let at = Rect::new(0.0, 0.0, 46.0, 46.0);
        let rail = |mode, pointer| dragged_mode(mode, at, SCREEN, pointer, 10.0);
        assert_eq!(rail(DockMode::Left, (42.0, 300.0)), DockMode::Left);
        assert_eq!(rail(DockMode::Left, (43.0, 300.0)), DockMode::Floating);
        assert_eq!(rail(DockMode::Right, (758.0, 300.0)), DockMode::Right);
        assert_eq!(rail(DockMode::Right, (757.0, 300.0)), DockMode::Floating);
        assert_eq!(rail(DockMode::Top, (400.0, 42.0)), DockMode::Top);
        assert_eq!(
            rail(DockMode::Top, (400.0, 43.0)),
            DockMode::FloatingHorizontal
        );
        assert_eq!(rail(DockMode::Bottom, (400.0, 558.0)), DockMode::Bottom);
        assert_eq!(
            rail(DockMode::Bottom, (400.0, 557.0)),
            DockMode::FloatingHorizontal
        );
    }

    #[test]
    fn a_rail_goes_by_the_pointer_so_one_snapped_by_its_far_end_stays() {
        // Snapped to the bottom with the pointer 138 above the edge: it holds
        // until the pointer is 32 further away still.
        let at = Rect::new(300.0, 554.0, 200.0, 46.0);
        let rail = |y| dragged_mode(DockMode::Bottom, at, SCREEN, (310.0, y), 138.0);
        assert_eq!(rail(450.0), DockMode::Bottom);
        assert_eq!(rail(429.0), DockMode::FloatingHorizontal);
    }

    #[test]
    fn room_comes_from_the_screen_and_a_floating_dock_keeps_clear_of_its_ends() {
        let (w, h) = (1280.0, 720.0);
        assert_eq!(max_length(DockMode::Left, w, h), 720.0);
        assert_eq!(max_length(DockMode::Bottom, w, h), 1280.0);
        assert_eq!(max_length(DockMode::Floating, w, h), 672.0);
        assert_eq!(max_length(DockMode::FloatingHorizontal, w, h), 1232.0);
    }
}
