//! Plugin UI records (the plugin API's `ui` interface): markup panels with declared bindings, canvases and their
//! paint commands, images, world labels and lines, client windows, and the
//! events a plugin hears from its UI ([`UiEvent`]).
//!
//! Everything here is session state on the client side: the answers are the
//! same with or without a window, except where an import says it needs one
//! ([`crate::Import::needs_window`]).

use crate::wire::{DecodeError, Reader, Wire, Writer};
use crate::wire_record;

/// [`crate::Event::Ui`]'s tag: well clear of the other event groups'
/// numbering.
pub const EVENT_TAG: u32 = 100;

/// Writes a fieldless-or-tuple enum's `Wire` from one list: each variant's
/// tag and its fields in order.
macro_rules! wire_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident ($what:literal) {
            $( $(#[$vmeta:meta])* $variant:ident $( ( $($field:ident : $ty:ty),* ) )? = $tag:literal ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        pub enum $name {
            $( $(#[$vmeta])* $variant $( ( $($ty),* ) )?, )*
        }

        impl Wire for $name {
            const SHAPE: u64 = $crate::wire::Shape::new("enum")
                $(
                    .tag($tag as u64)
                    .name(stringify!($variant))
                    $( $( .name(stringify!($field)).of(<$ty as Wire>::SHAPE) )* )?
                )*
                .finish();
            fn encode(&self, w: &mut Writer) {
                match self {
                    $( Self::$variant $( ( $($field),* ) )? => {
                        ($tag as u8).encode(w);
                        $( $( $field.encode(w); )* )?
                    } )*
                }
            }
            fn read(r: &mut Reader<'_>) -> Self {
                match u8::read(r) {
                    $( $tag => Self::$variant $( ( $( <$ty as Wire>::read(r) ),* ) )?, )*
                    tag => {
                        r.fail(DecodeError::UnknownTag { what: $what, tag: u32::from(tag) });
                        // A stand-in: the first variant, read from the
                        // failed reader (no bytes left).
                        wire_enum!(@first r; $( $variant $( ( $($field : $ty),* ) )? );*)
                    }
                }
            }
        }
    };
    (@first $r:ident; $variant:ident $( ( $($field:ident : $ty:ty),* ) )? $(; $($rest:tt)*)?) => {
        Self::$variant $( ( $( <$ty as Wire>::read($r) ),* ) )?
    };
}

wire_enum! {
    /// What a markup binding name is (OpenAC: the type of the C# property
    /// it names).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub enum BindingKind ("binding kind") {
        /// `bool`.
        Bool = 0,
        /// `int`.
        Int = 1,
        /// `uint`.
        Uint = 2,
        /// `float`.
        Float = 3,
        /// `string`.
        #[default]
        Text = 4,
        /// Strings.
        TextList = 5,
        /// `uint`s.
        UintList = 6,
        /// `int`s.
        IntList = 7,
        /// `bool`s.
        BoolList = 8,
        /// A handler with no argument (`Action`).
        Action = 9,
        /// A handler taking an `int` (`Action<int>`).
        IntAction = 10,
        /// A handler taking a `float` (`Action<float>`).
        FloatAction = 11,
        /// A handler taking a `string` (`Action<string>`).
        TextAction = 12,
    }
}

wire_enum! {
    /// A value pushed for a binding.
    #[derive(Debug, Clone, PartialEq)]
    pub enum UiValue ("binding value") {
        /// `bool`.
        Bool(v: bool) = 0,
        /// `int`.
        Int(v: i32) = 1,
        /// `uint`.
        Uint(v: u32) = 2,
        /// `float`.
        Float(v: f32) = 3,
        /// `string`.
        Text(v: String) = 4,
        /// Strings.
        TextList(v: Vec<String>) = 5,
        /// `uint`s.
        UintList(v: Vec<u32>) = 6,
        /// `int`s.
        IntList(v: Vec<i32>) = 7,
        /// `bool`s.
        BoolList(v: Vec<bool>) = 8,
    }
}

impl From<bool> for UiValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i32> for UiValue {
    fn from(v: i32) -> Self {
        Self::Int(v)
    }
}

impl From<u32> for UiValue {
    fn from(v: u32) -> Self {
        Self::Uint(v)
    }
}

impl From<f32> for UiValue {
    fn from(v: f32) -> Self {
        Self::Float(v)
    }
}

impl From<&str> for UiValue {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}

impl From<String> for UiValue {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}

impl From<Vec<String>> for UiValue {
    fn from(v: Vec<String>) -> Self {
        Self::TextList(v)
    }
}

impl From<Vec<u32>> for UiValue {
    fn from(v: Vec<u32>) -> Self {
        Self::UintList(v)
    }
}

impl From<Vec<i32>> for UiValue {
    fn from(v: Vec<i32>) -> Self {
        Self::IntList(v)
    }
}

impl From<Vec<bool>> for UiValue {
    fn from(v: Vec<bool>) -> Self {
        Self::BoolList(v)
    }
}

wire_record! {
    /// One binding name a panel's markup may use, and its kind.
    pub struct BindingDecl {
        /// The name inside `{...}`.
        pub name: String,
        /// What it is.
        pub kind: BindingKind,
    }
}

impl Default for UiValue {
    fn default() -> Self {
        Self::Bool(false)
    }
}

wire_record! {
    /// A window from markup ([`crate::Import::UiPanelRegister`]; OpenAC
    /// `RegisterPanelContent` with a `PluginPanelDescriptor`). Reply
    /// `Result<u32, String>`: the panel's number, or why the markup is
    /// refused.
    pub struct PanelRegistration {
        /// The plugin's own short name for the window.
        pub window_id: String,
        /// The caption.
        pub title: String,
        /// Initials on the shelf button when there is no icon.
        pub icon_text: Option<String>,
        /// The shelf icon (a full image id or a bare index; 0 none).
        pub icon_surface_id: u32,
        /// Open at once.
        pub start_visible: bool,
        /// A button on the plugin shelf.
        pub show_in_side_panel: bool,
        /// The markup.
        pub markup: String,
        /// The binding type's name, for messages.
        pub binding_type: String,
        /// Every binding name the markup may use.
        pub bindings: Vec<BindingDecl>,
    }
}

wire_record! {
    /// One pushed value.
    pub struct NamedValue {
        /// The binding name.
        pub name: String,
        /// Its value.
        pub value: UiValue,
    }
}

wire_record! {
    /// Bound values for a panel ([`crate::Import::UiPanelSet`]); reply
    /// `Result<(), String>` (all or nothing).
    pub struct PanelValues {
        /// The panel.
        pub panel: u32,
        /// The values.
        pub values: Vec<NamedValue>,
    }
}

wire_record! {
    /// Show or hide one of the plugin's windows by id or title
    /// ([`crate::Import::UiPanelShow`]); reply `bool`.
    pub struct ViewShow {
        /// The window's id or title.
        pub view: String,
        /// Show (true) or hide.
        pub show: bool,
    }
}

wire_record! {
    /// A named control in one of the plugin's windows
    /// ([`crate::Import::UiControlExists`]); reply `bool`.
    pub struct ControlRef {
        /// The window's id or title.
        pub view: String,
        /// The control's `name` (or `id`).
        pub control: String,
    }
}

wire_record! {
    /// Replace a control's text ([`crate::Import::UiControlLabel`]); reply
    /// `bool`.
    pub struct ControlLabel {
        /// The window's id or title.
        pub view: String,
        /// The control.
        pub control: String,
        /// The text.
        pub label: String,
    }
}

wire_record! {
    /// Show or hide a control ([`crate::Import::UiControlVisible`]); reply
    /// `bool`.
    pub struct ControlVisible {
        /// The window's id or title.
        pub view: String,
        /// The control.
        pub control: String,
        /// Shown.
        pub visible: bool,
    }
}

wire_record! {
    /// One of the client's windows ([`crate::Import::UiClientWindow`]):
    /// `window` numbers Inventory, Character, CharacterInformation,
    /// Spellbook, Map, Options, Social, Journal, PositiveEffects,
    /// NegativeEffects, LinkStatus, Vitae, Radar (0..=12); `action` 0
    /// toggle, 1 show, 2 hide, 3 is it visible. Reply `bool`.
    pub struct ClientWindowRequest {
        /// Which window.
        pub window: u32,
        /// What to do.
        pub action: u8,
    }
}

wire_record! {
    /// A canvas ([`crate::Import::UiCanvasRegister`]; OpenAC
    /// `PluginCanvasDescriptor`). Reply `Result<u32, String>`.
    pub struct CanvasRegistration {
        /// The plugin's own name for it.
        pub canvas_id: String,
        /// Width, pixels.
        pub width: i32,
        /// Height, pixels.
        pub height: i32,
        /// Anchor: TopLeft, TopCenter, TopRight, CenterLeft, Center,
        /// CenterRight, BottomLeft, BottomCenter, BottomRight (0..=8).
        pub anchor: u8,
        /// Pixels from the anchor.
        pub offset_x: f32,
        /// Pixels from the anchor.
        pub offset_y: f32,
        /// Shown at once.
        pub start_visible: bool,
        /// Takes pointer input while the plugin listens.
        pub accepts_pointer_input: bool,
    }
}

wire_record! {
    /// A paint colour: red, green, blue, alpha bytes.
    pub struct Color {
        /// Red.
        pub r: u8,
        /// Green.
        pub g: u8,
        /// Blue.
        pub b: u8,
        /// Alpha.
        pub a: u8,
    }
}

wire_record! {
    /// A rectangle in canvas pixels.
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
}

wire_enum! {
    /// One drawing step (OpenAC `IPluginPainter`).
    #[derive(Debug, Clone, PartialEq)]
    pub enum PaintCmd ("paint command") {
        /// Fill the canvas.
        Clear(color: Color) = 0,
        /// Fill a rectangle.
        FillRect(rect: Rect, color: Color) = 1,
        /// Outline a rectangle (inside its edges), with a thickness.
        StrokeRect(rect: Rect, color: Color, thickness: f32) = 2,
        /// A line: from x, from y, to x, to y, colour, thickness.
        Line(x0: f32, y0: f32, x1: f32, y1: f32, color: Color, thickness: f32) = 3,
        /// Text at x, y (top-left), colour, outline.
        Text(text: String, x: f32, y: f32, color: Color, outline: bool) = 4,
        /// An image handle over a rectangle, tinted.
        Image(image: u32, rect: Rect, tint: Color) = 5,
        /// An image over a rectangle turned (radians, clockwise) and scaled
        /// about a pivot (pixels from the rectangle's top-left).
        ImageTransformed(image: u32, rect: Rect, tint: Color, rotation: f32, pivot_x: f32, pivot_y: f32, scale_x: f32, scale_y: f32) = 6,
        /// Clip what follows.
        PushClip(rect: Rect) = 7,
        /// Undo the last clip.
        PopClip = 8,
    }
}

wire_record! {
    /// A canvas's new paint ([`crate::Import::UiCanvasPaint`]); reply
    /// `Result<(), String>`.
    pub struct CanvasPaint {
        /// The canvas.
        pub canvas: u32,
        /// What it shows from now on.
        pub commands: Vec<PaintCmd>,
    }
}

wire_enum! {
    /// A change to a canvas.
    #[derive(Debug, Clone, PartialEq, Default)]
    pub enum CanvasChange ("canvas change") {
        /// Show or hide.
        Visible(visible: bool) = 0,
        /// Move its anchor (0..=8).
        Anchor(anchor: u8) = 1,
        /// Move its offset.
        Offset(x: f32, y: f32) = 2,
        /// Start or stop listening for its pointer (OpenAC setting or
        /// clearing `PointerHandler`).
        Listen(listening: bool) = 3,
        /// End the press it holds (OpenAC `ReleasePointer`).
        #[default]
        ReleasePointer = 4,
    }
}

wire_record! {
    /// One canvas change ([`crate::Import::UiCanvasSet`]); reply `bool`.
    pub struct CanvasSet {
        /// The canvas.
        pub canvas: u32,
        /// The change.
        pub change: CanvasChange,
    }
}

wire_record! {
    /// A canvas as it stands ([`crate::Import::UiCanvasState`] replies
    /// `Option<CanvasState>`).
    pub struct CanvasState {
        /// Width, pixels.
        pub width: i32,
        /// Height, pixels.
        pub height: i32,
        /// Shown.
        pub visible: bool,
        /// Anchor (0..=8).
        pub anchor: u8,
        /// Offset.
        pub offset_x: f32,
        /// Offset.
        pub offset_y: f32,
        /// The plugin listens for its pointer.
        pub listening: bool,
    }
}

wire_record! {
    /// A measured piece of text ([`crate::Import::UiMeasureText`]).
    pub struct TextSize {
        /// Width, pixels.
        pub width: f32,
        /// The font's line height, pixels.
        pub height: f32,
    }
}

wire_record! {
    /// An image the plugin holds (handle 0: refused, see the session log).
    pub struct ImageInfo {
        /// The handle.
        pub handle: u32,
        /// Width, pixels.
        pub width: u32,
        /// Height, pixels.
        pub height: u32,
    }
}

wire_record! {
    /// The plugin's own pixels ([`crate::Import::UiImageRgba`]): 8-bit
    /// RGBA rows from the top, under the plugin's own name for them.
    pub struct RgbaImage {
        /// The plugin's name for the image.
        pub name: String,
        /// Width, pixels.
        pub width: u32,
        /// Height, pixels.
        pub height: u32,
        /// `width * height * 4` bytes.
        pub rgba: Vec<u8>,
    }
}

wire_record! {
    /// The plugin's own art as a file ([`crate::Import::UiImageDecode`]):
    /// PNG, JPEG, BMP, TGA or GIF bytes, under the plugin's own name for
    /// them. The client decodes them; a GIF gives its first frame.
    pub struct EncodedImage {
        /// The plugin's name for the image (unique within the plugin, such
        /// as its file path).
        pub name: String,
        /// The encoded file.
        pub bytes: Vec<u8>,
    }
}

wire_record! {
    /// A point on the global map grid: east-west and north-south in map
    /// units (240 m, as the location readout), elevation in the same units.
    pub struct MapPoint {
        /// East-west, growing east.
        pub east_west: f64,
        /// North-south, growing north.
        pub north_south: f64,
        /// Height.
        pub elevation: f64,
    }
}

wire_record! {
    /// A line of text over an object (OpenAC `PluginWorldLabel`).
    pub struct WorldLabel {
        /// The object.
        pub object_id: u32,
        /// The text (one line, at most 128 characters).
        pub text: String,
        /// Red, 0..=1.
        pub r: f32,
        /// Green.
        pub g: f32,
        /// Blue.
        pub b: f32,
        /// Alpha.
        pub a: f32,
        /// Metres added to the object's height.
        pub height_offset: f32,
        /// Which line over the object (0 at the head).
        pub line: i32,
        /// Shown up to this many metres away.
        pub max_range: f32,
        /// A dark outline.
        pub outline: bool,
    }
}

wire_record! {
    /// A line in the world (OpenAC `PluginWorldLine`).
    pub struct WorldLine {
        /// One end.
        pub start: MapPoint,
        /// The other end.
        pub end: MapPoint,
        /// `0xRRGGBB`.
        pub color_rgb: u32,
        /// Kept just above the ground along its length.
        pub follow_terrain: bool,
        /// Thickness, metres.
        pub width_metres: f32,
    }
}

wire_record! {
    /// A layer's lines ([`crate::Import::UiLinesSet`]); reply `bool`.
    pub struct LineSet {
        /// The layer.
        pub layer: u32,
        /// Its lines from now on.
        pub lines: Vec<WorldLine>,
    }
}

wire_record! {
    /// A projectile marker (OpenAC `PluginProjectileDebugSample`).
    pub struct ProjectileSample {
        /// Where the step ended.
        pub position: MapPoint,
        /// Unobstructed.
        pub is_clear: bool,
        /// The probe's radius, metres.
        pub radius: f32,
    }
}

wire_enum! {
    /// A handler's argument.
    #[derive(Debug, Clone, PartialEq, Default)]
    pub enum ActionValue ("action value") {
        /// A click.
        #[default]
        None = 0,
        /// A row (`Action<int>`).
        Int(v: i32) = 1,
        /// A slider value (`Action<float>`).
        Float(v: f32) = 2,
        /// Text (`Action<string>`).
        Text(v: String) = 3,
    }
}

wire_record! {
    /// A handler of one of the plugin's panels fired.
    pub struct UiAction {
        /// The panel.
        pub panel: u32,
        /// The handler's binding name (`onclick="{Go}"` names `Go`).
        pub handler: String,
        /// Its argument.
        pub value: ActionValue,
    }
}

wire_record! {
    /// Pointer input on one of the plugin's canvases (OpenAC
    /// `PluginPointerEvent`).
    pub struct PointerEvent {
        /// The canvas.
        pub canvas: u32,
        /// 0 down, 1 up, 2 move, 3 wheel, 4 cancelled.
        pub kind: u8,
        /// Canvas pixels from its top-left (outside it only in a drag).
        pub x: f32,
        /// Canvas pixels.
        pub y: f32,
        /// 0 none, 1 left, 2 right, 3 middle.
        pub button: u8,
        /// Bits: 1 shift, 2 control, 4 alt.
        pub modifiers: u8,
        /// Wheel notches, positive away from the user.
        pub wheel_delta: i32,
    }
}

wire_record! {
    /// A key and its modifiers (OpenAC `PluginKeyChord`): `key` is OpenAC's
    /// `PluginKey` number (`ac_plugin_ui::hotkeys::PluginKey::code`).
    pub struct KeyChord {
        /// The key (0: none).
        pub key: u32,
        /// Ctrl held.
        pub ctrl: bool,
        /// Alt held.
        pub alt: bool,
        /// Shift held.
        pub shift: bool,
    }
}

wire_record! {
    /// A hotkey to register ([`crate::Import::UiHotkeyRegister`]).
    pub struct HotkeyRegistration {
        /// The plugin's id for it (overrides are stored under
        /// `<plugin id>:<id>`).
        pub id: String,
        /// Its name for the player.
        pub display_name: String,
        /// The chord it asks for.
        pub chord: KeyChord,
    }
}

wire_record! {
    /// How a hotkey stands (OpenAC `IsBound`, `EffectiveChord`).
    pub struct HotkeyState {
        /// It fires: a window is attached and nothing else holds the chord.
        pub is_bound: bool,
        /// The chord it binds to (empty with no window attached).
        pub effective: KeyChord,
    }
}

wire_enum! {
    /// A plugin theme the player can pick under Appearance (OpenAC
    /// `PluginUiThemeKind`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum ThemeKind ("theme kind") {
        /// The client's own black and gold; themed windows draw unthemed.
        #[default]
        Classic = 0,
        /// Charcoal with moss-green accents.
        Moss = 1,
        /// Warm graphite with brass accents.
        Brass = 2,
    }
}

wire_record! {
    /// The colours a plugin theme draws themed windows in (OpenAC
    /// `PluginThemePalette`).
    pub struct ThemePalette {
        /// The window's face.
        pub background: Color,
        /// Inputs, lists and other sunken areas.
        pub field: Color,
        /// Window and control edges.
        pub border: Color,
        /// Body text.
        pub text: Color,
        /// Captions, hints and disabled labels.
        pub muted: Color,
        /// Highlights: a switch that is on, focus, links.
        pub accent: Color,
        /// The fill behind a selected row or tab.
        pub selected: Color,
    }
}

wire_record! {
    /// The plugin theme the player picked ([`crate::Import::UiTheme`],
    /// [`crate::Event::ThemeChanged`]; OpenAC `PluginUiThemeInfo`), so a
    /// canvas can draw like the plugin's themed windows.
    pub struct ThemeInfo {
        /// Which theme.
        pub kind: ThemeKind,
        /// What the Appearance picker calls it: "Classic", "Charcoal +
        /// moss" or "Warm graphite + brass".
        pub display_name: String,
        /// Its colours; none under Classic, where themed windows draw
        /// unthemed.
        pub palette: Option<ThemePalette>,
    }
}

wire_record! {
    /// A rebind ([`crate::Import::UiHotkeyRebind`]).
    pub struct HotkeyRebind {
        /// The hotkey.
        pub id: String,
        /// The new chord.
        pub chord: KeyChord,
    }
}

wire_enum! {
    /// Something a plugin hears from its UI ([`crate::Event::Ui`]).
    #[derive(Debug, Clone, PartialEq)]
    pub enum UiEvent ("ui event") {
        /// A panel handler fired.
        Action(action: UiAction) = 0,
        /// Pointer input on a canvas.
        Pointer(event: PointerEvent) = 1,
        /// One of the plugin's hotkeys was pressed (its id).
        Hotkey(id: String) = 2,
        /// The client's key map fired an input action (the plugin listens,
        /// [`crate::Import::UiInputActionsListen`]): the action's name as
        /// the key bindings store it (`MovementForward`, `UseQuickSlot_1`)
        /// and how it fired (`Press`, `Release`, `Click`, `DoubleClick`).
        InputAction(action: String, activation: String) = 3,
    }
}

impl Default for UiEvent {
    fn default() -> Self {
        Self::Action(UiAction::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_records_round_trip() {
        let registration = PanelRegistration {
            window_id: "main".into(),
            title: "Demo".into(),
            icon_text: Some("UD".into()),
            icon_surface_id: 0x0600_2C41,
            start_visible: true,
            show_in_side_panel: true,
            markup: "<panel/>".into(),
            binding_type: "Demo".into(),
            bindings: vec![
                BindingDecl {
                    name: "Go".into(),
                    kind: BindingKind::Action,
                },
                BindingDecl {
                    name: "Rows".into(),
                    kind: BindingKind::TextList,
                },
            ],
        };
        assert_eq!(
            PanelRegistration::from_bytes(&registration.to_bytes()),
            Ok(registration)
        );
        let paint = CanvasPaint {
            canvas: 3,
            commands: vec![
                PaintCmd::Clear(Color::default()),
                PaintCmd::Line(
                    0.0,
                    1.0,
                    2.0,
                    3.0,
                    Color {
                        r: 1,
                        g: 2,
                        b: 3,
                        a: 4,
                    },
                    1.5,
                ),
                PaintCmd::Text("hi".into(), 1.0, 2.0, Color::default(), true),
                PaintCmd::ImageTransformed(
                    7,
                    Rect::default(),
                    Color::default(),
                    0.5,
                    1.0,
                    2.0,
                    1.0,
                    1.0,
                ),
                PaintCmd::PopClip,
            ],
        };
        assert_eq!(CanvasPaint::from_bytes(&paint.to_bytes()), Ok(paint));
        let values = PanelValues {
            panel: 1,
            values: vec![
                NamedValue {
                    name: "A".into(),
                    value: UiValue::TextList(vec!["x".into()]),
                },
                NamedValue {
                    name: "B".into(),
                    value: UiValue::Float(0.5),
                },
            ],
        };
        assert_eq!(PanelValues::from_bytes(&values.to_bytes()), Ok(values));
        let event = UiEvent::Action(UiAction {
            panel: 1,
            handler: "Go".into(),
            value: ActionValue::Text("x".into()),
        });
        assert_eq!(UiEvent::from_bytes(&event.to_bytes()), Ok(event));
        assert_eq!(
            UiValue::from_bytes(&[42]),
            Err(DecodeError::UnknownTag {
                what: "binding value",
                tag: 42
            })
        );
    }
}
