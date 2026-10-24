//! Plugin UI in the window: the session's plugin windows built from their
//! markup and kept in step with the session's copy (`ac_plugin_ui`), the
//! plugins' canvases on the overlay band, and the plugin shelf.
//!
//! Nothing here decides anything: the session owns the plugin UI state
//! (which values, which windows are shown, what a canvas shows), the
//! window draws its copy and reports what the player did into a
//! [`PluginOutbox`] the client sends back to the session.

pub mod canvas;
pub mod color;
pub mod content_size;
pub mod controls;
pub mod dialog;
pub mod flex_box;
pub mod fold;
pub mod hotkeys;
pub mod layout;
pub mod list_ext;
pub mod shelf;
pub mod title_bar;
pub mod widgets;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use ac_plugin_ui::flex::{FlexMeasurement, FlexNode, FlexSize};
use ac_plugin_ui::list::ColumnPress;
use ac_plugin_ui::markup::{
    Anchors, ColorSource, ColumnKind, EXPANDER_HEADER_HEIGHT, Element as Markup, IconKind,
    IconSource, ListBody, MarkupDocument, normalize_icon,
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

/// A transparent, borderless container.
fn transparent_panel() -> Panel {
    Panel {
        background_color: Color::TRANSPARENT,
        border_color: Color::TRANSPARENT,
        border_thickness: 0.0,
        background_sprite: 0,
    }
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
    /// Each element's context menu element, by markup element.
    menus: HashMap<usize, ElementId>,
    synced: Option<u64>,
    /// The font the window was built with.
    classic_font: Option<Arc<UiFont>>,
    /// The bundled sans font the modern themes draw text in, when loaded.
    modern_font: Option<Arc<UiFont>>,
    /// The title's colour without a theme (`controls.ini`'s `[title] color`,
    /// else white).
    title_color: Color,
    /// The title bar, on a window with one.
    title_bar: Option<ElementId>,
    /// The content area, on a window with chrome or a flex layout.
    content: Option<ElementId>,
    /// The chrome above and below the content area.
    chrome_h: f32,
    /// Menus a flex container sizes: their popup follows their width.
    flex_menus: Vec<ElementId>,
    /// The size the window had before its content grew it, and the size it
    /// grew to; cleared once the player sizes it otherwise.
    growth: Option<((f32, f32), (f32, f32))>,
}

/// Enters `id`, just built for markup element `index` under a flex
/// container, into that container's layout: a flex group is already an
/// inner node; an absolute group is a leaf of its authored size; any other
/// control is a leaf measured from its content. The container places it, so
/// it keeps no anchors.
fn add_flex_item(
    ui: &mut UiRoot,
    flex_of: &HashMap<Option<usize>, (layout::SharedFlex, Option<usize>)>,
    node: &ac_plugin_ui::markup::Node,
    index: usize,
    id: ElementId,
    attrs: &ac_plugin_ui::markup::FlexItem,
) {
    ui.set_anchors(id, AnchorEdges::NONE);
    let nested_flex = matches!(
        node.element,
        Markup::Group {
            layout: Some(_),
            ..
        }
    );
    if nested_flex {
        return;
    }
    let Some((shared, parent_item)) = flex_of.get(&node.parent) else {
        return;
    };
    let mut flex_node = FlexNode::default();
    attrs.apply(&mut flex_node);
    let authored = match &node.element {
        Markup::Group {
            scroll_x, scroll_y, ..
        } => Some(absolute_group_size(
            node.rect.w,
            node.rect.h,
            *scroll_x,
            *scroll_y,
        )),
        _ => None,
    };
    let item = shared
        .borrow_mut()
        .add(*parent_item, id, index, flex_node, false, authored);
    match &node.element {
        Markup::Expander { .. } => {
            // Its h is the expanded height, given only while it is
            // expanded; collapsed it measures as its header.
            shared.borrow_mut().set_expander(item, node.rect.h);
            if let Some(x) = ui.widget_mut::<fold::MarkupExpander>(id) {
                x.flex_placed = true;
            }
        }
        Markup::Label { .. } => {
            if let Some(l) = ui.widget_mut::<MarkupLabel>(id) {
                l.sized_by_layout = true;
            }
        }
        _ => {}
    }
}

/// An absolute group's content size in a flex container: its authored
/// size, which is also its minimum, except along an axis it scrolls, where
/// it can shrink to a 40-pixel viewport (plus the other axis's bar).
fn absolute_group_size(w: f32, h: f32, scroll_x: bool, scroll_y: bool) -> FlexMeasurement {
    use ac_plugin_ui::flex::MINIMUM_SCROLL_VIEWPORT;
    let size = FlexSize::new(w, h);
    if !scroll_x && !scroll_y {
        return FlexMeasurement::new(size, size);
    }
    let min_w = if scroll_x {
        MINIMUM_SCROLL_VIEWPORT + if scroll_y { layout::BAR_SIZE } else { 0.0 }
    } else {
        w
    };
    let min_h = if scroll_y {
        MINIMUM_SCROLL_VIEWPORT + if scroll_x { layout::BAR_SIZE } else { 0.0 }
    } else {
        h
    };
    FlexMeasurement::new(size, FlexSize::new(min_w.min(w), min_h.min(h)))
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
    build_panel_with_state(ui, view, font, outbox, None)
}

/// [`build_panel`] with the window's remembered control values: named
/// expanders, splits and lists remember what the player left them at for
/// the character.
#[allow(clippy::too_many_lines)]
pub fn build_panel_with_state(
    ui: &mut UiRoot,
    view: &PanelView,
    font: Option<Arc<UiFont>>,
    outbox: PluginOutbox,
    control_state: Option<fold::SharedControlState>,
) -> PluginPanelController {
    let doc = Arc::clone(&view.document);
    let window_name = view.window_name();
    let emit = controls::Emit {
        panel: view.id,
        outbox: Rc::clone(&outbox),
    };
    let shifts: fold::Shifts = Rc::default();
    let remembered = |control: Option<&String>, kind: &'static str| {
        fold::Remembered::new(
            control_state.clone(),
            &window_name,
            control.map(String::as_str),
            kind,
        )
    };
    let mut containers: Vec<ElementId> = Vec::with_capacity(doc.nodes.len());
    let mut menus: HashMap<usize, ElementId> = HashMap::new();
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
    // A window with chrome or a flex layout has a content area inside the
    // border (and title bar) that holds every authored child.
    let content_area = spec.has_content_area();
    let chrome_w = if content_area {
        ac_plugin_ui::chrome::HORIZONTAL_INSETS
    } else {
        0.0
    };
    let chrome_h = if content_area {
        ac_plugin_ui::chrome::vertical_insets_for(spec.title_bar)
    } else {
        0.0
    };
    let title_bar = spec.title_bar.then(|| {
        let bar = title_bar::TitleBar::add(ui, root, spec.rect.w, font.clone());
        if let Some(b) = ui.widget_mut::<title_bar::TitleBar>(bar) {
            b.title = spec
                .title
                .clone()
                .unwrap_or_else(|| view.descriptor.title.clone());
        }
        bar
    });
    // The legacy title label: only on a window without a bar or a layout.
    let title = if spec.title_bar || spec.layout.is_some() {
        None
    } else {
        spec.title.as_ref().map(|t| {
            let mut e = Element::at(8.0, 4.0, 0.0, 0.0);
            e.click_through = true;
            let mut label = MarkupLabel::new(font.clone());
            label.text.clone_from(t);
            ui.add(root, e, label)
        })
    };
    let content = content_area.then(|| {
        let mut e = Element::at(
            ac_plugin_ui::chrome::BORDER,
            ac_plugin_ui::chrome::top_inset(spec.title_bar),
            spec.rect.w - chrome_w,
            spec.rect.h - chrome_h,
        )
        .with_anchors(AnchorEdges::ALL);
        e.click_through = true;
        e.name = Some("PluginContent".to_owned());
        ui.add(root, e, layout::ContentHost::default())
    });
    let content_parent = content.unwrap_or(root);
    // Each flex root's layout, by the markup element that is its container
    // (`None`: the content area), with the item a nested container is.
    let mut flex_of: HashMap<Option<usize>, (layout::SharedFlex, Option<usize>)> = HashMap::new();
    if let (Some(container), Some(c)) = (&spec.layout, content) {
        let mut node = FlexNode::default();
        container.apply(&mut node);
        let mut layout = flex_box::FlexBox::new(Arc::clone(&doc), node);
        layout.tracks_minimum = true;
        let shared = Rc::new(RefCell::new(layout));
        if let Some(host) = ui.widget_mut::<layout::ContentHost>(c) {
            host.flex = Some(Rc::clone(&shared));
        }
        flex_of.insert(None, (shared, None));
    }
    if let Some(c) = content
        && (spec.scroll_x || spec.scroll_y)
    {
        let area = layout::ScrollArea::new(ui, c, spec.scroll_x, spec.scroll_y);
        if let Some((shared, _)) = flex_of.get(&None) {
            let mut layout = shared.borrow_mut();
            let node = layout.root_node_mut();
            node.scroll_x = spec.scroll_x;
            node.scroll_y = spec.scroll_y;
            node.scrollbar_size = layout::BAR_SIZE;
        }
        if let Some(host) = ui.widget_mut::<layout::ContentHost>(c) {
            host.scroll = Some(area);
        }
    }
    let mut flex_menus = Vec::new();
    let mut nodes = Vec::with_capacity(doc.nodes.len());
    let mut by_element = HashMap::new();
    for (index, node) in doc.nodes.iter().enumerate() {
        let parent = node.parent.map_or(content_parent, |p| containers[p]);
        let mut container = None;
        let mut e = Element::at(node.rect.x, node.rect.y, node.rect.w, node.rect.h)
            .with_anchors(anchors(node.common.anchors));
        e.name.clone_from(&node.common.name);
        let font = font.clone();
        let menu_font = font.clone();
        let id = match &node.element {
            Markup::Group {
                layout: group_layout,
                scroll_x,
                scroll_y,
                ..
            } => {
                let scrolls = *scroll_x || *scroll_y;
                // A scrolling group is hit-testable so the wheel reaches it
                // over empty space.
                e.click_through = !scrolls;
                let id = ui.add(parent, e, layout::MarkupGroup::new(transparent_panel()));
                let mut flex = None;
                if let Some(container) = group_layout {
                    let mut flex_node = FlexNode::default();
                    container.apply(&mut flex_node);
                    if scrolls {
                        flex_node.scroll_x = *scroll_x;
                        flex_node.scroll_y = *scroll_y;
                        flex_node.scrollbar_size = layout::BAR_SIZE;
                    }
                    let (shared, item) = match (&node.flex, flex_of.get(&node.parent)) {
                        // Nested in a flex container: an inner node of its
                        // root's tree.
                        (Some(attrs), Some((shared, parent_item))) => {
                            attrs.apply(&mut flex_node);
                            let item = shared.borrow_mut().add(
                                *parent_item,
                                id,
                                index,
                                flex_node,
                                true,
                                None,
                            );
                            (Rc::clone(shared), Some(item))
                        }
                        _ => (
                            Rc::new(RefCell::new(flex_box::FlexBox::new(
                                Arc::clone(&doc),
                                flex_node,
                            ))),
                            None,
                        ),
                    };
                    flex_of.insert(Some(index), (Rc::clone(&shared), item));
                    flex = Some(layout::FlexPart {
                        layout: shared,
                        item,
                    });
                }
                let scroll = scrolls.then(|| layout::ScrollArea::new(ui, id, *scroll_x, *scroll_y));
                if let Some(g) = ui.widget_mut::<layout::MarkupGroup>(id) {
                    g.flex = flex;
                    g.scroll = scroll;
                }
                id
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
                header,
                tree,
                multi,
                onchange,
                ..
            } => {
                let mut list = MarkupList::new(font, *row_height);
                list.selection_band = *selection_band;
                if let ListBody::Columns(columns) = body {
                    list.columns.clone_from(columns);
                    list.markup_columns.clone_from(columns);
                    let mut ext = list_ext::ListExt::new(
                        columns,
                        remembered(node.common.name.as_ref(), "sort"),
                        remembered(node.common.name.as_ref(), "widths"),
                        emit.clone(),
                    );
                    ext.header = header.show;
                    ext.onsort.clone_from(&header.onsort);
                    ext.resizable = header.resizable;
                    ext.onresize.clone_from(&header.onresize);
                    ext.multi = *multi;
                    if *multi {
                        ext.onchange.clone_from(onchange);
                    }
                    ext.tree = tree.as_ref().map(|t| {
                        list_ext::TreeState::new(
                            t.column,
                            t.start_collapsed,
                            t.onexpand.clone(),
                            remembered(node.common.name.as_ref(), "expanded"),
                        )
                    });
                    if *multi || tree.is_some() {
                        e = keyboard_activated(e);
                    }
                    list.ext = Some(Box::new(ext));
                }
                ui.add(parent, e, list)
            }
            Markup::Checkbox { onchange, .. } => ui.add(
                parent,
                keyboard_activated(e),
                controls::MarkupCheckbox::new(font, onchange.clone(), emit.clone()),
            ),
            Markup::Separator { .. } => {
                e.click_through = true;
                ui.add(parent, e, controls::MarkupSeparator::default())
            }
            Markup::Radio { onchange, .. } => ui.add(
                parent,
                keyboard_activated(e),
                controls::MarkupRadio::new(font, onchange.clone(), emit.clone()),
            ),
            Markup::Number {
                onchange,
                kind,
                spec: number_spec,
                ..
            } => {
                Field::configure(&mut e);
                e.tab_stop = true;
                let mut field = Field::default();
                field.font = font;
                field.one_line = true;
                field.selectable = true;
                field.select_all_on_focus = true;
                field.record_history = false;
                field.max_characters = 41;
                ui.add(
                    parent,
                    e,
                    controls::MarkupNumber::new(
                        field,
                        *number_spec,
                        *kind == ac_plugin_ui::markup::NumberKind::Int,
                        onchange.clone(),
                        emit.clone(),
                    ),
                )
            }
            Markup::Hotkey { onchange, .. } => {
                e.accepts_focus = true;
                e.focus_on_mouse_click = true;
                e.tab_stop = true;
                ui.add(
                    parent,
                    e,
                    controls::MarkupHotkey::new(font, onchange.clone(), emit.clone()),
                )
            }
            Markup::Color { .. } => {
                e.accepts_focus = true;
                e.focus_on_mouse_click = false;
                e.tab_stop = true;
                ui.add(
                    parent,
                    e,
                    color::MarkupColor::new(font, &node.element, emit.clone()),
                )
            }
            Markup::Expander { open, onchange, .. } => {
                e.click_through = true;
                let h = node.rect.h;
                // In flex without a w: start at the content width, so the
                // content's anchors are captured against a real box.
                let w = if node.flex.is_some() && node.rect.w <= 0.0 {
                    content_size::LIST_WIDTH
                } else {
                    node.rect.w
                };
                e.width = w;
                let mut header_element =
                    keyboard_activated(Element::at(0.0, 0.0, w, EXPANDER_HEADER_HEIGHT))
                        .with_anchors(AnchorEdges::LEFT | AnchorEdges::RIGHT | AnchorEdges::TOP);
                header_element.runtime_tooltip_text =
                    node.common.tooltip.as_ref().and_then(|t| t.read(view));
                let header = ui.insert(header_element, fold::ExpanderHeader::new(font));
                let mut content_element = Element::at(
                    0.0,
                    EXPANDER_HEADER_HEIGHT,
                    w,
                    (h - EXPANDER_HEADER_HEIGHT).max(0.0),
                )
                .with_anchors(
                    AnchorEdges::LEFT | AnchorEdges::TOP | AnchorEdges::RIGHT | AnchorEdges::BOTTOM,
                );
                content_element.click_through = true;
                let content = ui.insert(content_element, transparent_panel());
                let expander = fold::MarkupExpander::new(
                    header,
                    content,
                    *open,
                    remembered(node.common.name.as_ref(), "expanded"),
                    Rc::clone(&shifts),
                    onchange.clone(),
                    emit.clone(),
                );
                let id = ui.add(parent, e, expander);
                ui.add_child(id, header);
                ui.add_child(id, content);
                container = Some(content);
                id
            }
            Markup::Split {
                horizontal,
                min,
                max,
                default_position,
                at_given,
                onchange,
                ..
            } => {
                e.click_through = true;
                let mut bar_element = keyboard_activated(Element::at(0.0, 0.0, 0.0, 0.0))
                    .with_anchors(AnchorEdges::NONE)
                    .with_z_order(1);
                bar_element.captures_pointer_drag = true;
                bar_element.runtime_tooltip_text =
                    node.common.tooltip.as_ref().and_then(|t| t.read(view));
                let bar = ui.insert(bar_element, fold::SplitBar::new(*horizontal));
                let mut split = fold::MarkupSplit::new(
                    *horizontal,
                    bar,
                    remembered(node.common.name.as_ref(), "position"),
                    onchange.clone(),
                    emit.clone(),
                );
                split.min = *min;
                split.max = *max;
                split.default_position = *default_position;
                if node.flex.is_some() && (e.width <= 0.0 || e.height <= 0.0) {
                    // In flex without a w or h: start at the content size,
                    // so the panes' children capture their anchors against
                    // real panes.
                    if e.width <= 0.0 {
                        e.width = content_size::LIST_WIDTH;
                    }
                    if e.height <= 0.0 {
                        e.height = content_size::LIST_HEIGHT;
                    }
                    if !*at_given {
                        let room = if *horizontal { e.width } else { e.height }
                            - ac_plugin_ui::markup::SPLIT_BAR_SIZE;
                        split.default_position = (room.max(0.0) / 2.0).floor();
                    }
                }
                let id = ui.add(parent, e, split);
                ui.add_child(id, bar);
                id
            }
        };
        if let Some(attrs) = &node.flex {
            add_flex_item(ui, &flex_of, node, index, id, attrs);
            if matches!(node.element, Markup::Menu { .. }) {
                flex_menus.push(id);
            }
        }
        if let Some(t) = &node.common.tooltip
            && let Some(el) = ui.element_mut(id)
        {
            el.runtime_tooltip_text = t.read(view);
        }
        if let Some(menu) = &node.common.context_menu {
            let mut menu_element = Element::at(0.0, 0.0, 0.0, 0.0).with_anchors(AnchorEdges::NONE);
            menu_element.accepts_focus = true;
            menu_element.focus_on_mouse_click = true;
            menu_element.captures_pointer_drag = true;
            let menu = ui.add(
                id,
                menu_element,
                fold::MarkupContextMenu::new(
                    menu_font,
                    menu.onselect.clone(),
                    menu.onopen.clone(),
                    emit.clone(),
                ),
            );
            if let Some(el) = ui.element_mut(id) {
                el.context_menu = Some(menu);
            }
            menus.insert(index, menu);
        }
        by_element.insert(id, index);
        nodes.push(id);
        containers.push(container.unwrap_or(id));
    }
    // A split sizes its two panes: lay it out at its authored start now,
    // so the panes' children record their anchors against those sizes.
    for (index, node) in doc.nodes.iter().enumerate() {
        if let Markup::Split { .. } = node.element {
            let id = nodes[index];
            let panes: Vec<ElementId> = node.children.iter().map(|c| nodes[*c]).collect();
            for pane in &panes {
                ui.set_anchors(*pane, AnchorEdges::NONE);
            }
            ui.with::<fold::MarkupSplit, _>(id, |split, ui| {
                split.first = panes.first().copied();
                split.second = panes.get(1).copied();
                split.layout_at_default(ui, id);
            });
        }
    }
    let mut controller = PluginPanelController {
        panel: view.id,
        window_name: view.window_name(),
        root,
        document: Arc::clone(&doc),
        nodes,
        by_element,
        title,
        minimize: None,
        outbox,
        expected_visible: false,
        menu_items: HashMap::new(),
        menus,
        synced: None,
        classic_font: font.clone(),
        modern_font: None,
        title_color: Color::WHITE,
        title_bar,
        content,
        chrome_h,
        flex_menus,
        growth: None,
    };
    if let Some((shared, _)) = flex_of.get(&None).cloned() {
        // The window's first measurement, with the values and fonts known
        // now (a seeding sync: captions, and what is bound hidden, such as
        // a tabbed window's other pages): it sizes a root that states no
        // w/h, and sets the minimum, so mounting and registration see real
        // geometry. An authored size below the content's minimum opens at
        // the minimum.
        controller.sync(ui, view, 0, &mut NoMarkupIcons, PluginTheme::Classic);
        controller.synced = None;
        let measured = shared.borrow_mut().measure_now(ui);
        let (authored_w, authored_h) = spec.authored_size;
        let (authored_min_w, authored_min_h) = spec.authored_min;
        let min_w = authored_min_w.unwrap_or(measured.minimum.width);
        let min_h = authored_min_h.unwrap_or(measured.minimum.height);
        let content_w = authored_w.unwrap_or(measured.preferred.width).max(min_w);
        let content_h = authored_h.unwrap_or(measured.preferred.height).max(min_h);
        if let Some(e) = ui.element_mut(root) {
            e.width = content_w + chrome_w;
            e.height = content_h + chrome_h;
            e.min_width = ac_plugin_ui::chrome::frame_minimum_width(min_w, spec.title_bar);
            e.min_height = min_h + chrome_h;
        }
        if let Some(c) = content
            && let Some(e) = ui.element_mut(c)
        {
            e.width = content_w;
            e.height = content_h;
        }
    }
    if let Some(bar) = title_bar {
        // A window narrower than its title bar needs opens at the bar's
        // minimum.
        let (width, min_width) = ui
            .element(root)
            .map_or((0.0, 0.0), |e| (e.width, e.min_width));
        let width = if width < ac_plugin_ui::chrome::MINIMUM_BAR_FRAME_WIDTH {
            if let Some(e) = ui.element_mut(root) {
                e.width = min_width;
            }
            if let Some(c) = content
                && let Some(e) = ui.element_mut(c)
            {
                e.width = min_width - chrome_w;
            }
            min_width
        } else {
            width
        };
        title_bar::TitleBar::set_frame_width(ui, bar, width);
        if view.descriptor.show_in_side_panel
            && let Some(close) = ui.widget::<title_bar::TitleBar>(bar).map(|b| b.close)
            && let Some(e) = ui.element_mut(close)
        {
            // The bar's close button takes the shelf's minimize button's
            // place and says where the window went.
            e.runtime_tooltip_text = Some(ac_plugin_ui::chrome::DOCKED_CLOSE_TOOLTIP.to_owned());
        }
    }
    // A window with a title bar already has a close button, which does what
    // "-" would; a layout window without a bar has content right up to its
    // border, where "-" would sit over it, and is closed from its shelf
    // button, as the author chose no chrome.
    let frame_width = ui.element(root).map_or(spec.rect.w, |e| e.width);
    controller.minimize = (view.descriptor.show_in_side_panel && !content_area).then(|| {
        let mut e = Element::at((frame_width - 23.0).max(8.0), 3.0, 18.0, 17.0)
            .with_anchors(AnchorEdges::TOP | AnchorEdges::RIGHT)
            .with_z_order(1);
        e.name = Some("PluginMinimize".to_owned());
        e.runtime_tooltip_text = Some("Minimize to plugin sidepanel".to_owned());
        let button = MarkupButton {
            text: "\u{2013}".to_owned(),
            background: MINIMIZE_BACKGROUND,
            border: MINIMIZE_BORDER,
            ..MarkupButton::new(font.clone())
        };
        ui.add(root, e, button)
    });
    // The whole document now sits at its first sizes, so this is the one
    // moment every anchor margin can be read off the layout its author
    // wrote.
    ui.capture_authored_anchor_baselines(root);
    controller
}

/// The shelf's minimize button's fill in Classic.
const MINIMIZE_BACKGROUND: Color = Color::rgba(0.02, 0.02, 0.015, 0.94);
/// Its border in Classic.
const MINIMIZE_BORDER: Color = Color::rgba(0.58, 0.46, 0.17, 1.0);

/// No icons: the build's seeding sync shows no images (the first real sync
/// resolves them).
struct NoMarkupIcons;

impl MarkupIcons for NoMarkupIcons {
    fn surface(&mut self, _id: u32) -> Option<Icon> {
        None
    }
    fn spell(&mut self, _spell: u32) -> Option<Icon> {
        None
    }
    fn item(&mut self, _object: u32) -> Option<Icon> {
        None
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

    /// The title bar, on a window with one.
    #[must_use]
    pub fn title_bar(&self) -> Option<ElementId> {
        self.title_bar
    }

    /// The content area, on a window with chrome or a flex layout.
    #[must_use]
    pub fn content_root(&self) -> Option<ElementId> {
        self.content
    }

    /// The revision the window's saved geometry is kept under.
    #[must_use]
    pub fn geometry_revision(&self) -> i32 {
        self.document.panel.geometry_revision
    }

    /// What the build ignored, one line each.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.document.warnings
    }

    /// Keeps the frame's minimum on the content's: when a flex content area
    /// finds its content needs more room (a longer caption), the frame's
    /// minimum becomes that plus the chrome, unless the markup states its
    /// own `minw`/`minh`, and the window grows to it. When the content needs
    /// less again, a window it grew goes back toward the size it had
    /// before, unless it was sized otherwise in between.
    fn track_content_minimum(&mut self, ui: &mut UiRoot) {
        let Some(content) = self.content else {
            return;
        };
        let Some(minimum) = ui
            .widget::<layout::ContentHost>(content)
            .and_then(|h| h.flex.as_ref().map(Rc::clone))
            .and_then(|f| f.borrow_mut().take_minimum_changed())
        else {
            return;
        };
        let spec = &self.document.panel;
        let (authored_min_w, authored_min_h) = spec.authored_min;
        let title_bar = self.title_bar.is_some();
        let Some((current, min)) = ui.element_mut(self.root).map(|e| {
            e.min_width = ac_plugin_ui::chrome::frame_minimum_width(
                authored_min_w.unwrap_or(minimum.width),
                title_bar,
            );
            e.min_height = authored_min_h.unwrap_or(minimum.height) + self.chrome_h;
            ((e.width, e.height), (e.min_width, e.min_height))
        }) else {
            return;
        };
        if let Some((_, grown_to)) = self.growth
            && current != grown_to
        {
            self.growth = None;
        }
        let from = self.growth.map_or(current, |(before, _)| before);
        if ui.window(&self.window_name).is_some() {
            ui.enforce_minimum_size(&self.window_name, from.0, from.1);
        } else if let Some(e) = ui.element_mut(self.root) {
            e.width = from.0.max(min.0);
            e.height = from.1.max(min.1);
        }
        let now = ui.element(self.root).map_or(from, |e| (e.width, e.height));
        self.growth = (now != from).then_some((from, now));
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
        if let Some(bar) = self.title_bar {
            // The theme's font and text colour without outline under a
            // theme; Classic's own font, title colour and outline back
            // under Classic. The themed close button's "x" takes the
            // theme's font.
            let font = text_font
                .cloned()
                .unwrap_or_else(|| self.classic_font.clone());
            title_bar::TitleBar::set_palette(ui, bar, palette, font, self.title_color);
        }
        if let Some(content) = self.content
            && let Some(area) = ui
                .widget::<layout::ContentHost>(content)
                .and_then(|h| h.scroll.clone())
        {
            area.apply_theme(ui, p);
        }
        if let Some(minimize) = self.minimize
            && let Some(b) = ui.widget_mut::<MarkupButton>(minimize)
        {
            // The shelf's "-" follows the window's theme as the bar's close
            // button does: a flat face, edge and caption in the palette.
            b.background = p.map_or(MINIMIZE_BACKGROUND, |p| p.field);
            b.border = p.map_or(MINIMIZE_BORDER, |p| p.border);
            b.text_color = p.map_or(Color::WHITE, |p| p.text);
            b.outline = p.is_none();
            if let Some(f) = text_font {
                b.font.clone_from(f);
            }
        }
        for (index, node) in doc.nodes.iter().enumerate() {
            let el = self.nodes[index];
            if let (Some(menu), Some(&menu_el)) =
                (&node.common.context_menu, self.menus.get(&index))
            {
                let items = menu.items.read(view);
                let enabled = menu
                    .enabled
                    .as_ref()
                    .map(|e| bools(view, e))
                    .unwrap_or_default();
                if let Some(w) = ui.widget_mut::<fold::MarkupContextMenu>(menu_el) {
                    w.items = items;
                    w.enabled = enabled;
                    w.palette = palette;
                    if let Some(f) = text_font {
                        w.font.clone_from(f);
                    }
                }
            }
            ui.set_visible(el, view.node_visible(index));
            ui.set_enabled(el, node.common.enabled.read(view, true));
            if let Some(t) = &node.common.tooltip
                && let Some(e) = ui.element_mut(el)
            {
                e.runtime_tooltip_text = t.read(view);
            }
            let label = view.label_overrides.get(&index).cloned();
            match &node.element {
                Markup::Group {
                    background, border, ..
                } => {
                    let mut bars = None;
                    if let Some(g) = ui.widget_mut::<layout::MarkupGroup>(el) {
                        let w = &mut g.panel;
                        w.background_color = background
                            .as_ref()
                            .map_or(Color::TRANSPARENT, |c| read_color(c, view, p));
                        w.border_color = border
                            .as_ref()
                            .map_or(Color::TRANSPARENT, |c| read_color(c, view, p));
                        w.border_thickness = if border.is_some() { 1.0 } else { 0.0 };
                        bars.clone_from(&g.scroll);
                    }
                    if let Some(area) = bars {
                        area.apply_theme(ui, p);
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
                Markup::List {
                    selected,
                    body,
                    header,
                    tree,
                    multi,
                    ..
                } => {
                    let selected_row = if *multi {
                        -1
                    } else {
                        int_value(view, selected)
                    };
                    let (rows, column_rows, row_icons) = list_rows(view, body, icons, p);
                    let floats = |name: &str| match view.values.get(name) {
                        Some(UiValue::FloatList(v)) => Some(v.clone()),
                        _ => None,
                    };
                    let ints = |name: &str| match view.values.get(name) {
                        Some(UiValue::IntList(v)) => v.clone(),
                        _ => Vec::new(),
                    };
                    if let Some(w) = ui.widget_mut::<MarkupList>(el) {
                        if let (Some(ext), ListBody::Columns(columns)) = (w.ext.as_mut(), body) {
                            ext.bound_sort = header
                                .sort
                                .as_ref()
                                .map(|(c, d)| (int_value(view, c), bool_value(view, d)));
                            ext.bound_widths = header
                                .widths
                                .as_deref()
                                .map(|n| floats(n).unwrap_or_default());
                            for (i, column) in columns.iter().enumerate() {
                                ext.captions[i] = match &column.header {
                                    ac_plugin_ui::markup::ColumnHeader::Text(t) => t.read(view),
                                    _ => None,
                                };
                                ext.number_keys[i] = None;
                                ext.text_keys[i] = None;
                                match &column.sort_keys {
                                    Some(ac_plugin_ui::markup::SortKeysSource::Numbers(n)) => {
                                        ext.number_keys[i] = Some(floats(n).unwrap_or_default());
                                    }
                                    Some(ac_plugin_ui::markup::SortKeysSource::Texts(n)) => {
                                        ext.text_keys[i] = Some(texts(view, n));
                                    }
                                    None => {}
                                }
                            }
                            if *multi {
                                ext.selected_rows = ints(selected);
                            }
                            if let (Some(state), Some(spec)) = (ext.tree.as_mut(), tree) {
                                state.levels = ints(&spec.levels);
                                state.keys = texts(view, &spec.keys);
                                state.bound = spec
                                    .expanded
                                    .as_deref()
                                    .map(|n| texts(view, n).into_iter().collect());
                            }
                            w.all_rows = rows;
                            w.row_icons = row_icons;
                        } else {
                            w.rows = rows;
                            w.column_rows = column_rows;
                            w.row_icons = row_icons;
                        }
                        w.selected = selected_row;
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                        w.arrange();
                    }
                }
                Markup::Checkbox {
                    text,
                    checked,
                    color: c,
                    ..
                } => {
                    let s = label.or_else(|| text.read(view)).unwrap_or_default();
                    let state = match view.values.get(checked) {
                        Some(UiValue::Bool(b)) => Some(*b),
                        Some(UiValue::OptionalBool(b)) => *b,
                        _ => Some(false),
                    };
                    let col = match c {
                        Some(c) => read_color(c, view, p),
                        None => p.map_or(controls::CLASSIC_CAPTION, |p| p.text),
                    };
                    if let Some(w) = ui.widget_mut::<controls::MarkupCheckbox>(el) {
                        w.text = s;
                        w.state = state;
                        w.color = col;
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                    }
                }
                Markup::Separator { color: c } => {
                    let col = match c {
                        Some(c) => read_color(c, view, p),
                        None => p.map_or(controls::UNLIT_INNER, |p| p.border),
                    };
                    if let Some(w) = ui.widget_mut::<controls::MarkupSeparator>(el) {
                        w.color = col;
                    }
                }
                Markup::Radio {
                    items,
                    selected,
                    horizontal,
                    segmented,
                    color: c,
                    ..
                } => {
                    let entries = items.read(view);
                    let chosen = int_value(view, selected);
                    let col = match c {
                        Some(c) => read_color(c, view, p),
                        None => p.map_or(controls::CLASSIC_CAPTION, |p| p.text),
                    };
                    if let Some(w) = ui.widget_mut::<controls::MarkupRadio>(el) {
                        w.items = entries;
                        w.selected = chosen;
                        w.horizontal = *horizontal;
                        w.segmented = *segmented;
                        w.color = col;
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                    }
                }
                Markup::Number {
                    value,
                    background,
                    color: c,
                    default_background,
                    default_color,
                    ..
                } => {
                    let current = match view.values.get(value) {
                        Some(UiValue::Int(v)) => ac_plugin_ui::number::Dec::from_i64(i64::from(*v)),
                        Some(UiValue::Float(v)) => ac_plugin_ui::number::Dec::from_f32(*v),
                        _ => ac_plugin_ui::number::Dec::ZERO,
                    };
                    let bg = match (p, default_background) {
                        (Some(p), true) => p.field,
                        _ => read_color(background, view, p),
                    };
                    let col = match (p, default_color) {
                        (Some(p), true) => p.text,
                        _ => read_color(c, view, p),
                    };
                    if let Some(w) = ui.widget_mut::<controls::MarkupNumber>(el) {
                        w.set_current(current);
                        w.inner.background_color = bg;
                        w.text_color = col;
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.inner.font.clone_from(f);
                        }
                    }
                }
                Markup::Hotkey { value, .. } => {
                    let text = match view.values.get(value) {
                        Some(UiValue::Text(t)) => t.clone(),
                        _ => String::new(),
                    };
                    if let Some(w) = ui.widget_mut::<controls::MarkupHotkey>(el) {
                        w.value = text;
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                    }
                }
                Markup::Color { value, kind, .. } => {
                    let argb = color::MarkupColor::read(*kind, view.values.get(value));
                    if let Some(w) = ui.widget_mut::<color::MarkupColor>(el) {
                        w.value = argb;
                        w.palette = palette;
                        if let Some(f) = text_font {
                            w.font.clone_from(f);
                        }
                    }
                }
                Markup::Expander {
                    text,
                    expanded,
                    color: c,
                    ..
                } => {
                    let s = label.or_else(|| text.read(view)).unwrap_or_default();
                    let col = match c {
                        Some(c) => read_color(c, view, p),
                        None => p.map_or(controls::CLASSIC_CAPTION, |p| p.text),
                    };
                    let bound = expanded.as_ref().map(|e| bool_value(view, e));
                    let header = ui.widget_mut::<fold::MarkupExpander>(el).map(|w| {
                        w.bound = bound;
                        w.header
                    });
                    if let Some(h) = header.and_then(|h| ui.widget_mut::<fold::ExpanderHeader>(h)) {
                        h.text = s;
                        h.color = col;
                        h.palette = palette;
                        if let Some(f) = text_font {
                            h.font.clone_from(f);
                        }
                    }
                }
                Markup::Split { position, .. } => {
                    let bound = position.as_ref().map(|name| match view.values.get(name) {
                        Some(UiValue::Float(v)) => *v,
                        _ => 0.0,
                    });
                    let bar = ui.widget_mut::<fold::MarkupSplit>(el).map(|w| {
                        w.bound = bound;
                        w.bar
                    });
                    if let Some(b) = bar.and_then(|b| ui.widget_mut::<fold::SplitBar>(b)) {
                        b.palette = palette;
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
    fn tick(&mut self, ui: &mut UiRoot, _dt: f64) {
        self.track_content_minimum(ui);
        // A flex menu has no authored w (the container sizes it): its popup
        // column follows the laid-out menu.
        for &menu in &self.flex_menus {
            let width = ui.element(menu).map_or(0.0, |e| e.width);
            if let Some(m) = ui.widget_mut::<Menu>(menu) {
                m.column_width = width.max(20.0);
            }
        }
    }

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
        if let Some(bar) = self.title_bar
            && ui
                .widget::<title_bar::TitleBar>(bar)
                .is_some_and(|b| b.close == element)
        {
            if matches!(message, ElementMessage::Clicked { .. }) {
                ui.close_window(&self.window_name);
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
                let row = |i: usize| {
                    ActionValue::Int(i32::try_from(list.plugin_row(i)).unwrap_or(i32::MAX))
                };
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
                let plugin = ui
                    .widget::<MarkupList>(element)
                    .map_or(*row, |l| l.plugin_row(*row));
                self.action(
                    &handler,
                    ActionValue::Int(i32::try_from(plugin).unwrap_or(i32::MAX)),
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
mod flex_tests;
#[cfg(test)]
mod list_button_tests;
#[cfg(test)]
mod tests;
