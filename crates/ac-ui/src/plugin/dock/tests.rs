//! The dock's behaviour, on a real root with a real plugin UI copy.

use std::cell::RefCell;
use std::rc::Rc;

use ac_plugin_ui::{BindingSchema, PanelDescriptor, PluginUi, UiOwner};

use super::layout::{self, DockLayout};
use super::*;
use crate::draw::{DrawCmd, DrawList, NoSprites, TextureRef};
use crate::event::MouseButton;
use crate::host::UiHost;
use crate::plugin::style::tests::{corners as draw_corners, has_color};
use crate::plugin::style::{hover, mix};
use crate::shapes::CornerRadii;

const FILE_TEXTURE: TextureRef = TextureRef::Runtime(7);
const SCREEN: (f32, f32) = (800.0, 600.0);

/// The window's textures as a test wants them: a file icon uploads as texture
/// 7; the surface resolver answers (counting its calls) or must not run.
struct StubIcons {
    surface: Option<Icon>,
    surface_must_not_run: bool,
    surface_calls: Vec<u32>,
}

impl StubIcons {
    fn none() -> Self {
        Self {
            surface: None,
            surface_must_not_run: false,
            surface_calls: Vec::new(),
        }
    }
}

impl DockIcons for StubIcons {
    fn file(&mut self, _owner: &str, icon: &PluginIcon) -> Option<Icon> {
        Some((FILE_TEXTURE, icon.width as f32, icon.height as f32))
    }

    fn surface(&mut self, id: u32) -> Option<Icon> {
        assert!(!self.surface_must_not_run, "the dat resolver must not run");
        self.surface_calls.push(id);
        self.surface
    }
}

/// Where a slot is and whether it shows.
type SlotPlace = (f32, f32, bool);

/// A dock on a host, and the plugin UI copy it was synced from.
struct Rig {
    host: UiHost,
    ui: PluginUi,
    outbox: PluginOutbox,
    now: i64,
}

impl Rig {
    fn new(height: f32, theme: PluginTheme) -> Self {
        let mut host = UiHost::new();
        host.root.set_size(SCREEN.0, height);
        let outbox: PluginOutbox = Rc::new(RefCell::new(Vec::new()));
        let mut dock = PluginDock::build(&mut host.root, None, None, Rc::clone(&outbox));
        let frame = dock.root;
        let top = host.root.root();
        host.root.add_child(top, frame);
        host.root
            .register_window(WINDOW, frame, Some(frame), 0)
            .unwrap();
        dock.set_theme(&mut host.root, theme);
        host.attach_controller(WINDOW, Box::new(dock)).unwrap();
        Self {
            host,
            ui: PluginUi::new(),
            outbox,
            now: 0,
        }
    }

    /// One window of plugin `owner`.
    fn add(&mut self, owner: &str, owner_name: &str, descriptor: PanelDescriptor) -> u32 {
        self.ui
            .register_panel(
                &UiOwner::new(owner, owner_name),
                descriptor,
                "<panel/>",
                BindingSchema::new("Binding"),
            )
            .unwrap()
    }

    /// `count` plugins with a window each.
    fn add_plugins(&mut self, count: usize) {
        for i in 0..count {
            self.add(
                &format!("test.{i}"),
                &format!("Plugin {i}"),
                PanelDescriptor::new("main", format!("Plugin {i}")),
            );
        }
    }

    fn sync(&mut self, icons: &mut StubIcons) {
        let panels: Vec<PanelView> = self.ui.panels().cloned().collect();
        self.host.with_controller::<PluginDock, _>(WINDOW, |c, ui| {
            c.sync(ui, &panels, icons);
        });
        self.tick();
    }

    fn sync_plain(&mut self) {
        self.sync(&mut StubIcons::none());
    }

    fn tick(&mut self) {
        self.now += 16;
        self.host.tick(0.016, self.now, None);
    }

    fn draw(&mut self) -> DrawList {
        self.host.draw(SCREEN, &NoSprites)
    }

    fn with<R>(&mut self, f: impl FnOnce(&mut PluginDock, &mut UiRoot) -> R) -> R {
        self.host
            .with_controller::<PluginDock, _>(WINDOW, f)
            .unwrap()
    }

    fn layout(&mut self) -> DockLayout {
        self.with(|c, _| c.layout().clone())
    }

    fn frame(&mut self) -> ElementId {
        self.with(|c, _| c.root)
    }

    fn at(&mut self) -> (f32, f32) {
        let frame = self.frame();
        let e = self.host.root.element(frame).unwrap();
        (e.left, e.top)
    }

    fn size(&mut self) -> (f32, f32) {
        let frame = self.frame();
        let e = self.host.root.element(frame).unwrap();
        (e.width, e.height)
    }

    /// `r` in the dock's space as a point on the screen, near its top-left.
    fn point(&mut self, r: Rect) -> (i32, i32) {
        let (left, top) = self.at();
        ((left + r.x + 4.0) as i32, (top + r.y + 4.0) as i32)
    }

    fn click(&mut self, r: Rect) {
        let (x, y) = self.point(r);
        self.host.mouse_move(x, y);
        self.host.mouse_down(MouseButton::Left, x, y, 0);
        self.host.mouse_up(MouseButton::Left, x, y, 0);
        self.tick();
    }

    fn hover(&mut self, r: Rect) {
        let (x, y) = self.point(r);
        self.host.mouse_move(x, y);
        self.tick();
    }

    /// Each slot's place and whether it shows.
    fn slot_rects(&mut self) -> Vec<Option<SlotPlace>> {
        let panels: Vec<u32> = self.ui.panels().map(|p| p.id).collect();
        panels
            .iter()
            .map(|panel| {
                let slot = self.with(|c, _| c.slot(*panel))?;
                let e = self.host.root.element(slot)?;
                Some((e.left, e.top, e.visible()))
            })
            .collect()
    }

    fn label(&mut self) -> Option<HoverLabel> {
        let frame = self.frame();
        self.host
            .root
            .widget::<DockFrame>(frame)
            .and_then(|f| f.label.clone())
    }
}

fn sprites_of(list: &DrawList, texture: TextureRef) -> Vec<(f32, f32, f32)> {
    list.commands()
        .filter_map(|c| match c {
            DrawCmd::Sprite {
                texture: t,
                quad,
                tint,
                ..
            } if *t == texture => Some((quad.w, quad.h, tint.a)),
            _ => None,
        })
        .collect()
}

fn icon_file() -> PluginIcon {
    PluginIcon {
        width: 64,
        height: 64,
        rgba: Arc::new(vec![255; 64 * 64 * 4]),
    }
}

// ── Letters and icons ───────────────────────────────────────────────────

#[test]
fn monogram_letters_come_from_the_plugin_or_the_title_two_at_most() {
    assert_eq!(initials(None, "Test Plugin"), "TP");
    assert_eq!(initials(Some("  MT "), "Tank"), "MT");
    assert_eq!(initials(Some("ABCD"), "Tank"), "AB");
    assert_eq!(initials(None, "tank"), "TA");
    assert_eq!(initials(None, "  "), "?");
    assert_eq!(initials(None, "mosswart massacre extra"), "MM");
    assert_eq!(initials(None, "Loot Editor"), "LE");
    assert_eq!(initials(None, "Golem"), "GO");
}

#[test]
fn with_a_file_icon_draws_it_and_never_calls_the_resolver() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Classic);
    rig.ui.set_file_icon("acdream.test", Some(icon_file()));
    let descriptor = PanelDescriptor {
        icon_surface_id: 0x165,
        ..PanelDescriptor::new("main", "Test Plugin")
    };
    rig.add("acdream.test", "Test Plugin", descriptor);
    let mut icons = StubIcons {
        surface: None,
        surface_must_not_run: true,
        surface_calls: Vec::new(),
    };
    rig.sync(&mut icons);
    let list = rig.draw();
    // Drawn at the art's size whatever the file's.
    let drawn = sprites_of(&list, FILE_TEXTURE);
    assert_eq!(drawn.len(), 1);
    assert_eq!((drawn[0].0, drawn[0].1), (ART, ART));
}

#[test]
fn with_no_file_icon_still_resolves_the_surface_icon() {
    let surface = TextureRef::Surface(0x0600_0165);
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Classic);
    let descriptor = PanelDescriptor {
        icon_surface_id: 0x165,
        ..PanelDescriptor::new("main", "Test Plugin")
    };
    rig.add("acdream.test", "Test Plugin", descriptor);
    let mut icons = StubIcons {
        surface: Some((surface, 32.0, 32.0)),
        surface_must_not_run: false,
        surface_calls: Vec::new(),
    };
    rig.sync(&mut icons);
    assert!(!icons.surface_calls.is_empty());
    let list = rig.draw();
    assert_eq!(sprites_of(&list, surface).len(), 1);
}

#[test]
fn with_neither_icon_a_plugin_gets_its_monogram_hue_in_every_theme() {
    for theme in [
        PluginTheme::Classic,
        PluginTheme::Brass,
        PluginTheme::Parchment,
    ] {
        let mut rig = Rig::new(SCREEN.1, theme);
        rig.add_plugins(1);
        rig.sync_plain();
        let list = rig.draw();
        assert!(has_color(
            &draw_corners(&list),
            style::monogram_hue("test.0")
        ));
    }
}

#[test]
#[ignore = "needs the retail dat files"]
fn a_dock_icon_id_resolves_to_a_real_installed_render_surface() {
    // The image MossTank and MosswartMassacre name as their dock icon.
    use crate::draw::SpriteSizes;
    let dir = std::env::var_os("OPENAC_DAT_DIR")
        .map(std::path::PathBuf::from)
        .or_else(ac_dat::default_dat_dir)
        .expect("no dat directory: set OPENAC_DAT_DIR");
    let content = crate::content::DatContent::new(Arc::new(
        ac_dat::DatCollection::open(dir).expect("open the retail dats"),
    ))
    .expect("master property table");
    let id = normalize_icon(0x0600_2C41);
    let (w, h) = content.sprite_size(id).expect("a real render surface");
    assert!(w > 0 && h > 0);
}

#[test]
fn colour_art_is_dimmer_while_its_window_is_closed() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Classic);
    rig.ui.set_file_icon("acdream.test", Some(icon_file()));
    rig.add("acdream.test", "Test", PanelDescriptor::new("main", "Test"));
    rig.ui.show_panel("acdream.test", "main", false);
    rig.sync_plain();
    let closed = sprites_of(&rig.draw(), FILE_TEXTURE);
    assert_eq!(closed[0].2, style::CLOSED_ART_ALPHA);
    rig.ui.show_panel("acdream.test", "main", true);
    rig.sync_plain();
    let open = sprites_of(&rig.draw(), FILE_TEXTURE);
    assert_eq!(open[0].2, 1.0);
}

// ── Chrome ──────────────────────────────────────────────────────────────

#[test]
fn the_handle_and_toggle_follow_the_layout_expanded_and_minimized() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Classic);
    rig.add_plugins(2);
    rig.sync_plain();
    let (grip, toggle) = rig.with(|c, _| (c.grip, c.toggle));
    let rect = |rig: &Rig, id: ElementId| {
        let e = rig.host.root.element(id).unwrap();
        (e.left, e.top, e.width, e.height)
    };
    assert_eq!(rect(&rig, grip), (0.0, 0.0, 30.0, 16.0));
    assert_eq!(rect(&rig, toggle), (30.0, 2.0, 12.0, 12.0));
    let tooltip = |rig: &Rig| {
        rig.host
            .root
            .element(toggle)
            .unwrap()
            .runtime_tooltip_text
            .clone()
            .unwrap()
    };
    assert_eq!(tooltip(&rig), MINIMIZE_TOOLTIP);

    let target = rig.layout().toggle;
    rig.click(target);
    assert_eq!(rig.size(), (48.0, 24.0));
    assert_eq!(rect(&rig, grip), (0.0, 0.0, 30.0, 24.0));
    assert_eq!(rect(&rig, toggle), (30.0, 0.0, 12.0, 24.0));
    assert_eq!(tooltip(&rig), EXPAND_TOOLTIP);

    let target = rig.layout().toggle;
    rig.click(target);
    assert_eq!(rig.size().0, 48.0);
    assert!(rig.size().1 > 24.0);
}

#[test]
fn after_scrolling_every_visible_slot_matches_a_fresh_layout() {
    let mut rig = Rig::new(300.0, PluginTheme::Classic);
    rig.add_plugins(12);
    rig.sync_plain();
    let frame = rig.frame();
    rig.host.root.send_message(frame, ElementMessage::Wheel(-1));
    rig.host.root.send_message(frame, ElementMessage::Wheel(-1));
    rig.host.pump();
    rig.tick();

    let owners: Vec<String> = (0..12).map(|i| format!("test.{i}")).collect();
    let owners: Vec<&str> = owners.iter().map(String::as_str).collect();
    let longest = layout::max_length(DockMode::Floating, SCREEN.0, 300.0);
    let fresh = layout::compute(DockMode::Floating, false, &owners, longest, 2);
    assert_eq!(fresh.first, 2);
    for (slot, want) in rig.slot_rects().iter().zip(&fresh.slots) {
        let (x, y, visible) = slot.unwrap();
        assert_eq!(visible, want.is_some());
        if let Some(r) = want {
            assert_eq!((x, y), (r.x, r.y));
        }
    }
    assert_eq!(rig.size().1, fresh.height);
}

#[test]
fn a_plugin_going_leaves_every_other_slot_as_a_fresh_layout_has_it() {
    let mut rig = Rig::new(300.0, PluginTheme::Classic);
    rig.add_plugins(12);
    rig.sync_plain();
    let first = rig.ui.panels().next().unwrap().id;
    rig.ui.remove_panel("test.0", first);
    rig.sync_plain();
    let owners: Vec<String> = (1..12).map(|i| format!("test.{i}")).collect();
    let owners: Vec<&str> = owners.iter().map(String::as_str).collect();
    let longest = layout::max_length(DockMode::Floating, SCREEN.0, 300.0);
    let fresh = layout::compute(DockMode::Floating, false, &owners, longest, 0);
    assert_eq!(rig.layout(), fresh);
    assert_eq!(rig.with(|c, _| c.entry_count()), 11);
}

#[test]
fn a_dock_too_long_for_the_screen_scrolls_and_fits_wherever_it_is_placed() {
    let mut rig = Rig::new(300.0, PluginTheme::Classic);
    rig.add_plugins(12);
    rig.sync_plain();
    let l = rig.layout();
    assert!(l.max_first > 0);
    assert!(l.fade_end);
    assert!(l.height <= 300.0 - 2.0 * layout::FLOATING_MARGIN);
    rig.host.root.move_window(WINDOW, 10.0, 280.0);
    rig.tick();
    let (_, top) = rig.at();
    assert!(top + l.height <= 300.0);
    // Where it is placed changes nothing about how long it is.
    assert_eq!(rig.layout().height, l.height);
}

#[test]
fn the_dots_show_only_while_the_pointer_is_over_the_dock_and_the_chevron_always() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Moss);
    rig.add_plugins(1);
    rig.sync_plain();
    rig.host.mouse_move(700, 500);
    rig.tick();
    let (grip, toggle) = rig.with(|c, _| (c.grip, c.toggle));
    let over = |rig: &Rig| {
        rig.host
            .root
            .widget::<DockGrip>(grip)
            .is_some_and(|g| g.pointer_over)
    };
    assert!(!over(&rig));
    let away = rig.draw().commands().count();
    assert!(rig.host.root.element(toggle).unwrap().visible());

    let handle = rig.layout().handle;
    rig.hover(handle);
    assert!(over(&rig));
    let near = rig.draw().commands().count();
    assert!(near > away, "{near} commands, {away} away");
}

#[test]
fn the_grip_dims_while_the_interface_is_locked() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Moss);
    rig.add_plugins(1);
    rig.sync_plain();
    let grip = rig.with(|c, _| c.grip);
    assert_eq!(rig.host.root.element(grip).unwrap().opacity, 1.0);
    rig.host.root.set_ui_locked(true);
    rig.tick();
    assert_eq!(rig.host.root.element(grip).unwrap().opacity, 0.5);
}

#[test]
fn pressing_a_slot_toggles_its_window_and_the_gear_asks_for_appearance() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Moss);
    rig.add_plugins(1);
    rig.sync_plain();
    let l = rig.layout();
    rig.click(l.slots[0].unwrap());
    assert!(matches!(
        rig.outbox.borrow().as_slice(),
        [UiInput::PanelShown { .. }]
    ));
    assert!(!rig.with(|c, _| c.take_appearance_request()));
    rig.outbox.borrow_mut().clear();
    rig.click(l.gear);
    assert!(rig.outbox.borrow().is_empty());
    assert!(rig.with(|c, _| c.take_appearance_request()));
}

#[test]
fn the_dock_stays_over_a_window_that_was_raised() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Moss);
    rig.add_plugins(1);
    rig.sync_plain();
    let frame = rig.frame();
    let other = rig
        .host
        .root
        .insert(Element::at(0.0, 0.0, 100.0, 100.0), DockGrip::default());
    let top = rig.host.root.root();
    rig.host.root.add_child(top, other);
    rig.host.root.bring_to_front(other);
    rig.tick();
    let z = |rig: &Rig, id| rig.host.root.element(id).unwrap().z_order();
    assert!(z(&rig, frame) > z(&rig, other));
}

#[test]
fn a_covered_dock_stays_as_the_covering_screen_left_it_and_comes_back_after() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Moss);
    rig.add_plugins(1);
    rig.sync_plain();
    assert!(rig.host.root.is_window_visible(WINDOW));
    let frame = rig.frame();
    let other = rig
        .host
        .root
        .insert(Element::at(0.0, 0.0, 100.0, 100.0), DockGrip::default());
    let top = rig.host.root.root();
    rig.host.root.add_child(top, other);

    rig.with(|c, _| c.set_covered(true));
    rig.host.root.set_visible(frame, false);
    rig.host.root.bring_to_front(other);
    let z = rig.host.root.element(frame).unwrap().z_order();
    rig.tick();
    assert!(!rig.host.root.is_window_visible(WINDOW));
    // Neither a new window nor a change of screen brings it back.
    rig.add("test.new", "New", PanelDescriptor::new("main", "New"));
    rig.sync_plain();
    rig.host.root.set_size(900.0, 700.0);
    rig.tick();
    assert!(!rig.host.root.is_window_visible(WINDOW));
    assert_eq!(rig.host.root.element(frame).unwrap().z_order(), z);

    rig.with(|c, _| c.set_covered(false));
    rig.tick();
    assert!(rig.host.root.is_window_visible(WINDOW));
    let z = |rig: &Rig, id| rig.host.root.element(id).unwrap().z_order();
    assert!(z(&rig, frame) > z(&rig, other));
}

#[test]
fn hiding_keeps_it_hidden_and_showing_again_also_expands_it() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Moss);
    rig.add_plugins(1);
    rig.sync_plain();
    let toggle = rig.layout().toggle;
    rig.click(toggle);
    assert_eq!(rig.size().1, layout::PILL_LENGTH);
    rig.with(|c, ui| c.set_hidden(ui, true));
    rig.tick();
    assert!(!rig.host.root.is_window_visible(WINDOW));
    rig.with(|c, ui| c.set_hidden(ui, false));
    rig.tick();
    assert!(rig.host.root.is_window_visible(WINDOW));
    assert!(rig.size().1 > layout::PILL_LENGTH);
}

// ── Hover labels ────────────────────────────────────────────────────────

/// A window titled `title` of a plugin named `owner`, at the default place.
fn labelled(title: &str, owner: &str) -> Rig {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Classic);
    rig.add("test", owner, PanelDescriptor::new("main", title));
    rig.sync_plain();
    rig
}

#[test]
fn hovering_a_slot_shows_its_label_and_it_has_no_tooltip() {
    let mut rig = labelled("Loot Editor", "MossTank");
    assert!(rig.label().is_none());
    let slot = rig.layout().slots[0].unwrap();
    rig.hover(slot);
    let label = rig.label().unwrap();
    assert_eq!(label.title, "Loot Editor");
    assert_eq!(label.subtitle.as_deref(), Some("MossTank"));
    let panel = rig.ui.panels().next().unwrap().id;
    let id = rig.with(|c, _| c.slot(panel)).unwrap();
    assert!(
        rig.host
            .root
            .element(id)
            .unwrap()
            .runtime_tooltip_text
            .is_none()
    );
}

#[test]
fn a_label_is_one_line_when_the_title_is_the_plugins_name() {
    let mut rig = labelled("GoArrow", "GoArrow");
    let slot = rig.layout().slots[0].unwrap();
    rig.hover(slot);
    assert_eq!(rig.label().unwrap().subtitle, None);
}

#[test]
fn the_label_sits_away_from_the_screen_edge() {
    let mut rig = labelled("Loot Editor", "MossTank");
    let slot = rig.layout().slots[0].unwrap();
    rig.hover(slot);
    let (w, _) = rig.size();
    let left = rig.label().unwrap().rect;
    assert_eq!(left.x, w + style::LABEL_GAP);
    assert!(((left.y + left.h / 2.0) - (slot.y + slot.h / 2.0)).abs() <= 1.0);

    // Floating in the right half of the screen, it opens to the left.
    rig.host.root.move_window(WINDOW, 700.0, 100.0);
    rig.hover(slot);
    let right = rig.label().unwrap().rect;
    assert_eq!(right.x, -style::LABEL_GAP - right.w);
}

#[test]
fn a_label_is_kept_on_the_screen() {
    let mut rig = labelled("Loot Editor", "MossTank");
    let (_, h) = rig.size();
    rig.host.root.move_window(WINDOW, 10.0, SCREEN.1 - h);
    let gear = rig.layout().gear;
    rig.tick();
    let (_, top) = rig.at();
    // Near the bottom, a tall label would run off without the clamp.
    rig.with(|c, ui| {
        let r = c.label_rect(
            ui,
            DockSide::Left,
            Rect::new(gear.x, h, gear.w, 40.0),
            "Title",
            Some("Sub"),
        );
        assert!(top + r.y + r.h <= SCREEN.1);
        assert!(top + r.y >= 0.0);
    });
}

#[test]
fn the_label_is_not_hit_and_is_drawn_in_the_overlay() {
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Moss);
    rig.add(
        "test",
        "MossTank",
        PanelDescriptor::new("main", "Loot Editor"),
    );
    rig.sync_plain();
    let slot = rig.layout().slots[0].unwrap();
    rig.hover(slot);
    let label = rig.label().unwrap().rect;
    let (left, top) = rig.at();
    let (x, y) = (
        left + label.x + label.w / 2.0,
        top + label.y + label.h / 2.0,
    );
    let frame = rig.frame();
    let root = rig.host.root.root();
    let hit = rig.host.root.hit_test(root, x, y);
    assert!(hit.is_none_or(|h| h != frame));
    let list = rig.draw();
    let overlay = DrawList {
        main: list.overlay.clone(),
        ..DrawList::default()
    };
    let moss = Palette::MOSS;
    assert!(
        draw_corners(&overlay)
            .iter()
            .any(|(cx, _, c)| *cx > left + 48.0 && *c == moss.background)
    );
}

#[test]
fn hovering_the_gear_labels_it_plugin_appearance() {
    let mut rig = labelled("Loot Editor", "MossTank");
    let gear = rig.layout().gear;
    rig.hover(gear);
    let label = rig.label().unwrap();
    assert_eq!(label.title, APPEARANCE_LABEL);
    assert_eq!(label.subtitle, None);
}

// ── Shapes ──────────────────────────────────────────────────────────────

fn drawn(f: impl FnOnce(&mut RenderContext<'_>)) -> Vec<(f32, f32, Color)> {
    let mut list = DrawList::default();
    let mut rc = RenderContext::new(&mut list, (800.0, 600.0), &NoSprites);
    f(&mut rc);
    drop(rc);
    draw_corners(&list)
}

/// What a draw put down, without its transparent soft edge.
fn ink(corners: &[(f32, f32, Color)]) -> Vec<(f32, f32)> {
    corners
        .iter()
        .filter(|(_, _, c)| c.a > 0.5)
        .map(|(x, y, _)| (*x, *y))
        .collect()
}

#[test]
fn a_floating_body_fills_in_the_background_with_a_rim_and_a_shadow_below() {
    let moss = Palette::MOSS;
    let corners = drawn(|rc| {
        rc.push_transform(100.0, 100.0);
        style::body(rc, &moss, DockMode::Floating, 48.0, 200.0);
    });
    assert!(has_color(&corners, moss.background));
    assert!(has_color(&corners, mix(moss.border, moss.text, 0.08)));
    assert!(corners.iter().any(|(_, y, c)| *y > 300.0 && c.a > 0.0));
}

#[test]
fn a_rail_is_square_on_its_edge_side() {
    let r = style::RAIL_RADIUS;
    assert_eq!(
        style::corners(DockMode::Left),
        CornerRadii::new(0.0, r, r, 0.0)
    );
    assert_eq!(
        style::corners(DockMode::Right),
        CornerRadii::new(r, 0.0, 0.0, r)
    );
    assert_eq!(
        style::corners(DockMode::Top),
        CornerRadii::new(0.0, 0.0, r, r)
    );
    assert_eq!(
        style::corners(DockMode::Bottom),
        CornerRadii::new(r, r, 0.0, 0.0)
    );
    assert_eq!(
        style::corners(DockMode::Floating),
        CornerRadii::all(style::RADIUS)
    );
}

#[test]
fn an_edge_pill_sits_on_the_screen_edge_centred_on_its_slot() {
    let moss = Palette::MOSS;
    let dock = (46.0, 200.0);
    let slot = Rect::new(0.0, 40.0, 46.0, 38.0);
    for (side, min_x, max_x) in [(DockSide::Left, 0.0, 3.0), (DockSide::Right, 43.0, 46.0)] {
        let corners =
            drawn(|rc| style::edge_pill(rc, moss.accent, side, dock, slot, style::PILL_OPEN));
        let points = ink(&corners);
        assert!(!points.is_empty());
        assert!(
            points
                .iter()
                .all(|(x, _)| *x >= min_x - 0.5 && *x <= max_x + 0.5)
        );
        // 8 long, centred on a slot from 40 to 78: 55 to 63.
        let (ys_min, ys_max) = points.iter().fold((f32::MAX, f32::MIN), |(a, b), (_, y)| {
            (a.min(*y), b.max(*y))
        });
        assert!((54.0..=56.0).contains(&ys_min), "{ys_min}");
        assert!((62.0..=64.0).contains(&ys_max), "{ys_max}");
    }
    // Along a top or bottom rail.
    let dock = (200.0, 46.0);
    let slot = Rect::new(40.0, 0.0, 38.0, 46.0);
    for (side, min_y, max_y) in [(DockSide::Top, 0.0, 3.0), (DockSide::Bottom, 43.0, 46.0)] {
        let corners =
            drawn(|rc| style::edge_pill(rc, moss.accent, side, dock, slot, style::PILL_OPEN));
        let points = ink(&corners);
        assert!(
            points
                .iter()
                .all(|(_, y)| *y >= min_y - 0.5 && *y <= max_y + 0.5)
        );
        assert!(points.iter().all(|(x, _)| (54.0..=64.0).contains(x)));
    }
}

#[test]
fn an_edge_pill_of_no_length_draws_nothing() {
    let moss = Palette::MOSS;
    let corners = drawn(|rc| {
        style::edge_pill(
            rc,
            moss.accent,
            DockSide::Left,
            (46.0, 200.0),
            Rect::new(0.0, 40.0, 46.0, 38.0),
            0.0,
        );
    });
    assert!(corners.is_empty());
}

#[test]
fn an_open_dot_sits_on_the_side_facing_the_screen_edge() {
    let moss = Palette::MOSS;
    let slot = Rect::new(6.0, 16.0, 36.0, 36.0);
    for (side, min_x, max_x) in [(DockSide::Left, 1.5, 4.5), (DockSide::Right, 43.5, 46.5)] {
        let corners = drawn(|rc| style::open_dot(rc, &moss, side, (48.0, 200.0), slot));
        assert!(has_color(&corners, moss.accent));
        assert!(
            ink(&corners)
                .iter()
                .all(|(x, _)| *x >= min_x - 0.5 && *x <= max_x + 0.5)
        );
    }
    let slot = Rect::new(16.0, 6.0, 36.0, 36.0);
    for (side, min_y, max_y) in [(DockSide::Top, 1.5, 4.5), (DockSide::Bottom, 43.5, 46.5)] {
        let corners = drawn(|rc| style::open_dot(rc, &moss, side, (200.0, 48.0), slot));
        assert!(
            ink(&corners)
                .iter()
                .all(|(_, y)| *y >= min_y - 0.5 && *y <= max_y + 0.5)
        );
    }
}

#[test]
fn a_slot_well_is_empty_at_rest_and_filled_under_the_pointer() {
    let moss = Palette::MOSS;
    let r = Rect::new(0.0, 0.0, 36.0, 36.0);
    assert!(drawn(|rc| style::slot_well(rc, &moss, r, ButtonLook::Normal)).is_empty());
    let hovered = drawn(|rc| style::slot_well(rc, &moss, r, ButtonLook::Hovered));
    assert!(has_color(&hovered, hover(&moss, moss.field)));
}

#[test]
fn a_rail_slots_well_leaves_the_edge_pill_uncovered_and_a_floating_one_spans_the_slot() {
    let vertical = style::slot_well_rect(DockMode::Left, 46.0, 38.0);
    assert_eq!(vertical, Rect::new(5.0, 1.0, 36.0, 36.0));
    assert!(vertical.x >= style::PILL_HOVER);
    let horizontal = style::slot_well_rect(DockMode::Top, 38.0, 46.0);
    assert_eq!(horizontal, Rect::new(1.0, 5.0, 36.0, 36.0));
    assert_eq!(
        style::slot_well_rect(DockMode::Floating, 40.0, 30.0),
        Rect::new(0.0, 0.0, 40.0, 30.0)
    );
}

#[test]
fn a_monogram_hue_is_stable_for_an_id_and_one_of_eight() {
    assert_eq!(
        style::monogram_hue("acdream.mosstank"),
        style::monogram_hue("acdream.mosstank")
    );
    let mut seen: Vec<(u32, u32, u32)> = Vec::new();
    for id in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"] {
        let c = style::monogram_hue(id);
        let key = (
            (c.r * 255.0) as u32,
            (c.g * 255.0) as u32,
            (c.b * 255.0) as u32,
        );
        if !seen.contains(&key) {
            seen.push(key);
        }
    }
    assert!(seen.len() > 1 && seen.len() <= 8);
    // FNV-1a of "a" is 0xE40C292C, and that % 8 is 4: the fifth hue, #8B8259.
    assert_eq!(
        style::monogram_hue("a"),
        Color::rgba(
            0x8B as f32 / 255.0,
            0x82 as f32 / 255.0,
            0x59 as f32 / 255.0,
            1.0
        )
    );
}

#[test]
fn a_hover_label_draws_its_box_and_a_shadow() {
    let moss = Palette::MOSS;
    let corners = drawn(|rc| style::hover_label(rc, &moss, Rect::new(60.0, 20.0, 90.0, 36.0)));
    assert!(has_color(&corners, moss.background));
    assert!(has_color(&corners, moss.border));
    assert!(corners.iter().any(|(_, y, c)| *y > 56.0 && c.a > 0.0));
}

#[test]
fn a_divider_is_a_one_point_hairline_faded_from_the_border_toward_the_fill() {
    for p in [Palette::MOSS, Palette::CLASSIC_DOCK] {
        let expected = mix(p.border, p.background, style::DIVIDER_FADE);
        assert_ne!(expected, p.border);
        let corners = drawn(|rc| style::divider(rc, &p, DockMode::Floating, (48.0, 200.0), 50.0));
        assert!(corners.iter().all(|(_, _, c)| *c == expected));
        let ys: Vec<f32> = corners.iter().map(|(_, y, _)| *y).collect();
        let span = ys.iter().cloned().fold(f32::MIN, f32::max)
            - ys.iter().cloned().fold(f32::MAX, f32::min);
        assert_eq!(span, 1.0);
        let turned = drawn(|rc| style::divider(rc, &p, DockMode::Top, (200.0, 46.0), 50.0));
        let xs: Vec<f32> = turned.iter().map(|(x, _, _)| *x).collect();
        let span = xs.iter().cloned().fold(f32::MIN, f32::max)
            - xs.iter().cloned().fold(f32::MAX, f32::min);
        assert_eq!(span, 1.0);
    }
}

#[test]
fn the_classic_dock_uses_its_own_colours_and_is_still_the_black_and_gold() {
    let p = Palette::CLASSIC_DOCK;
    assert_eq!(p.background.a, 0.88);
    assert_eq!(p.background.r, 0.0);
    assert!((p.border.r - 0x9E as f32 / 255.0).abs() < 0.001);
    assert!((p.accent.r - 0xDB as f32 / 255.0).abs() < 0.001);
    assert!((p.selected.g - 0x30 as f32 / 255.0).abs() < 0.001);
    assert_eq!(PluginTheme::Classic.palette(), None);
    let mut rig = Rig::new(SCREEN.1, PluginTheme::Classic);
    rig.add_plugins(1);
    rig.sync_plain();
    let list = rig.draw();
    assert!(
        has_color(&draw_corners(&list), p.border)
            || has_color(&draw_corners(&list), mix(p.border, p.text, 0.08))
    );
}
