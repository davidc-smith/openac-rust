//! The plugin dock's shapes, in interface points. Every theme draws the dock
//! through here; Classic passes [`Palette::CLASSIC_DOCK`], so this is the one
//! place Classic takes the rounded look.

use std::sync::Arc;

use super::layout::DockMode;
use crate::draw::{Color, DEFAULT_OUTLINE_COLOR, RenderContext};
use crate::font::UiFont;
use crate::geometry::Rect;
use crate::plugin::style::{hover, mix, pressed, surface, with_alpha};
use crate::plugin::widgets::{ButtonLook, Palette};
use crate::shapes::CornerRadii;

/// A floating dock's corner radius.
pub const RADIUS: f32 = 14.0;
/// A rail's inner corner radius.
pub const RAIL_RADIUS: f32 = 12.0;
/// How far a divider sits in from the dock's sides.
pub const DIVIDER_INSET: f32 = 8.0;
/// How far a divider's colour moves from the border toward the fill.
pub const DIVIDER_FADE: f32 = 0.6;
/// A slot's hover well's corner radius.
pub const SLOT_RADIUS: f32 = 9.0;
/// The dot beside an open window's slot, each way.
pub const OPEN_DOT: f32 = 3.0;
/// How far that dot sits in from the dock's edge.
pub const OPEN_DOT_INSET: f32 = 1.5;
/// A rail slot's well is inset this far across the rail, clear of the edge pill.
pub const RAIL_WELL_ACROSS: f32 = 5.0;
/// A rail slot's well is inset this far along the rail.
pub const RAIL_WELL_ALONG: f32 = 1.0;
/// An edge pill's thickness.
pub const PILL_THICKNESS: f32 = 3.0;
/// An edge pill's length under the pointer.
pub const PILL_HOVER: f32 = 4.0;
/// An edge pill's length while its window is open.
pub const PILL_OPEN: f32 = 8.0;
/// An edge pill's length beside the front window.
pub const PILL_FRONT: f32 = 20.0;
/// How long an edge pill takes to grow to the front window's length.
pub const PILL_SECONDS: f32 = 0.12;
/// Between the dock and a hover label.
pub const LABEL_GAP: f32 = 8.0;
/// A hover label's padding along its text.
pub const LABEL_PAD_X: f32 = 9.0;
/// A hover label's padding above and below its text.
pub const LABEL_PAD_Y: f32 = 5.0;
/// A hover label's corner radius.
pub const LABEL_RADIUS: f32 = 7.0;
/// A monogram's corner radius.
pub const MONOGRAM_RADIUS: f32 = 6.0;
/// Colour art's alpha while its window is closed.
pub const CLOSED_ART_ALPHA: f32 = 0.85;

const fn rgb(packed: u32) -> Color {
    Color::rgba(
        ((packed >> 16) & 0xFF) as f32 / 255.0,
        ((packed >> 8) & 0xFF) as f32 / 255.0,
        (packed & 0xFF) as f32 / 255.0,
        1.0,
    )
}

const MONOGRAM_INK: Color = rgb(0xF1_ECE2);
const MONOGRAM_HUES: [Color; 8] = [
    rgb(0x6E_8B5E),
    rgb(0x5E_7F8B),
    rgb(0x8B_6E5E),
    rgb(0x7A_6A99),
    rgb(0x8B_8259),
    rgb(0x5E_6E8B),
    rgb(0x8B_5E74),
    rgb(0x5E_8B7A),
];

/// The screen edge a piece of the dock faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DockSide {
    /// The left edge.
    #[default]
    Left,
    /// The right edge.
    Right,
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
}

/// Which way a chevron points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Up.
    Up,
    /// Down.
    Down,
    /// Left.
    Left,
    /// Right.
    Right,
}

/// The dock's corners: all four when floating, only the two inner ones on a
/// rail.
#[must_use]
pub const fn corners(mode: DockMode) -> CornerRadii {
    let r = RAIL_RADIUS;
    match mode {
        DockMode::Floating | DockMode::FloatingHorizontal => CornerRadii::all(RADIUS),
        DockMode::Left => CornerRadii::new(0.0, r, r, 0.0),
        DockMode::Right => CornerRadii::new(r, 0.0, 0.0, r),
        DockMode::Top => CornerRadii::new(0.0, 0.0, r, r),
        DockMode::Bottom => CornerRadii::new(r, r, 0.0, 0.0),
    }
}

/// The dock's body: shadow, fill, a faint sheen along its top and a rim, which
/// a rail leaves off its screen-edge side.
pub fn body(rc: &mut RenderContext<'_>, p: &Palette, mode: DockMode, w: f32, h: f32) {
    let rail = mode.is_rail();
    let radius = if rail { RAIL_RADIUS } else { RADIUS };
    let shadow = with_alpha(Color::BLACK, 0.42);
    rc.soft_shadow(
        0.0,
        0.0,
        w,
        h,
        radius,
        if rail { 2.0 } else { 4.0 },
        14.0,
        shadow,
    );
    let radii = corners(mode);
    rc.fill_rounded_rect(0.0, 0.0, w, h, radii, p.background);
    let sheen = with_alpha(p.text, 0.04);
    rc.fill_vertical_gradient(
        0.0,
        0.0,
        w,
        h.min(12.0),
        CornerRadii::new(radii.top_left, radii.top_right, 0.0, 0.0),
        sheen,
        with_alpha(sheen, 0.0),
    );
    // A rail's rim runs off the screen on its edge side.
    let (x, y, width, height) = match mode {
        DockMode::Left => (-radius, 0.0, w + radius, h),
        DockMode::Right => (0.0, 0.0, w + radius, h),
        DockMode::Top => (0.0, -radius, w, h + radius),
        DockMode::Bottom => (0.0, 0.0, w, h + radius),
        DockMode::Floating | DockMode::FloatingHorizontal => (0.0, 0.0, w, h),
    };
    rc.stroke_rounded_rect(
        x + 0.5,
        y + 0.5,
        width - 1.0,
        height - 1.0,
        CornerRadii::all(radius - 0.5),
        mix(p.border, p.text, 0.08),
        1.0,
    );
}

/// A slot's hover well in the slot's space: the whole slot when floating, on
/// a rail inset clear of the edge pill.
#[must_use]
pub const fn slot_well_rect(mode: DockMode, w: f32, h: f32) -> Rect {
    let (ax, ay) = match mode {
        DockMode::Floating | DockMode::FloatingHorizontal => (0.0, 0.0),
        DockMode::Left | DockMode::Right => (RAIL_WELL_ACROSS, RAIL_WELL_ALONG),
        DockMode::Top | DockMode::Bottom => (RAIL_WELL_ALONG, RAIL_WELL_ACROSS),
    };
    Rect::new(ax, ay, w - 2.0 * ax, h - 2.0 * ay)
}

/// The well behind a slot while it is under the pointer or held; nothing at
/// rest.
pub fn slot_well(rc: &mut RenderContext<'_>, p: &Palette, r: Rect, look: ButtonLook) {
    let face = match look {
        ButtonLook::Hovered => hover(p, p.field),
        ButtonLook::Pressed => pressed(hover(p, p.field)),
        ButtonLook::Normal | ButtonLook::Disabled => return,
    };
    rc.fill_rounded_rect(r.x, r.y, r.w, r.h, CornerRadii::all(SLOT_RADIUS), face);
}

/// The small accent dot beside an open window's slot on a floating dock, on
/// the side facing `side`.
pub fn open_dot(
    rc: &mut RenderContext<'_>,
    p: &Palette,
    side: DockSide,
    (w, h): (f32, f32),
    slot: Rect,
) {
    let (x, y) = match side {
        DockSide::Left => (OPEN_DOT_INSET, slot.y + (slot.h - OPEN_DOT) / 2.0),
        DockSide::Right => (
            w - OPEN_DOT_INSET - OPEN_DOT,
            slot.y + (slot.h - OPEN_DOT) / 2.0,
        ),
        DockSide::Top => (slot.x + (slot.w - OPEN_DOT) / 2.0, OPEN_DOT_INSET),
        DockSide::Bottom => (
            slot.x + (slot.w - OPEN_DOT) / 2.0,
            h - OPEN_DOT_INSET - OPEN_DOT,
        ),
    };
    rc.fill_ellipse(x, y, OPEN_DOT, OPEN_DOT, p.accent);
}

/// The pill on a rail's screen edge beside a slot, `length` long along the
/// rail.
pub fn edge_pill(
    rc: &mut RenderContext<'_>,
    color: Color,
    side: DockSide,
    (w, h): (f32, f32),
    slot: Rect,
    length: f32,
) {
    if length <= 0.0 {
        return;
    }
    let r = PILL_THICKNESS;
    let along_x = slot.x + (slot.w - length) / 2.0;
    let along_y = slot.y + (slot.h - length) / 2.0;
    let (x, y, pw, ph, radii) = match side {
        DockSide::Left => (0.0, along_y, r, length, CornerRadii::new(0.0, r, r, 0.0)),
        DockSide::Right => (w - r, along_y, r, length, CornerRadii::new(r, 0.0, 0.0, r)),
        DockSide::Top => (along_x, 0.0, length, r, CornerRadii::new(0.0, 0.0, r, r)),
        DockSide::Bottom => (along_x, h - r, length, r, CornerRadii::new(r, r, 0.0, 0.0)),
    };
    rc.fill_rounded_rect(x, y, pw, ph, radii, color);
}

/// A faint hairline between two plugins' slots, `at` along the dock: the
/// border colour faded toward the fill.
pub fn divider(
    rc: &mut RenderContext<'_>,
    p: &Palette,
    mode: DockMode,
    (w, h): (f32, f32),
    at: f32,
) {
    let color = mix(p.border, p.background, DIVIDER_FADE);
    if mode.is_horizontal() {
        rc.fill(at, DIVIDER_INSET, 1.0, h - 2.0 * DIVIDER_INSET, color);
    } else {
        rc.fill(DIVIDER_INSET, at, w - 2.0 * DIVIDER_INSET, 1.0, color);
    }
}

/// A fade into the dock's fill over the slots cut off, `at` along the dock,
/// running to the fill at its far end when `to_end`, else at its near end.
pub fn fade(
    rc: &mut RenderContext<'_>,
    p: &Palette,
    mode: DockMode,
    (w, h): (f32, f32),
    at: f32,
    length: f32,
    to_end: bool,
) {
    let solid = p.background;
    let clear = with_alpha(solid, 0.0);
    let (start, end) = if to_end {
        (clear, solid)
    } else {
        (solid, clear)
    };
    let none = CornerRadii::all(0.0);
    if mode.is_horizontal() {
        rc.fill_horizontal_gradient(at, 0.0, length, h, none, start, end);
    } else {
        rc.fill_vertical_gradient(0.0, at, w, length, none, start, end);
    }
}

/// Three small dots, the dock's move handle, centred in `r` and running along
/// its longer side.
pub fn handle_dots(rc: &mut RenderContext<'_>, p: &Palette, r: Rect) {
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    for i in [-1.0, 0.0, 1.0] {
        let (x, y) = if r.w >= r.h {
            (cx + i * 5.0, cy)
        } else {
            (cx, cy + i * 5.0)
        };
        rc.fill_ellipse(x - 1.0, y - 1.0, 2.0, 2.0, p.muted);
    }
}

/// A two-stroke chevron centred at (`cx`, `cy`), `size` across.
pub fn chevron(
    rc: &mut RenderContext<'_>,
    cx: f32,
    cy: f32,
    size: f32,
    direction: Direction,
    color: Color,
) {
    let (a, b) = (size / 2.0, size / 4.0);
    let (p0, tip, p1) = match direction {
        Direction::Up => ((-a, b), (0.0, -b), (a, b)),
        Direction::Down => ((-a, -b), (0.0, b), (a, -b)),
        Direction::Left => ((b, -a), (-b, 0.0), (b, a)),
        Direction::Right => ((-b, -a), (b, 0.0), (-b, a)),
    };
    rc.smooth_line(cx + p0.0, cy + p0.1, cx + tip.0, cy + tip.1, color, 1.5);
    rc.smooth_line(cx + tip.0, cy + tip.1, cx + p1.0, cy + p1.1, color, 1.5);
}

/// The gear on the dock's appearance slot, filling a `size` box at (`x`, `y`).
pub fn gear(rc: &mut RenderContext<'_>, x: f32, y: f32, size: f32, color: Color) {
    let c = size / 2.0;
    let ring = size * 0.34;
    rc.stroke_ellipse(
        x + c - ring,
        y + c - ring,
        ring * 2.0,
        ring * 2.0,
        color,
        1.75,
    );
    for i in 0..8 {
        let (dy, dx) = (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
        rc.smooth_line(
            x + c + dx * ring,
            y + c + dy * ring,
            x + c + dx * size * 0.46,
            y + c + dy * size * 0.46,
            color,
            2.0,
        );
    }
}

/// The colour behind a plugin's monogram: one of eight fixed hues, picked by
/// FNV-1a of the plugin's id, so a plugin keeps its colour between sessions.
#[must_use]
pub fn monogram_hue(plugin_id: &str) -> Color {
    let hash = plugin_id.bytes().fold(2_166_136_261_u32, |hash, b| {
        (hash ^ u32::from(b)).wrapping_mul(16_777_619)
    });
    MONOGRAM_HUES[hash as usize % MONOGRAM_HUES.len()]
}

/// A plugin's letters on its hue, filling the `size` box at (`x`, `y`).
#[allow(clippy::too_many_arguments)]
pub fn monogram(
    rc: &mut RenderContext<'_>,
    font: Option<&Arc<UiFont>>,
    letters: &str,
    hue: Color,
    x: f32,
    y: f32,
    size: f32,
    alpha: f32,
) {
    let top = mix(hue, Color::WHITE, 0.12);
    rc.fill_vertical_gradient(
        x,
        y,
        size,
        size,
        CornerRadii::all(MONOGRAM_RADIUS),
        with_alpha(top, alpha),
        with_alpha(hue, alpha),
    );
    if let Some(font) = font
        && !letters.is_empty()
    {
        let tx = x + (size - font.measure_width(letters)) / 2.0;
        let ty = y + (size - font.line_height) / 2.0;
        let ink = with_alpha(MONOGRAM_INK, alpha);
        rc.string_dat(font, letters, tx, ty, ink, false, DEFAULT_OUTLINE_COLOR);
    }
}

/// A hover label's box: fill, edge and shadow, at `r` in the dock's space.
pub fn hover_label(rc: &mut RenderContext<'_>, p: &Palette, r: Rect) {
    let shadow = with_alpha(Color::BLACK, 0.35);
    rc.soft_shadow(r.x, r.y, r.w, r.h, LABEL_RADIUS, 3.0, 8.0, shadow);
    surface(rc, r.x, r.y, r.w, r.h, LABEL_RADIUS, p.background, p.border);
}
