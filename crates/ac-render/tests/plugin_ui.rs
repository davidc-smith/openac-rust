//! The plugins' UI in the game window on the retail dats: a window
//! built from the session's snapshot mounts the plugin's window and its
//! shelf button, a press reaches the session as a command, and a plugin's
//! request to toggle a client window is carried out and reported back.
//! Ignored by default; `scripts/check.*` runs them when the dats are
//! present.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ac_dat::DatCollection;
use ac_plugin_ui::{
    ActionValue, BindingKind as K, BindingSchema, ClientWindow, ClientWindowAction,
    PanelDescriptor, PluginUi, UiEvent, UiInput, UiOwner, UiValue,
};
use ac_render::scene::Scene;
use ac_render::ui::WindowUi;
use ac_render::ui::window::{INVENTORY_WINDOW, UiScript};
use ac_runtime::session::interface::InterfaceEvent;
use ac_runtime::session::{PresentationEvent, SessionCommand};

fn dats() -> Arc<DatCollection> {
    let dir = std::env::var_os("OPENAC_DAT_DIR")
        .map(PathBuf::from)
        .or_else(ac_dat::default_dat_dir)
        .expect("no dat directory: set OPENAC_DAT_DIR");
    Arc::new(DatCollection::open(dir).expect("open the retail dats"))
}

const MARKUP: &str = r#"<panel x="300" y="200" w="240" h="120" title="Demo">
  <label name="Status" x="8" y="24" w="200" h="16" text="{Status}"/>
  <button name="Go" x="8" y="50" w="80" h="20" text="Go" onclick="{Go}"/>
</panel>"#;

/// The session's side: one plugin window, shown.
fn session() -> (PluginUi, u32) {
    let mut ui = PluginUi::new();
    ui.set_window_attached(true);
    let schema = BindingSchema::new("Demo")
        .with("Status", K::Text)
        .with("Go", K::Action);
    let id = ui
        .register_panel(
            &UiOwner::new("test.demo", "Demo"),
            PanelDescriptor::new("main", "Demo"),
            MARKUP,
            schema,
        )
        .unwrap();
    ui.set_values(
        "test.demo",
        id,
        vec![("Status".into(), UiValue::Text("idle".into()))],
    )
    .unwrap();
    ui.show_panel("test.demo", "main", true);
    (ui, id)
}

/// The window's commands handed to the session.
fn deliver(state: &mut PluginUi, commands: Vec<SessionCommand>) -> usize {
    let mut n = 0;
    for command in commands {
        if let SessionCommand::PluginUi(input) = command {
            state.input(input);
            n += 1;
        }
    }
    n
}

#[test]
#[ignore = "needs the retail dat files"]
fn a_plugin_window_mounts_from_the_snapshot_and_reports_presses() {
    let size = (1280, 720);
    let (mut state, panel) = session();
    let mut ui = WindowUi::build(dats(), size).expect("the UI builds");
    let mut scene = Scene::default();
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.snapshot(),
    )));
    ui.feed(&mut scene);
    ui.frame(Instant::now(), size);
    let name = "plugin:test.demo:main";
    assert_eq!(ui.plugin_windows(), [name]);
    assert!(ui.host().root.is_window_visible(name));
    assert!(
        ui.host()
            .root
            .is_window_visible(ac_ui::plugin::shelf::WINDOW)
    );
    // The window tells the session which client windows it has.
    let commands = ui.drain();
    assert!(commands.iter().any(|c| matches!(
        c,
        SessionCommand::PluginUi(UiInput::ClientWindows { mounted, .. })
            if mounted & ClientWindow::Inventory.bit() != 0
    )));
    deliver(&mut state, commands);

    // The button at (8, 50) in a window at (300, 200).
    let out = ui.script(&UiScript::Click {
        x: 300 + 20,
        y: 200 + 58,
        button: 0,
        double: false,
    });
    let commands = ui.handle(out);
    assert!(commands.iter().any(|c| matches!(
        c,
        SessionCommand::PluginUi(UiInput::Action { panel: p, handler, value: ActionValue::None })
            if *p == panel && handler == "Go"
    )));
    deliver(&mut state, commands);
    let events = state.take_events();
    assert!(
        events.iter().any(|(owner, e)| owner == "test.demo"
            && matches!(e, UiEvent::Action { handler, .. } if handler == "Go")),
        "{events:?}"
    );

    // The plugin opens the inventory; the window does and says so.
    assert!(!ui.host().root.is_window_visible(INVENTORY_WINDOW));
    assert!(state.client_window(ClientWindow::Inventory, ClientWindowAction::Toggle));
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.take_changes(),
    )));
    ui.feed(&mut scene);
    assert!(ui.host().root.is_window_visible(INVENTORY_WINDOW));
    deliver(&mut state, ui.drain());
    assert!(state.is_client_window_visible(ClientWindow::Inventory));

    // The plugin goes: its window and its shelf button go with it.
    state.release("test.demo");
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.take_changes(),
    )));
    ui.feed(&mut scene);
    ui.frame(Instant::now(), size);
    assert!(ui.plugin_windows().is_empty());
    assert!(ui.host().root.window(name).is_none());
    assert!(
        !ui.host()
            .root
            .is_window_visible(ac_ui::plugin::shelf::WINDOW)
    );
}

/// Plugin hotkeys in the window: the window names its key map's
/// chords, the session binds around them and sends the bound table back,
/// and a press of a bound chord reaches the plugin (a client chord never
/// binds). Toggle Plugin Manager (Shift+Ctrl+F1) hides and shows the
/// shelf (OpenAC `RetailUiRuntime.TogglePluginManager`).
#[test]
#[ignore = "needs the retail dat files"]
fn plugin_hotkeys_fire_from_the_window_and_the_shelf_toggles() {
    use ac_plugin_ui::hotkeys::Chord;
    use ac_ui::event::{Key, Modifiers};
    use ac_ui::input::{InputAction, KeyBindings};
    const H: u32 = 8;
    const ESCAPE: u32 = 51;
    // A key the retail key map leaves free.
    const PAUSE: u32 = 92;
    let size = (1280, 720);
    let (mut state, _) = session();
    let heal = Chord::key(H).with_ctrl();
    state
        .register_hotkey("test.demo", "heal", "Heal", heal)
        .unwrap();
    state
        .register_hotkey("test.demo", "escape", "Escape", Chord::key(ESCAPE))
        .unwrap();
    let mut ui = WindowUi::build(dats(), size).expect("the UI builds");
    let mut scene = Scene::default();
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.snapshot(),
    )));
    ui.feed(&mut scene);
    ui.frame(Instant::now(), size);
    ui.report_client_chords(&KeyBindings::retail_defaults());
    deliver(&mut state, ui.take_plugin_commands());
    assert!(state.hotkeys().state("test.demo", "heal").unwrap().is_bound);
    assert!(
        !state
            .hotkeys()
            .state("test.demo", "escape")
            .unwrap()
            .is_bound
    );
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.take_changes(),
    )));
    ui.feed(&mut scene);
    ui.plugin_hotkeys(Key::H, Modifiers::CTRL, false);
    ui.plugin_hotkeys(Key::H, Modifiers::NONE, false);
    ui.plugin_hotkeys(Key::Escape, Modifiers::NONE, false);
    ui.plugin_hotkeys(Key::H, Modifiers::CTRL, true);
    deliver(&mut state, ui.take_plugin_commands());
    let events = state.take_events();
    assert_eq!(
        events,
        [("test.demo".to_owned(), UiEvent::Hotkey("heal".into()))]
    );

    // OpenAC `HotkeyUiFocusTests`: the
    // interface's own focus and modal decide. While any element holds
    // keyboard focus only a chord with Ctrl or Alt fires; an open modal
    // lets nothing fire; with both gone, plain chords fire again.
    let plain = Chord::key(PAUSE);
    state
        .register_hotkey("test.demo", "plain", "Plain", plain)
        .unwrap();
    ui.report_client_chords(&KeyBindings::retail_defaults());
    deliver(&mut state, ui.take_plugin_commands());
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.take_changes(),
    )));
    ui.feed(&mut scene);
    let element = ui.host().root.root();
    ui.host_mut().root.set_keyboard_focus(Some(element));
    ui.plugin_hotkeys(Key::Pause, Modifiers::NONE, false);
    ui.plugin_hotkeys(Key::H, Modifiers::CTRL, false);
    ui.host_mut().root.set_keyboard_focus(None);
    ui.host_mut().root.modal = Some(element);
    ui.plugin_hotkeys(Key::H, Modifiers::CTRL, false);
    ui.plugin_hotkeys(Key::Pause, Modifiers::NONE, false);
    ui.host_mut().root.modal = None;
    ui.plugin_hotkeys(Key::Pause, Modifiers::NONE, false);
    deliver(&mut state, ui.take_plugin_commands());
    assert_eq!(
        state.take_events(),
        [
            ("test.demo".to_owned(), UiEvent::Hotkey("heal".into())),
            ("test.demo".to_owned(), UiEvent::Hotkey("plain".into())),
        ]
    );

    let shelf = ac_ui::plugin::shelf::WINDOW;
    assert!(ui.host().root.is_window_visible(shelf));
    assert!(ui.handle_input_action(InputAction::TogglePluginManager));
    ui.frame(Instant::now(), size);
    assert!(!ui.host().root.is_window_visible(shelf));
    assert!(ui.handle_input_action(InputAction::TogglePluginManager));
    ui.frame(Instant::now(), size);
    assert!(ui.host().root.is_window_visible(shelf));
}

/// A reader that hands out a 4 x 4 image for every path (OpenAC PR #271's
/// file icons; the real reader is `ac-plugin`'s).
#[derive(Debug)]
struct SquareFiles;

impl ac_plugin_ui::FileImageLoader for SquareFiles {
    fn load(&self, _directory: &std::path::Path, path: &str) -> ac_plugin_ui::FileImageRead {
        ac_plugin_ui::FileImageRead::finished(Ok(Arc::new(ac_plugin_ui::FileImage {
            path: path.to_owned(),
            version: "1".to_owned(),
            levels: ac_plugin_ui::mips::build(vec![255; 4 * 4 * 4], 4, 4),
            full_path: None,
        })))
    }
}

fn runtime_sprites(list: &ac_ui::draw::DrawList) -> usize {
    list.main
        .iter()
        .filter(|c| {
            matches!(
                c,
                ac_ui::draw::DrawCmd::Sprite {
                    texture: ac_ui::draw::TextureRef::Runtime(_),
                    ..
                }
            )
        })
        .count()
}

/// A plugin window's file icon: the window tells the session the path it
/// draws, the session holds the image and says which, the window draws
/// it; hidden, the window draws nothing and says so.
#[test]
#[ignore = "needs the retail dat files"]
fn a_plugin_windows_file_icon_is_held_while_drawn_and_drawn_once_held() {
    let size = (1280, 720);
    let mut state = PluginUi::new();
    state.set_window_attached(true);
    state.set_file_images(Some(Arc::new(SquareFiles)));
    state.set_plugin_folder("test.files", Some(PathBuf::from("plugin")));
    let panel = state
        .register_panel(
            &UiOwner::new("test.files", "Files"),
            PanelDescriptor::new("main", "Files"),
            r#"<panel x="300" y="200" w="240" h="120" title="Files">
  <group x="8" y="30" w="100" h="40" visible="{PageOpen}">
    <icon x="0" y="0" w="24" h="24" file="icons/sword.png"/>
  </group>
</panel>"#,
            BindingSchema::new("Files").with("PageOpen", K::Bool),
        )
        .unwrap();
    state
        .set_values(
            "test.files",
            panel,
            vec![("PageOpen".into(), UiValue::Bool(true))],
        )
        .unwrap();
    state.show_panel("test.files", "main", true);
    let mut ui = WindowUi::build(dats(), size).expect("the UI builds");
    let mut scene = Scene::default();
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.snapshot(),
    )));
    ui.feed(&mut scene);
    let before = runtime_sprites(&ui.frame(Instant::now(), size));
    let commands = ui.drain();
    assert!(commands.iter().any(|c| matches!(c,
        SessionCommand::PluginUi(UiInput::FileIconsShown { panel: p, paths })
            if *p == panel && paths == &["icons/sword.png".to_owned()])));
    deliver(&mut state, commands);
    assert!(state.file_icon(panel, "icons/sword.png").is_some());

    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.take_changes(),
    )));
    ui.feed(&mut scene);
    let after = runtime_sprites(&ui.frame(Instant::now(), size));
    assert_eq!(after, before + 1, "the file icon draws once held");

    // Its page closes while the window stays shown: the window says it no
    // longer draws it, and the session lets go of it 5 s later.
    state
        .set_values(
            "test.files",
            panel,
            vec![("PageOpen".into(), UiValue::Bool(false))],
        )
        .unwrap();
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.take_changes(),
    )));
    ui.feed(&mut scene);
    ui.frame(Instant::now(), size);
    let commands = ui.drain();
    assert!(commands.iter().any(|c| matches!(c,
        SessionCommand::PluginUi(UiInput::FileIconsShown { panel: p, paths })
            if *p == panel && paths.is_empty())));
    deliver(&mut state, commands);
    assert!(state.panels().next().unwrap().shown());
    for second in 1..=6 {
        state.tick_file_icons(second * 1000);
    }
    assert_eq!(state.image_count("test.files"), 0);
    assert!(state.file_icon(panel, "icons/sword.png").is_none());

    // Open again: drawn, said, read and held again.
    state
        .set_values(
            "test.files",
            panel,
            vec![("PageOpen".into(), UiValue::Bool(true))],
        )
        .unwrap();
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.take_changes(),
    )));
    ui.feed(&mut scene);
    ui.frame(Instant::now(), size);
    deliver(&mut state, ui.drain());
    assert!(state.file_icon(panel, "icons/sword.png").is_some());

    // Hidden: the window draws no file icons and tells the session.
    state.show_panel("test.files", "main", false);
    scene.apply(PresentationEvent::Interface(InterfaceEvent::PluginUi(
        state.take_changes(),
    )));
    ui.feed(&mut scene);
    ui.frame(Instant::now(), size);
    let commands = ui.drain();
    assert!(commands.iter().any(|c| matches!(c,
        SessionCommand::PluginUi(UiInput::FileIconsShown { panel: p, paths })
            if *p == panel && paths.is_empty())));
}
