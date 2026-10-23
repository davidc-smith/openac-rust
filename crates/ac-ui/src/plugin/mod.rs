//! Plugin UI in the window: the session's plugin windows built from their
//! markup and kept in step with the session's copy (`ac_plugin_ui`), the
//! plugins' canvases on the overlay band, and the plugin shelf.
//!
//! Nothing here decides anything: the session owns the plugin UI state
//! (which values, which windows are shown, what a canvas shows), the
//! window draws its copy and reports what the player did into a
//! [`PluginOutbox`] the client sends back to the session.

pub mod canvas;
pub mod hotkeys;
pub mod shelf;
pub mod widgets;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use ac_plugin_ui::list::ColumnPress;
use ac_plugin_ui::markup::{
    Anchors, ColorSource, ColumnKind, Element as Markup, IconKind, IconSource, ListBody,
    MarkupDocument, normalize_icon,
};
use ac_plugin_ui::{ActionValue, PanelView, Rgba, UiInput, UiValue};

use crate::draw::Color;
use crate::font::UiFont;
use crate::geometry::AnchorEdges;
use crate::host::PanelController;
use crate::root::{ElementMessage, UiRoot};
use crate::tree::{Element, ElementId};
use crate::widgets::{Field, Menu, MenuItem, Meter, Panel, Scrollbar};

pub use widgets::{
    Cell, Icon, MarkupButton, MarkupFrame, MarkupIcon, MarkupLabel, MarkupList, MarkupLog,
    MarkupToggle, Palette,
};

/// What the player did with the plugins' UI, for the client to send to the
/// session, oldest first. Shared by every plugin window, canvas and the
/// shelf of one window.
pub type PluginOutbox = Rc<RefCell<Vec<UiInput>>>;

/// Resolves the images markup names: dat images, spell icons and live
/// object icons (the window's art and icon composer).
pub trait MarkupIcons {
    /// A render surface (a full id).
    fn surface(&mut self, id: u32) -> Option<Icon>;
    /// A spell's icon.
    fn spell(&mut self, spell: u32) -> Option<Icon>;
    /// A live object's composed icon.
    fn item(&mut self, object: u32) -> Option<Icon>;
}

/// The player's choice of look for plugin windows that opt in
/// (`theme="plugin"`; OpenAC `PluginUiTheme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PluginTheme {
    /// The retail look.
    #[default]
    Classic,
    /// Charcoal and moss.
    Moss,
    /// Warm graphite and brass.
    Brass,
}

impl From<ac_plugin_ui::ThemeKind> for PluginTheme {
    fn from(kind: ac_plugin_ui::ThemeKind) -> Self {
        match kind {
            ac_plugin_ui::ThemeKind::Classic => Self::Classic,
            ac_plugin_ui::ThemeKind::Moss => Self::Moss,
            ac_plugin_ui::ThemeKind::Brass => Self::Brass,
        }
    }
}

impl PluginTheme {
    /// Its colours (none for the retail look).
    #[must_use]
    pub fn palette(self) -> Option<Palette> {
        match self {
            Self::Classic => None,
            Self::Moss => Some(Palette::MOSS),
            Self::Brass => Some(Palette::BRASS),
        }
    }
}

/// Converts a markup colour.
#[must_use]
pub fn color(c: Rgba) -> Color {
    Color::rgba(c.r, c.g, c.b, c.a)
}

/// A button, tab or toggle: a tab stop that Enter or Space activates once
/// Tab focused it, while a click acts without taking keyboard focus from
/// the game (OpenAC `MarkupDocument`: `AcceptsFocus`, `TabStop`,
/// `FocusOnMouseClick = false`).
fn keyboard_activated(mut e: Element) -> Element {
    e.accepts_focus = true;
    e.focus_on_mouse_click = false;
    e.tab_stop = true;
    e
}

fn anchors(a: Anchors) -> AnchorEdges {
    let mut edges = AnchorEdges::NONE;
    for (bit, edge) in [
        (Anchors::LEFT, AnchorEdges::LEFT),
        (Anchors::TOP, AnchorEdges::TOP),
        (Anchors::RIGHT, AnchorEdges::RIGHT),
        (Anchors::BOTTOM, AnchorEdges::BOTTOM),
    ] {
        if a.contains(bit) {
            edges |= edge;
        }
    }
    edges
}

fn read_color(src: &ColorSource, view: &PanelView, palette: Option<&Palette>) -> Color {
    if let ColorSource::Theme { token, classic } = src {
        return palette
            .and_then(|p| p.token(token))
            .unwrap_or_else(|| color(*classic));
    }
    color(src.read(view))
}

fn texts(view: &PanelView, name: &str) -> Vec<String> {
    match view.values.get(name) {
        Some(UiValue::TextList(v)) => v.clone(),
        _ => Vec::new(),
    }
}

fn uints(view: &PanelView, name: &str) -> Vec<u32> {
    match view.values.get(name) {
        Some(UiValue::UintList(v)) => v.clone(),
        Some(UiValue::IntList(v)) => v.iter().map(|i| u32::try_from(*i).unwrap_or(0)).collect(),
        _ => Vec::new(),
    }
}

fn bools(view: &PanelView, name: &str) -> Vec<bool> {
    match view.values.get(name) {
        Some(UiValue::BoolList(v)) => v.clone(),
        _ => Vec::new(),
    }
}

fn bool_value(view: &PanelView, name: &str) -> bool {
    matches!(view.values.get(name), Some(UiValue::Bool(true)))
}

fn int_value(view: &PanelView, name: &str) -> i32 {
    match view.values.get(name) {
        Some(UiValue::Int(v)) => *v,
        _ => -1,
    }
}

fn icon(icons: &mut dyn MarkupIcons, kind: IconKind, id: u32) -> Option<Icon> {
    if id == 0 {
        return None;
    }
    match kind {
        IconKind::Did => icons.surface(normalize_icon(id)),
        IconKind::Spell => icons.spell(id),
        IconKind::Item => icons.item(id),
    }
}

fn icon_of(icons: &mut dyn MarkupIcons, source: &IconSource, view: &PanelView) -> Option<Icon> {
    icon(icons, source.kind, source.id.read(view))
}

/// One plugin window built from its markup: its frame and an element per
/// markup element. See [`build_panel`].
#[derive(Debug)]
pub struct PluginPanelController {
    /// The session's number for the window.
    pub panel: u32,
    /// The window's name.
    pub window_name: String,
    /// The frame (the window's root).
    pub root: ElementId,
    document: Arc<MarkupDocument>,
    nodes: Vec<ElementId>,
    by_element: HashMap<ElementId, usize>,
    title: Option<ElementId>,
    minimize: Option<ElementId>,
    outbox: PluginOutbox,
    expected_visible: bool,
    menu_items: HashMap<usize, Vec<String>>,
    synced: Option<u64>,
    /// The font the window was built with.
    classic_font: Option<Arc<UiFont>>,
    /// The bundled sans font the modern themes draw text in, when loaded.
    modern_font: Option<Arc<UiFont>>,
    /// The title's colour without a theme (`controls.ini`'s `[title] color`,
    /// else white).
    title_color: Color,
}

/// Builds the window of `view` (OpenAC `MarkupDocument.Build`): the frame
/// at the markup's position and size, its title, an element per markup
/// element in document order, and (for a window on the shelf) the minimize
/// button. Nothing is shown until [`PluginPanelController::sync`].
pub fn build_panel(
    ui: &mut UiRoot,
    view: &PanelView,
    font: Option<Arc<UiFont>>,
    outbox: PluginOutbox,
) -> PluginPanelController {
    let doc = Arc::clone(&view.document);
    let spec = &doc.panel;
    let mut frame_element = Element::at(spec.rect.x, spec.rect.y, spec.rect.w, spec.rect.h)
        .with_anchors(AnchorEdges::NONE)
        .with_visible(false);
    frame_element.name = Some(view.window_name());
    frame_element.draggable = true;
    frame_element.resizable = spec.resizable || spec.resize_x || spec.resize_y;
    frame_element.resize_x = spec.resize_x;
    frame_element.resize_y = spec.resize_y;
    frame_element.min_width = spec.min_width;
    frame_element.min_height = spec.min_height;
    let root = ui.insert(
        frame_element,
        MarkupFrame {
            chrome: crate::widgets::NineSliceFrame {
                draw_resize_affordances: spec.resizable,
                ..crate::widgets::NineSliceFrame::default()
            },
            palette: None,
        },
    );
    let title = spec.title.as_ref().map(|t| {
        let mut e = Element::at(8.0, 4.0, 0.0, 0.0);
        e.click_through = true;
        let mut label = MarkupLabel::new(font.clone());
        label.text.clone_from(t);
        ui.add(root, e, label)
    });
    let mut nodes = Vec::with_capacity(doc.nodes.len());
    let mut by_element = HashMap::new();
    for (index, node) in doc.nodes.iter().enumerate() {
        let parent = node.parent.map_or(root, |p| nodes[p]);
        let mut e = Element::at(node.rect.x, node.rect.y, node.rect.w, node.rect.h)
            .with_anchors(anchors(node.common.anchors));
        e.name.clone_from(&node.common.name);
        let font = font.clone();
        let id = match &node.element {
            Markup::Group { .. } => {
                e.click_through = true;
                ui.add(
                    parent,
                    e,
                    Panel {
                        background_color: Color::TRANSPARENT,
                        border_color: Color::TRANSPARENT,
                        border_thickness: 0.0,
                        background_sprite: 0,
                    },
                )
            }
            Markup::Meter { sprites, .. } => {
                Meter::configure(&mut e);
                e.click_through = true;
                let mut meter = Meter::default();
                meter.font = font;
                [
                    meter.back_left,
                    meter.back_tile,
                    meter.back_right,
                    meter.front_left,
                    meter.front_tile,
                    meter.front_right,
                ] = *sprites;
                meter.fill = None;
                ui.add(parent, e, meter)
            }
            Markup::Label { .. } => {
                e.click_through = node.common.tooltip.is_none();
                ui.add(parent, e, MarkupLabel::new(font))
            }
            Markup::Button { .. } => ui.add(parent, keyboard_activated(e), MarkupButton::new(font)),
            Markup::Tab { .. } => ui.add(parent, keyboard_activated(e), MarkupButton::tab(font)),
            Markup::Toggle { .. } => ui.add(parent, keyboard_activated(e), MarkupToggle::new(font)),
            Markup::Icon { .. } => {
                e.click_through = node.common.tooltip.is_none();
                ui.add(parent, e, MarkupIcon::default())
            }
            Markup::Slider { retail_art, .. } => {
                Scrollbar::configure(&mut e);
                let mut bar = Scrollbar::default();
                bar.scalar = true;
                bar.horizontal = true;
                bar.retail_art = *retail_art;
                if *retail_art {
                    crate::panels::external_container::apply_horizontal_bar_art(&mut bar);
                }
                ui.add(parent, e, bar)
            }
            Markup::Field {
                one_line,
                editable,
                max_length,
                clear_on_submit,
                text,
                onup,
                ondown,
                onsubmit,
                onblur,
                ..
            } => {
                Field::configure(&mut e);
                e.tab_stop = true;
                let mut field = Field::default();
                field.font = font;
                field.one_line = *one_line;
                field.selectable = true;
                field.max_characters = usize::try_from(*max_length).unwrap_or(1).max(1);
                field.clear_on_submit = *clear_on_submit;
                field.record_history = false;
                field.reports_focus_loss = onblur.is_some();
                field.has_submit_handler = onsubmit.is_some();
                field.set_editable_detached(&mut e, *editable);
                field.set_text(&text.read(view).unwrap_or_default());
                ui.add(
                    parent,
                    e,
                    MarkupField {
                        inner: field,
                        panel: view.id,
                        onup: onup.clone(),
                        ondown: ondown.clone(),
                        outbox: Rc::clone(&outbox),
                    },
                )
            }
            Markup::Menu {
                rows,
                row_height,
                open_upward,
                retail_art,
                searchable,
                ..
            } => {
                Menu::configure(&mut e);
                let mut menu = Menu::default();
                menu.searchable = *searchable;
                menu.font = font.clone();
                menu.rows_per_column = usize::try_from(*rows).unwrap_or(1).max(1);
                menu.row_height = *row_height;
                menu.column_width = node.rect.w.max(20.0);
                menu.open_upward = *open_upward;
                menu.scroll_track_sprite = widgets::vertical_bar::TRACK;
                menu.scroll_thumb_sprite = widgets::vertical_bar::THUMB_MID;
                menu.scroll_thumb_top_sprite = widgets::vertical_bar::THUMB_TOP;
                menu.scroll_thumb_bottom_sprite = widgets::vertical_bar::THUMB_BOT;
                menu.scroll_up_sprite = widgets::vertical_bar::UP;
                menu.scroll_down_sprite = widgets::vertical_bar::DOWN;
                menu.text_indent = 6.0;
                menu.button_text_indent = 6.0;
                menu.normal_sprite = 0x0600_4D65;
                menu.pressed_sprite = 0x0600_4D66;
                menu.popup_bg_sprite = 0x0600_124C;
                menu.item_normal_sprite = 0x0600_124E;
                menu.item_highlight_sprite = 0x0600_124D;
                menu.retail_button_art = *retail_art;
                menu.scrollable = true;
                menu.popup_scrollbar_hide_when_disabled = true;
                menu.fits_long_labels = true;
                ui.add(parent, e, menu)
            }
            Markup::Log { .. } => ui.add(parent, e, MarkupLog::new(font)),
            Markup::List {
                row_height,
                selection_band,
                body,
                ..
            } => {
                let mut list = MarkupList::new(font, *row_height);
                list.selection_band = *selection_band;
                if let ListBody::Columns(columns) = body {
                    list.columns.clone_from(columns);
                }
                ui.add(parent, e, list)
            }
        };
        if let Some(t) = &node.common.tooltip
            && let Some(el) = ui.element_mut(id)
        {
            el.runtime_tooltip_text = t.read(view);
        }
        by_element.insert(id, index);
        nodes.push(id);
    }
    let minimize = view.descriptor.show_in_side_panel.then(|| {
        let mut e = Element::at((spec.rect.w - 23.0).max(8.0), 3.0, 18.0, 17.0)
            .with_anchors(AnchorEdges::TOP | AnchorEdges::RIGHT)
            .with_z_order(1);
        e.name = Some("PluginMinimize".to_owned());
        e.runtime_tooltip_text = Some("Minimize to plugin sidepanel".to_owned());
        let button = MarkupButton {
            text: "\u{2013}".to_owned(),
            background: Color::rgba(0.02, 0.02, 0.015, 0.94),
            border: Color::rgba(0.58, 0.46, 0.17, 1.0),
            ..MarkupButton::new(font.clone())
        };
        ui.add(root, e, button)
    });
    ui.capture_authored_anchor_baselines(root);
    PluginPanelController {
        panel: view.id,
        window_name: view.window_name(),
        root,
        document: doc,
        nodes,
        by_element,
        title,
        minimize,
        outbox,
        expected_visible: false,
        menu_items: HashMap::new(),
        synced: None,
        classic_font: font,
        modern_font: None,
        title_color: Color::WHITE,
    }
}

impl PluginPanelController {
    /// The window with the bundled sans font: a themed (`theme="plugin"`)
    /// window draws its title, labels, buttons, tabs, toggles, fields,
    /// menus, logs and lists in it under a modern theme, and in the font it
    /// was built with under Classic (OpenAC `PluginMarkupTheme` with
    /// `PluginUiThemeSettings.ModernFont`). Without it, every theme keeps
    /// the built font.
    #[must_use]
    pub fn with_modern_font(mut self, font: Option<Arc<UiFont>>) -> Self {
        self.modern_font = font;
        self
    }

    /// The window's title in `color` where no theme colours it (OpenAC
    /// `MarkupDocument.Build`: the player's `controlsntrols.ini` `[title]
    /// color`, else white).
    #[must_use]
    pub fn with_title_color(mut self, ui: &mut UiRoot, color: Color) -> Self {
        self.title_color = color;
        if let Some(title) = self.title
            && let Some(label) = ui.widget_mut::<MarkupLabel>(title)
        {
            label.color = color;
        }
        self
    }

    /// The element built for markup element `node`.
    #[must_use]
    pub fn element(&self, node: usize) -> Option<ElementId> {
        self.nodes.get(node).copied()
    }

    /// The minimize button, on a window that is on the shelf.
    #[must_use]
    pub fn minimize_button(&self) -> Option<ElementId> {
        self.minimize
    }

    fn push(&self, input: UiInput) {
        self.outbox.borrow_mut().push(input);
    }

    fn action(&self, handler: &Option<String>, value: ActionValue) -> bool {
        let Some(handler) = handler else {
            return false;
        };
        self.push(UiInput::Action {
            panel: self.panel,
            handler: handler.clone(),
            value,
        });
        true
    }

    /// Shows the window as the session wants it and every element as the
    /// panel's values say (OpenAC's per-frame bindings, re-read here when
    /// the session's copy changed: `revision`).
    #[allow(clippy::too_many_lines)]
    pub fn sync(
        &mut self,
        ui: &mut UiRoot,
        view: &PanelView,
        revision: u64,
        icons: &mut dyn MarkupIcons,
        theme: PluginTheme,
    ) {
        let shown = view.shown();
        if shown != ui.is_window_visible(&self.window_name) || self.synced.is_none() {
            self.expected_visible = shown;
            if shown {
                ui.show_window(&self.window_name);
            } else {
                ui.hide_window(&self.window_name);
            }
        }
        if self.synced == Some(revision) {
            return;
        }
        self.synced = Some(revision);
        let doc = Arc::clone(&self.document);
        let palette = if doc.panel.themed {
            theme.palette()
        } else {
            None
        };
        let p = palette.as_ref();
        // With the bundled font loaded: the text font and a menu's caption
        // font for this theme (OpenAC `PluginMarkupTheme`'s font bindings;
        // a menu is built without a caption font of its own).
        let fonts = self.modern_font.as_ref().map(|modern| match p {
            Some(_) => (Some(Arc::clone(modern)), Some(Arc::clone(modern))),
            None => (self.classic_font.clone(), None),
        });
        let text_font = fonts.as_ref().map(|(text, _)| text);
        if let Some(frame) = ui.widget_mut::<MarkupFrame>(self.root) {
            frame.palette = palette;
        }
        if let Some(title) = self.title
            && let Some(label) = ui.widget_mut::<MarkupLabel>(title)
        {
            label.color = p.map_or(self.title_color, |p| p.text);
            label.outline = p.is_none();
            if let Some(f) = text_font {
                label.font.clone_from(f);
            }
        }
        for (index, node) in doc.nodes.iter().enumerate() {
            let el = self.nodes[index];
            ui.set_visible(el, view.node_visible(index));
            ui.set_enabled(el, node.common.enabled.read(view, true));
            if let Some(t) = &node.common.tooltip
                && let Some(e) = ui.element_mut(el)
            {
                e.runtime_tooltip_text = t.read(view);
            }
            let label = view.label_overrides.get(&index).cloned();
            match &node.element {
                Markup::Group { background, border } => {
                    if let Some(w) = ui.widget_mut::<Panel>(el) {
                        w.background_color = background
                            .as_ref()
                            .map_or(Color::TRANSPARENT, |c| read_color(c, view, p));
                        w.border_color = border
                            .as_ref()
                            .map_or(Color::TRANSPARENT, |c| read_color(c, view, p));
                        w.border_thickness = if border.is_some() { 1.0 } else { 0.0 };
                    }
                }
                Markup::Meter {
                    fill,
                    cur,
                    max,
                    color: bar,
                    ..
                } => {
                    let caption = match (cur.read_u32(view), max.read_u32(view)) {
                        (Some(c), Some(m)) => Some(format!("{c}/{m}")),
                        _ => None,
                    };
                    let bar_color = read_color(bar, view, p);
                    if let Some(w) = ui.widget_mut::<Meter>(el) {
                        w.fill = fill.read_f32(view);
                        w.label = caption;
                        w.bar_color = bar_color;
                    }
                }
                Markup::Label { text, color: c } => {
                    let s = label.or_else(|| text.read(view)).unwrap_or_default();
                    let col = match c {
                        Some(c) => read_color(c, view, p),
                        None => p.map_or(Color::WHITE, |p| p.text),
                    };
                    if let Some(w) = ui.widget_mut::<MarkupLabel>(el) {
                        w.text = s;
                        w.color = col;
                        w.outline = p.is_none();
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                    }
                }
                Markup::Button {
                    text,
                    color: c,
                    background,
                    border,
                    icon: source,
                    ..
                } => {
                    let s = label.or_else(|| text.read(view)).unwrap_or_default();
                    let resolved = source.as_ref().and_then(|s| icon_of(icons, s, view));
                    let text_color = match c {
                        Some(c) => read_color(c, view, p),
                        None => p.map_or(Color::WHITE, |p| p.text),
                    };
                    let bg = match background {
                        Some(c) => read_color(c, view, p),
                        None => p.map_or(Color::rgba(0.1, 0.1, 0.15, 0.8), |p| p.field),
                    };
                    let edge = match border {
                        Some(c) => read_color(c, view, p),
                        None => p.map_or(Color::rgba(0.45, 0.45, 0.55, 1.0), |p| p.border),
                    };
                    if let Some(w) = ui.widget_mut::<MarkupButton>(el) {
                        w.text = s;
                        w.text_color = text_color;
                        w.background = bg;
                        w.border = edge;
                        w.icon = resolved;
                        w.outline = p.is_none();
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                    }
                }
                Markup::Icon { icon: source } => {
                    let resolved = icon_of(icons, source, view);
                    if let Some(w) = ui.widget_mut::<MarkupIcon>(el) {
                        w.icon = resolved;
                    }
                }
                Markup::Tab {
                    text,
                    selected,
                    icon: source,
                    ..
                } => {
                    let s = label.unwrap_or_else(|| text.clone());
                    let on = bool_value(view, selected);
                    let resolved = source.as_ref().and_then(|s| icon_of(icons, s, view));
                    if let Some(w) = ui.widget_mut::<MarkupButton>(el) {
                        w.text = s;
                        w.icon = resolved;
                        w.tab_selected = Some(on);
                        w.outline = p.is_none();
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                    }
                }
                Markup::Toggle {
                    text,
                    checked,
                    color: c,
                    ..
                } => {
                    let s = label.or_else(|| text.read(view)).unwrap_or_default();
                    let col = match c {
                        Some(c) => read_color(c, view, p),
                        None => p.map_or(Color::rgba(0.86, 0.84, 0.74, 1.0), |p| p.text),
                    };
                    let on = bool_value(view, checked);
                    if let Some(w) = ui.widget_mut::<MarkupToggle>(el) {
                        w.text = s;
                        w.checked = on;
                        w.color = col;
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                    }
                }
                Markup::Slider {
                    min,
                    max,
                    value,
                    retail_art,
                    ..
                } => {
                    let position = value
                        .read_f32(view)
                        .map(|v| ((v - min) / (max - min)).clamp(0.0, 1.0));
                    if let Some(w) = ui.widget_mut::<Scrollbar>(el) {
                        // OpenAC `PluginMarkupTheme`: themed, flat in the
                        // theme's colours; Classic, as authored.
                        let classic = Scrollbar::default();
                        w.retail_art = p.is_none() && *retail_art;
                        w.plain_track_color = p.map_or(classic.plain_track_color, |p| p.field);
                        w.plain_border_color = p.map_or(classic.plain_border_color, |p| p.border);
                        w.plain_nub_color = p.map_or(classic.plain_nub_color, |p| p.muted);
                        if !w.is_dragging()
                            && let Some(position) = position
                        {
                            w.set_scalar_position(position);
                        }
                    }
                }
                Markup::Field {
                    text,
                    background,
                    color: c,
                    default_background,
                    default_color,
                    ..
                } => {
                    let bound = text.read(view).unwrap_or_default();
                    let bg = match (p, default_background) {
                        (Some(p), true) => p.field,
                        _ => read_color(background, view, p),
                    };
                    let col = match (p, default_color) {
                        (Some(p), true) => p.text,
                        _ => read_color(c, view, p),
                    };
                    if let Some(w) = ui.widget_mut::<MarkupField>(el) {
                        w.inner.follow_bound_text(&bound);
                        w.inner.background_color = bg;
                        w.inner.text_color = col;
                        if let Some(f) = text_font {
                            w.inner.font.clone_from(f);
                        }
                    }
                }
                Markup::Menu {
                    items,
                    selected,
                    retail_art,
                    ..
                } => {
                    let entries = texts(view, items);
                    let chosen = selected.read(view);
                    if let Some(w) = ui.widget_mut::<Menu>(el) {
                        w.set_items(
                            entries
                                .iter()
                                .enumerate()
                                .map(|(i, s)| MenuItem::new(s.clone(), i as u64))
                                .collect(),
                        );
                        w.selected = chosen
                            .as_ref()
                            .and_then(|c| entries.iter().position(|s| s == c))
                            .map(|i| i as u64);
                        w.button_label = chosen.unwrap_or_default();
                        // OpenAC `PluginMarkupTheme`: themed, the plain look
                        // in the theme's colours with a flat scrollbar;
                        // Classic, the authored art and the classic colours.
                        let classic = Menu::default();
                        w.retail_button_art = p.is_none() && *retail_art;
                        w.plain_popup_scrollbar = p.is_some();
                        w.plain_background_color =
                            p.map_or(classic.plain_background_color, |p| p.field);
                        w.plain_border_color = p.map_or(classic.plain_border_color, |p| p.border);
                        w.plain_open_border_color =
                            p.map_or(classic.plain_open_border_color, |p| p.accent);
                        w.plain_text_color = p.map_or(classic.plain_text_color, |p| p.text);
                        w.plain_triangle_color =
                            p.map_or(classic.plain_triangle_color, |p| p.muted);
                        w.plain_selected_color =
                            p.map_or(classic.plain_selected_color, |p| p.selected);
                        w.plain_hover_color = p.map_or(classic.plain_hover_color, |p| p.selected);
                        if let Some((text, caption)) = &fonts {
                            w.font.clone_from(text);
                            w.button_font.clone_from(caption);
                        }
                    }
                    self.menu_items.insert(index, entries);
                }
                Markup::Log { items, first_index } => {
                    let entries = texts(view, items);
                    let first = first_index.as_ref().map_or(0, |f| int_value(view, f));
                    if let Some(w) = ui.widget_mut::<MarkupLog>(el) {
                        w.set_entries(&entries, first);
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.set_font(f.clone());
                        }
                    }
                }
                Markup::List { selected, body, .. } => {
                    let selected = int_value(view, selected);
                    let (rows, column_rows, row_icons) = list_rows(view, body, icons, p);
                    if let Some(w) = ui.widget_mut::<MarkupList>(el) {
                        w.rows = rows;
                        w.column_rows = column_rows;
                        w.row_icons = row_icons;
                        w.selected = selected;
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                    }
                }
            }
        }
    }
}

type Rows = (Vec<Vec<Cell>>, Vec<usize>, Option<Vec<Option<Icon>>>);

/// A list's rows from the panel's values (OpenAC `UiMarkupList`'s sources).
fn list_rows(
    view: &PanelView,
    body: &ListBody,
    icons: &mut dyn MarkupIcons,
    palette: Option<&Palette>,
) -> Rows {
    let default = palette.map_or(Color::rgba(0.91, 0.87, 0.76, 1.0), |p| p.text);
    let row_color =
        |colors: &[u32], i: usize| colors.get(i).map_or(default, |c| color(Rgba::rgb(*c)));
    match body {
        ListBody::Single {
            items,
            colors,
            icons: row_icons,
        } => {
            let entries = texts(view, items);
            let colors = colors
                .as_deref()
                .map(|c| uints(view, c))
                .unwrap_or_default();
            let rows: Vec<Vec<Cell>> = entries
                .iter()
                .enumerate()
                .map(|(i, s)| vec![Cell::Text(s.clone(), row_color(&colors, i))])
                .collect();
            let resolved = row_icons.as_ref().map(|(name, kind)| {
                uints(view, name)
                    .into_iter()
                    .map(|id| icon(icons, *kind, id))
                    .collect()
            });
            let count = rows.len();
            (rows, vec![count], resolved)
        }
        ListBody::Columns(columns) => {
            let mut cells: Vec<Vec<Cell>> = Vec::with_capacity(columns.len());
            let mut counts = Vec::with_capacity(columns.len());
            for column in columns {
                let column_cells: Vec<Cell> = match &column.kind {
                    ColumnKind::Text { items, colors, .. } => {
                        let colors = colors
                            .as_deref()
                            .map(|c| uints(view, c))
                            .unwrap_or_default();
                        texts(view, items)
                            .into_iter()
                            .enumerate()
                            .map(|(i, s)| Cell::Text(s, row_color(&colors, i)))
                            .collect()
                    }
                    ColumnKind::Check { values, .. } => {
                        bools(view, values).into_iter().map(Cell::Check).collect()
                    }
                    ColumnKind::Icon { values, kind, .. } => uints(view, values)
                        .into_iter()
                        .map(|id| Cell::Icon(icon(icons, *kind, id)))
                        .collect(),
                    ColumnKind::Button { items, .. } => {
                        texts(view, items).into_iter().map(Cell::Button).collect()
                    }
                };
                counts.push(column_cells.len());
                cells.push(column_cells);
            }
            let count = counts.iter().copied().max().unwrap_or(0);
            let rows = (0..count)
                .map(|row| {
                    columns
                        .iter()
                        .zip(&cells)
                        .map(|(column, column_cells)| match column_cells.get(row) {
                            Some(cell) => cell.clone(),
                            // A short check column still draws its empty box.
                            None if matches!(column.kind, ColumnKind::Check { .. }) => {
                                Cell::Check(false)
                            }
                            None => Cell::Empty,
                        })
                        .collect()
                })
                .collect();
            (rows, counts, None)
        }
    }
}

impl PanelController for PluginPanelController {
    fn on_shown(&mut self, _ui: &mut UiRoot) {
        if !self.expected_visible {
            self.expected_visible = true;
            self.push(UiInput::PanelShown {
                panel: self.panel,
                shown: true,
            });
        }
    }

    fn on_hidden(&mut self, _ui: &mut UiRoot) {
        if self.expected_visible {
            self.expected_visible = false;
            self.push(UiInput::PanelShown {
                panel: self.panel,
                shown: false,
            });
        }
    }

    fn on_element_message(
        &mut self,
        ui: &mut UiRoot,
        element: ElementId,
        message: &ElementMessage,
    ) -> bool {
        if Some(element) == self.minimize {
            if matches!(message, ElementMessage::Clicked { .. }) {
                ui.hide_window(&self.window_name);
            }
            return true;
        }
        let Some(&index) = self.by_element.get(&element) else {
            return false;
        };
        let doc = Arc::clone(&self.document);
        match (&doc.nodes[index].element, message) {
            (
                Markup::Button { onclick, .. }
                | Markup::Tab { onclick, .. }
                | Markup::Toggle { onclick, .. },
                ElementMessage::Clicked { .. },
            ) => self.action(onclick, ActionValue::None),
            (
                Markup::Slider {
                    onchange, min, max, ..
                },
                ElementMessage::ScalarChanged(position),
            ) => self.action(onchange, ActionValue::Float(min + position * (max - min))),
            (Markup::Field { onchange, .. }, ElementMessage::TextChanged(text)) => {
                self.action(onchange, ActionValue::Text(text.clone()))
            }
            (Markup::Field { onsubmit, .. }, ElementMessage::Submitted(text)) => {
                self.action(onsubmit, ActionValue::Text(text.clone()))
            }
            (Markup::Field { onblur, .. }, ElementMessage::FocusLost(text)) => {
                self.action(onblur, ActionValue::Text(text.clone()))
            }
            (Markup::Menu { onchange, .. }, ElementMessage::MenuSelected(payload)) => {
                let chosen = self
                    .menu_items
                    .get(&index)
                    .and_then(|items| items.get(usize::try_from(*payload).ok()?))
                    .cloned();
                match chosen {
                    Some(text) => self.action(onchange, ActionValue::Text(text)),
                    None => true,
                }
            }
            (Markup::List { onchange, body, .. }, ElementMessage::Clicked { x, y }) => {
                let (w, h) = ui
                    .element(element)
                    .map_or((0.0, 0.0), |e| (e.width, e.height));
                let Some(list) = ui.widget::<MarkupList>(element) else {
                    return true;
                };
                let row = |i: usize| ActionValue::Int(i32::try_from(i).unwrap_or(i32::MAX));
                let column_handler = |c: usize, pick: fn(&ColumnKind) -> Option<&String>| match body
                {
                    ListBody::Columns(columns) => {
                        columns.get(c).and_then(|col| pick(&col.kind)).cloned()
                    }
                    ListBody::Single { .. } => None,
                };
                match list.press(w, h, *x as f32, *y as f32) {
                    ColumnPress::Select(i) => self.action(onchange, row(i)),
                    ColumnPress::TextClicked(c, i) => {
                        let handler = column_handler(c, |k| match k {
                            ColumnKind::Text { onclick, .. } => onclick.as_ref(),
                            _ => None,
                        });
                        self.action(&handler, row(i))
                    }
                    ColumnPress::CheckChanged(c, i) => {
                        let handler = column_handler(c, |k| match k {
                            ColumnKind::Check { onchange, .. } => Some(onchange),
                            _ => None,
                        });
                        self.action(&handler, row(i))
                    }
                    ColumnPress::IconClicked(c, i) => {
                        let handler = column_handler(c, |k| match k {
                            ColumnKind::Icon { onclick, .. } => Some(onclick),
                            _ => None,
                        });
                        self.action(&handler, row(i))
                    }
                    ColumnPress::Nothing => true,
                }
            }
            (
                Markup::List {
                    body: ListBody::Columns(columns),
                    ..
                },
                ElementMessage::ListButtonClicked { column, row },
            ) => {
                let handler = match columns.get(*column).map(|c| &c.kind) {
                    Some(ColumnKind::Button { onclick, .. }) => Some(onclick.clone()),
                    _ => None,
                };
                self.action(
                    &handler,
                    ActionValue::Int(i32::try_from(*row).unwrap_or(i32::MAX)),
                )
            }
            _ => false,
        }
    }
}

/// A markup field: the retail text field, with the up and down arrows
/// going to the plugin's `onup`/`ondown` handlers when it has them
/// (OpenAC `UiField.OnUp`/`OnDown`).
#[derive(Debug)]
pub struct MarkupField {
    /// The text field.
    pub inner: Field,
    panel: u32,
    onup: Option<String>,
    ondown: Option<String>,
    outbox: PluginOutbox,
}

impl crate::tree::Widget for MarkupField {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &crate::event::UiEvent) -> bool {
        if e.target == Some(me) && e.kind == crate::event::kind::KEY_DOWN {
            let handler = match e.key {
                crate::event::Key::Up => self.onup.clone(),
                crate::event::Key::Down => self.ondown.clone(),
                _ => None,
            };
            if let Some(handler) = handler {
                self.outbox.borrow_mut().push(UiInput::Action {
                    panel: self.panel,
                    handler,
                    value: ActionValue::None,
                });
                return true;
            }
        }
        self.inner.on_event(ui, me, e)
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut crate::draw::RenderContext<'_>) {
        self.inner.draw(ui, me, rc);
    }

    fn draw_after_children(
        &mut self,
        ui: &mut UiRoot,
        me: ElementId,
        rc: &mut crate::draw::RenderContext<'_>,
    ) {
        self.inner.draw_after_children(ui, me, rc);
    }

    fn tick(&mut self, ui: &mut UiRoot, me: ElementId, dt: f64) {
        self.inner.tick(ui, me, dt);
    }

    fn global_time(&mut self, ui: &mut UiRoot, me: ElementId, now: f64) {
        self.inner.global_time(ui, me, now);
    }

    fn handles_click(&self, element: &Element) -> bool {
        self.inner.handles_click(element)
    }

    fn receives_hover_mouse_move(&self) -> bool {
        self.inner.receives_hover_mouse_move()
    }

    fn scroll_model(&mut self) -> Option<&mut crate::text::Scrollable> {
        self.inner.scroll_model()
    }

    fn on_enabled_changed(&mut self, ui: &mut UiRoot, me: ElementId) {
        self.inner.on_enabled_changed(ui, me);
    }
}

#[cfg(test)]
mod list_button_tests;
#[cfg(test)]
mod tests;
