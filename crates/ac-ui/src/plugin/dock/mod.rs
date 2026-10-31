//! The plugin dock: a tile that moves it and opens and closes it, one slot per
//! plugin window that asked for one, and a gear that opens and closes the
//! Appearance window. A slot shows its plugin's `icon.png` (else its window's icon, else
//! a monogram) and opens or closes the window. It floats running either way,
//! or snaps to a screen edge as a rail. It draws the same shape in every theme,
//! in the theme's colours or its own under Classic; where things go is worked
//! out by [`layout`] and how they look by [`style`].
//!
//! Closed, the dock is only the tile, with a count of the plugin windows open,
//! and those windows stay as they are. The state is saved per character with
//! the dock's place. With hovering turned on, the pointer on a closed dock also
//! peeks it open until the pointer leaves, which saves nothing. A press and a drag start the same way, on the tile: the
//! root starts moving the window at once, so the dock holds still until the
//! pointer has gone [`PRESS_SLOP`] points, and a release before that is a press.
//!
//! The root sets a dragged window's place from the pointer on every move, so
//! the dock reads that place each frame, decides its mode, and puts itself
//! back on its edge. Pointer input arrives before the frame's tick, so a snap
//! shows in the frame of the move that caused it.

pub mod layout;
pub mod style;

use std::sync::Arc;

use ac_plugin_ui::markup::normalize_icon;
use ac_plugin_ui::{PanelView, PluginIcon, UiInput};

use self::layout::{DockLayout, FADE_LENGTH, Metrics};
pub use self::layout::{DockMode, DockOpens, DockSize};
use self::style::DockSide;
use super::style::PointerState;
use super::widgets::ButtonLook;
use super::{Icon, Palette, PluginOutbox, PluginTheme};
use crate::bundled_font::ModernFonts;
use crate::draw::{Color, DEFAULT_OUTLINE_COLOR, RenderContext};
use crate::event::{UiEvent, kind};
use crate::font::UiFont;
use crate::geometry::{AnchorEdges, Rect};
use crate::host::PanelController;
use crate::layout_persistence::WindowState;
use crate::root::{ElementMessage, UiRoot};
use crate::tree::{Element, ElementId, Widget};

/// The dock's window name. It keeps the name it was first saved under, so
/// placements the player has already saved still apply.
pub const WINDOW: &str = "plugin-shelf";

/// Where the dock goes first: its top.
pub const DEFAULT_TOP: f32 = 116.0;
/// Where the dock goes first: its left edge.
pub const DEFAULT_LEFT: f32 = 10.0;
/// The gear's hover label.
pub const APPEARANCE_LABEL: &str = "Plugin appearance";
/// The tile's hover label.
pub const TILE_LABEL: &str = "Plugins";
/// How long the pointer must stay on a closed dock before it peeks open, seconds.
pub const PEEK_OPEN_SECONDS: f32 = 0.12;
/// How long the pointer must stay away before a peeking dock closes, seconds.
pub const PEEK_CLOSE_SECONDS: f32 = 0.5;
/// How far the pointer may go from where it pressed the tile and still be a
/// press rather than a drag.
pub const PRESS_SLOP: f32 = 4.0;

/// Where the dock's slot images come from: the window's textures.
pub trait DockIcons {
    /// Plugin `owner`'s decoded `icon.png` as a texture.
    fn file(&mut self, owner: &str, icon: &PluginIcon) -> Option<Icon>;
    /// A render surface (a full image id) as a texture.
    fn surface(&mut self, id: u32) -> Option<Icon>;
}

/// The letters a slot shows without an icon, two at most: the plugin's own,
/// else the title's first two letters (one word) or its first two words'
/// first letters, upper case.
#[must_use]
pub fn initials(requested: Option<&str>, title: &str) -> String {
    if let Some(r) = requested.map(str::trim).filter(|r| !r.is_empty()) {
        return r.chars().take(2).collect();
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

/// A slot's hover label: the window's title, and its plugin's name under it
/// unless the two are the same.
fn label_text<'a>(title: &'a str, owner: &'a str) -> (&'a str, Option<&'a str>) {
    (title, (title != owner).then_some(owner))
}

#[derive(Debug, Clone)]
struct Entry {
    panel: u32,
    slot: ElementId,
    /// The window's name.
    window: String,
    /// How long its edge pill is now; it eases toward its state's length.
    pill: f32,
    owner: String,
    owner_name: String,
    title: String,
    open: bool,
}

/// What the window gives the dock to draw with: the tile's icon, the font of its
/// count, and the letters' fonts for the sizes that do not use the title font.
#[derive(Debug, Clone, Default)]
pub struct DockArt {
    /// The OpenAC icon.
    pub icon: Option<Icon>,
    /// The count on a closed tile.
    pub count_font: Option<Arc<UiFont>>,
    /// Monograms on the smaller dock.
    pub smaller_letters: Option<Arc<UiFont>>,
    /// Monograms on the bigger dock.
    pub bigger_letters: Option<Arc<UiFont>>,
}

/// The dock's controller: its entries and how it lays them out.
#[derive(Debug)]
pub struct PluginDock {
    /// The dock (its window's root).
    pub root: ElementId,
    tile: ElementId,
    gear: ElementId,
    entries: Vec<Entry>,
    mode: DockMode,
    size: DockSize,
    /// The player closed it, which is saved. It can still be open while
    /// `peeking`.
    minimized: bool,
    opens: DockOpens,
    /// The pointer has it open for now.
    peeking: bool,
    /// How long the pointer has wanted the peek changed, seconds.
    peek_wait: f32,
    /// A press closed it with the pointer still on it: no peek until the pointer
    /// has left.
    peek_held_off: bool,
    /// The player hid it (Toggle Plugin Manager).
    hidden: bool,
    /// A screen that has no in-game windows covers it.
    covered: bool,
    font: Option<Arc<UiFont>>,
    modern: Option<ModernFonts>,
    art: DockArt,
    outbox: PluginOutbox,
    theme: PluginTheme,
    first: usize,
    layout: DockLayout,
    screen: (f32, f32),
    /// The dock is being dragged, with the closest the pointer has been to its
    /// rail's edge since it was grabbed or snapped there.
    dragging: Option<f32>,
    /// The pointer has gone past [`PRESS_SLOP`] since the drag began.
    moved_in_drag: bool,
    /// Where a closed dock was when it opened, if that made it move to fit:
    /// it goes back there when it closes.
    closed_at: Option<(f32, f32)>,
    /// The mode it had when the drag began.
    mode_before_drag: DockMode,
    /// A drag ended in another mode since the window last asked.
    mode_dragged: bool,
    /// The player right-clicked the dock or pressed the gear since the window
    /// last asked.
    appearance_requested: bool,
}

impl PluginDock {
    /// Builds the dock (hidden until it has entries) at its first place; the
    /// caller registers its window.
    pub fn build(
        ui: &mut UiRoot,
        font: Option<Arc<UiFont>>,
        modern: Option<ModernFonts>,
        outbox: PluginOutbox,
    ) -> Self {
        let mode = DockMode::default();
        let layout = layout::compute(mode, DockSize::default(), false, &[], f32::INFINITY, 0);
        let mut e = Element::at(DEFAULT_LEFT, DEFAULT_TOP, layout.width, layout.height)
            .with_anchors(AnchorEdges::NONE)
            .with_visible(false);
        e.name = Some("PluginDock".to_owned());
        // The dock's size is worked out here, so a restored layout's saved
        // size never applies.
        e.resize_x = false;
        e.resize_y = false;
        let root = ui.insert(e, DockFrame::default());
        let mut t = Element::at(0.0, 0.0, 0.0, 0.0).with_anchors(AnchorEdges::NONE);
        t.window_move_handle = true;
        t.name = Some("PluginDockTile".to_owned());
        let tile = ui.add(root, t, DockTile::default());
        let mut c = Element::at(0.0, 0.0, 0.0, 0.0).with_anchors(AnchorEdges::NONE);
        c.name = Some("PluginDockGear".to_owned());
        let gear = ui.add(root, c, DockGear::default());
        let mut dock = Self {
            root,
            tile,
            gear,
            entries: Vec::new(),
            mode,
            size: DockSize::default(),
            minimized: false,
            opens: DockOpens::default(),
            peeking: false,
            peek_wait: 0.0,
            peek_held_off: false,
            hidden: false,
            covered: false,
            font,
            modern,
            art: DockArt::default(),
            outbox,
            theme: PluginTheme::Classic,
            first: 0,
            layout,
            screen: ui.size(),
            dragging: None,
            moved_in_drag: false,
            closed_at: None,
            mode_before_drag: mode,
            mode_dragged: false,
            appearance_requested: false,
        };
        dock.reflow(ui);
        dock
    }

    /// What it draws with.
    pub fn set_art(&mut self, ui: &mut UiRoot, art: DockArt) {
        self.art = art;
        self.reflow(ui);
    }

    /// How big it is drawn.
    #[must_use]
    pub fn size(&self) -> DockSize {
        self.size
    }

    /// Draws it at `size`.
    pub fn set_size(&mut self, ui: &mut UiRoot, size: DockSize) {
        if self.size != size {
            self.size = size;
            self.reflow(ui);
        }
    }

    /// Whether the dock shows: it has a window to offer and the player has
    /// not hidden it (OpenAC `PluginSidePanel.Visible`).
    #[must_use]
    pub fn is_shown(&self) -> bool {
        !self.entries.is_empty() && !self.hidden
    }

    /// The player hides (`true`) or shows the dock (OpenAC `Hide`, `Show`).
    /// Showing it again also expands it, so the key always brings the slots
    /// back.
    pub fn set_hidden(&mut self, ui: &mut UiRoot, hidden: bool) {
        self.hidden = hidden;
        if !hidden {
            self.set_minimized(ui, false);
        }
        self.reflow(ui);
    }

    /// While a screen with no in-game windows shows (`covered`), the dock
    /// leaves its window as that screen left it, and does not bring itself
    /// back or raise itself.
    pub fn set_covered(&mut self, covered: bool) {
        self.covered = covered;
    }

    /// How many windows have a slot.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// The slot of panel `panel`.
    #[must_use]
    pub fn slot(&self, panel: u32) -> Option<ElementId> {
        self.entries
            .iter()
            .find(|e| e.panel == panel)
            .map(|e| e.slot)
    }

    /// Where its parts are now.
    #[must_use]
    pub fn layout(&self) -> &DockLayout {
        &self.layout
    }

    /// Whether the player has closed it, peeking or not.
    #[must_use]
    pub fn is_minimized(&self) -> bool {
        self.minimized
    }

    /// Whether it shows closed now.
    fn closed(&self) -> bool {
        self.minimized && !self.peeking
    }

    /// How it opens.
    #[must_use]
    pub fn opens(&self) -> DockOpens {
        self.opens
    }

    /// Sets how it opens. Turned off, a peek ends at once.
    pub fn set_opens(&mut self, ui: &mut UiRoot, opens: DockOpens) {
        self.opens = opens;
        if opens == DockOpens::Click && self.peeking {
            self.change_openness(ui, |dock| dock.peeking = false);
        }
    }

    /// Where it is and which way it runs.
    #[must_use]
    pub fn mode(&self) -> DockMode {
        self.mode
    }

    /// Puts the dock in `mode`, as saved or picked: a rail goes to its edge, and a floating dock
    /// that was a rail goes 10 points in from the edge it was on. It keeps its
    /// place along the edge.
    pub fn set_mode(&mut self, ui: &mut UiRoot, mode: DockMode) {
        let from = self.mode;
        if from == mode {
            return;
        }
        self.mode = mode;
        self.reflow(ui);
        let Some((left, top)) = ui.element(self.root).map(|e| (e.left, e.top)) else {
            return;
        };
        let (sw, sh) = self.screen;
        let (w, h) = (self.layout.width, self.layout.height);
        let (left, top) = match from {
            DockMode::Left => (DEFAULT_LEFT, top),
            DockMode::Right => (sw - w - DEFAULT_LEFT, top),
            DockMode::Top => (left, DEFAULT_LEFT),
            DockMode::Bottom => (left, sh - h - DEFAULT_LEFT),
            DockMode::Floating | DockMode::FloatingHorizontal => (left, top),
        };
        if !mode.is_rail() {
            ui.move_window(WINDOW, left, top);
        }
    }

    /// The mode a drag left the dock in, once, when the drag has ended (the
    /// window saves it). A drag in progress does not change what is saved.
    pub fn take_dragged_mode(&mut self) -> Option<DockMode> {
        std::mem::take(&mut self.mode_dragged).then_some(self.mode)
    }

    /// Whether the player right-clicked the dock or pressed the gear since
    /// the last call (the window opens or closes the Appearance window).
    pub fn take_appearance_request(&mut self) -> bool {
        std::mem::take(&mut self.appearance_requested)
    }

    /// The plugin theme: the dock takes its colours and fonts.
    pub fn set_theme(&mut self, ui: &mut UiRoot, theme: PluginTheme) {
        if self.theme == theme {
            return;
        }
        self.theme = theme;
        self.reflow(ui);
    }

    /// The colours it draws in: the theme's, or its own under Classic.
    fn palette(&self) -> Palette {
        self.theme.palette().unwrap_or(Palette::CLASSIC_DOCK)
    }

    /// The font for monograms and label titles: the bundled title weight
    /// under a theme, the interface font in Classic.
    fn title_font(&self) -> Option<Arc<UiFont>> {
        match (&self.modern, self.theme) {
            (Some(m), theme) if theme != PluginTheme::Classic => Some(Arc::clone(&m.semibold)),
            _ => self.font.clone(),
        }
    }

    /// The font for a monogram's letters, which fits the art at this size.
    fn letters_font(&self) -> Option<Arc<UiFont>> {
        let sized = match self.size {
            DockSize::Smaller => self.art.smaller_letters.clone(),
            DockSize::Default => None,
            DockSize::Bigger => self.art.bigger_letters.clone(),
        };
        sized
            .filter(|_| self.theme != PluginTheme::Classic)
            .or_else(|| self.title_font())
    }

    /// The font for the line under a label's title.
    fn body_font(&self) -> Option<Arc<UiFont>> {
        match (&self.modern, self.theme) {
            (Some(m), theme) if theme != PluginTheme::Classic => Some(Arc::clone(&m.regular)),
            _ => self.font.clone(),
        }
    }

    /// The panels with a slot: the windows that asked for one, in
    /// registration order, each open or not, with its icon: the plugin's
    /// `icon.png`, else the descriptor's render surface, else a monogram
    /// (OpenAC `PluginShelfButton`).
    pub fn sync(&mut self, ui: &mut UiRoot, panels: &[PanelView], icons: &mut dyn DockIcons) {
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
                ui.destroy(entry.slot);
            }
            for panel in &wanted {
                let mut e = Element::at(0.0, 0.0, 0.0, 0.0).with_anchors(AnchorEdges::NONE);
                e.name = Some(format!("PluginDockSlot:{}", panel.window_name()));
                let surface = normalize_icon(panel.descriptor.icon_surface_id);
                // A file icon is shown without asking for the surface.
                let icon = match &panel.file_icon {
                    Some(file) => icons.file(&panel.owner.id, file),
                    None => None,
                }
                .or_else(|| (surface != 0).then(|| icons.surface(surface)).flatten());
                let letters = initials(
                    panel.descriptor.icon_text.as_deref(),
                    &panel.descriptor.title,
                );
                let slot = DockSlot {
                    icon,
                    letters,
                    hue: style::monogram_hue(&panel.owner.id),
                    ..DockSlot::default()
                };
                let id = ui.add(self.root, e, slot);
                self.entries.push(Entry {
                    panel: panel.id,
                    slot: id,
                    window: panel.window_name(),
                    pill: 0.0,
                    owner: panel.owner.id.clone(),
                    owner_name: panel.owner.display_name.clone(),
                    title: panel.descriptor.title.clone(),
                    open: false,
                });
            }
        }
        for (entry, panel) in self.entries.iter_mut().zip(&wanted) {
            entry.open = panel.shown();
            entry.title.clone_from(&panel.descriptor.title);
        }
        self.reflow(ui);
    }

    /// Lays the dock out for its mode and the screen, and tells its parts how
    /// to look.
    fn reflow(&mut self, ui: &mut UiRoot) {
        self.screen = ui.size();
        let owners: Vec<&str> = self.entries.iter().map(|e| e.owner.as_str()).collect();
        let longest = layout::max_length(self.mode, self.screen.0, self.screen.1);
        let closed = self.closed();
        let l = layout::compute(self.mode, self.size, closed, &owners, longest, self.first);
        self.first = l.first;
        let palette = self.palette();
        let metrics = self.size.metrics();
        let letters = self.letters_font();
        for (entry, slot) in self.entries.iter().zip(&l.slots) {
            if let Some(r) = slot {
                place(ui, entry.slot, *r);
            }
            ui.set_visible(entry.slot, slot.is_some());
            if let Some(w) = ui.widget_mut::<DockSlot>(entry.slot) {
                w.palette = palette;
                w.mode = self.mode;
                w.open = entry.open;
                w.metrics = metrics;
                w.font.clone_from(&letters);
                // A slot put away is not left looking hovered for when it
                // comes back.
                if slot.is_none() {
                    w.pointer = PointerState::default();
                }
            }
        }
        place(ui, self.tile, l.tile);
        place(ui, self.gear, l.gear);
        ui.set_visible(self.gear, !closed);
        if let Some(e) = ui.element_mut(self.root) {
            e.width = l.width;
            e.height = l.height;
        }
        let open_count = self.entries.iter().filter(|e| e.open).count();
        let (app_icon, count_font) = (self.art.icon, self.art.count_font.clone());
        if let Some(w) = ui.widget_mut::<DockTile>(self.tile) {
            w.palette = palette;
            w.mode = self.mode;
            w.metrics = metrics;
            w.icon = app_icon;
            w.count_font = count_font;
            w.count = if closed { open_count } else { 0 };
            w.closed = closed;
        }
        if let Some(w) = ui.widget_mut::<DockGear>(self.gear) {
            w.palette = palette;
            w.mode = self.mode;
            w.metrics = metrics;
        }
        if let Some(frame) = ui.widget_mut::<DockFrame>(self.root) {
            frame.palette = palette;
            frame.mode = self.mode;
            frame.metrics = metrics;
            frame.minimized = closed;
            frame.dividers.clone_from(&l.dividers);
            frame.fades = (l.fade_start, l.fade_end);
            frame.list = (l.list_start, l.list_end);
        }
        self.layout = l;
        self.mark_slots(ui);
        self.keep_in_place(ui);
        self.follow_pointer(ui);
        self.apply_visibility(ui);
    }

    /// Tells the frame what to draw beside each slot.
    fn mark_slots(&self, ui: &mut UiRoot) {
        let marks: Vec<SlotMark> = self
            .entries
            .iter()
            .zip(&self.layout.slots)
            .filter_map(|(entry, slot)| {
                slot.map(|slot| SlotMark {
                    slot,
                    open: entry.open,
                    pill: entry.pill,
                })
            })
            .collect();
        if let Some(frame) = ui.widget_mut::<DockFrame>(self.root) {
            frame.marks = marks;
        }
    }

    /// Whether the window shows matches whether the dock should.
    fn apply_visibility(&mut self, ui: &mut UiRoot) {
        if self.covered {
            return;
        }
        let shown = self.is_shown();
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

    /// A rail stays on its edge, and a dock that grew, or a screen that
    /// shrank, leaves it where it all fits.
    fn keep_in_place(&self, ui: &mut UiRoot) {
        let (sw, sh) = self.screen;
        let (w, h) = (self.layout.width, self.layout.height);
        let (far_x, far_y) = ((sw - w).max(0.0), (sh - h).max(0.0));
        if let Some(e) = ui.element_mut(self.root) {
            e.left = e.left.clamp(0.0, far_x);
            e.top = e.top.clamp(0.0, far_y);
            match self.mode {
                DockMode::Left => e.left = 0.0,
                DockMode::Right => e.left = far_x,
                DockMode::Top => e.top = 0.0,
                DockMode::Bottom => e.top = far_y,
                DockMode::Floating | DockMode::FloatingHorizontal => {}
            }
        }
    }

    /// Opens or closes the dock for the player. A press that closes it leaves the
    /// pointer on the tile, which must not peek it straight open again.
    fn set_minimized(&mut self, ui: &mut UiRoot, minimized: bool) {
        self.peek_held_off = minimized;
        self.change_openness(ui, |dock| {
            dock.minimized = minimized;
            dock.peeking = false;
        });
    }

    /// Makes `change`, and lays the dock out for it. A dock that has to move to
    /// fit when it opens goes back to where it was when it closes.
    fn change_openness(&mut self, ui: &mut UiRoot, change: impl FnOnce(&mut Self)) {
        let was_closed = self.closed();
        let before = ui.element(self.root).map(|e| (e.left, e.top));
        change(self);
        self.reflow(ui);
        let closed = self.closed();
        if closed && !was_closed {
            if let (Some((left, top)), Some(e)) = (self.closed_at.take(), ui.element_mut(self.root))
            {
                e.left = left;
                e.top = top;
            }
            self.keep_in_place(ui);
        } else if !closed && was_closed {
            let after = ui.element(self.root).map(|e| (e.left, e.top));
            self.closed_at = before.filter(|_| before != after);
        }
    }

    /// With hovering on, the pointer on a closed dock peeks it open after a
    /// moment, and away from it closes it again after a longer one. Nothing peeks
    /// open while something else has the pointer's attention, and a peek stays
    /// open while it is being dragged.
    fn follow_peek(&mut self, ui: &mut UiRoot, dt: f32) {
        if self.opens != DockOpens::Hover || !self.minimized {
            self.peek_wait = 0.0;
            return;
        }
        let (mx, my) = ui.mouse();
        let (mx, my) = (mx as f32, my as f32);
        let over = ui.element(self.root).is_some_and(|e| {
            e.visible()
                && mx >= e.left
                && mx < e.left + e.width
                && my >= e.top
                && my < e.top + e.height
        });
        if !over {
            self.peek_held_off = false;
        }
        let held = ui
            .widget::<DockTile>(self.tile)
            .is_some_and(|t| t.held.is_some());
        let busy = held || ui.is_window_move_active() || ui.modal.is_some() || self.covered;
        let want = if busy {
            self.peeking
        } else {
            over && !self.peek_held_off
        };
        if want == self.peeking {
            self.peek_wait = 0.0;
            return;
        }
        self.peek_wait += dt.max(0.0);
        let delay = if want {
            PEEK_OPEN_SECONDS
        } else {
            PEEK_CLOSE_SECONDS
        };
        if self.peek_wait >= delay {
            self.peek_wait = 0.0;
            self.change_openness(ui, |dock| dock.peeking = want);
        }
    }

    /// While the tile is held the dock takes the mode its place and the pointer
    /// say, once the pointer has gone far enough for this to be a drag. The
    /// drag's last move is followed in the frame after it ends; a whole drag can
    /// fall between two frames when a frame is slow. Released before that, it
    /// was a press.
    fn follow_drag(&mut self, ui: &mut UiRoot) {
        let Some(held) = ui.widget::<DockTile>(self.tile).and_then(|t| t.held) else {
            self.dragging = None;
            return;
        };
        let moving = ui.is_window_move_active();
        let (mx, my) = ui.mouse();
        let pointer = (mx as f32, my as f32);
        let start = (held.pointer.0 as f32, held.pointer.1 as f32);
        let from_edge = |mode, at| layout::pointer_from_edge(mode, self.screen, at);
        if self.dragging.is_none() {
            self.mode_before_drag = self.mode;
            self.moved_in_drag = false;
        }
        let far = (pointer.0 - start.0).hypot(pointer.1 - start.1) >= PRESS_SLOP;
        self.moved_in_drag = self.moved_in_drag || far;
        let closest = self
            .dragging
            .or_else(|| from_edge(self.mode, start))
            .unwrap_or(f32::INFINITY);
        let closest = from_edge(self.mode, pointer).map_or(closest, |d| closest.min(d));
        if !self.moved_in_drag {
            if let Some(e) = ui.element_mut(self.root) {
                (e.left, e.top) = held.at;
            }
            self.dragging = moving.then_some(closest);
            if !moving {
                self.release_tile(ui);
                let minimized = !self.minimized;
                self.set_minimized(ui, minimized);
            }
            return;
        }
        self.closed_at = None;
        let Some(at) = ui
            .element(self.root)
            .map(|e| Rect::new(e.left, e.top, e.width, e.height))
        else {
            return;
        };
        let next = layout::dragged_mode(self.mode, at, self.screen, pointer, closest);
        let closest = if next == self.mode {
            closest
        } else {
            from_edge(next, pointer).unwrap_or(f32::INFINITY)
        };
        if next != self.mode {
            self.mode = next;
            self.reflow(ui);
        }
        self.dragging = moving.then_some(closest);
        if !moving {
            self.mode_dragged = self.mode != self.mode_before_drag;
            self.release_tile(ui);
        }
    }

    fn release_tile(&mut self, ui: &mut UiRoot) {
        self.dragging = None;
        if let Some(t) = ui.widget_mut::<DockTile>(self.tile) {
            t.held = None;
        }
    }

    /// The plugin window drawn over the dock's other open windows.
    fn front_window(&self, ui: &UiRoot) -> Option<usize> {
        let mut front: Option<(usize, i32)> = None;
        for (i, entry) in self.entries.iter().enumerate() {
            if !entry.open {
                continue;
            }
            let Some(z) = ui
                .window(&entry.window)
                .and_then(|w| ui.element(w.frame))
                .map(Element::z_order)
            else {
                continue;
            };
            if front.is_none_or(|(_, top)| z >= top) {
                front = Some((i, z));
            }
        }
        front.map(|(i, _)| i)
    }

    /// Each edge pill eases toward its state's length: tallest beside the
    /// front window, then an open window, then under the pointer.
    fn ease_pills(&mut self, ui: &mut UiRoot, dt: f32) {
        if !self.mode.is_rail() {
            return;
        }
        let front = self.front_window(ui);
        let step = style::PILL_FRONT / style::PILL_SECONDS * dt.max(0.0);
        for i in 0..self.entries.len() {
            let entry = &self.entries[i];
            let hovered = ui
                .widget::<DockSlot>(entry.slot)
                .is_some_and(|s| s.pointer.look(true) != ButtonLook::Normal);
            let target = if front == Some(i) {
                style::PILL_FRONT
            } else if entry.open {
                style::PILL_OPEN
            } else if hovered {
                style::PILL_HOVER
            } else {
                0.0
            };
            let entry = &mut self.entries[i];
            entry.pill += (target - entry.pill).clamp(-step, step);
        }
        self.mark_slots(ui);
    }

    /// The screen edge the dock faces: a rail's own edge, else the nearer of
    /// the two its slots' sides face.
    fn side(&self, ui: &UiRoot) -> DockSide {
        let (left, top) = ui
            .element(self.root)
            .map_or((0.0, 0.0), |e| (e.left, e.top));
        let (sw, sh) = self.screen;
        let (w, h) = (self.layout.width, self.layout.height);
        match self.mode {
            DockMode::Left => DockSide::Left,
            DockMode::Right => DockSide::Right,
            DockMode::Top => DockSide::Top,
            DockMode::Bottom => DockSide::Bottom,
            DockMode::Floating if left + w / 2.0 > sw / 2.0 => DockSide::Right,
            DockMode::Floating => DockSide::Left,
            DockMode::FloatingHorizontal if top + h / 2.0 > sh / 2.0 => DockSide::Bottom,
            DockMode::FloatingHorizontal => DockSide::Top,
        }
    }

    /// What follows the pointer each frame: the tile's look and the hover
    /// label.
    fn follow_pointer(&mut self, ui: &mut UiRoot) {
        let (mx, my) = ui.mouse();
        let (mx, my) = (mx as f32, my as f32);
        let tile = self.layout.tile;
        let over = ui.element(self.root).is_some_and(|e| {
            let (x, y) = (mx - e.left - tile.x, my - e.top - tile.y);
            e.visible() && x >= 0.0 && x < tile.w && y >= 0.0 && y < tile.h
        });
        if let Some(t) = ui.widget_mut::<DockTile>(self.tile) {
            t.hovered = over;
        }
        let side = self.side(ui);
        let label = self.hover_label(ui, side, over);
        if let Some(frame) = ui.widget_mut::<DockFrame>(self.root) {
            frame.side = side;
            frame.label = label;
        }
    }

    /// The label of the tile, slot or gear under the pointer.
    fn hover_label(&self, ui: &UiRoot, side: DockSide, over_tile: bool) -> Option<HoverLabel> {
        let held = ui
            .widget::<DockTile>(self.tile)
            .is_some_and(|t| t.held.is_some());
        let hovered = |look: Option<ButtonLook>| {
            matches!(look, Some(ButtonLook::Hovered | ButtonLook::Pressed))
        };
        let (slot, title, subtitle) = if over_tile {
            if held {
                return None;
            }
            (self.layout.tile, TILE_LABEL, None)
        } else if self.closed() {
            return None;
        } else if hovered(
            ui.widget::<DockGear>(self.gear)
                .map(|g| g.pointer.look(true)),
        ) {
            (self.layout.gear, APPEARANCE_LABEL, None)
        } else {
            let (entry, slot) = self.entries.iter().zip(&self.layout.slots).find(|(e, s)| {
                s.is_some() && hovered(ui.widget::<DockSlot>(e.slot).map(|w| w.pointer.look(true)))
            })?;
            let (title, subtitle) = label_text(&entry.title, &entry.owner_name);
            ((*slot)?, title, subtitle)
        };
        let title_font = self.title_font();
        let body_font = self.body_font();
        let rect = self.label_rect(ui, side, slot, title, subtitle);
        Some(HoverLabel {
            rect,
            title: title.to_owned(),
            subtitle: subtitle.map(str::to_owned),
            title_font,
            body_font,
        })
    }

    /// Where a slot's label goes in the dock's space: beside the slot, on the
    /// side away from the screen edge, and kept on the screen both ways.
    fn label_rect(
        &self,
        ui: &UiRoot,
        side: DockSide,
        slot: Rect,
        title: &str,
        subtitle: Option<&str>,
    ) -> Rect {
        let title_font = self.title_font();
        let body_font = self.body_font();
        let measure = |font: &Option<Arc<UiFont>>, text: &str| {
            font.as_ref().map_or(0.0, |f| f.measure_width(text))
        };
        let line = |font: &Option<Arc<UiFont>>| font.as_ref().map_or(12.0, |f| f.line_height);
        let text =
            measure(&title_font, title).max(subtitle.map_or(0.0, |s| measure(&body_font, s)));
        let lines = line(&title_font) + subtitle.map_or(0.0, |_| line(&body_font) + 1.0);
        let w = (text + 2.0 * style::LABEL_PAD_X).ceil();
        let h = (lines + 2.0 * style::LABEL_PAD_Y).ceil();
        let (dw, dh) = (self.layout.width, self.layout.height);
        let along_x = (slot.x + (slot.w - w) / 2.0).round();
        let along_y = (slot.y + (slot.h - h) / 2.0).round();
        let (x, y) = match side {
            DockSide::Left => (dw + style::LABEL_GAP, along_y),
            DockSide::Right => (-style::LABEL_GAP - w, along_y),
            DockSide::Top => (along_x, dh + style::LABEL_GAP),
            DockSide::Bottom => (along_x, -style::LABEL_GAP - h),
        };
        let (left, top) = ui
            .element(self.root)
            .map_or((0.0, 0.0), |e| (e.left, e.top));
        let (sw, sh) = self.screen;
        Rect::new(
            (left + x).clamp(0.0, (sw - w).max(0.0)) - left,
            (top + y).clamp(0.0, (sh - h).max(0.0)) - top,
            w,
            h,
        )
    }

    /// The dock sits over the other windows, whichever was raised last. A
    /// modal window keeps the top.
    fn keep_on_top(&self, ui: &mut UiRoot) {
        if ui.modal.is_none() && self.is_shown() && !self.covered {
            ui.bring_to_front(self.root);
        }
    }
}

/// Puts `id` at `r` in its parent.
fn place(ui: &mut UiRoot, id: ElementId, r: Rect) {
    if let Some(e) = ui.element_mut(id) {
        e.left = r.x;
        e.top = r.y;
        e.width = r.w;
        e.height = r.h;
    }
}

impl PanelController for PluginDock {
    /// Minimized or not is saved per character, with its place.
    fn capture_window_state(&self, _ui: &UiRoot) -> Option<WindowState> {
        Some(WindowState {
            collapsed: self.minimized,
            ..WindowState::default()
        })
    }

    fn restore_window_state(&mut self, ui: &mut UiRoot, state: &WindowState) -> bool {
        self.closed_at = None;
        self.peeking = false;
        self.minimized = state.collapsed;
        self.reflow(ui);
        true
    }

    fn tick(&mut self, ui: &mut UiRoot, dt: f64) {
        self.follow_drag(ui);
        let size = ui.element(self.root).map(|e| (e.width, e.height));
        if self.screen != ui.size() || size != Some((self.layout.width, self.layout.height)) {
            self.reflow(ui);
        }
        self.follow_peek(ui, dt as f32);
        self.keep_in_place(ui);
        self.ease_pills(ui, dt as f32);
        self.follow_pointer(ui);
        self.apply_visibility(ui);
        self.keep_on_top(ui);
    }

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
                if !self.closed() && self.layout.max_first > 0 {
                    self.first = if *dy > 0 {
                        self.first.saturating_sub(1)
                    } else {
                        (self.first + 1).min(self.layout.max_first)
                    };
                    self.reflow(ui);
                }
                return true;
            }
            ElementMessage::Clicked { .. } => {}
            _ => return false,
        }
        if element == self.tile {
            let minimized = !self.minimized;
            self.set_minimized(ui, minimized);
            return true;
        }
        if element == self.gear {
            self.appearance_requested = true;
            return true;
        }
        let Some(entry) = self.entries.iter().find(|e| e.slot == element) else {
            return false;
        };
        self.outbox.borrow_mut().push(UiInput::PanelShown {
            panel: entry.panel,
            shown: !entry.open,
        });
        true
    }
}

/// Where a slot is on the dock, whether its window is open and how long its
/// edge pill is.
#[derive(Debug, Clone, Copy)]
struct SlotMark {
    slot: Rect,
    open: bool,
    pill: f32,
}

/// What a hover label says and where it goes.
#[derive(Debug, Clone)]
struct HoverLabel {
    rect: Rect,
    title: String,
    subtitle: Option<String>,
    title_font: Option<Arc<UiFont>>,
    body_font: Option<Arc<UiFont>>,
}

/// The dock's own frame: its body, what is drawn beside each slot, the fades
/// and the hover label; and the right click (on it or any slot) and the wheel,
/// which it tells its controller.
#[derive(Debug, Clone)]
struct DockFrame {
    palette: Palette,
    mode: DockMode,
    metrics: Metrics,
    side: DockSide,
    minimized: bool,
    dividers: Vec<f32>,
    marks: Vec<SlotMark>,
    fades: (bool, bool),
    list: (f32, f32),
    label: Option<HoverLabel>,
}

impl Default for DockFrame {
    fn default() -> Self {
        Self {
            palette: Palette::CLASSIC_DOCK,
            mode: DockMode::default(),
            metrics: DockSize::default().metrics(),
            side: DockSide::default(),
            minimized: false,
            dividers: Vec::new(),
            marks: Vec::new(),
            fades: (false, false),
            list: (0.0, 0.0),
            label: None,
        }
    }
}

fn element_size(ui: &UiRoot, me: ElementId) -> (f32, f32) {
    ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height))
}

impl Widget for DockFrame {
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
        let size = element_size(ui, me);
        style::body(rc, &self.palette, self.mode, &self.metrics, size.0, size.1);
        for at in &self.dividers {
            style::divider(rc, &self.palette, self.mode, size, *at);
        }
        if self.minimized {
            return;
        }
        let p = &self.palette;
        for mark in &self.marks {
            if self.mode.is_rail() {
                let color = if mark.open { p.accent } else { p.muted };
                style::edge_pill(rc, color, self.side, size, mark.slot, mark.pill);
            } else if mark.open {
                style::open_dot(rc, p, self.side, size, mark.slot);
            }
        }
    }

    fn draw_after_children(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        if self.minimized {
            return;
        }
        let size = element_size(ui, me);
        if self.fades.0 {
            let at = self.list.0;
            style::fade(rc, &self.palette, self.mode, size, at, FADE_LENGTH, false);
        }
        if self.fades.1 {
            let at = self.list.1 - FADE_LENGTH;
            style::fade(rc, &self.palette, self.mode, size, at, FADE_LENGTH, true);
        }
    }

    fn draw_overlay(&mut self, _ui: &mut UiRoot, _me: ElementId, rc: &mut RenderContext<'_>) {
        let Some(label) = &self.label else {
            return;
        };
        let p = &self.palette;
        style::hover_label(rc, p, label.rect);
        let x = label.rect.x + style::LABEL_PAD_X;
        let mut y = label.rect.y + style::LABEL_PAD_Y;
        if let Some(font) = &label.title_font {
            rc.string_dat(
                font,
                &label.title,
                x,
                y,
                p.text,
                false,
                DEFAULT_OUTLINE_COLOR,
            );
            y += font.line_height + 1.0;
        }
        if let (Some(font), Some(subtitle)) = (&label.body_font, &label.subtitle) {
            rc.string_dat(font, subtitle, x, y, p.muted, false, DEFAULT_OUTLINE_COLOR);
        }
    }

    /// The hover label is drawn outside the dock.
    fn expands_clip_for_popup(&self) -> bool {
        true
    }
}

/// A press and a release on the element send it [`ElementMessage::Clicked`].
fn click_event(pointer: &mut PointerState, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
    if e.target != Some(me) {
        return false;
    }
    pointer.observe(e);
    if e.kind != kind::CLICK {
        return false;
    }
    ui.send_message(
        me,
        ElementMessage::Clicked {
            x: e.data1,
            y: e.data2,
        },
    );
    true
}

/// Where the left button went down on the tile and where the dock was then.
#[derive(Debug, Clone, Copy)]
struct TileHold {
    pointer: (i32, i32),
    at: (f32, f32),
}

/// The tile: the OpenAC icon, which moves the dock and opens and closes it, with
/// a count of open plugin windows while closed. While windows are locked the
/// root does not move the dock and sends an ordinary click, so a press still
/// opens and closes it.
#[derive(Debug, Clone)]
struct DockTile {
    palette: Palette,
    mode: DockMode,
    metrics: Metrics,
    hovered: bool,
    held: Option<TileHold>,
    closed: bool,
    icon: Option<Icon>,
    count: usize,
    count_font: Option<Arc<UiFont>>,
}

impl Default for DockTile {
    fn default() -> Self {
        Self {
            palette: Palette::CLASSIC_DOCK,
            mode: DockMode::default(),
            metrics: DockSize::default().metrics(),
            hovered: false,
            held: None,
            closed: false,
            icon: None,
            count: 0,
            count_font: None,
        }
    }
}

impl Widget for DockTile {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        if e.target != Some(me) {
            return false;
        }
        match e.kind {
            kind::MOUSE_DOWN if !ui.ui_locked() => {
                let at = ui
                    .parent(me)
                    .and_then(|p| ui.element(p))
                    .map_or((0.0, 0.0), |e| (e.left, e.top));
                self.held = Some(TileHold {
                    pointer: ui.mouse(),
                    at,
                });
                false
            }
            kind::CLICK => {
                ui.send_message(
                    me,
                    ElementMessage::Clicked {
                        x: e.data1,
                        y: e.data2,
                    },
                );
                true
            }
            _ => false,
        }
    }

    fn handles_click(&self, _element: &Element) -> bool {
        true
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = element_size(ui, me);
        let look = match (self.held.is_some(), self.hovered) {
            (true, _) => ButtonLook::Pressed,
            (false, true) => ButtonLook::Hovered,
            (false, false) => ButtonLook::Normal,
        };
        let well = if self.closed {
            Rect::new(3.0, 3.0, w - 6.0, h - 6.0)
        } else {
            style::slot_well_rect(self.mode, w, h)
        };
        style::slot_well(rc, &self.palette, well, self.metrics.well_radius, look);
        if let Some((texture, iw, ih)) = self.icon
            && iw > 0.0
            && ih > 0.0
        {
            let art = self.metrics.art;
            let (x, y) = ((w - art) / 2.0, (h - art) / 2.0);
            rc.sprite(texture, x, y, art, art, (0.0, 0.0, 1.0, 1.0), Color::WHITE);
        }
        if self.count > 0 {
            style::count_badge(rc, &self.palette, self.count_font.as_ref(), w, self.count);
        }
    }
}

/// The gear slot, which opens and closes the Appearance window. Its hover
/// label says what it is.
#[derive(Debug, Clone)]
struct DockGear {
    palette: Palette,
    pointer: PointerState,
    mode: DockMode,
    metrics: Metrics,
}

impl Default for DockGear {
    fn default() -> Self {
        Self {
            palette: Palette::CLASSIC_DOCK,
            pointer: PointerState::default(),
            mode: DockMode::default(),
            metrics: DockSize::default().metrics(),
        }
    }
}

impl Widget for DockGear {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        click_event(&mut self.pointer, ui, me, e)
    }

    fn handles_click(&self, _element: &Element) -> bool {
        true
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = element_size(ui, me);
        let p = &self.palette;
        let look = self.pointer.look(true);
        let radius = self.metrics.well_radius;
        style::slot_well(rc, p, style::slot_well_rect(self.mode, w, h), radius, look);
        let color = if look == ButtonLook::Normal {
            p.muted
        } else {
            p.text
        };
        let art = self.metrics.art;
        style::gear(rc, (w - art) / 2.0, (h - art) / 2.0, art, color);
    }
}

/// One plugin window's slot: its art, which opens or closes the window. Its
/// hover label names the window.
#[derive(Debug, Clone)]
struct DockSlot {
    palette: Palette,
    pointer: PointerState,
    mode: DockMode,
    metrics: Metrics,
    open: bool,
    icon: Option<Icon>,
    letters: String,
    hue: Color,
    font: Option<Arc<UiFont>>,
}

impl Default for DockSlot {
    fn default() -> Self {
        Self {
            palette: Palette::CLASSIC_DOCK,
            pointer: PointerState::default(),
            mode: DockMode::default(),
            metrics: DockSize::default().metrics(),
            open: false,
            icon: None,
            letters: String::new(),
            hue: Color::BLACK,
            font: None,
        }
    }
}

impl Widget for DockSlot {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        click_event(&mut self.pointer, ui, me, e)
    }

    fn handles_click(&self, _element: &Element) -> bool {
        true
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = element_size(ui, me);
        let look = self.pointer.look(true);
        style::slot_well(
            rc,
            &self.palette,
            style::slot_well_rect(self.mode, w, h),
            self.metrics.well_radius,
            look,
        );
        let alpha = if self.open || look != ButtonLook::Normal {
            1.0
        } else {
            style::CLOSED_ART_ALPHA
        };
        let art = self.metrics.art;
        let (x, y) = ((w - art) / 2.0, (h - art) / 2.0);
        match self.icon {
            Some((texture, iw, ih)) if iw > 0.0 && ih > 0.0 => {
                let tint = Color::rgba(1.0, 1.0, 1.0, alpha);
                rc.sprite(texture, x, y, art, art, (0.0, 0.0, 1.0, 1.0), tint);
            }
            _ => style::monogram(
                rc,
                self.font.as_ref(),
                &self.letters,
                self.hue,
                x,
                y,
                art,
                alpha,
            ),
        }
    }
}

#[cfg(test)]
mod tests;
