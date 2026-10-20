//! The plugin shelf (OpenAC `PluginSidePanel`): a strip of buttons, one
//! per plugin window that asked for one, docked at the left of the screen
//! below the radar's row; a button shows its plugin's `icon.png` (else its
//! window's icon, else its initials) and opens or closes the window. A grip band moves the shelf,
//! a toggle collapses it, and it wraps into columns to stay on the screen.
//! Under a plugin theme it is a compact single column of smaller buttons
//! that the wheel scrolls; a right click on it asks for the Appearance
//! window.

use std::sync::Arc;

use ac_plugin_ui::markup::normalize_icon;
use ac_plugin_ui::{PanelView, PluginIcon, UiInput};

use crate::draw::{Color, RenderContext};
use crate::event::{UiEvent, kind};
use crate::font::UiFont;
use crate::geometry::AnchorEdges;
use crate::host::PanelController;
use crate::root::{ElementMessage, UiRoot};
use crate::tree::{Element, ElementId, Widget};

use super::{Icon, MarkupButton, PluginOutbox, PluginTheme};

/// The shelf's window name.
pub const WINDOW: &str = "plugin-shelf";

const OUTER_PADDING: f32 = 4.0;
const BUTTON_EXTENT: f32 = 28.0;
const BUTTON_GAP: f32 = 4.0;
const GRIP_HEIGHT: f32 = 12.0;
const TOGGLE_WIDTH: f32 = 16.0;
const COLLAPSED_WIDTH: f32 = TOGGLE_WIDTH + OUTER_PADDING * 2.0;
/// The compact (themed) shelf: its padding, button size and gap.
const COMPACT_PADDING: f32 = 1.0;
const COMPACT_EXTENT: f32 = 22.0;
const COMPACT_GAP: f32 = 2.0;
/// The compact shelf's grip and toggle band.
const COMPACT_BAND: f32 = 24.0;
/// Where the shelf docks first: its top.
pub const DEFAULT_TOP: f32 = 116.0;
/// Where the shelf docks first: its left edge.
pub const DEFAULT_LEFT: f32 = 10.0;
/// The shelf's tooltip once it offers the Appearance window.
pub const TOOLTIP: &str = "Right-click for plugin appearance. Scroll to browse plugins.";

const CLASSIC_BACKGROUND: Color = Color::rgba(0.0, 0.0, 0.0, 0.88);
const CLASSIC_BORDER: Color = Color::rgba(0.62, 0.48, 0.16, 1.0);
const TOGGLE_GLYPH: Color = Color::rgba(0.86, 0.72, 0.32, 1.0);

/// Where the shelf's button images come from: the window's textures.
pub trait ShelfIcons {
    /// Plugin `owner`'s decoded `icon.png` as a texture.
    fn file(&mut self, owner: &str, icon: &PluginIcon) -> Option<Icon>;
    /// A render surface (a full image id) as a texture.
    fn surface(&mut self, id: u32) -> Option<Icon>;
}

/// The initials a button shows without an icon (OpenAC
/// `PluginShelfButton.Initials`): the plugin's own (at most three
/// characters), else the title's first two letters (one word) or its first
/// two words' first letters, upper case.
#[must_use]
pub fn initials(requested: Option<&str>, title: &str) -> String {
    if let Some(r) = requested.map(str::trim).filter(|r| !r.is_empty()) {
        return r.chars().take(3).collect();
    }
    let words: Vec<&str> = title
        .split(' ')
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    match words.as_slice() {
        [] => "?".to_owned(),
        [one] => one.chars().take(2).collect::<String>().to_uppercase(),
        many => many
            .iter()
            .take(2)
            .filter_map(|w| w.chars().next())
            .flat_map(char::to_uppercase)
            .collect(),
    }
}

/// The shelf's geometry for `count` buttons: its size, the grip band and
/// each button's top-left and whether it shows (OpenAC
/// `PluginSidePanel.Reflow`).
#[derive(Debug, Clone, PartialEq)]
pub struct ShelfLayout {
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
    /// The grip band's height (the classic toggle's too).
    pub band: f32,
    /// A button's side.
    pub extent: f32,
    /// Each button's top-left.
    pub buttons: Vec<(f32, f32)>,
    /// Each button shows.
    pub shown: Vec<bool>,
    /// How many rows fit.
    pub visible_rows: usize,
    /// The first row shown (the compact shelf scrolls), clamped.
    pub first_row: usize,
}

/// The metrics of the classic or the compact shelf: padding, button side,
/// gap.
fn metrics(compact: bool) -> (f32, f32, f32) {
    if compact {
        (COMPACT_PADDING, COMPACT_EXTENT, COMPACT_GAP)
    } else {
        (OUTER_PADDING, BUTTON_EXTENT, BUTTON_GAP)
    }
}

/// The grip band's height: the compact shelf's fixed band, else the font's
/// line height plus two, at least the grip's height.
#[must_use]
pub fn band_height(compact: bool, line_height: Option<f32>) -> f32 {
    if compact {
        COMPACT_BAND
    } else {
        line_height.map_or(GRIP_HEIGHT, |h| GRIP_HEIGHT.max(h + 2.0))
    }
}

/// The layout of `count` buttons when `available` pixels of height are
/// left below the shelf's grip band (`None`: no limit yet), collapsed or
/// not, with the font's `line_height`; the compact shelf is one column
/// starting at `first_row`.
#[must_use]
pub fn layout(
    count: usize,
    available: Option<f32>,
    collapsed: bool,
    line_height: Option<f32>,
    compact: bool,
    first_row: usize,
) -> ShelfLayout {
    let (padding, extent, gap) = metrics(compact);
    let band = band_height(compact, line_height);
    let rows_max = match available {
        None => count.max(1),
        Some(h) => (((h - padding * 2.0 + gap) / (extent + gap)).floor() as usize).max(1),
    };
    let first_row = first_row.min(count.saturating_sub(rows_max));
    let mut buttons = Vec::with_capacity(count);
    let mut shown = Vec::with_capacity(count);
    for i in 0..count {
        let column = if compact { 0 } else { i / rows_max };
        let row = if compact {
            i as isize - first_row as isize
        } else {
            (i % rows_max) as isize
        };
        buttons.push((
            padding + column as f32 * (extent + gap),
            band + padding + row as f32 * (extent + gap),
        ));
        shown.push(!collapsed && (!compact || (row >= 0 && (row as usize) < rows_max)));
    }
    let rows = count.min(rows_max) as f32;
    let columns = if compact || count == 0 {
        1
    } else {
        count.div_ceil(rows_max)
    } as f32;
    let (width, height) = if collapsed {
        if compact {
            (24.0, 24.0)
        } else {
            (COLLAPSED_WIDTH, BUTTON_EXTENT)
        }
    } else {
        (
            padding * 2.0 + columns * extent + (columns - 1.0).max(0.0) * gap,
            band + padding * 2.0 + rows * extent + (rows - 1.0).max(0.0) * gap,
        )
    };
    ShelfLayout {
        width,
        height,
        band: if collapsed && !compact {
            BUTTON_EXTENT
        } else {
            band
        },
        extent,
        buttons,
        shown,
        visible_rows: rows_max,
        first_row,
    }
}

#[derive(Debug, Clone)]
struct Entry {
    panel: u32,
    button: ElementId,
    shown: bool,
    initials: String,
    has_icon: bool,
}

/// The shelf's controller: its entries and how it lays them out.
#[derive(Debug)]
pub struct PluginShelf {
    /// The shelf (its window's root).
    pub root: ElementId,
    grip: ElementId,
    toggle: ElementId,
    entries: Vec<Entry>,
    collapsed: bool,
    /// The player hid it (Toggle Plugin Manager).
    hidden: bool,
    font: Option<Arc<UiFont>>,
    outbox: PluginOutbox,
    theme: PluginTheme,
    first_row: usize,
    visible_rows: usize,
    /// The player right-clicked the shelf since the window last asked.
    appearance_requested: bool,
}

const HIDDEN_BACKGROUND: Color = Color::rgba(0.025, 0.025, 0.02, 0.96);
const VISIBLE_BACKGROUND: Color = Color::rgba(0.09, 0.19, 0.055, 0.96);
const HIDDEN_BORDER: Color = Color::rgba(0.48, 0.38, 0.14, 1.0);
const VISIBLE_BORDER: Color = Color::rgba(0.76, 0.64, 0.25, 1.0);

impl PluginShelf {
    /// Builds the shelf (hidden until it has entries) at its dock; the
    /// caller registers its window.
    pub fn build(ui: &mut UiRoot, font: Option<Arc<UiFont>>, outbox: PluginOutbox) -> Self {
        let mut e = Element::at(
            DEFAULT_LEFT,
            DEFAULT_TOP,
            BUTTON_EXTENT + OUTER_PADDING * 2.0,
            GRIP_HEIGHT + OUTER_PADDING * 2.0,
        )
        .with_anchors(AnchorEdges::NONE)
        .with_visible(false);
        e.name = Some("PluginShelf".to_owned());
        e.resize_x = false;
        e.resize_y = false;
        e.runtime_tooltip_text = Some(TOOLTIP.to_owned());
        let root = ui.insert(
            e,
            ShelfFrame {
                background: CLASSIC_BACKGROUND,
                border: CLASSIC_BORDER,
            },
        );
        let mut g = Element::at(0.0, 0.0, 0.0, GRIP_HEIGHT).with_anchors(AnchorEdges::NONE);
        g.window_move_handle = true;
        g.name = Some("PluginShelfGrip".to_owned());
        let grip = ui.add(
            root,
            g,
            ShelfGrip {
                compact: false,
                dash: CLASSIC_BORDER,
            },
        );
        let mut t =
            Element::at(0.0, 0.0, TOGGLE_WIDTH, GRIP_HEIGHT).with_anchors(AnchorEdges::NONE);
        t.name = Some("PluginShelfToggle".to_owned());
        let toggle = ui.add(
            root,
            t,
            ShelfToggle {
                button: MarkupButton {
                    text: ">".to_owned(),
                    text_color: TOGGLE_GLYPH,
                    background: Color::TRANSPARENT,
                    border: Color::TRANSPARENT,
                    ..MarkupButton::new(font.clone())
                },
                compact: false,
                collapsed: false,
            },
        );
        Self {
            root,
            grip,
            toggle,
            entries: Vec::new(),
            collapsed: false,
            hidden: false,
            font,
            outbox,
            theme: PluginTheme::Classic,
            first_row: 0,
            visible_rows: 1,
            appearance_requested: false,
        }
    }

    /// Whether the shelf shows: it has a window to offer and the player
    /// has not hidden it (OpenAC `PluginSidePanel.Visible`).
    #[must_use]
    pub fn is_shown(&self) -> bool {
        !self.entries.is_empty() && !self.hidden
    }

    /// The player hides (`true`) or shows the shelf (OpenAC `Hide`,
    /// `Show`).
    pub fn set_hidden(&mut self, ui: &mut UiRoot, hidden: bool) {
        self.hidden = hidden;
        self.reflow(ui);
    }

    /// How many windows have a button.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// The button of panel `panel`.
    #[must_use]
    pub fn button(&self, panel: u32) -> Option<ElementId> {
        self.entries
            .iter()
            .find(|e| e.panel == panel)
            .map(|e| e.button)
    }

    /// Whether the player right-clicked the shelf since the last call (the
    /// window opens the Appearance window).
    pub fn take_appearance_request(&mut self) -> bool {
        std::mem::take(&mut self.appearance_requested)
    }

    /// The plugin theme: a theme other than Classic makes the shelf the
    /// compact column in the theme's colours (scrolled back to its top).
    pub fn set_theme(&mut self, ui: &mut UiRoot, theme: PluginTheme) {
        if self.theme == theme {
            return;
        }
        self.theme = theme;
        self.first_row = 0;
        self.restyle_buttons(ui);
        self.reflow(ui);
    }

    fn compact(&self) -> bool {
        self.theme != PluginTheme::Classic
    }

    /// The panels with a button: the windows that asked for one, in
    /// registration order, each shown or not, with its icon: the plugin's
    /// `icon.png`, else the descriptor's render surface, else initials
    /// (OpenAC `PluginShelfButton`).
    pub fn sync(&mut self, ui: &mut UiRoot, panels: &[PanelView], icons: &mut dyn ShelfIcons) {
        let wanted: Vec<&PanelView> = panels
            .iter()
            .filter(|p| p.descriptor.show_in_side_panel)
            .collect();
        let same = wanted.len() == self.entries.len()
            && wanted
                .iter()
                .zip(&self.entries)
                .all(|(p, e)| p.id == e.panel);
        if !same {
            for entry in self.entries.drain(..) {
                ui.destroy(entry.button);
            }
            for panel in &wanted {
                let mut e = Element::at(0.0, 0.0, BUTTON_EXTENT, BUTTON_EXTENT)
                    .with_anchors(AnchorEdges::NONE);
                e.name = Some(format!("PluginShelfButton:{}", panel.window_name()));
                e.runtime_tooltip_text =
                    Some(if panel.descriptor.title == panel.owner.display_name {
                        panel.descriptor.title.clone()
                    } else {
                        format!(
                            "{} \u{2014} {}",
                            panel.owner.display_name, panel.descriptor.title
                        )
                    });
                let surface = normalize_icon(panel.descriptor.icon_surface_id);
                // A file icon is shown without asking for the surface.
                let resolved = match &panel.file_icon {
                    Some(file) => icons.file(&panel.owner.id, file),
                    None => None,
                }
                .or_else(|| (surface != 0).then(|| icons.surface(surface)).flatten());
                let initials = initials(
                    panel.descriptor.icon_text.as_deref(),
                    &panel.descriptor.title,
                );
                let button = MarkupButton {
                    icon: resolved,
                    ..MarkupButton::new(self.font.clone())
                };
                let id = ui.add(
                    self.root,
                    e,
                    ShelfButton {
                        button,
                        icon_inset: 6.0,
                    },
                );
                self.entries.push(Entry {
                    panel: panel.id,
                    button: id,
                    shown: false,
                    initials,
                    has_icon: resolved.is_some(),
                });
            }
        }
        for (entry, panel) in self.entries.iter_mut().zip(&wanted) {
            entry.shown = panel.shown();
        }
        self.restyle_buttons(ui);
        self.reflow(ui);
    }

    /// Each button in the classic or the theme's look, shown or not
    /// (OpenAC `PluginShelfButton.RefreshPresentation`).
    fn restyle_buttons(&self, ui: &mut UiRoot) {
        let palette = self.theme.palette();
        for entry in &self.entries {
            let Some(w) = ui.widget_mut::<ShelfButton>(entry.button) else {
                continue;
            };
            let initials = if entry.has_icon {
                String::new()
            } else if palette.is_some() {
                entry.initials.chars().take(1).collect()
            } else {
                entry.initials.clone()
            };
            w.button.text = initials;
            match palette {
                Some(p) => {
                    w.button.background = if entry.shown { p.selected } else { p.field };
                    w.button.border = if entry.shown { p.accent } else { p.border };
                    w.button.outline = false;
                    w.button.text_color = p.text;
                    w.icon_inset = 2.0;
                }
                None => {
                    w.button.background = if entry.shown {
                        VISIBLE_BACKGROUND
                    } else {
                        HIDDEN_BACKGROUND
                    };
                    w.button.border = if entry.shown {
                        VISIBLE_BORDER
                    } else {
                        HIDDEN_BORDER
                    };
                    w.button.outline = true;
                    w.button.text_color = Color::WHITE;
                    w.icon_inset = 6.0;
                }
            }
        }
    }

    fn reflow(&mut self, ui: &mut UiRoot) {
        let compact = self.compact();
        let (padding, extent, _) = metrics(compact);
        let (top, parent_height) = (
            ui.element(self.root).map_or(DEFAULT_TOP, |e| e.top),
            ui.size().1,
        );
        let grip = self.font.as_ref().map(|f| f.line_height);
        let band = band_height(compact, grip);
        let available = (parent_height - top - band - padding).max(extent + padding * 2.0);
        let l = layout(
            self.entries.len(),
            Some(available),
            self.collapsed,
            grip,
            compact,
            self.first_row,
        );
        self.first_row = l.first_row;
        self.visible_rows = l.visible_rows;
        for ((entry, (x, y)), shown) in self.entries.iter().zip(&l.buttons).zip(&l.shown) {
            if let Some(e) = ui.element_mut(entry.button) {
                e.left = *x;
                e.top = *y;
                e.width = l.extent;
                e.height = l.extent;
            }
            ui.set_visible(entry.button, *shown);
        }
        if let Some(e) = ui.element_mut(self.root) {
            e.width = l.width;
            e.height = l.height;
        }
        let palette = self.theme.palette();
        if let Some(frame) = ui.widget_mut::<ShelfFrame>(self.root) {
            frame.background = palette.map_or(CLASSIC_BACKGROUND, |p| p.background);
            frame.border = palette.map_or(CLASSIC_BORDER, |p| p.border);
        }
        let muted = palette.map_or(TOGGLE_GLYPH, |p| p.muted);
        if let Some(g) = ui.widget_mut::<ShelfGrip>(self.grip) {
            g.compact = compact;
            g.dash = palette.map_or(CLASSIC_BORDER, |p| p.muted);
        }
        if let Some(t) = ui.widget_mut::<ShelfToggle>(self.toggle) {
            t.compact = compact;
            t.collapsed = self.collapsed;
            t.button.text_color = muted;
            t.button.outline = !compact;
            t.button.text = if compact {
                String::new()
            } else if self.collapsed {
                "<".to_owned()
            } else {
                ">".to_owned()
            };
        }
        // The grip and the toggle (OpenAC `LayoutChrome`).
        let (grip_rect, toggle_rect) = if compact {
            ((1.0, 0.0, extent, 10.0), (1.0, 10.0, extent, 14.0))
        } else {
            (
                (0.0, 0.0, (l.width - TOGGLE_WIDTH).max(0.0), l.band),
                (l.width - TOGGLE_WIDTH, 0.0, TOGGLE_WIDTH, l.band),
            )
        };
        for (id, (x, y, w, h)) in [(self.grip, grip_rect), (self.toggle, toggle_rect)] {
            if let Some(e) = ui.element_mut(id) {
                e.left = x;
                e.top = y;
                e.width = w;
                e.height = h;
            }
        }
        let shown = !self.entries.is_empty() && !self.hidden;
        if ui.window(WINDOW).is_some() {
            if shown != ui.is_window_visible(WINDOW) {
                if shown {
                    ui.show_window(WINDOW);
                } else {
                    ui.hide_window(WINDOW);
                }
            }
        } else {
            ui.set_visible(self.root, shown);
        }
    }
}

impl PanelController for PluginShelf {
    fn on_element_message(
        &mut self,
        ui: &mut UiRoot,
        element: ElementId,
        message: &ElementMessage,
    ) -> bool {
        match message {
            ElementMessage::RightClicked if element == self.root => {
                self.appearance_requested = true;
                return true;
            }
            ElementMessage::Wheel(dy) if element == self.root => {
                if self.compact() && !self.collapsed {
                    let last = self.entries.len().saturating_sub(self.visible_rows);
                    self.first_row = if *dy > 0 {
                        self.first_row.saturating_sub(1)
                    } else {
                        (self.first_row + 1).min(last)
                    };
                    self.reflow(ui);
                }
                return true;
            }
            ElementMessage::Clicked { .. } => {}
            _ => return false,
        }
        if element == self.toggle {
            self.collapsed = !self.collapsed;
            self.reflow(ui);
            return true;
        }
        let Some(entry) = self.entries.iter().find(|e| e.button == element) else {
            return false;
        };
        self.outbox.borrow_mut().push(UiInput::PanelShown {
            panel: entry.panel,
            shown: !entry.shown,
        });
        true
    }
}

/// The shelf's own frame: its fill and border, and the right click (on it
/// or any button) and the wheel, which it tells its controller.
#[derive(Debug, Clone, Copy)]
struct ShelfFrame {
    background: Color,
    border: Color,
}

impl Widget for ShelfFrame {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        match e.kind {
            kind::RIGHT_CLICK => {
                ui.send_message(me, ElementMessage::RightClicked);
                true
            }
            kind::SCROLL => {
                ui.send_message(me, ElementMessage::Wheel(e.data0));
                true
            }
            _ => false,
        }
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height));
        rc.fill(0.0, 0.0, w, h, self.background);
        rc.rect_outline(0.0, 0.0, w, h, self.border, 1.0);
    }
}

/// The grip band: three dashes (two short lines on the compact shelf);
/// dragging it moves the shelf.
#[derive(Debug, Clone, Copy)]
struct ShelfGrip {
    compact: bool,
    dash: Color,
}

impl Widget for ShelfGrip {
    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        if self.compact {
            rc.fill(4.0, 3.0, 8.0, 1.0, self.dash);
            rc.fill(4.0, 6.0, 8.0, 1.0, self.dash);
            return;
        }
        let (w, h) = ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height));
        let total = 5.0 * 3.0 + 4.0 * 2.0;
        let x = ((w - total) * 0.5).max(2.0);
        let y = h * 0.5 - 1.0;
        for i in 0..3 {
            rc.fill(x + i as f32 * 9.0, y, 5.0, 2.0, self.dash);
        }
    }
}

/// The collapse toggle: a caption on the classic shelf, a small chevron
/// on the compact one (OpenAC `ShelfToggleButton`).
#[derive(Debug, Clone)]
struct ShelfToggle {
    button: MarkupButton,
    compact: bool,
    collapsed: bool,
}

impl Widget for ShelfToggle {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        self.button.on_event(ui, me, e)
    }

    fn handles_click(&self, element: &Element) -> bool {
        self.button.handles_click(element)
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        self.button.draw(ui, me, rc);
        if !self.compact {
            return;
        }
        let (w, h) = ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height));
        let direction = if self.collapsed { -1.0 } else { 1.0 };
        let center = w * 0.5;
        for i in 0..3 {
            let x = center + direction * (i as f32 - 1.0);
            rc.fill(
                x,
                h * 0.5 - 3.0 + i as f32,
                1.0,
                1.0,
                self.button.text_color,
            );
            rc.fill(
                x,
                h * 0.5 + 1.0 - i as f32,
                1.0,
                1.0,
                self.button.text_color,
            );
        }
    }
}

/// A shelf button: a markup button whose icon is inset (6 pixels, 2 on
/// the compact shelf), as OpenAC draws it.
#[derive(Debug, Clone)]
struct ShelfButton {
    button: MarkupButton,
    icon_inset: f32,
}

impl Widget for ShelfButton {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        self.button.on_event(ui, me, e)
    }

    fn handles_click(&self, element: &Element) -> bool {
        self.button.handles_click(element)
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let icon = self.button.icon.take();
        self.button.draw(ui, me, rc);
        self.button.icon = icon;
        if let Some((texture, iw, ih)) = icon
            && iw > 0.0
            && ih > 0.0
        {
            let (w, h) = ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height));
            let extent = (w - self.icon_inset).min(h - self.icon_inset);
            rc.sprite(
                texture,
                (w - extent) * 0.5,
                (h - extent) * 0.5,
                extent,
                extent,
                (0.0, 0.0, 1.0, 1.0),
                Color::WHITE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    //! Ported from OpenAC `tests/AcDream.App.Tests/UI/PluginSidePanelTests.cs`
    //! and `PluginSidePanelThemeTests.cs`.

    use super::*;

    #[test]
    fn initials_come_from_the_plugin_or_the_title() {
        // ShelfButton_FallsBackToInitials_WhenTheResolveNeverYieldsATexture,
        // ShelfButton_WithNeitherFileIconNorSurfaceIcon_ShowsInitials ("Test Plugin" -> "TP").
        assert_eq!(initials(None, "Test Plugin"), "TP");
        assert_eq!(initials(Some("  MT "), "Tank"), "MT");
        assert_eq!(initials(Some("ABCD"), "Tank"), "ABC");
        assert_eq!(initials(None, "tank"), "TA");
        assert_eq!(initials(None, "  "), "?");
        assert_eq!(initials(None, "mosswart massacre extra"), "MM");
    }

    #[test]
    fn many_plugins_wrap_into_columns_within_the_screen() {
        // ManyPluginsWrapIntoReachableColumnsWithinTheLiveScreenHeight: a
        // 260-pixel screen, the shelf at 116, twelve windows.
        let available = 260.0 - DEFAULT_TOP - GRIP_HEIGHT - OUTER_PADDING;
        let l = layout(12, Some(available), false, None, false, 0);
        assert!(l.width > 36.0);
        assert!(DEFAULT_TOP + l.height <= 260.0);
        assert!(l.buttons.iter().all(|(_, y)| y + BUTTON_EXTENT <= l.height));
        assert_eq!(l.buttons[0], (4.0, 16.0));
        assert_eq!(l.buttons[3], (36.0, 16.0));
        assert!(l.shown.iter().all(|s| *s));
    }

    #[test]
    fn collapsing_shrinks_to_the_toggle() {
        // Collapse_HidesButtonsAndShrinksWidth_RoundTripsThroughWindowState
        let l = layout(3, None, true, None, false, 0);
        assert_eq!((l.width, l.height), (24.0, 28.0));
        assert!(l.shown.iter().all(|s| !*s));
        let open = layout(3, None, false, None, false, 0);
        assert_eq!(
            (open.width, open.height),
            (36.0, 12.0 + 8.0 + 3.0 * 28.0 + 2.0 * 4.0)
        );
    }

    #[test]
    fn the_compact_shelf_stays_24_pixels_and_scrolls_every_entry_into_view() {
        // PluginSidePanelThemeTests.CompactShelfRemains24PixelsAndScrollsEveryEntryIntoView:
        // a 260-pixel screen, twelve windows, a themed shelf.
        let available = 260.0 - DEFAULT_TOP - COMPACT_BAND - COMPACT_PADDING;
        let top = layout(12, Some(available), false, None, true, 0);
        assert_eq!(top.width, 24.0);
        assert!(DEFAULT_TOP + top.height <= 260.0);
        assert!(top.shown[0]);
        assert!(!top.shown[11]);
        let bottom = layout(12, Some(available), false, None, true, 20);
        assert_eq!(bottom.first_row, 12 - bottom.visible_rows);
        assert!(bottom.shown[11]);
        assert!(!bottom.shown[0]);
        for (i, (x, y)) in bottom.buttons.iter().enumerate() {
            if bottom.shown[i] {
                assert_eq!(*x, 1.0);
                assert!(*y >= COMPACT_BAND);
                assert!(y + COMPACT_EXTENT <= bottom.height);
            }
        }
        assert_eq!(bottom.extent, 22.0);
        // Classic again: columns, every button shown at 28.
        let classic = layout(12, Some(available), false, None, false, 0);
        assert!(classic.width > 36.0);
        assert!(classic.shown.iter().all(|s| *s));
        assert_eq!(classic.extent, 28.0);
        // Collapsed, the compact shelf is a 24-pixel square.
        let collapsed = layout(12, Some(available), true, None, true, 0);
        assert_eq!((collapsed.width, collapsed.height), (24.0, 24.0));
    }

    // ── The button's image: icon.png > IconSurfaceId > IconText ─────────

    use std::cell::RefCell;
    use std::rc::Rc;

    use ac_plugin_ui::{BindingSchema, PanelDescriptor, PluginUi, UiOwner};

    use crate::draw::{DrawCmd, NoSprites, TextureRef};
    use crate::host::UiHost;

    const FILE_TEXTURE: TextureRef = TextureRef::Runtime(7);

    /// The window's textures as the OpenAC tests stub them: a file icon
    /// uploads as texture 7; the surface resolver answers (counting its
    /// calls) or must not run at all.
    struct StubIcons {
        surface: Option<Icon>,
        surface_must_not_run: bool,
        surface_calls: Vec<u32>,
    }

    impl ShelfIcons for StubIcons {
        fn file(&mut self, _owner: &str, icon: &PluginIcon) -> Option<Icon> {
            Some((FILE_TEXTURE, icon.width as f32, icon.height as f32))
        }

        fn surface(&mut self, id: u32) -> Option<Icon> {
            assert!(!self.surface_must_not_run, "the DAT resolver must not run");
            self.surface_calls.push(id);
            self.surface
        }
    }

    /// One "Test Plugin" window on a mounted shelf; returns the host, the
    /// button and its text.
    fn shelf_with(
        descriptor: PanelDescriptor,
        file_icon: Option<PluginIcon>,
        icons: &mut StubIcons,
    ) -> (UiHost, ElementId, String) {
        let mut ui = PluginUi::new();
        ui.set_file_icon("acdream.test", file_icon);
        ui.register_panel(
            &UiOwner::new("acdream.test", "Test Plugin"),
            descriptor,
            "<panel/>",
            BindingSchema::new("Binding"),
        )
        .unwrap();
        let panels: Vec<PanelView> = ui.panels().cloned().collect();
        let mut host = UiHost::new();
        host.root.set_size(800.0, 600.0);
        let mut shelf = PluginShelf::build(&mut host.root, None, Rc::new(RefCell::new(Vec::new())));
        let frame = shelf.root;
        let top = host.root.root();
        host.root.add_child(top, frame);
        host.root
            .register_window(WINDOW, frame, Some(frame), 0)
            .unwrap();
        shelf.sync(&mut host.root, &panels, icons);
        let button = shelf.button(panels[0].id).unwrap();
        let text = host
            .root
            .widget_mut::<ShelfButton>(button)
            .unwrap()
            .button
            .text
            .clone();
        (host, button, text)
    }

    fn sprites_of(host: &mut UiHost, texture: TextureRef) -> Vec<(f32, f32)> {
        host.draw((800.0, 600.0), &NoSprites)
            .main
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Sprite {
                    texture: t, quad, ..
                } if *t == texture => Some((quad.w, quad.h)),
                _ => None,
            })
            .collect()
    }

    fn icon_file() -> PluginIcon {
        PluginIcon {
            width: 64,
            height: 64,
            rgba: std::sync::Arc::new(vec![255; 64 * 64 * 4]),
        }
    }

    #[test]
    fn with_a_file_icon_draws_it_and_never_calls_the_resolver() {
        // ShelfButton_WithAFileIcon_DrawsItAndNeverCallsTheResolver: the
        // descriptor names a surface too, which is never asked for.
        let mut icons = StubIcons {
            surface: None,
            surface_must_not_run: true,
            surface_calls: Vec::new(),
        };
        let descriptor = PanelDescriptor {
            icon_surface_id: 0x165,
            ..PanelDescriptor::new("main", "Test Plugin")
        };
        let (mut host, _, text) = shelf_with(descriptor, Some(icon_file()), &mut icons);
        assert_eq!(text, "");
        // Drawn square, inset 6 pixels in the 28-pixel classic button.
        assert_eq!(sprites_of(&mut host, FILE_TEXTURE), vec![(22.0, 22.0)]);
    }

    #[test]
    fn with_no_file_icon_still_resolves_the_surface_icon() {
        // ShelfButton_WithNoFileIcon_StillResolvesTheSurfaceIcon
        let surface = TextureRef::Surface(0x0600_0165);
        let mut icons = StubIcons {
            surface: Some((surface, 32.0, 32.0)),
            surface_must_not_run: false,
            surface_calls: Vec::new(),
        };
        let descriptor = PanelDescriptor {
            icon_surface_id: 0x165,
            ..PanelDescriptor::new("main", "Test Plugin")
        };
        let (mut host, _, text) = shelf_with(descriptor, None, &mut icons);
        assert!(!icons.surface_calls.is_empty());
        assert_eq!(text, "");
        assert_eq!(sprites_of(&mut host, surface).len(), 1);
    }

    #[test]
    fn with_neither_file_icon_nor_surface_icon_shows_initials() {
        // ShelfButton_WithNeitherFileIconNorSurfaceIcon_ShowsInitials
        let mut icons = StubIcons {
            surface: None,
            surface_must_not_run: true,
            surface_calls: Vec::new(),
        };
        let descriptor = PanelDescriptor {
            icon_text: Some("TP".into()),
            ..PanelDescriptor::new("main", "Test Plugin")
        };
        let (mut host, _, text) = shelf_with(descriptor, None, &mut icons);
        assert_eq!(text, "TP");
        assert!(sprites_of(&mut host, FILE_TEXTURE).is_empty());
    }

    #[test]
    #[ignore = "needs the retail dat files"]
    fn a_shelf_icon_id_resolves_to_a_real_installed_render_surface() {
        // PluginShelfIconInstalledDatTests.AShelfIconId_ResolvesToARealInstalledRenderSurface:
        // the image MossTank and MosswartMassacre name as their shelf icon.
        use crate::draw::SpriteSizes;
        let dir = std::env::var_os("OPENAC_DAT_DIR")
            .map(std::path::PathBuf::from)
            .or_else(ac_dat::default_dat_dir)
            .expect("no dat directory: set OPENAC_DAT_DIR");
        let content = crate::content::DatContent::new(std::sync::Arc::new(
            ac_dat::DatCollection::open(dir).expect("open the retail dats"),
        ))
        .expect("master property table");
        let id = normalize_icon(0x0600_2C41);
        let (w, h) = content.sprite_size(id).expect("a real render surface");
        assert!(w > 0 && h > 0);
    }
}
