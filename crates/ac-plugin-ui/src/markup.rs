//! Plugin panel markup: OpenAC's XML vocabulary (`panel`, `group`, `label`,
//! `button`, `icon`, `meter`, `tab`, `toggle`, `slider`, `field`, `menu`,
//! `log`, `list` with `column`) read into a [`MarkupDocument`].
//!
//! Every `{Binding}` is checked against the plugin's declared
//! [`BindingSchema`] when the panel is registered, with the messages
//! OpenAC's markup builder gives when a C# property does not resolve, so a
//! plugin finds a broken panel at registration, with or without a window.
//! What a binding means for the drawing (read every frame, silent when
//! unresolved, ...) is kept in the document's sources ([`TextSource`],
//! [`BoolSource`], ...), which a window reads against the panel's pushed
//! values.

use std::fmt;

use roxmltree::Node as XmlNode;

use crate::value::{
    BindingKind, BindingSchema, Rgba, UiValue, bound_color, parse_bool, parse_color, parse_float,
    parse_hex, parse_int,
};

/// Why markup cannot become a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkupError(pub String);

impl fmt::Display for MarkupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MarkupError {}

fn err<T>(message: impl Into<String>) -> Result<T, MarkupError> {
    Err(MarkupError(message.into()))
}

/// Which edges of its parent an element keeps its distance to when the
/// parent resizes (bit set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Anchors(pub u8);

impl Anchors {
    /// No edge.
    pub const NONE: Self = Self(0);
    /// The left edge.
    pub const LEFT: Self = Self(1);
    /// The top edge.
    pub const TOP: Self = Self(2);
    /// The right edge.
    pub const RIGHT: Self = Self(4);
    /// The bottom edge.
    pub const BOTTOM: Self = Self(8);
    /// The default: left and top.
    pub const TOP_LEFT: Self = Self(1 | 2);

    /// Both sets of edges.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// True when every edge of `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// A rectangle in its parent, pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

/// A text attribute: absent, literal, or a declared binding read as text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TextSource {
    /// No attribute.
    #[default]
    None,
    /// The attribute's text (also a `{Name}` the plugin did not declare,
    /// which shows as written).
    Literal(String),
    /// A declared binding; its value as text, nothing while unset.
    Bound(String),
}

impl TextSource {
    /// The text now, given the panel's values.
    #[must_use]
    pub fn read(&self, values: &dyn Values) -> Option<String> {
        match self {
            Self::None => None,
            Self::Literal(text) => Some(text.clone()),
            Self::Bound(name) => values.value(name).map(UiValue::display),
        }
    }
}

/// A `visible` or `enabled` attribute.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BoolSource {
    /// No attribute, or a literal that is not `true`/`false`: the element's
    /// default stays.
    #[default]
    Unset,
    /// `true` or `false`.
    Literal(bool),
    /// A declared `bool` binding: true only while it is `true`.
    Bound(String),
}

impl BoolSource {
    /// The flag now, `default` when unset.
    #[must_use]
    pub fn read(&self, values: &dyn Values, default: bool) -> bool {
        match self {
            Self::Unset => default,
            Self::Literal(v) => *v,
            Self::Bound(name) => matches!(values.value(name), Some(UiValue::Bool(true))),
        }
    }
}

/// A number attribute bound to a value (a meter's fill, a slider's value).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NumberSource {
    /// No attribute or no such binding: reads `missing`.
    #[default]
    Missing,
    /// A declared binding: its value converted, nothing while unset.
    Bound(String),
}

impl NumberSource {
    /// A float: `Some(0.0)` when missing (OpenAC's fill of an unknown
    /// property), `None` while the bound value is unset.
    #[must_use]
    pub fn read_f32(&self, values: &dyn Values) -> Option<f32> {
        match self {
            Self::Missing => Some(0.0),
            Self::Bound(name) => values.value(name).and_then(UiValue::to_f32),
        }
    }

    /// A uint: `None` when missing or unset.
    #[must_use]
    pub fn read_u32(&self, values: &dyn Values) -> Option<u32> {
        match self {
            Self::Missing => None,
            Self::Bound(name) => values.value(name).and_then(UiValue::to_u32),
        }
    }
}

/// A colour attribute.
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSource {
    /// A literal (`#AARRGGBB`, white when unreadable, or the element's
    /// default when the attribute is absent).
    Literal(Rgba),
    /// A declared binding read as a colour.
    Bound(String),
    /// `theme:token|#fallback`: the plugin theme's colour on a themed
    /// panel, the fallback otherwise.
    Theme {
        /// The theme colour's name (`text`, `muted`, `field`, `border`,
        /// `accent`, `background`).
        token: String,
        /// The colour when no theme applies.
        classic: Rgba,
    },
}

impl ColorSource {
    /// The colour now (a theme colour reads its classic fallback; the
    /// window picks the theme's when one applies).
    #[must_use]
    pub fn read(&self, values: &dyn Values) -> Rgba {
        match self {
            Self::Literal(c) => *c,
            Self::Bound(name) => bound_color(values.value(name)),
            Self::Theme { classic, .. } => *classic,
        }
    }
}

/// A `uint` attribute that may be a literal or a binding (an icon id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UintSource {
    /// A number written in the markup.
    Literal(u32),
    /// A declared binding converted to `uint`; 0 while unset or when it
    /// does not convert.
    Bound(String),
}

impl UintSource {
    /// The number now.
    #[must_use]
    pub fn read(&self, values: &dyn Values) -> u32 {
        match self {
            Self::Literal(v) => *v,
            Self::Bound(name) => values.value(name).and_then(UiValue::to_u32).unwrap_or(0),
        }
    }
}

/// What an icon id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IconKind {
    /// A dat image (a render surface, a bare index gets `0x06000000`).
    #[default]
    Did,
    /// A spell's icon.
    Spell,
    /// A live object's composed icon.
    Item,
}

/// Turns a bare image index into the full render-surface id: below
/// `0x01000000` the image block `0x06000000` is added; 0 stays 0.
#[must_use]
pub const fn normalize_icon(id_or_index: u32) -> u32 {
    if id_or_index == 0 {
        0
    } else if id_or_index < 0x0100_0000 {
        0x0600_0000 + id_or_index
    } else {
        id_or_index
    }
}

/// An icon: its kind and its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconSource {
    /// What the id names.
    pub kind: IconKind,
    /// The id.
    pub id: UintSource,
}

/// One column of a `list`.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    /// Width in pixels (unused when `auto`).
    pub width: f32,
    /// Shares the room the fixed columns leave (`width="*"`; the last
    /// column always does).
    pub auto: bool,
    /// What the column shows.
    pub kind: ColumnKind,
}

/// What a column shows.
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnKind {
    /// Text per row.
    Text {
        /// The rows' text (a string-list binding).
        items: String,
        /// Per-row `0xRRGGBB` colours (a uint-list binding).
        colors: Option<String>,
        /// A row clicked in this column (an `Action<int>` binding); without
        /// one a click selects the row.
        onclick: Option<String>,
    },
    /// A check box per row.
    Check {
        /// The rows' states (a bool-list binding).
        values: String,
        /// A row's box clicked (an `Action<int>` binding).
        onchange: String,
    },
    /// An icon per row.
    Icon {
        /// The rows' icon ids (a uint-list binding).
        values: String,
        /// What the ids name.
        kind: IconKind,
        /// A row's icon clicked (an `Action<int>` binding).
        onclick: String,
    },
    /// A button per row, with the row's caption on it; an empty caption
    /// draws no button.
    Button {
        /// The rows' captions (a string-list binding).
        items: String,
        /// A row's button clicked (an `Action<int>` binding); the click
        /// does not select the row.
        onclick: String,
        /// The button's width inside its cell, pixels; 0 fills the cell.
        /// Never larger than the cell.
        button_width: f32,
        /// The button's height inside its cell, pixels; 0 fills the cell.
        button_height: f32,
    },
}

/// A list's rows: one column of text (with optional colours and icons), or
/// columns.
#[derive(Debug, Clone, PartialEq)]
pub enum ListBody {
    /// The single-column form.
    Single {
        /// The rows (a string-list binding).
        items: String,
        /// Per-row colours (a uint-list binding).
        colors: Option<String>,
        /// Per-row icons (a uint-list binding) and their kind.
        icons: Option<(String, IconKind)>,
    },
    /// `<column>` children.
    Columns(Vec<Column>),
}

/// An element and its own attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    /// A container.
    Group {
        /// Fill (transparent when absent).
        background: Option<ColorSource>,
        /// One-pixel border (none when absent).
        border: Option<ColorSource>,
    },
    /// A bar filled to a share.
    Meter {
        /// The share, 0..=1.
        fill: NumberSource,
        /// The current amount (the caption is `cur/max`).
        cur: NumberSource,
        /// The maximum.
        max: NumberSource,
        /// The bar colour.
        color: ColorSource,
        /// Sprite ids: back left, back tile, back right, front left, front
        /// tile, front right (0: none).
        sprites: [u32; 6],
    },
    /// Text.
    Label {
        /// The text.
        text: TextSource,
        /// The colour (`None`: the label's default).
        color: Option<ColorSource>,
    },
    /// A push button.
    Button {
        /// The caption.
        text: TextSource,
        /// The click handler.
        onclick: Option<String>,
        /// Caption colour.
        color: Option<ColorSource>,
        /// Fill.
        background: Option<ColorSource>,
        /// Border.
        border: Option<ColorSource>,
        /// An icon drawn on it.
        icon: Option<IconSource>,
    },
    /// An image.
    Icon {
        /// What it shows.
        icon: IconSource,
    },
    /// A tab header.
    Tab {
        /// The caption (literal).
        text: String,
        /// Whether it is the selected tab (a bool binding).
        selected: String,
        /// The click handler.
        onclick: Option<String>,
        /// An icon drawn on it (dimmed while the tab is not selected).
        icon: Option<IconSource>,
    },
    /// A check box with a caption.
    Toggle {
        /// The caption.
        text: TextSource,
        /// Whether it is checked (a bool binding).
        checked: String,
        /// The click handler.
        onclick: Option<String>,
        /// Caption colour.
        color: Option<ColorSource>,
    },
    /// A value slider.
    Slider {
        /// The value at the left end.
        min: f32,
        /// The value at the right end.
        max: f32,
        /// The value shown.
        value: NumberSource,
        /// The value-changed handler (`Action<float>`).
        onchange: Option<String>,
        /// Retail scrollbar art (`style="retail"`) instead of the plain look.
        retail_art: bool,
    },
    /// A text entry.
    Field {
        /// The text shown while nobody types in it.
        text: TextSource,
        /// Every change typed (`Action<string>`).
        onchange: Option<String>,
        /// Enter (`Action<string>`).
        onsubmit: Option<String>,
        /// Keyboard focus lost, with the text then (`Action<string>`): a
        /// click elsewhere, Escape, Tab, the panel hiding or closing, or an
        /// Enter that `onsubmit` (or `clearonsubmit`) did not already take.
        onblur: Option<String>,
        /// The up arrow (`Action`).
        onup: Option<String>,
        /// The down arrow (`Action`).
        ondown: Option<String>,
        /// One line (default) or wrapped lines.
        one_line: bool,
        /// Typing allowed (default).
        editable: bool,
        /// Fill.
        background: ColorSource,
        /// Text colour.
        color: ColorSource,
        /// The background attribute was absent (a theme may restyle it).
        default_background: bool,
        /// The color attribute was absent.
        default_color: bool,
        /// The most characters.
        max_length: i32,
        /// Empty after Enter.
        clear_on_submit: bool,
    },
    /// A drop-down menu.
    Menu {
        /// The entries (a string-list binding).
        items: String,
        /// The entry shown on the button.
        selected: TextSource,
        /// An entry picked (`Action<string>`).
        onchange: Option<String>,
        /// Typing filters the entries.
        searchable: bool,
        /// Rows per popup column.
        rows: i32,
        /// Popup row height.
        row_height: f32,
        /// The popup opens above the button.
        open_upward: bool,
        /// Retail button art (`style="retail"`).
        retail_art: bool,
    },
    /// A wrapped transcript.
    Log {
        /// The entries (a string-list binding).
        items: String,
        /// The number of the first entry (an int binding), for keeping the
        /// reader's place when old entries drop off.
        first_index: Option<String>,
    },
    /// A list of rows.
    List {
        /// The selected row (an int binding; -1 none).
        selected: String,
        /// A row selected (`Action<int>`).
        onchange: Option<String>,
        /// Row height.
        row_height: f32,
        /// Draw a band behind the selected row.
        selection_band: bool,
        /// The rows.
        body: ListBody,
    },
}

impl Element {
    /// The markup element's name.
    #[must_use]
    pub const fn tag(&self) -> &'static str {
        match self {
            Self::Group { .. } => "group",
            Self::Meter { .. } => "meter",
            Self::Label { .. } => "label",
            Self::Button { .. } => "button",
            Self::Icon { .. } => "icon",
            Self::Tab { .. } => "tab",
            Self::Toggle { .. } => "toggle",
            Self::Slider { .. } => "slider",
            Self::Field { .. } => "field",
            Self::Menu { .. } => "menu",
            Self::Log { .. } => "log",
            Self::List { .. } => "list",
        }
    }

    /// Whether `SetControlLabel` reaches it (buttons, tabs, toggles and
    /// labels carry text).
    #[must_use]
    pub const fn carries_text(&self) -> bool {
        matches!(
            self,
            Self::Button { .. } | Self::Tab { .. } | Self::Toggle { .. } | Self::Label { .. }
        )
    }
}

/// What every element has.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Common {
    /// Its control name (`name`, else `id`).
    pub name: Option<String>,
    /// The parent edges it follows.
    pub anchors: Anchors,
    /// Shown.
    pub visible: BoolSource,
    /// Takes input.
    pub enabled: BoolSource,
    /// Tooltip text.
    pub tooltip: Option<TextSource>,
    /// The `color` attribute was absent (a theme may restyle it).
    pub default_color: bool,
    /// The `background` attribute was absent.
    pub default_background: bool,
    /// The `border` attribute was absent.
    pub default_border: bool,
}

/// One element in the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// What it is.
    pub element: Element,
    /// Its common attributes.
    pub common: Common,
    /// Where it sits in its parent.
    pub rect: Rect,
    /// Its parent (`None`: the panel).
    pub parent: Option<usize>,
    /// Its children (groups only), in order.
    pub children: Vec<usize>,
}

/// The `<panel>` root's own attributes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PanelSpec {
    /// Where it opens, pixels.
    pub rect: Rect,
    /// Resizable from its border.
    pub resizable: bool,
    /// Smallest width (default: its width).
    pub min_width: f32,
    /// Smallest height (default: its height).
    pub min_height: f32,
    /// Width may change.
    pub resize_x: bool,
    /// Height may change.
    pub resize_y: bool,
    /// A bool binding the window is shown only while true.
    pub visible: Option<String>,
    /// The caption drawn at (8, 4).
    pub title: Option<String>,
    /// `theme="plugin"`: follows the player's plugin theme.
    pub themed: bool,
}

/// A parsed, checked panel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarkupDocument {
    /// The root's attributes.
    pub panel: PanelSpec,
    /// Every element, parents before children (document order).
    pub nodes: Vec<Node>,
    /// The root's children.
    pub roots: Vec<usize>,
}

impl MarkupDocument {
    /// The first element (document order) named `name`.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.common.name.as_deref() == Some(name))
    }
}

/// The panel's values, as the document's sources read them.
pub trait Values {
    /// The value pushed for `name`, if any.
    fn value(&self, name: &str) -> Option<&UiValue>;
}

impl Values for std::collections::BTreeMap<String, UiValue> {
    fn value(&self, name: &str) -> Option<&UiValue> {
        self.get(name)
    }
}

/// No values (reads every binding as unset).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoValues;

impl Values for NoValues {
    fn value(&self, _name: &str) -> Option<&UiValue> {
        None
    }
}

/// The plugin theme's colour names a `theme:` colour may use.
pub const THEME_TOKENS: [&str; 6] = ["text", "muted", "field", "border", "accent", "background"];

/// What separates an anchor attribute's edge names.
const ANCHOR_SEPARATORS: [char; 5] = [',', ' ', '\t', '\r', '\n'];

/// Reads `xml` as a panel whose bindings are `schema`.
///
/// # Errors
/// Malformed XML, an unknown element, a malformed attribute, or a binding
/// that does not resolve where the markup requires it to, with OpenAC's
/// message for each.
pub fn parse(xml: &str, schema: &BindingSchema) -> Result<MarkupDocument, MarkupError> {
    let document = roxmltree::Document::parse(xml).map_err(|e| MarkupError(e.to_string()))?;
    let root = document.root_element();
    let name = root.tag_name().name();
    if name != "panel" {
        return err(format!("root must be <panel>, got <{name}>"));
    }
    let mut parser = Parser {
        schema,
        doc: MarkupDocument::default(),
    };
    let resizable = b(root, "resizable", false);
    let rect = Rect {
        x: f(root, "x"),
        y: f(root, "y"),
        w: f(root, "w"),
        h: f(root, "h"),
    };
    let mut panel = PanelSpec {
        rect,
        resizable,
        min_width: f_or(root, "minw", rect.w),
        min_height: f_or(root, "minh", rect.h),
        resize_x: resizable,
        resize_y: resizable,
        themed: attr(root, "theme") == Some("plugin"),
        ..PanelSpec::default()
    };
    if let Some(resize) = attr(root, "resize") {
        panel.resize_x = matches!(resize, "x" | "both");
        panel.resize_y = matches!(resize, "y" | "both");
    }
    if let Some(visible) = attr(root, "visible")
        && is_binding(visible)
    {
        let name = inner(visible);
        if schema.kind(name) != Some(BindingKind::Bool) {
            return err(format!(
                "<panel visible=\"{visible}\"> did not resolve to a bool property on {}",
                schema.type_name
            ));
        }
        panel.visible = Some(name.to_owned());
    }
    panel.title = attr(root, "title")
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    parser.doc.panel = panel;
    for child in root.children().filter(XmlNode::is_element) {
        let index = parser.element(child, None)?;
        parser.doc.roots.push(index);
    }
    Ok(parser.doc)
}

struct Parser<'a> {
    schema: &'a BindingSchema,
    doc: MarkupDocument,
}

fn attr<'a>(el: XmlNode<'a, '_>, name: &str) -> Option<&'a str> {
    el.attribute(name)
}

fn f(el: XmlNode<'_, '_>, name: &str) -> f32 {
    attr(el, name).and_then(parse_float).unwrap_or(0.0)
}

fn f_or(el: XmlNode<'_, '_>, name: &str, fallback: f32) -> f32 {
    attr(el, name).and_then(parse_float).unwrap_or(fallback)
}

fn i(el: XmlNode<'_, '_>, name: &str, fallback: i32) -> i32 {
    attr(el, name).and_then(parse_int).unwrap_or(fallback)
}

fn b(el: XmlNode<'_, '_>, name: &str, fallback: bool) -> bool {
    attr(el, name).and_then(parse_bool).unwrap_or(fallback)
}

fn rect(el: XmlNode<'_, '_>) -> Rect {
    Rect {
        x: f(el, "x"),
        y: f(el, "y"),
        w: f(el, "w"),
        h: f(el, "h"),
    }
}

/// `{Name}`: longer than two characters, braced.
#[must_use]
pub fn is_binding(value: &str) -> bool {
    value.len() > 2 && value.starts_with('{') && value.ends_with('}')
}

fn inner(binding: &str) -> &str {
    &binding[1..binding.len() - 1]
}

/// `.NET` formatting of a float in a message.
fn num(v: f32) -> String {
    crate::value::net_float(v)
}

fn element_identity(el: XmlNode<'_, '_>) -> String {
    let tag = el.tag_name().name();
    match attr(el, "name").or_else(|| attr(el, "id")) {
        Some(name) => format!("<{tag} name=\"{name}\">"),
        None => format!("<{tag}>"),
    }
}

fn validate_icon_kind(kind: Option<&str>, context: &str) -> Result<IconKind, MarkupError> {
    match kind.unwrap_or("did") {
        "did" => Ok(IconKind::Did),
        "spell" => Ok(IconKind::Spell),
        "item" => Ok(IconKind::Item),
        other => err(format!(
            "{context} must be did, spell, or item (got \"{other}\")"
        )),
    }
}

fn validate_art_style(element: &str, style: Option<&str>) -> Result<bool, MarkupError> {
    match style {
        None | Some("plain") => Ok(false),
        Some("retail") => Ok(true),
        Some(other) => err(format!(
            "<{element} style=\"{other}\"> must be plain or retail"
        )),
    }
}

fn parse_uint_literal(text: &str, context: &str) -> Result<u32, MarkupError> {
    let trimmed = text.trim();
    let parsed = if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse::<u32>().ok()
    };
    parsed.ok_or_else(|| MarkupError(format!("{context}=\"{text}\" is not a valid uint literal")))
}

impl Parser<'_> {
    fn kind(&self, name: &str) -> Option<BindingKind> {
        self.schema.kind(name)
    }

    fn type_name(&self) -> &str {
        &self.schema.type_name
    }

    /// OpenAC `BindString`: absent → none; literal or undeclared → the text
    /// as written; declared → bound.
    fn text(&self, attribute: Option<&str>) -> TextSource {
        match attribute {
            None => TextSource::None,
            Some(a) if is_binding(a) && self.kind(inner(a)).is_some() => {
                TextSource::Bound(inner(a).to_owned())
            }
            Some(a) => TextSource::Literal(a.to_owned()),
        }
    }

    /// A handler attribute: `Ok(None)` when absent; the name when it
    /// resolves to `kind`; OpenAC's error otherwise.
    fn handler(
        &self,
        attribute: Option<&str>,
        kind: BindingKind,
        message: impl FnOnce(&str) -> String,
    ) -> Result<Option<String>, MarkupError> {
        let Some(a) = attribute else {
            return Ok(None);
        };
        if is_binding(a) && self.kind(inner(a)) == Some(kind) {
            return Ok(Some(inner(a).to_owned()));
        }
        err(message(a))
    }

    /// A button-like control's optional `icon`/`iconkind` pair, both
    /// checked at build time.
    fn button_icon(
        &self,
        el: XmlNode<'_, '_>,
        context: &str,
    ) -> Result<Option<IconSource>, MarkupError> {
        let Some(icon) = attr(el, "icon") else {
            return Ok(None);
        };
        let kind = validate_icon_kind(attr(el, "iconkind"), "iconkind")?;
        let id = self.uint_literal_or_binding(icon, context)?;
        Ok(Some(IconSource { kind, id }))
    }

    fn action_message(
        &self,
        element: &str,
        attribute: &str,
        kind: &str,
    ) -> impl FnOnce(&str) -> String {
        let type_name = self.type_name().to_owned();
        let element = element.to_owned();
        let attribute = attribute.to_owned();
        let kind = kind.to_owned();
        move |value| {
            format!(
                "<{element} {attribute}=\"{value}\"> did not resolve to an {kind} property on {type_name}"
            )
        }
    }

    /// OpenAC `BindColorSource` (and its theme branch): `None` when the
    /// attribute is absent.
    fn color(&self, attribute: Option<&str>) -> Result<Option<ColorSource>, MarkupError> {
        let Some(a) = attribute else {
            return Ok(None);
        };
        if let Some(rest) = a.strip_prefix("theme:") {
            let token = rest.split('|').next().unwrap_or("");
            if !THEME_TOKENS.contains(&token) {
                return err(format!("Unknown plugin theme color '{token}'."));
            }
            return Ok(Some(ColorSource::Theme {
                token: token.to_owned(),
                classic: parse_color(Some(a)),
            }));
        }
        if is_binding(a) && self.kind(inner(a)).is_some() {
            return Ok(Some(ColorSource::Bound(inner(a).to_owned())));
        }
        Ok(Some(ColorSource::Literal(parse_color(Some(a)))))
    }

    /// `BindBool` (visible, enabled).
    fn bool_attr(&self, attribute: Option<&str>) -> Result<BoolSource, MarkupError> {
        let Some(a) = attribute.filter(|a| !a.trim().is_empty()) else {
            return Ok(BoolSource::Unset);
        };
        if !is_binding(a) {
            return Ok(parse_bool(a).map_or(BoolSource::Unset, BoolSource::Literal));
        }
        if self.kind(inner(a)) != Some(BindingKind::Bool) {
            return err(format!(
                "{a} did not resolve to a bool property on {}",
                self.type_name()
            ));
        }
        Ok(BoolSource::Bound(inner(a).to_owned()))
    }

    /// `BindRequiredBoolReader`.
    fn required_bool(&self, attribute: Option<&str>, context: &str) -> Result<String, MarkupError> {
        let Some(a) = attribute.filter(|a| !a.trim().is_empty() && is_binding(a)) else {
            return err(format!("{context} must be a bool binding"));
        };
        if self.kind(inner(a)) != Some(BindingKind::Bool) {
            return err(format!(
                "{a} did not resolve to a bool property on {}",
                self.type_name()
            ));
        }
        Ok(inner(a).to_owned())
    }

    /// `BindRequiredIntReader`.
    fn required_int(&self, attribute: Option<&str>, context: &str) -> Result<String, MarkupError> {
        let Some(a) = attribute.filter(|a| !a.trim().is_empty() && is_binding(a)) else {
            return err(format!("{context} must be an int binding"));
        };
        if self.kind(inner(a)) != Some(BindingKind::Int) {
            return err(format!(
                "{a} did not resolve to an int property on {}",
                self.type_name()
            ));
        }
        Ok(inner(a).to_owned())
    }

    /// `BindStringList`.
    fn string_list(&self, attribute: Option<&str>, context: &str) -> Result<String, MarkupError> {
        let Some(a) = attribute.filter(|a| !a.trim().is_empty() && is_binding(a)) else {
            return err(format!("{context} must be a string-list binding"));
        };
        if self.kind(inner(a)) != Some(BindingKind::TextList) {
            return err(format!(
                "{a} did not resolve to an IEnumerable<string> property on {}",
                self.type_name()
            ));
        }
        Ok(inner(a).to_owned())
    }

    /// `BindUintList`: `None` when absent or blank.
    fn uint_list(
        &self,
        attribute: Option<&str>,
        context: &str,
    ) -> Result<Option<String>, MarkupError> {
        let Some(a) = attribute.filter(|a| !a.trim().is_empty()) else {
            return Ok(None);
        };
        if !is_binding(a) {
            return err(format!("{context} must be a uint-list binding"));
        }
        match self.kind(inner(a)) {
            Some(BindingKind::UintList | BindingKind::IntList) => Ok(Some(inner(a).to_owned())),
            _ => err(format!(
                "{a} did not resolve to an IEnumerable<uint> or IEnumerable<int> property on {}",
                self.type_name()
            )),
        }
    }

    /// `BindBoolList`.
    fn bool_list(&self, attribute: Option<&str>, context: &str) -> Result<String, MarkupError> {
        let Some(a) = attribute.filter(|a| !a.trim().is_empty() && is_binding(a)) else {
            return err(format!("{context} must be a bool-list binding"));
        };
        if self.kind(inner(a)) != Some(BindingKind::BoolList) {
            return err(format!(
                "{a} did not resolve to an IEnumerable<bool> property on {} ({context})",
                self.type_name()
            ));
        }
        Ok(inner(a).to_owned())
    }

    /// `BindRequiredIntAction`.
    fn required_int_action(
        &self,
        attribute: Option<&str>,
        context: &str,
    ) -> Result<String, MarkupError> {
        let Some(a) = attribute.filter(|a| is_binding(a)) else {
            return err(format!("{context} must be an Action<int> binding"));
        };
        if self.kind(inner(a)) != Some(BindingKind::IntAction) {
            return err(format!(
                "{a} did not resolve to an Action<int> property on {} ({context})",
                self.type_name()
            ));
        }
        Ok(inner(a).to_owned())
    }

    /// `BindUintLiteralOrBinding`.
    fn uint_literal_or_binding(
        &self,
        expression: &str,
        context: &str,
    ) -> Result<UintSource, MarkupError> {
        if !is_binding(expression) {
            return parse_uint_literal(expression, context).map(UintSource::Literal);
        }
        if self.kind(inner(expression)).is_none() {
            return err(format!(
                "{expression} did not resolve to a property on {} ({context})",
                self.type_name()
            ));
        }
        Ok(UintSource::Bound(inner(expression).to_owned()))
    }

    /// `BindFloat` / `BindUint`: a declared binding, else missing.
    fn number(&self, attribute: Option<&str>) -> NumberSource {
        match attribute {
            Some(a) if is_binding(a) && self.kind(inner(a)).is_some() => {
                NumberSource::Bound(inner(a).to_owned())
            }
            _ => NumberSource::Missing,
        }
    }

    fn parse_anchor(&self, el: XmlNode<'_, '_>) -> Result<Anchors, MarkupError> {
        let Some(tokens) = attr(el, "anchor") else {
            return Ok(Anchors::TOP_LEFT);
        };
        let mut edges = Anchors::NONE;
        for token in tokens.split(ANCHOR_SEPARATORS).filter(|t| !t.is_empty()) {
            edges = edges.union(match token.to_lowercase().as_str() {
                "left" => Anchors::LEFT,
                "top" => Anchors::TOP,
                "right" => Anchors::RIGHT,
                "bottom" => Anchors::BOTTOM,
                _ => {
                    return err(format!(
                        "{} anchor=\"{tokens}\" has unknown token \"{token}\" (expected left, top, right, bottom)",
                        element_identity(el)
                    ));
                }
            });
        }
        if edges == Anchors::NONE {
            return err(format!(
                "{} anchor=\"{tokens}\" names no edge (expected a comma- or space-separated subset of left, top, right, bottom)",
                element_identity(el)
            ));
        }
        Ok(edges)
    }

    /// OpenAC `ApplyCommon`: name, anchors, visible, enabled, tooltip.
    fn common(&self, el: XmlNode<'_, '_>) -> Result<Common, MarkupError> {
        let name = attr(el, "name")
            .or_else(|| attr(el, "id"))
            .map(str::to_owned);
        let anchors = self.parse_anchor(el)?;
        let visible = self.bool_attr(attr(el, "visible"))?;
        let enabled = self.bool_attr(attr(el, "enabled"))?;
        let tooltip = attr(el, "tooltip")
            .filter(|t| !t.trim().is_empty())
            .map(|t| self.text(Some(t)));
        Ok(Common {
            name,
            anchors,
            visible,
            enabled,
            tooltip,
            default_color: attr(el, "color").is_none(),
            default_background: attr(el, "background").is_none(),
            default_border: attr(el, "border").is_none(),
        })
    }

    fn push(
        &mut self,
        element: Element,
        common: Common,
        rect: Rect,
        parent: Option<usize>,
    ) -> usize {
        let index = self.doc.nodes.len();
        self.doc.nodes.push(Node {
            element,
            common,
            rect,
            parent,
            children: Vec::new(),
        });
        if let Some(p) = parent {
            self.doc.nodes[p].children.push(index);
        }
        index
    }

    #[allow(clippy::too_many_lines)]
    fn element(
        &mut self,
        el: XmlNode<'_, '_>,
        parent: Option<usize>,
    ) -> Result<usize, MarkupError> {
        let tag = el.tag_name().name();
        let element = match tag {
            "group" => {
                let background = self.color(attr(el, "background"))?;
                let border = self.color(attr(el, "border"))?;
                let common = self.common(el)?;
                let index = self.push(
                    Element::Group { background, border },
                    common,
                    rect(el),
                    parent,
                );
                for child in el.children().filter(XmlNode::is_element) {
                    self.element(child, Some(index))?;
                }
                return Ok(index);
            }
            "meter" => {
                let cur = self.number(attr(el, "cur"));
                let max = self.number(attr(el, "max"));
                let fill = self.number(attr(el, "fill"));
                let color = self
                    .color(attr(el, "color"))?
                    .unwrap_or(ColorSource::Literal(parse_color(None)));
                let sprites = [
                    parse_hex(attr(el, "backleft")),
                    parse_hex(attr(el, "backtile")),
                    parse_hex(attr(el, "backright")),
                    parse_hex(attr(el, "frontleft")),
                    parse_hex(attr(el, "fronttile")),
                    parse_hex(attr(el, "frontright")),
                ];
                Element::Meter {
                    fill,
                    cur,
                    max,
                    color,
                    sprites,
                }
            }
            "label" => Element::Label {
                text: self.text(attr(el, "text")),
                color: self.color(attr(el, "color"))?,
            },
            "button" => {
                let onclick = self.handler(
                    attr(el, "onclick"),
                    BindingKind::Action,
                    self.action_message("button", "onclick", "Action"),
                )?;
                let text = match attr(el, "text") {
                    None => TextSource::Literal(String::new()),
                    Some(caption) => self.text(Some(caption)),
                };
                let color = self.color(attr(el, "color"))?;
                let background = self.color(attr(el, "background"))?;
                let border = self.color(attr(el, "border"))?;
                let icon = self.button_icon(el, "button icon")?;
                Element::Button {
                    text,
                    onclick,
                    color,
                    background,
                    border,
                    icon,
                }
            }
            "icon" => {
                if attr(el, "iconkind").is_some() {
                    return err(
                        "iconkind applies to button, tab and list; icon derives its kind from did/spell/item",
                    );
                }
                let did = attr(el, "did");
                let spell = attr(el, "spell");
                let item = attr(el, "item");
                let sources = usize::from(did.is_some())
                    + usize::from(spell.is_some())
                    + usize::from(item.is_some());
                if sources != 1 {
                    return err("<icon> requires exactly one of did/spell/item");
                }
                let (kind, name, expression) = match (did, spell, item) {
                    (Some(e), _, _) => (IconKind::Did, "did", e),
                    (_, Some(e), _) => (IconKind::Spell, "spell", e),
                    (_, _, Some(e)) => (IconKind::Item, "item", e),
                    _ => unreachable!("exactly one source was checked above"),
                };
                let id = self.uint_literal_or_binding(expression, &format!("icon {name}"))?;
                let common = self.common(el)?;
                let rect = Rect {
                    x: f(el, "x"),
                    y: f(el, "y"),
                    w: f_or(el, "w", 32.0),
                    h: f_or(el, "h", 32.0),
                };
                return Ok(self.push(
                    Element::Icon {
                        icon: IconSource { kind, id },
                    },
                    common,
                    rect,
                    parent,
                ));
            }
            "tab" => {
                let onclick = self.handler(
                    attr(el, "onclick"),
                    BindingKind::Action,
                    self.action_message("tab", "onclick", "Action"),
                )?;
                let selected = self.required_bool(attr(el, "selected"), "tab selected")?;
                let icon = self.button_icon(el, "tab icon")?;
                Element::Tab {
                    text: attr(el, "text").unwrap_or("").to_owned(),
                    selected,
                    onclick,
                    icon,
                }
            }
            "toggle" => {
                let onclick = self.handler(
                    attr(el, "onclick"),
                    BindingKind::Action,
                    self.action_message("toggle", "onclick", "Action"),
                )?;
                let caption = attr(el, "text");
                let checked = self.required_bool(attr(el, "checked"), "toggle checked")?;
                let text = match caption {
                    None => TextSource::Literal(String::new()),
                    Some(c) => self.text(Some(c)),
                };
                Element::Toggle {
                    text,
                    checked,
                    onclick,
                    color: self.color(attr(el, "color"))?,
                }
            }
            "slider" => {
                let onchange = self.handler(
                    attr(el, "onchange"),
                    BindingKind::FloatAction,
                    self.action_message("slider", "onchange", "Action<float>"),
                )?;
                let min = f_or(el, "min", 0.0);
                let max = f_or(el, "max", 1.0);
                if max <= min {
                    return err(format!(
                        "<slider min=\"{}\" max=\"{}\"> must have max > min",
                        num(min),
                        num(max)
                    ));
                }
                let value = self.number(attr(el, "value"));
                let retail_art = validate_art_style("slider", attr(el, "style"))?;
                Element::Slider {
                    min,
                    max,
                    value,
                    onchange,
                    retail_art,
                }
            }
            "field" => {
                let onchange = self.handler(
                    attr(el, "onchange"),
                    BindingKind::TextAction,
                    self.action_message("field", "onchange", "Action<string>"),
                )?;
                let onsubmit = self.handler(
                    attr(el, "onsubmit"),
                    BindingKind::TextAction,
                    self.action_message("field", "onsubmit", "Action<string>"),
                )?;
                let onblur = self.handler(
                    attr(el, "onblur"),
                    BindingKind::TextAction,
                    self.action_message("field", "onblur", "Action<string>"),
                )?;
                let onup = self.handler(
                    attr(el, "onup"),
                    BindingKind::Action,
                    self.action_message("field", "onup", "Action"),
                )?;
                let ondown = self.handler(
                    attr(el, "ondown"),
                    BindingKind::Action,
                    self.action_message("field", "ondown", "Action"),
                )?;
                let background = match attr(el, "background") {
                    None => ColorSource::Literal(Rgba::new(0.0, 0.0, 0.0, 0.9)),
                    Some(_) => self
                        .color(attr(el, "background"))?
                        .unwrap_or(ColorSource::Literal(Rgba::WHITE)),
                };
                let color = match attr(el, "color") {
                    None => ColorSource::Literal(Rgba::new(0.91, 0.87, 0.76, 1.0)),
                    Some(_) => self
                        .color(attr(el, "color"))?
                        .unwrap_or(ColorSource::Literal(Rgba::WHITE)),
                };
                Element::Field {
                    text: self.text(attr(el, "text")),
                    onchange,
                    onsubmit,
                    onblur,
                    onup,
                    ondown,
                    one_line: b(el, "oneline", true),
                    editable: b(el, "editable", true),
                    background,
                    color,
                    default_background: attr(el, "background").is_none(),
                    default_color: attr(el, "color").is_none(),
                    max_length: i(el, "maxlength", 128).max(1),
                    clear_on_submit: b(el, "clearonsubmit", false),
                }
            }
            "menu" => {
                let onchange = self.handler(
                    attr(el, "onchange"),
                    BindingKind::TextAction,
                    self.action_message("menu", "onchange", "Action<string>"),
                )?;
                let items = self.string_list(attr(el, "items"), "menu items")?;
                let selected = self.text(attr(el, "selected"));
                let retail_art = validate_art_style("menu", attr(el, "style"))?;
                Element::Menu {
                    items,
                    selected,
                    onchange,
                    searchable: b(el, "searchable", false),
                    rows: i(el, "rows", 7).max(1),
                    row_height: f_or(el, "rowheight", 18.0).max(12.0),
                    open_upward: b(el, "openupward", false),
                    retail_art,
                }
            }
            "log" => {
                let items = self.string_list(attr(el, "items"), "log items")?;
                let first_index = match attr(el, "firstindex") {
                    None => None,
                    Some(a) => Some(self.required_int(Some(a), "log firstindex")?),
                };
                Element::Log { items, first_index }
            }
            "list" => {
                let onchange = self.handler(
                    attr(el, "onchange"),
                    BindingKind::IntAction,
                    self.action_message("list", "onchange", "Action<int>"),
                )?;
                let children: Vec<XmlNode<'_, '_>> =
                    el.children().filter(XmlNode::is_element).collect();
                for child in &children {
                    let name = child.tag_name().name();
                    if name != "column" {
                        return err(format!(
                            "<list> children must all be <column>, got <{name}>"
                        ));
                    }
                }
                let columns = !children.is_empty();
                if columns
                    && (attr(el, "items").is_some()
                        || attr(el, "icons").is_some()
                        || attr(el, "colors").is_some())
                {
                    return err(
                        "<list> with <column> children cannot also use the items/icons/colors attributes (the single-column form) — express every row source as a <column> instead",
                    );
                }
                let selected = self.required_int(attr(el, "selected"), "list selected")?;
                let body = if columns {
                    let last = children.len() - 1;
                    let mut built = Vec::with_capacity(children.len());
                    for (index, column) in children.iter().enumerate() {
                        built.push(self.column(*column, index, index == last)?);
                    }
                    ListBody::Columns(built)
                } else {
                    let items = self.string_list(attr(el, "items"), "list items")?;
                    let colors = self.uint_list(attr(el, "colors"), "list colors")?;
                    let icons = match attr(el, "icons").filter(|a| !a.trim().is_empty()) {
                        None => None,
                        Some(icons) => {
                            let kind = validate_icon_kind(attr(el, "iconkind"), "iconkind")?;
                            self.uint_list(Some(icons), "list icons")?
                                .map(|name| (name, kind))
                        }
                    };
                    ListBody::Single {
                        items,
                        colors,
                        icons,
                    }
                };
                Element::List {
                    selected,
                    onchange,
                    row_height: f_or(el, "rowheight", 18.0).max(12.0),
                    selection_band: b(el, "selectionband", false),
                    body,
                }
            }
            other => return err(format!("unknown element <{other}>")),
        };
        let common = self.common(el)?;
        Ok(self.push(element, common, rect(el), parent))
    }

    fn column(&self, el: XmlNode<'_, '_>, index: usize, last: bool) -> Result<Column, MarkupError> {
        let kind_name = attr(el, "type");
        let context = |type_name: &str, attribute: &str| {
            format!("column[{index}] type=\"{type_name}\" {attribute}")
        };
        let raw = attr(el, "width");
        let (width, auto) = if raw == Some("*") {
            (0.0, true)
        } else if last {
            (f(el, "width"), false)
        } else {
            match raw.and_then(parse_float).filter(|w| *w > 0.0) {
                Some(w) => (w, false),
                None => {
                    return err(format!(
                        "{} must be a positive number or \"*\", got {}",
                        context(kind_name.unwrap_or("(missing)"), "width"),
                        raw.map_or_else(|| "(missing)".to_owned(), |r| format!("\"{r}\""))
                    ));
                }
            }
        };
        let kind = match kind_name {
            Some("text") => {
                let items = self.string_list(attr(el, "items"), &context("text", "items"))?;
                let colors = match attr(el, "colors") {
                    None => None,
                    Some(c) => self.uint_list(Some(c), &context("text", "colors"))?,
                };
                let onclick = self.handler(attr(el, "onclick"), BindingKind::IntAction, |_| {
                    format!(
                        "{} did not resolve to an Action<int> property on {}",
                        context("text", "onclick"),
                        self.type_name()
                    )
                })?;
                ColumnKind::Text {
                    items,
                    colors,
                    onclick,
                }
            }
            Some("check") => {
                let values = self.bool_list(attr(el, "values"), &context("check", "values"))?;
                let onchange =
                    self.required_int_action(attr(el, "onchange"), &context("check", "onchange"))?;
                ColumnKind::Check { values, onchange }
            }
            Some("icon") => {
                let values_context = context("icon", "values");
                let values = match attr(el, "values").filter(|a| !a.trim().is_empty()) {
                    None => return err(format!("{values_context} must be a uint-list binding")),
                    Some(v) => self
                        .uint_list(Some(v), &values_context)?
                        .unwrap_or_default(),
                };
                let kind = validate_icon_kind(attr(el, "iconkind"), &context("icon", "iconkind"))?;
                let onclick =
                    self.required_int_action(attr(el, "onclick"), &context("icon", "onclick"))?;
                ColumnKind::Icon {
                    values,
                    kind,
                    onclick,
                }
            }
            Some("button") => {
                let items = self.string_list(attr(el, "items"), &context("button", "items"))?;
                let onclick =
                    self.required_int_action(attr(el, "onclick"), &context("button", "onclick"))?;
                let size = |attribute: &str| {
                    let size = f_or(el, attribute, 0.0);
                    if size < 0.0 {
                        err(format!(
                            "{} must not be negative",
                            context("button", attribute)
                        ))
                    } else {
                        Ok(size)
                    }
                };
                ColumnKind::Button {
                    items,
                    onclick,
                    button_width: size("buttonw")?,
                    button_height: size("buttonh")?,
                }
            }
            other => {
                return err(format!(
                    "column[{index}] has unknown type=\"{}\" (expected text, check, icon, or button)",
                    other.unwrap_or("")
                ));
            }
        };
        Ok(Column { width, auto, kind })
    }
}

#[cfg(test)]
mod tests;
