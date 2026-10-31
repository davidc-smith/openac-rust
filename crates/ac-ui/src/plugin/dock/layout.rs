//! Where everything in the plugin dock goes, worked out from its mode,
//! whether it is minimized, which plugin owns each slot and how long it may
//! be. It is pure: the dock applies the result to its children.
//!
//! It works along the dock's main axis, the way its slots run, and across
//! it, so a horizontal dock is the vertical one turned on its side. Along the
//! main axis: the handle row, the slots (a divider between two plugins, a gap
//! between one plugin's windows), a divider, the gear slot and padding. When
//! the slots do not fit, only those that do from the first one are laid out;
//! the handle row and the gear stay.

use crate::geometry::Rect;

/// The padding round a floating dock's slots.
pub const PADDING: f32 = 6.0;
/// A floating dock's slot, each way.
pub const SLOT: f32 = 36.0;
/// Between a floating dock's slots of one plugin.
pub const GAP: f32 = 4.0;
/// A slot's art, each way.
pub const ART: f32 = 24.0;
/// A floating dock's thickness across its main axis.
pub const FLOATING_THICKNESS: f32 = PADDING + SLOT + PADDING;
/// Where the row with the handle and the minimize button starts its contents.
pub const HANDLE_START: f32 = 2.0;
/// How long those contents are along the main axis.
pub const HANDLE_LENGTH: f32 = 12.0;
/// Where the first slot starts along the main axis.
pub const SLOTS_START: f32 = HANDLE_START + HANDLE_LENGTH + 2.0;
/// The handle's extent across the main axis.
pub const HANDLE_ACROSS: f32 = 30.0;
/// The minimize button, each way.
pub const TOGGLE_SIZE: f32 = 12.0;
/// The space each side of a divider.
pub const DIVIDER_SPACE: f32 = 4.0;
/// How long a fade over cut-off slots is.
pub const FADE_LENGTH: f32 = 12.0;
/// A minimized floating dock's length along its main axis.
pub const PILL_LENGTH: f32 = 24.0;
/// A rail's thickness across its main axis.
pub const RAIL_THICKNESS: f32 = 46.0;
/// A rail's slot along its main axis.
pub const RAIL_SLOT: f32 = 38.0;
/// Between a rail's slots of one plugin.
pub const RAIL_GAP: f32 = 2.0;
/// A minimized rail's thickness.
pub const TAB_THICKNESS: f32 = 12.0;
/// A minimized rail's length along its edge.
pub const TAB_LENGTH: f32 = 44.0;
/// The part of a minimized rail that moves it.
pub const TAB_HANDLE: f32 = 20.0;
/// How far a floating dock's longest stays from each end of the screen, so
/// that one which fills the screen is not touching an edge wherever it is
/// dragged.
pub const FLOATING_MARGIN: f32 = 24.0;

const DIVIDER_BLOCK: f32 = DIVIDER_SPACE * 2.0 + 1.0;

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
    /// The gear's place; empty on a minimized dock.
    pub gear: Rect,
    /// The part that moves the dock.
    pub handle: Rect,
    /// The minimize button.
    pub toggle: Rect,
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

/// The layout of a dock in `mode`, minimized or not, with one slot per entry
/// of `owners` (each slot's plugin), at most `max_length` long, scrolled to
/// start at slot `first`.
#[must_use]
pub fn compute(
    mode: DockMode,
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

    if minimized {
        let (cross, main, handle, toggle) = if rail {
            (
                TAB_THICKNESS,
                TAB_LENGTH,
                axes.rect(0.0, 0.0, TAB_HANDLE, TAB_THICKNESS),
                axes.rect(TAB_HANDLE, 0.0, TAB_LENGTH - TAB_HANDLE, TAB_THICKNESS),
            )
        } else {
            (
                FLOATING_THICKNESS,
                PILL_LENGTH,
                axes.rect(0.0, 0.0, PILL_LENGTH, HANDLE_ACROSS),
                axes.rect(0.0, HANDLE_ACROSS, PILL_LENGTH, TOGGLE_SIZE),
            )
        };
        let (width, height) = axes.size(main, cross);
        return DockLayout {
            width,
            height,
            slots,
            dividers: Vec::new(),
            gear: Rect::new(0.0, 0.0, 0.0, 0.0),
            handle,
            toggle,
            first: 0,
            visible: 0,
            max_first: 0,
            fade_start: false,
            fade_end: false,
            list_start: 0.0,
            list_end: 0.0,
        };
    }

    let (cross, slot_cross_at, slot_across, slot_along, gap) = if rail {
        (RAIL_THICKNESS, 0.0, RAIL_THICKNESS, RAIL_SLOT, RAIL_GAP)
    } else {
        (FLOATING_THICKNESS, PADDING, SLOT, SLOT, GAP)
    };
    let spacing = |i: usize| {
        if owners[i] == owners[i - 1] {
            gap
        } else {
            DIVIDER_BLOCK
        }
    };
    // Everything that is not the slot list: the handle row before it, and the
    // divider, gear and padding after it.
    let divider = if count > 0 { DIVIDER_BLOCK } else { 0.0 };
    let room = max_length - (SLOTS_START + divider + slot_along + PADDING);

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

    let mut dividers = Vec::new();
    let mut at = SLOTS_START;
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
    let (width, height) = axes.size(at + slot_along + PADDING, cross);
    DockLayout {
        width,
        height,
        slots,
        dividers,
        gear,
        handle: axes.rect(0.0, 0.0, SLOTS_START, HANDLE_ACROSS),
        toggle: axes.rect(
            HANDLE_START,
            cross - PADDING - TOGGLE_SIZE,
            TOGGLE_SIZE,
            TOGGLE_SIZE,
        ),
        first,
        visible,
        max_first,
        fade_start: first > 0,
        fade_end: first + visible < count,
        list_start: SLOTS_START,
        list_end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn one_slot_is_the_handle_row_a_slot_a_divider_the_gear_and_padding() {
        let l = floating(&["a"], f32::INFINITY, 0);
        assert_eq!(l.width, 48.0);
        // 16 (handle row) + 36 (slot) + 9 (divider) + 36 (gear) + 6 (padding)
        assert_eq!(l.height, 103.0);
        assert_eq!(l.slots[0], Some(Rect::new(6.0, 16.0, 36.0, 36.0)));
        assert_eq!(l.dividers, [56.5]);
        assert_eq!(l.gear, Rect::new(6.0, 61.0, 36.0, 36.0));
        assert_eq!(l.handle, Rect::new(0.0, 0.0, 30.0, 16.0));
        assert_eq!(l.toggle, Rect::new(30.0, 2.0, 12.0, 12.0));
    }

    #[test]
    fn slots_of_one_plugin_have_a_gap_and_another_plugin_a_divider() {
        let l = floating(&["moss", "moss", "goarrow"], f32::INFINITY, 0);
        let main = |i: usize| l.slots[i].unwrap().y;
        assert_eq!(main(0), 16.0);
        assert_eq!(main(1), 56.0); // 16 + 36 + 4
        assert_eq!(main(2), 101.0); // 56 + 36 + 9
        assert_eq!(l.dividers, [96.5, 141.5]);
        assert_eq!(l.gear.y, 146.0);
        assert_eq!(l.height, 188.0);
    }

    #[test]
    fn overflow_shows_what_fits_and_scrolling_stops_at_the_last_full_page() {
        let owners = twelve();
        let owners = refs(&owners);
        // Room for 16 + 3 slots (36 + 9 + 36 + 9 + 36) + 9 + 36 + 6 = 193.
        let top = floating(&owners, 200.0, 0);
        assert_eq!((top.visible, top.first, top.max_first), (3, 0, 9));
        assert!(!top.fade_start && top.fade_end);
        assert!(top.height <= 200.0);
        assert!(top.slots[2].is_some());
        assert!(top.slots[3].is_none());

        let end = floating(&owners, 200.0, 50);
        assert_eq!(end.first, 9);
        assert!(end.fade_start && !end.fade_end);
        assert!(end.slots[8].is_none());
        assert_eq!(end.slots[9].unwrap().y, 16.0);
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
            let free = floating(&["a", "a"], f32::INFINITY, 0);
            assert_eq!(rail.width, 46.0);
            assert_eq!(rail.slots[0], Some(Rect::new(0.0, 16.0, 46.0, 38.0)));
            assert_eq!(rail.slots[1].unwrap().y, 56.0); // 16 + 38 + 2
            // The pitch is 40 in both; the last slot and the gear are 2 taller.
            assert_eq!(rail.height, free.height + 4.0);
            assert_eq!(rail.toggle, Rect::new(28.0, 2.0, 12.0, 12.0));
        }
    }

    #[test]
    fn minimized_floating_is_a_pill_and_a_vertical_rail_a_tab() {
        let pill = compute(DockMode::Floating, true, &["a"], 500.0, 0);
        assert_eq!((pill.width, pill.height), (48.0, 24.0));
        assert_eq!(pill.handle, Rect::new(0.0, 0.0, 30.0, 24.0));
        assert_eq!(pill.toggle, Rect::new(30.0, 0.0, 12.0, 24.0));
        assert!(pill.slots.iter().all(Option::is_none));
        assert!(pill.dividers.is_empty());

        for mode in [DockMode::Left, DockMode::Right] {
            let tab = compute(mode, true, &["a"], 500.0, 0);
            assert_eq!((tab.width, tab.height), (12.0, 44.0));
            assert_eq!(tab.handle, Rect::new(0.0, 0.0, 12.0, 20.0));
            assert_eq!(tab.toggle, Rect::new(0.0, 20.0, 12.0, 24.0));
        }
    }

    #[test]
    fn no_entries_have_no_divider_before_the_gear() {
        let l = floating(&[], f32::INFINITY, 0);
        assert!(l.dividers.is_empty());
        assert_eq!(l.gear.y, 16.0);
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
                let v = compute(vertical, minimized, &owners, 200.0, 0);
                let h = compute(horizontal, minimized, &owners, 200.0, 0);
                assert_eq!((h.width, h.height), (v.height, v.width));
                assert_eq!(h.handle, turned(v.handle));
                assert_eq!(h.toggle, turned(v.toggle));
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
        assert_eq!((l.width, l.height), (103.0, 48.0));
        assert_eq!(l.slots[0], Some(Rect::new(16.0, 6.0, 36.0, 36.0)));
        assert_eq!(l.gear, Rect::new(61.0, 6.0, 36.0, 36.0));
        assert_eq!(l.handle, Rect::new(0.0, 0.0, 16.0, 30.0));
        assert_eq!(l.toggle, Rect::new(2.0, 30.0, 12.0, 12.0));
    }

    #[test]
    fn minimized_sizes_turn_with_the_dock() {
        let pill = compute(DockMode::FloatingHorizontal, true, &["a"], 500.0, 0);
        assert_eq!((pill.width, pill.height), (24.0, 48.0));
        for mode in [DockMode::Top, DockMode::Bottom] {
            let tab = compute(mode, true, &["a"], 500.0, 0);
            assert_eq!((tab.width, tab.height), (44.0, 12.0));
            assert_eq!(tab.handle, Rect::new(0.0, 0.0, 20.0, 12.0));
            assert_eq!(tab.toggle, Rect::new(20.0, 0.0, 24.0, 12.0));
        }
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
        assert_eq!(l.slots[2].unwrap().x, SLOTS_START);
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
