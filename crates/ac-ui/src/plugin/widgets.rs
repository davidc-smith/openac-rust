//! The markup widgets OpenAC's panel markup builds that the retail widgets
//! do not already cover: text, push buttons, tab headers, check toggles,
//! icons, the row list (single column or columns) and the log, and the
//! panel frame (retail chrome or the plugin theme). Their behaviour and
//! default colours are OpenAC's (`UiLabel`, `UiSimpleButton`,
//! `UiMarkupTabButton`, `UiMarkupToggle`, `UiMarkupIcon`, `UiMarkupList`,
//! `UiMarkupLog`, `UiCheckLamp`, `UiPluginMarkupPanel`).

use std::sync::Arc;

use ac_plugin_ui::list::{self, ColumnPress};
use ac_plugin_ui::markup::{Column, ColumnKind};

use crate::draw::{Color, DEFAULT_OUTLINE_COLOR, RenderContext, TextureRef};
use crate::event::{Key, UiEvent, kind};
use crate::font::UiFont;
use crate::root::{ElementMessage, UiRoot};
use crate::text::Scrollable;
use crate::tree::{Element, ElementId, Widget};
use crate::widgets::NineSliceFrame;
use crate::widgets::scrollbar::thumb_rect;

/// A resolved image: its texture and its size in pixels.
pub type Icon = (TextureRef, f32, f32);

/// The plugin theme's colours (OpenAC `PluginUiPalette`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// The window's fill.
    pub background: Color,
    /// Fields, lists and buttons.
    pub field: Color,
    /// Borders.
    pub border: Color,
    /// Text.
    pub text: Color,
    /// Quiet text and marks.
    pub muted: Color,
    /// Highlights.
    pub accent: Color,
    /// Selection.
    pub selected: Color,
}

const fn rgb(packed: u32) -> Color {
    Color::rgba(
        ((packed >> 16) & 0xFF) as f32 / 255.0,
        ((packed >> 8) & 0xFF) as f32 / 255.0,
        (packed & 0xFF) as f32 / 255.0,
        1.0,
    )
}

impl Palette {
    /// "Charcoal + moss".
    pub const MOSS: Self = Self {
        background: rgb(0x17_1D1B),
        field: rgb(0x10_1613),
        border: rgb(0x35_443B),
        text: rgb(0xE0_E8E2),
        muted: rgb(0x9A_A99E),
        accent: rgb(0x97_BE81),
        selected: rgb(0x34_4B37),
    };
    /// "Warm graphite + brass".
    pub const BRASS: Self = Self {
        background: rgb(0x22_1F1B),
        field: rgb(0x18_1612),
        border: rgb(0x4C_4335),
        text: rgb(0xE9_E2D5),
        muted: rgb(0xB2_A58E),
        accent: rgb(0xC9_A665),
        selected: rgb(0x51_442D),
    };

    /// A theme colour by its markup name.
    #[must_use]
    pub fn token(&self, name: &str) -> Option<Color> {
        Some(match name {
            "text" => self.text,
            "muted" => self.muted,
            "field" => self.field,
            "border" => self.border,
            "accent" => self.accent,
            "background" => self.background,
            _ => return None,
        })
    }
}

/// The check box OpenAC draws (`UiCheckLamp`, 11 pixels square), or the
/// theme's flat box.
pub fn draw_check(
    rc: &mut RenderContext<'_>,
    x: f32,
    y: f32,
    checked: bool,
    palette: Option<&Palette>,
) {
    if let Some(p) = palette {
        rc.fill(x, y, 11.0, 11.0, if checked { p.selected } else { p.field });
        rc.rect_outline(
            x,
            y,
            11.0,
            11.0,
            if checked { p.accent } else { p.border },
            1.0,
        );
        if checked {
            rc.fill(x + 3.0, y + 3.0, 5.0, 5.0, p.accent);
        }
        return;
    }
    let (outer, inner) = if checked {
        (
            Color::rgba(0.36, 0.58, 0.12, 1.0),
            Color::rgba(0.52, 1.0, 0.08, 1.0),
        )
    } else {
        (
            Color::rgba(0.26, 0.22, 0.13, 1.0),
            Color::rgba(0.38, 0.34, 0.23, 1.0),
        )
    };
    rc.fill(x + 3.0, y, 5.0, 1.0, outer);
    rc.fill(x + 1.0, y + 1.0, 9.0, 2.0, outer);
    rc.fill(x, y + 3.0, 11.0, 5.0, outer);
    rc.fill(x + 1.0, y + 8.0, 9.0, 2.0, outer);
    rc.fill(x + 3.0, y + 10.0, 5.0, 1.0, outer);
    rc.fill(x + 3.0, y + 3.0, 5.0, 5.0, inner);
}

/// The check box's side, pixels.
pub const CHECK_SIZE: f32 = 11.0;

/// The alpha a disabled toggle's caption draws at; a disabled list's
/// buttons fade by it too.
pub const DISABLED_ALPHA: f32 = 0.42;

fn text(
    rc: &mut RenderContext<'_>,
    font: Option<&Arc<UiFont>>,
    s: &str,
    x: f32,
    y: f32,
    color: Color,
    outline: bool,
) {
    if let Some(font) = font {
        rc.string_dat(font, s, x, y, color, outline, DEFAULT_OUTLINE_COLOR);
    }
}

fn line_height(font: Option<&Arc<UiFont>>) -> f32 {
    font.map_or(14.0, |f| f.line_height)
}

fn measure(font: Option<&Arc<UiFont>>, s: &str) -> f32 {
    font.map_or(s.chars().count() as f32 * 7.0, |f| f.measure_width(s))
}

/// The alpha an unselected tab draws its icon at, so it recedes like its
/// caption.
pub const INACTIVE_TAB_ICON_ALPHA: f32 = 0.55;

fn draw_icon(rc: &mut RenderContext<'_>, icon: Icon, x: f32, y: f32, w: f32, h: f32) {
    draw_icon_tinted(rc, icon, (x, y, w, h), Color::WHITE);
}

/// An icon fitted into a rectangle, drawn in `tint` (white: as authored).
fn draw_icon_tinted(
    rc: &mut RenderContext<'_>,
    icon: Icon,
    rect: (f32, f32, f32, f32),
    tint: Color,
) {
    let (x, y, w, h) = rect;
    let (texture, iw, ih) = icon;
    if iw <= 0.0 || ih <= 0.0 || w <= 0.0 || h <= 0.0 {
        return;
    }
    let scale = (w / iw).min(h / ih);
    let (dw, dh) = (iw * scale, ih * scale);
    rc.sprite(
        texture,
        x + (w - dw) * 0.5,
        y + (h - dh) * 0.5,
        dw,
        dh,
        (0.0, 0.0, 1.0, 1.0),
        tint,
    );
}

fn size(ui: &UiRoot, me: ElementId) -> (f32, f32) {
    ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height))
}

fn enabled(ui: &UiRoot, me: ElementId) -> bool {
    ui.element(me).is_some_and(|e| e.enabled)
}

/// The outline a tab stop with keyboard focus draws.
const KEYBOARD_FOCUS_OUTLINE: Color = Color::rgba(1.0, 0.82, 0.25, 1.0);

/// Keyboard focus on a tab stop (OpenAC `UiSimpleButton`/`UiMarkupToggle`
/// with `TabStop`): gaining and losing it switch the outline, and Enter,
/// keypad Enter or Space activates an enabled control. `Some(activate)`
/// when the event was one of those, which the control takes.
fn tab_stop_event(ui: &UiRoot, me: ElementId, e: &UiEvent, focused: &mut bool) -> Option<bool> {
    if e.target != Some(me) || !ui.element(me).is_some_and(|el| el.tab_stop) {
        return None;
    }
    match e.kind {
        kind::FOCUS_GAINED => {
            *focused = true;
            Some(false)
        }
        kind::FOCUS_LOST => {
            *focused = false;
            Some(false)
        }
        kind::KEY_DOWN
            if enabled(ui, me) && matches!(e.key, Key::Enter | Key::KeypadEnter | Key::Space) =>
        {
            Some(true)
        }
        _ => None,
    }
}

/// A line of text that sizes itself to its text (OpenAC `UiLabel`;
/// click-through).
#[derive(Debug, Clone)]
pub struct MarkupLabel {
    /// The text.
    pub text: String,
    /// Its colour.
    pub color: Color,
    /// Outlined (classic panels).
    pub outline: bool,
    /// The interface font.
    pub font: Option<Arc<UiFont>>,
}

impl MarkupLabel {
    /// Empty white outlined text in `font`.
    #[must_use]
    pub fn new(font: Option<Arc<UiFont>>) -> Self {
        Self {
            text: String::new(),
            color: Color::WHITE,
            outline: true,
            font,
        }
    }
}

impl Widget for MarkupLabel {
    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let w = measure(self.font.as_ref(), &self.text);
        let h = line_height(self.font.as_ref());
        if let Some(e) = ui.element_mut(me) {
            e.width = w;
            e.height = h;
        }
        text(
            rc,
            self.font.as_ref(),
            &self.text,
            0.0,
            0.0,
            self.color,
            self.outline,
        );
    }

    fn clips_children(&self) -> bool {
        false
    }
}

/// A push button (OpenAC `UiSimpleButton`), or a tab header (OpenAC
/// `UiMarkupTabButton`): a click sends [`ElementMessage::Clicked`].
#[derive(Debug, Clone)]
pub struct MarkupButton {
    /// The caption.
    pub text: String,
    /// Caption colour.
    pub text_color: Color,
    /// Fill.
    pub background: Color,
    /// Border.
    pub border: Color,
    /// Outlined caption.
    pub outline: bool,
    /// The interface font.
    pub font: Option<Arc<UiFont>>,
    /// An icon left of the caption.
    pub icon: Option<Icon>,
    /// A tab header: whether it is the selected tab (`None`: a button).
    pub tab_selected: Option<bool>,
    /// The theme's colours on a themed panel.
    pub palette: Option<Palette>,
    /// It has keyboard focus as a tab stop.
    pub keyboard_focused: bool,
}

impl MarkupButton {
    /// A button with OpenAC's colours.
    #[must_use]
    pub fn new(font: Option<Arc<UiFont>>) -> Self {
        Self {
            text: String::new(),
            text_color: Color::WHITE,
            background: Color::rgba(0.1, 0.1, 0.15, 0.8),
            border: Color::rgba(0.45, 0.45, 0.55, 1.0),
            outline: true,
            font,
            icon: None,
            tab_selected: None,
            palette: None,
            keyboard_focused: false,
        }
    }

    /// A tab header: no fill or border, its caption coloured by state.
    #[must_use]
    pub fn tab(font: Option<Arc<UiFont>>) -> Self {
        Self {
            background: Color::TRANSPARENT,
            border: Color::TRANSPARENT,
            tab_selected: Some(false),
            ..Self::new(font)
        }
    }
}

impl Widget for MarkupButton {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        if let Some(activate) = tab_stop_event(ui, me, e, &mut self.keyboard_focused) {
            if activate {
                ui.send_message(me, ElementMessage::Clicked { x: 0, y: 0 });
            }
            return true;
        }
        if e.target != Some(me) || e.kind != kind::CLICK {
            return false;
        }
        if enabled(ui, me) {
            ui.send_message(
                me,
                ElementMessage::Clicked {
                    x: e.data1,
                    y: e.data2,
                },
            );
        }
        true
    }

    fn handles_click(&self, _element: &Element) -> bool {
        true
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = size(ui, me);
        if self.background.a > 0.0 {
            rc.fill(0.0, 0.0, w, h, self.background);
        }
        if self.border.a > 0.0 {
            rc.rect_outline(0.0, 0.0, w, h, self.border, 1.0);
        }
        if self.keyboard_focused {
            rc.rect_outline(1.0, 1.0, w - 2.0, h - 2.0, KEYBOARD_FOCUS_OUTLINE, 1.0);
        }
        let mut icon_column = 0.0;
        if let Some(icon) = self.icon {
            let extent = (w.min(h) - 6.0).max(0.0);
            icon_column = extent + 6.0;
            // An unselected tab's icon recedes with its caption.
            let tint = match self.tab_selected {
                Some(false) => Color::rgba(1.0, 1.0, 1.0, INACTIVE_TAB_ICON_ALPHA),
                _ => Color::WHITE,
            };
            draw_icon_tinted(rc, icon, (3.0, (h - extent) * 0.5, extent, extent), tint);
        }
        let color = match self.tab_selected {
            None => self.text_color,
            Some(_) if !enabled(ui, me) => Color::rgba(0.34, 0.33, 0.29, 1.0),
            Some(true) => self
                .palette
                .map_or(Color::rgba(0.94, 0.76, 0.18, 1.0), |p| p.accent),
            Some(false) => self
                .palette
                .map_or(Color::rgba(0.78, 0.76, 0.67, 1.0), |p| p.muted),
        };
        if !self.text.is_empty() {
            let tw = measure(self.font.as_ref(), &self.text);
            let area = (w - icon_column).max(0.0);
            let x = icon_column + (area - tw) * 0.5;
            let y = (h - line_height(self.font.as_ref())) * 0.5;
            text(
                rc,
                self.font.as_ref(),
                &self.text,
                x,
                y,
                color,
                self.outline,
            );
        }
        if self.tab_selected == Some(true) {
            let underline = self
                .palette
                .map_or(Color::rgba(0.77, 0.59, 0.12, 1.0), |p| p.accent);
            rc.fill(2.0, h - 2.0, (w - 4.0).max(0.0), 1.0, underline);
        }
    }
}

/// A check box with a caption (OpenAC `UiMarkupToggle`): a click sends
/// [`ElementMessage::Clicked`]; the plugin flips its bound state.
#[derive(Debug, Clone)]
pub struct MarkupToggle {
    /// The caption.
    pub text: String,
    /// Checked.
    pub checked: bool,
    /// Caption colour.
    pub color: Color,
    /// The interface font.
    pub font: Option<Arc<UiFont>>,
    /// The theme's colours on a themed panel.
    pub palette: Option<Palette>,
    /// It has keyboard focus as a tab stop.
    pub keyboard_focused: bool,
}

impl MarkupToggle {
    /// Unchecked, OpenAC's caption colour.
    #[must_use]
    pub fn new(font: Option<Arc<UiFont>>) -> Self {
        Self {
            text: String::new(),
            checked: false,
            color: Color::rgba(0.86, 0.84, 0.74, 1.0),
            font,
            palette: None,
            keyboard_focused: false,
        }
    }
}

impl Widget for MarkupToggle {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        if let Some(activate) = tab_stop_event(ui, me, e, &mut self.keyboard_focused) {
            if activate {
                ui.send_message(me, ElementMessage::Clicked { x: 0, y: 0 });
            }
            return true;
        }
        if e.target != Some(me) || e.kind != kind::CLICK {
            return false;
        }
        if enabled(ui, me) {
            ui.send_message(
                me,
                ElementMessage::Clicked {
                    x: e.data1,
                    y: e.data2,
                },
            );
        }
        true
    }

    fn handles_click(&self, _element: &Element) -> bool {
        true
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = size(ui, me);
        if self.keyboard_focused {
            rc.rect_outline(0.0, 0.0, w, h, KEYBOARD_FOCUS_OUTLINE, 1.0);
        }
        let check_y = ((h - CHECK_SIZE) * 0.5).max(1.0);
        draw_check(rc, 1.0, check_y, self.checked, self.palette.as_ref());
        let mut color = self.color;
        if !enabled(ui, me) {
            color.a = DISABLED_ALPHA;
        }
        let y = (h - line_height(self.font.as_ref())) * 0.5;
        text(
            rc,
            self.font.as_ref(),
            &self.text,
            17.0,
            y,
            color,
            self.palette.is_none(),
        );
    }
}

/// An image fitted into the element, centred (OpenAC `UiMarkupIcon`).
#[derive(Debug, Clone, Default)]
pub struct MarkupIcon {
    /// What it shows.
    pub icon: Option<Icon>,
}

impl Widget for MarkupIcon {
    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = size(ui, me);
        if let Some(icon) = self.icon {
            draw_icon(rc, icon, 0.0, 0.0, w, h);
        }
    }
}

/// The panel's frame: the retail nine-piece chrome, or on a themed panel
/// the theme's fill and border (OpenAC `UiNineSlicePanel`,
/// `UiPluginMarkupPanel`).
#[derive(Debug, Clone, Default)]
pub struct MarkupFrame {
    /// The retail chrome.
    pub chrome: NineSliceFrame,
    /// The theme in force on a themed panel.
    pub palette: Option<Palette>,
}

impl Widget for MarkupFrame {
    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        match self.palette {
            Some(p) => {
                let (w, h) = size(ui, me);
                rc.fill(0.0, 0.0, w, h, p.background);
            }
            None => self.chrome.draw(ui, me, rc),
        }
    }

    fn draw_after_children(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        match self.palette {
            Some(p) => {
                let (w, h) = size(ui, me);
                rc.rect_outline(0.0, 0.0, w, h, p.border, 1.0);
                if ui.element(me).is_some_and(|e| e.resizable) {
                    rc.fill(w - 7.0, h - 3.0, 5.0, 1.0, p.muted);
                }
            }
            None => self.chrome.draw_after_children(ui, me, rc),
        }
    }
}

/// The retail vertical scrollbar art: track, up and down buttons, thumb
/// top, middle and bottom (OpenAC `RetailScrollbarChrome`).
pub mod vertical_bar {
    /// Track.
    pub const TRACK: u32 = 0x0600_4C5F;
    /// Up button.
    pub const UP: u32 = 0x0600_4C6C;
    /// Down button.
    pub const DOWN: u32 = 0x0600_4C69;
    /// Thumb top.
    pub const THUMB_TOP: u32 = 0x0600_4C60;
    /// Thumb middle.
    pub const THUMB_MID: u32 = 0x0600_4C63;
    /// Thumb bottom.
    pub const THUMB_BOT: u32 = 0x0600_4C66;
}

const SCROLL_BUTTON: f32 = 16.0;

/// Draws a vertical scrollbar at `x`, `width` wide, the element's height:
/// retail art, or the theme's flat bar (OpenAC `UiMarkupList.DrawScrollbar`).
fn draw_vbar(
    rc: &mut RenderContext<'_>,
    x: f32,
    width: f32,
    height: f32,
    scroll: &Scrollable,
    palette: Option<&Palette>,
) {
    if let Some(p) = palette {
        rc.fill(x, 0.0, width, height, p.field);
        rc.rect_outline(x, 0.0, width, height, p.border, 1.0);
        let button = SCROLL_BUTTON.min(height / 2.0);
        let (top, extent) = thumb_rect(scroll, button, (height - 2.0 * button).max(0.0));
        rc.fill(x + 3.0, top, width - 6.0, extent, p.muted);
        rc.fill(x + 5.0, button / 2.0, width - 10.0, 1.0, p.muted);
        rc.fill(x + 5.0, height - button / 2.0, width - 10.0, 1.0, p.muted);
        return;
    }
    let dec = SCROLL_BUTTON.clamp(0.0, height);
    let inc = SCROLL_BUTTON.clamp(0.0, height - dec);
    rc.surface(vertical_bar::TRACK, x, 0.0, width, height, Color::WHITE);
    let flat = |rc: &mut RenderContext<'_>, id: u32, y: f32, h: f32| {
        if h > 0.0 && rc.sprite_size(id).is_some() {
            rc.sprite(
                TextureRef::Surface(id),
                x,
                y,
                width,
                h,
                (0.0, 0.0, 1.0, 1.0),
                Color::WHITE,
            );
        }
    };
    flat(rc, vertical_bar::UP, 0.0, dec);
    flat(rc, vertical_bar::DOWN, height - inc, inc);
    let (top, extent) = thumb_rect(scroll, dec, (height - dec - inc).max(0.0));
    const CAP: f32 = 3.0;
    if extent >= 2.0 * CAP {
        flat(rc, vertical_bar::THUMB_TOP, top, CAP);
        rc.surface(
            vertical_bar::THUMB_MID,
            x,
            top + CAP,
            width,
            extent - 2.0 * CAP,
            Color::WHITE,
        );
        flat(rc, vertical_bar::THUMB_BOT, top + extent - CAP, CAP);
    } else {
        flat(rc, vertical_bar::THUMB_MID, top, extent);
    }
}

/// What a vertical scrollbar press at `y` does: step, page or grab the
/// thumb (the grab offset).
enum BarPress {
    Step(i32),
    Page(i32),
    Grab(f32),
}

fn press_vbar(scroll: &Scrollable, height: f32, y: f32) -> BarPress {
    let dec = SCROLL_BUTTON.clamp(0.0, height);
    let inc = SCROLL_BUTTON.clamp(0.0, height - dec);
    if y < dec {
        return BarPress::Step(-1);
    }
    if y >= height - inc {
        return BarPress::Step(1);
    }
    let (top, extent) = thumb_rect(scroll, dec, (height - dec - inc).max(0.0));
    if y >= top && y <= top + extent {
        BarPress::Grab(y - top)
    } else {
        BarPress::Page(if y < top { -1 } else { 1 })
    }
}

fn drag_ratio(scroll: &Scrollable, height: f32, y: f32, offset: f32) -> f32 {
    let dec = SCROLL_BUTTON.clamp(0.0, height);
    let inc = SCROLL_BUTTON.clamp(0.0, height - dec);
    let track = (height - dec - inc).max(0.0);
    let (_, extent) = thumb_rect(scroll, dec, track);
    let travel = (track - extent).max(1.0);
    (y - offset - dec) / travel
}

/// One list cell's content.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// Text in a colour.
    Text(String, Color),
    /// A check box.
    Check(bool),
    /// An icon (none for an empty one).
    Icon(Option<Icon>),
    /// A button with this caption (an empty caption draws no button).
    Button(String),
    /// Nothing (a short column).
    Empty,
}

/// A list of rows (OpenAC `UiMarkupList`): one column of text with
/// optional per-row colours and icons, or columns. A press on a row sends
/// [`ElementMessage::Clicked`] with the press point; the owner works out
/// what it means ([`MarkupList::press`]).
#[derive(Debug, Clone)]
pub struct MarkupList {
    /// The columns' layout and kinds (empty: the single-column form).
    pub columns: Vec<Column>,
    /// Each row's cells (one per column; the single-column form has one
    /// text cell and maybe an icon).
    pub rows: Vec<Vec<Cell>>,
    /// Each column's own row count.
    pub column_rows: Vec<usize>,
    /// The single-column form's per-row icons.
    pub row_icons: Option<Vec<Option<Icon>>>,
    /// The selected row (-1: none).
    pub selected: i32,
    /// Row height.
    pub row_height: f32,
    /// A band behind the selected row.
    pub selection_band: bool,
    /// The interface font.
    pub font: Option<Arc<UiFont>>,
    /// Fill.
    pub background: Color,
    /// Border.
    pub border: Color,
    /// Default text colour.
    pub text_color: Color,
    /// Selection band.
    pub selected_color: Color,
    /// The theme's colours on a themed panel.
    pub palette: Option<Palette>,
    /// A button column's face; Classic matches a markup `<button>` (a
    /// themed window uses its background colour, the list itself being
    /// drawn in the field colour).
    pub button_color: Color,
    /// A button column's border (a themed window uses its border colour).
    pub button_border_color: Color,
    top: usize,
    revealed: i32,
    scroll: Scrollable,
    dragging: Option<f32>,
    /// The button cell under the pointer, as (row, column).
    hover: Option<(usize, usize)>,
    /// The button the left button went down on, and what the list showed
    /// then.
    press: Option<ButtonPress>,
    /// The pointer's last list-local position, so the hovered button can be
    /// found again when the rows move under a pointer that did not.
    pointer: Option<(f32, f32)>,
}

/// A held list button. A release fires by row index, so the press is
/// dropped if the list scrolls, its row count changes, or the pressed
/// button's caption changes before then.
#[derive(Debug, Clone, PartialEq)]
struct ButtonPress {
    row: usize,
    column: usize,
    top: usize,
    row_count: usize,
    caption: String,
}

/// The look a list button takes from the pointer and the list's enabled
/// flag (OpenAC `UiControlState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonLook {
    /// At rest.
    Normal,
    /// Under the pointer.
    Hovered,
    /// Held, the pointer still over it.
    Pressed,
    /// The list is disabled.
    Disabled,
}

const PADDING: f32 = 3.0;

/// `a` moved `t` of the way to `b`, keeping `a`'s alpha.
fn toward(a: Color, b: Color, t: f32) -> Color {
    Color::rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a,
    )
}

fn dimmed(mut color: Color) -> Color {
    color.a *= DISABLED_ALPHA;
    color
}

impl MarkupList {
    /// An empty list with OpenAC's colours.
    #[must_use]
    pub fn new(font: Option<Arc<UiFont>>, row_height: f32) -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            column_rows: Vec::new(),
            row_icons: None,
            selected: -1,
            row_height,
            selection_band: false,
            font,
            background: Color::rgba(0.0, 0.0, 0.0, 0.92),
            border: Color::rgba(0.46, 0.37, 0.16, 1.0),
            text_color: Color::rgba(0.91, 0.87, 0.76, 1.0),
            selected_color: Color::rgba(0.28, 0.23, 0.08, 0.95),
            palette: None,
            button_color: Color::rgba(0.1, 0.1, 0.15, 0.8),
            button_border_color: Color::rgba(0.45, 0.45, 0.55, 1.0),
            top: 0,
            revealed: i32::MIN,
            scroll: Scrollable::default(),
            dragging: None,
            hover: None,
            press: None,
            pointer: None,
        }
    }

    fn has_buttons(&self) -> bool {
        self.columns
            .iter()
            .any(|c| matches!(c.kind, ColumnKind::Button { .. }))
    }

    /// The caption of a button cell, when it has one (an empty caption is
    /// no button).
    fn caption(&self, row: usize, column: usize) -> Option<&str> {
        if !matches!(
            self.columns.get(column).map(|c| &c.kind),
            Some(ColumnKind::Button { .. })
        ) {
            return None;
        }
        match self.rows.get(row)?.get(column)? {
            Cell::Button(caption) if !caption.is_empty() => Some(caption),
            _ => None,
        }
    }

    fn button_rect(
        &self,
        column: usize,
        cell_x: f32,
        cell_w: f32,
        row_y: f32,
    ) -> (f32, f32, f32, f32) {
        let (bw, bh) = match self.columns.get(column).map(|c| &c.kind) {
            Some(ColumnKind::Button {
                button_width,
                button_height,
                ..
            }) => (*button_width, *button_height),
            _ => (0.0, 0.0),
        };
        list::button_rect(bw, bh, cell_x, cell_w, row_y, self.row_height)
    }

    /// The button cell at a list-local point, as (row, column).
    fn button_cell_at(&self, width: f32, height: f32, x: f32, y: f32) -> Option<(usize, usize)> {
        let content = self.content_width(width, height);
        if self.columns.is_empty() || x < 0.0 || x >= content {
            return None;
        }
        let row = (y / self.row_height.max(1.0)).floor();
        if row < 0.0 || row as usize >= self.visible(height) {
            return None;
        }
        let index = self.top + row as usize;
        let layout = list::column_layout(&self.columns, content);
        for (c, &(cell_x, cell_w)) in layout.iter().enumerate() {
            if x < cell_x || x >= cell_x + cell_w {
                continue;
            }
            self.caption(index, c)?;
            let (bx, by, bw, bh) = self.button_rect(c, cell_x, cell_w, row * self.row_height);
            let on = x >= bx && x < bx + bw && y >= by && y < by + bh;
            return on.then_some((index, c));
        }
        None
    }

    /// The look of one button cell: pressed only while the pointer is still
    /// over the button it went down on, and no other button hovers while
    /// one is held; a cell with no caption is at rest (OpenAC
    /// `UiMarkupList.ButtonState`).
    #[must_use]
    pub fn button_look(&self, row: usize, column: usize, enabled: bool) -> ButtonLook {
        if self.caption(row, column).is_none() {
            return ButtonLook::Normal;
        }
        if !enabled {
            return ButtonLook::Disabled;
        }
        let over = self.hover == Some((row, column));
        match &self.press {
            Some(press) if press.row == row && press.column == column && over => {
                ButtonLook::Pressed
            }
            Some(_) => ButtonLook::Normal,
            None if over => ButtonLook::Hovered,
            None => ButtonLook::Normal,
        }
    }

    /// Finds the hovered button again from the last pointer position.
    fn refresh_hover(&mut self, width: f32, height: f32) {
        self.hover = match self.pointer {
            Some((x, y)) if self.dragging.is_none() => self.button_cell_at(width, height, x, y),
            _ => None,
        };
    }

    /// Drops the press when the row under it may no longer be the item that
    /// was pressed: the list scrolled, its row count changed, or the
    /// pressed button's caption changed or went empty. Other columns are
    /// left out on purpose: a live column such as a distance changes every
    /// frame and would make the buttons beside it unreliable.
    fn cancel_press_if_rows_moved(&mut self) {
        let Some(press) = &self.press else {
            return;
        };
        let moved = self.top != press.top
            || self.rows.len() != press.row_count
            || self.caption(press.row, press.column) != Some(press.caption.as_str());
        if moved {
            self.press = None;
        }
    }

    /// Follows the pointer over button cells and fires a button when the
    /// left button comes up over the one it went down on. Only a release
    /// that fires a button is handled here; everything else carries on to
    /// the list.
    fn track_button_pointer(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        let (w, h) = size(ui, me);
        match e.kind {
            kind::HOVER_ENTER | kind::MOUSE_MOVE | kind::MOUSE_DOWN => {
                self.pointer = Some((e.data1 as f32, e.data2 as f32));
                self.refresh_hover(w, h);
                false
            }
            kind::HOVER_LEAVE => {
                self.pointer = None;
                self.hover = None;
                false
            }
            kind::CAPTURE_CHANGED => {
                self.press = None;
                false
            }
            kind::MOUSE_UP => {
                self.pointer = Some((e.data1 as f32, e.data2 as f32));
                self.cancel_press_if_rows_moved();
                let press = self.press.take();
                self.hover = self.button_cell_at(w, h, e.data1 as f32, e.data2 as f32);
                match press {
                    Some(p) if enabled(ui, me) && self.hover == Some((p.row, p.column)) => {
                        ui.send_message(
                            me,
                            ElementMessage::ListButtonClicked {
                                column: p.column,
                                row: p.row,
                            },
                        );
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// One button cell: inset by a pixel all round, its caption centred in
    /// the list's text colour and cut at the button's edges; a disabled
    /// list fades face, border and caption (OpenAC
    /// `UiMarkupList.DrawButtonCell`).
    #[allow(clippy::too_many_arguments)]
    fn draw_button_cell(
        &self,
        rc: &mut RenderContext<'_>,
        row: usize,
        column: usize,
        cell_x: f32,
        cell_w: f32,
        y: f32,
        enabled: bool,
    ) {
        let Some(caption) = self.caption(row, column) else {
            return;
        };
        let (x, top, w, h) = self.button_rect(column, cell_x, cell_w, y);
        let p = self.palette;
        let base = p.map_or(self.button_color, |p| p.background);
        let mut border = p.map_or(self.button_border_color, |p| p.border);
        let mut text_color = p.map_or(self.text_color, |p| p.text);
        let look = self.button_look(row, column, enabled);
        let mut face = match look {
            ButtonLook::Hovered => toward(base, border, 0.3),
            ButtonLook::Pressed => toward(base, Color::rgba(0.0, 0.0, 0.0, 0.0), 0.25),
            _ => base,
        };
        if look == ButtonLook::Disabled {
            face = dimmed(face);
            border = dimmed(border);
            text_color = dimmed(text_color);
        }
        rc.fill(x, top, w, h, face);
        rc.rect_outline(x, top, w, h, border, 1.0);
        rc.push_clip(x, top, w, h);
        let font = self.font.as_ref();
        text(
            rc,
            font,
            caption,
            x + (w - measure(font, caption)) * 0.5,
            top + (h - line_height(font)) * 0.5,
            text_color,
            p.is_none(),
        );
        rc.pop_clip();
    }

    /// The first row shown.
    #[must_use]
    pub fn top(&self) -> usize {
        self.top
    }

    fn visible(&self, height: f32) -> usize {
        list::visible_rows(height, self.row_height)
    }

    fn content_width(&self, width: f32, height: f32) -> f32 {
        if self.rows.len() > self.visible(height) {
            (width - list::SCROLLBAR_WIDTH).max(0.0)
        } else {
            width
        }
    }

    fn configure_scroll(&mut self, visible: usize) {
        let line = self.row_height.round().max(1.0) as i32;
        self.scroll.line_height = line;
        self.scroll
            .set_extents(self.rows.len() as i32 * line, visible as i32 * line, false);
        self.scroll.set_scroll_y(self.top as i32 * line);
    }

    /// What a press at list-local (`x`, `y`) means, with the list as last
    /// drawn (OpenAC `UiMarkupList.OnEvent` / `OnEventColumns`).
    #[must_use]
    pub fn press(&self, width: f32, height: f32, x: f32, y: f32) -> ColumnPress {
        let visible = self.visible(height);
        if self.columns.is_empty() {
            let row = (y / self.row_height.max(1.0)).floor();
            let index = self.top as i64 + row as i64;
            return if row >= 0.0
                && (row as usize) < visible
                && index >= 0
                && (index as usize) < self.rows.len()
            {
                ColumnPress::Select(index as usize)
            } else {
                ColumnPress::Nothing
            };
        }
        let layout = list::column_layout(&self.columns, self.content_width(width, height));
        list::press_columns(
            &self.columns,
            &layout,
            &self.column_rows,
            self.top,
            visible,
            self.row_height,
            (x, y),
        )
    }
}

impl Widget for MarkupList {
    fn handles_click(&self, _element: &Element) -> bool {
        true
    }

    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        if e.target != Some(me) {
            return false;
        }
        let has_buttons = self.has_buttons();
        if has_buttons && self.track_button_pointer(ui, me, e) {
            return true;
        }
        let (w, h) = size(ui, me);
        let visible = self.visible(h);
        let count = self.rows.len();
        match e.kind {
            kind::SCROLL => {
                self.top = if e.data0 > 0 {
                    self.top.saturating_sub(1)
                } else {
                    self.top + 1
                };
                self.top = list::clamp_top(self.top, count, visible);
                if has_buttons {
                    // The rows moved under the pointer: a press no longer
                    // names the row it went down on, and the hovered button
                    // may differ.
                    self.cancel_press_if_rows_moved();
                    self.pointer = Some((e.data1 as f32, e.data2 as f32));
                    self.refresh_hover(w, h);
                }
                true
            }
            kind::MOUSE_MOVE if self.dragging.is_some() => {
                self.configure_scroll(visible);
                let ratio = drag_ratio(
                    &self.scroll,
                    h,
                    e.data2 as f32,
                    self.dragging.unwrap_or(0.0),
                );
                self.scroll.set_position_ratio(ratio);
                let line = self.row_height.round().max(1.0);
                self.top = list::clamp_top(
                    (self.scroll.scroll_y() as f32 / line).round() as usize,
                    count,
                    visible,
                );
                true
            }
            kind::MOUSE_UP | kind::CAPTURE_CHANGED if self.dragging.is_some() => {
                self.dragging = None;
                if ui.captured() == Some(me) {
                    ui.release_capture();
                }
                true
            }
            kind::MOUSE_DOWN => {
                let content = self.content_width(w, h);
                if count > visible && e.data1 as f32 >= content {
                    self.configure_scroll(visible);
                    match press_vbar(&self.scroll, h, e.data2 as f32) {
                        BarPress::Step(d) => {
                            self.top = list::clamp_top(
                                (self.top as i64 + i64::from(d)).max(0) as usize,
                                count,
                                visible,
                            );
                        }
                        BarPress::Page(d) => {
                            let next = self.top as i64 + i64::from(d) * visible as i64;
                            self.top = list::clamp_top(next.max(0) as usize, count, visible);
                        }
                        BarPress::Grab(offset) => {
                            self.dragging = Some(offset);
                            ui.set_capture(me);
                        }
                    }
                    return true;
                }
                if enabled(ui, me) {
                    // A press on a button fires when it comes up over the
                    // same button (`track_button_pointer`).
                    if let Some((row, column)) = self.hover {
                        self.press = self.caption(row, column).map(|caption| ButtonPress {
                            row,
                            column,
                            top: self.top,
                            row_count: count,
                            caption: caption.to_owned(),
                        });
                    }
                    ui.send_message(
                        me,
                        ElementMessage::Clicked {
                            x: e.data1,
                            y: e.data2,
                        },
                    );
                }
                true
            }
            _ => false,
        }
    }

    fn receives_hover_mouse_move(&self) -> bool {
        self.has_buttons()
    }

    fn on_enabled_changed(&mut self, ui: &mut UiRoot, me: ElementId) {
        if !enabled(ui, me) {
            self.press = None;
        }
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = size(ui, me);
        let visible = self.visible(h);
        let count = self.rows.len();
        if self.selected != self.revealed {
            self.revealed = self.selected;
            self.top = list::reveal(self.top, self.selected, count, visible);
        }
        self.top = list::clamp_top(self.top, count, visible);
        if self.has_buttons() {
            self.cancel_press_if_rows_moved();
            self.refresh_hover(w, h);
        }
        let is_enabled = enabled(ui, me);
        let content = self.content_width(w, h);
        let p = self.palette;
        rc.fill(0.0, 0.0, w, h, p.map_or(self.background, |p| p.field));
        rc.rect_outline(0.0, 0.0, w, h, p.map_or(self.border, |p| p.border), 1.0);
        let band = self.selection_band || p.is_some();
        let font = self.font.clone();
        let text_y = (self.row_height - line_height(font.as_ref())).max(0.0) * 0.5;
        let layout = if self.columns.is_empty() {
            Vec::new()
        } else {
            list::column_layout(&self.columns, content)
        };
        let icon_column = if self.row_icons.is_some() {
            (self.row_height - 2.0).max(0.0)
        } else {
            0.0
        };
        let end = count.min(self.top + visible);
        for index in self.top..end {
            let y = (index - self.top) as f32 * self.row_height;
            if band && i32::try_from(index).ok() == Some(self.selected) && self.columns.is_empty() {
                rc.fill(
                    1.0,
                    y + 1.0,
                    content - 2.0,
                    self.row_height - 1.0,
                    p.map_or(self.selected_color, |p| p.selected),
                );
            }
            if self.columns.is_empty() {
                if let Some(Some(icon)) = self.row_icons.as_ref().and_then(|icons| icons.get(index))
                {
                    let extent = (icon_column - 2.0).max(0.0);
                    draw_icon(
                        rc,
                        *icon,
                        1.0,
                        y + (self.row_height - extent) * 0.5,
                        extent,
                        extent,
                    );
                }
                if let Some(Cell::Text(s, color)) = self.rows[index].first() {
                    text(
                        rc,
                        font.as_ref(),
                        s,
                        PADDING + icon_column,
                        y + text_y,
                        *color,
                        p.is_none(),
                    );
                }
                continue;
            }
            for (c, cell) in self.rows[index].iter().enumerate() {
                let Some(&(cx, cw)) = layout.get(c) else {
                    continue;
                };
                if cw <= 0.0 {
                    continue;
                }
                rc.push_clip(cx, y, cw, self.row_height);
                match cell {
                    Cell::Text(s, color) => {
                        text(
                            rc,
                            font.as_ref(),
                            s,
                            cx + PADDING,
                            y + text_y,
                            *color,
                            p.is_none(),
                        );
                    }
                    Cell::Check(checked) => {
                        let extent = (cw - 2.0).max(0.0);
                        let lx = cx + 1.0 + ((extent - CHECK_SIZE) * 0.5).max(0.0);
                        let ly = y + ((self.row_height - CHECK_SIZE) * 0.5).max(1.0);
                        draw_check(rc, lx, ly, *checked, p.as_ref());
                    }
                    Cell::Icon(Some(icon)) => {
                        draw_icon(
                            rc,
                            *icon,
                            cx + 1.0,
                            y + 1.0,
                            (cw - 2.0).max(0.0),
                            (self.row_height - 2.0).max(0.0),
                        );
                    }
                    Cell::Button(_) => {
                        self.draw_button_cell(rc, index, c, cx, cw, y, is_enabled);
                    }
                    Cell::Icon(None) | Cell::Empty => {}
                }
                rc.pop_clip();
            }
        }
        if count > visible {
            self.configure_scroll(visible);
            draw_vbar(
                rc,
                content,
                list::SCROLLBAR_WIDTH,
                h,
                &self.scroll,
                p.as_ref(),
            );
        }
    }
}

/// A wrapped transcript that follows new entries only while the reader is
/// at its end (OpenAC `UiMarkupLog`).
#[derive(Debug, Clone)]
pub struct MarkupLog {
    /// The interface font.
    pub font: Option<Arc<UiFont>>,
    /// Text colour.
    pub text_color: Color,
    /// The theme's colours on a themed panel.
    pub palette: Option<Palette>,
    entries: Vec<String>,
    first: i32,
    rows: Vec<(i64, usize, String)>,
    scroll: Scrollable,
    follow_end: bool,
    laid_out: Option<(f32, f32)>,
    dirty: bool,
    dragging: Option<f32>,
}

impl MarkupLog {
    /// An empty log.
    #[must_use]
    pub fn new(font: Option<Arc<UiFont>>) -> Self {
        Self {
            font,
            text_color: Color::rgba(0.91, 0.87, 0.76, 1.0),
            palette: None,
            entries: Vec::new(),
            first: 0,
            rows: Vec::new(),
            scroll: Scrollable::default(),
            follow_end: true,
            laid_out: None,
            dirty: true,
            dragging: None,
        }
    }

    /// The entries and the number of the first one.
    pub fn set_entries(&mut self, entries: &[String], first: i32) {
        if self.entries != entries || self.first != first {
            self.entries = entries.to_vec();
            self.first = first;
            self.dirty = true;
        }
    }

    /// Another font: the rows are wrapped again (OpenAC `UiMarkupLog`
    /// re-lays out when its font changes).
    pub fn set_font(&mut self, font: Option<Arc<UiFont>>) {
        let same = match (&self.font, &font) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.font = font;
            self.dirty = true;
        }
    }

    /// Whether it follows the newest entry.
    #[must_use]
    pub fn following_end(&self) -> bool {
        self.follow_end
    }

    fn line_height(&self) -> i32 {
        (line_height(self.font.as_ref()).ceil() as i32 + 2).max(14)
    }

    /// The scroll position, for tests.
    #[cfg(test)]
    pub(crate) fn scroll(&self) -> &Scrollable {
        &self.scroll
    }

    /// Scrolls to `y` from outside, as OpenAC's tests move the log's
    /// scroll model: following resumes exactly when it lands at the end.
    #[cfg(test)]
    pub(crate) fn set_scroll_y(&mut self, y: i32) {
        self.scroll.set_scroll_y(y);
        self.follow_end = self.scroll.at_end();
    }

    /// The entry number and text of the top visible row, for tests.
    #[cfg(test)]
    pub(crate) fn first_visible(&self) -> Option<(i64, String)> {
        let line = self.line_height();
        let index = (self.scroll.scroll_y() / line).max(0) as usize;
        self.rows
            .get(index.min(self.rows.len().checked_sub(1)?))
            .map(|(entry, _, text)| (*entry, text.clone()))
    }

    fn wrap(&self, entry: i64, text_in: &str, available: f32, out: &mut Vec<(i64, usize, String)>) {
        let chars: Vec<char> = text_in.chars().collect();
        if chars.is_empty() {
            out.push((entry, 0, String::new()));
            return;
        }
        let advance = |c: char| self.font.as_ref().map_or(7.0, |f| f.advance(c));
        let mut start = 0usize;
        while start < chars.len() {
            let mut end = start;
            let mut space: Option<usize> = None;
            let mut width = 0.0;
            while end < chars.len() && chars[end] != '\n' {
                let a = advance(chars[end]);
                if end > start && width + a > available {
                    break;
                }
                width += a;
                if chars[end].is_whitespace() {
                    space = Some(end);
                }
                end += 1;
            }
            let newline = end < chars.len() && chars[end] == '\n';
            if !newline
                && end < chars.len()
                && let Some(s) = space.filter(|s| *s > start)
            {
                end = s;
            }
            let line: String = chars[start..end].iter().collect();
            out.push((entry, start, line.trim_end_matches('\r').to_owned()));
            start = end;
            if start < chars.len() && chars[start].is_whitespace() {
                start += 1;
            }
        }
    }

    fn refresh(&mut self, w: f32, h: f32) {
        if !self.dirty && self.laid_out == Some((w, h)) {
            return;
        }
        let line = self.line_height();
        let anchor = (!self.rows.is_empty()).then(|| {
            let i = (self.scroll.scroll_y() / line).max(0) as usize;
            self.rows[i.min(self.rows.len() - 1)].clone()
        });
        let remainder = self.scroll.scroll_y() % line;
        let available = (w - 26.0).max(1.0);
        let mut rows = Vec::new();
        for (i, entry) in self.entries.iter().enumerate() {
            self.wrap(
                i64::from(self.first) + i as i64,
                entry,
                available,
                &mut rows,
            );
        }
        self.rows = rows;
        self.scroll.line_height = line;
        self.scroll
            .set_extents(self.rows.len() as i32 * line, (h as i32 - 8).max(0), false);
        if self.follow_end {
            self.scroll.scroll_to_end();
        } else if let Some((entry, offset, _)) = anchor {
            let index = self
                .rows
                .iter()
                .rposition(|(e, o, _)| *e == entry && *o <= offset);
            self.scroll
                .set_scroll_y(index.map_or(0, |i| i as i32 * line + remainder.min(line - 1)));
        }
        self.laid_out = Some((w, h));
        self.dirty = false;
    }
}

impl Widget for MarkupLog {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        if e.target != Some(me) {
            return false;
        }
        let (w, h) = size(ui, me);
        self.refresh(w, h);
        match e.kind {
            kind::SCROLL => {
                self.scroll.scroll_by_lines(-e.data0 * 3);
                self.follow_end = self.scroll.at_end();
                true
            }
            kind::MOUSE_DOWN if self.scroll.has_overflow() && e.data1 as f32 >= w - 17.0 => {
                match press_vbar(&self.scroll, h - 2.0, e.data2 as f32 - 1.0) {
                    BarPress::Step(d) => self.scroll.scroll_by_lines(d),
                    BarPress::Page(d) => self
                        .scroll
                        .scroll_by_lines(d * ((h as i32 - 8) / self.line_height()).max(1)),
                    BarPress::Grab(offset) => {
                        self.dragging = Some(offset);
                        ui.set_capture(me);
                    }
                }
                self.follow_end = self.scroll.at_end();
                true
            }
            kind::MOUSE_MOVE if self.dragging.is_some() => {
                let ratio = drag_ratio(
                    &self.scroll,
                    h - 2.0,
                    e.data2 as f32 - 1.0,
                    self.dragging.unwrap_or(0.0),
                );
                self.scroll.set_position_ratio(ratio);
                self.follow_end = self.scroll.at_end();
                true
            }
            kind::MOUSE_UP | kind::CAPTURE_CHANGED if self.dragging.is_some() => {
                self.dragging = None;
                if ui.captured() == Some(me) {
                    ui.release_capture();
                }
                true
            }
            _ => false,
        }
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = size(ui, me);
        self.refresh(w, h);
        let p = self.palette;
        let background = Color::rgba(0.0, 0.0, 0.0, 0.92);
        let border = Color::rgba(0.46, 0.37, 0.16, 1.0);
        rc.fill(0.0, 0.0, w, h, p.map_or(background, |p| p.field));
        rc.rect_outline(0.0, 0.0, w, h, p.map_or(border, |p| p.border), 1.0);
        let line = self.line_height();
        rc.push_clip(4.0, 4.0, (w - 25.0).max(0.0), (h - 8.0).max(0.0));
        let top = (self.scroll.scroll_y() / line).max(0) as usize;
        let font = self.font.clone();
        let color = p.map_or(self.text_color, |p| p.text);
        for (i, (_, _, row)) in self.rows.iter().enumerate().skip(top) {
            let y = 4.0 + (i as i32 * line - self.scroll.scroll_y()) as f32;
            if y >= h - 4.0 {
                break;
            }
            text(rc, font.as_ref(), row, 4.0, y, color, p.is_none());
        }
        rc.pop_clip();
        if self.scroll.has_overflow() {
            rc.push_transform(0.0, 1.0);
            draw_vbar(
                rc,
                (w - 17.0).max(0.0),
                16.0,
                (h - 2.0).max(0.0),
                &self.scroll,
                p.as_ref(),
            );
            rc.pop_transform();
        }
    }
}
