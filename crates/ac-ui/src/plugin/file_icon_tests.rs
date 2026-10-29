//! Images a plugin ships in its folder, named from markup by path: what a
//! window asks its icon resolver for, and draws. Ported from OpenAC PR
//! #271's `tests/AcDream.App.Tests/UI/MarkupFileIconTests.cs` (the drawing
//! half; the build-time checks are `ac-plugin-ui`'s). A list's rows read
//! their paths directly, so the C# path-to-number table has no counterpart:
//! each row asks for its own path, and one path is one image.

use std::cell::RefCell;
use std::rc::Rc;

use ac_plugin_ui::{BindingKind as K, BindingSchema, PanelDescriptor, PluginUi, UiOwner, UiValue};

use super::*;
use crate::draw::{DrawCmd, TextureRef};
use crate::host::UiHost;

/// Hands out a texture per path asked, and records the asks.
#[derive(Default)]
struct FileResolver {
    files: Vec<String>,
}

impl MarkupIcons for FileResolver {
    fn surface(&mut self, _id: u32) -> Option<Icon> {
        Some((TextureRef::Surface(5), 32.0, 32.0))
    }
    fn spell(&mut self, _spell: u32) -> Option<Icon> {
        None
    }
    fn item(&mut self, _object: u32) -> Option<Icon> {
        None
    }
    fn file(&mut self, path: &str) -> Option<Icon> {
        self.files.push(path.to_owned());
        Some((
            TextureRef::Runtime(40 + self.files.len() as u32),
            24.0,
            16.0,
        ))
    }
}

/// OpenAC's `DidOnlyResolver`: no file support.
struct DidOnly;

impl MarkupIcons for DidOnly {
    fn surface(&mut self, _id: u32) -> Option<Icon> {
        Some((TextureRef::Surface(5), 32.0, 32.0))
    }
    fn spell(&mut self, _spell: u32) -> Option<Icon> {
        None
    }
    fn item(&mut self, _object: u32) -> Option<Icon> {
        None
    }
}

const MARKUP: &str = r#"<panel x="0" y="0" w="400" h="300">
  <icon x="0" y="0" w="24" h="24" file="icons/sword.png"/>
  <icon x="30" y="0" w="24" h="24" file="{IconPath}"/>
  <button x="0" y="30" w="60" h="20" text="Go" icon="{IconPath}" iconkind="file"/>
  <tab x="70" y="30" w="60" h="20" text="Notes" selected="{TabOn}" icon="icons/notes.png" iconkind="file"/>
  <list x="0" y="60" w="100" h="60" rowheight="20" selected="{Selected}" items="{Rows}" icons="{RowIcons}" iconkind="file"/>
  <list x="120" y="60" w="100" h="60" rowheight="20" selected="{Selected}">
    <column type="icon" width="20" iconkind="file" values="{RowIcons}" onclick="{ClickIcon}"/>
  </list>
</panel>"#;

fn window(icon_path: &str) -> ac_plugin_ui::PanelView {
    let mut ui = PluginUi::new();
    let schema = BindingSchema::new("FileBinding")
        .with("IconPath", K::Text)
        .with("TabOn", K::Bool)
        .with("Rows", K::TextList)
        .with("RowIcons", K::TextList)
        .with("Selected", K::Int)
        .with("ClickIcon", K::IntAction);
    let panel = ui
        .register_panel(
            &UiOwner::new("acme.hello", "Hello"),
            PanelDescriptor::new("main", "Hello"),
            MARKUP,
            schema,
        )
        .unwrap();
    ui.set_values(
        "acme.hello",
        panel,
        vec![
            ("IconPath".into(), UiValue::Text(icon_path.into())),
            ("TabOn".into(), UiValue::Bool(true)),
            (
                "Rows".into(),
                UiValue::TextList(vec!["one".into(), "two".into(), "three".into()]),
            ),
            (
                "RowIcons".into(),
                UiValue::TextList(vec![
                    "icons/a.png".into(),
                    String::new(),
                    "icons/a.png".into(),
                ]),
            ),
            ("Selected".into(), UiValue::Int(-1)),
        ],
    )
    .unwrap();
    ui.panels().next().unwrap().clone()
}

/// Builds the window, syncs it through `icons` and draws it: the runtime
/// textures drawn.
fn drawn(view: &ac_plugin_ui::PanelView, icons: &mut dyn MarkupIcons) -> Vec<u32> {
    let mut host = UiHost::new();
    host.root.set_size(1024.0, 768.0);
    let outbox: PluginOutbox = Rc::new(RefCell::new(Vec::new()));
    let mut controller = build_panel(&mut host.root, view, None, outbox);
    let frame = controller.root;
    let root = host.root.root();
    host.root.add_child(root, frame);
    host.root
        .register_window(&controller.window_name, frame, Some(frame), 0)
        .unwrap();
    controller.sync(&mut host.root, view, 1, icons, PluginTheme::Classic);
    host.attach_controller(&view.window_name(), Box::new(controller))
        .unwrap();
    let list = host.draw((1024.0, 768.0), &crate::draw::NoSprites);
    let mut textures: Vec<u32> = list
        .main
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Sprite {
                texture: TextureRef::Runtime(t),
                ..
            } => Some(*t),
            _ => None,
        })
        .collect();
    textures.sort_unstable();
    textures.dedup();
    textures
}

// Icon_FileLiteral_ResolvesThePathThroughTheResolver,
// Icon_FileBinding_ReadsTheStringPropertyEachDraw,
// ButtonIcon_KindFile_ResolvesTheIconAsAPath,
// TabIcon_KindFile_ResolvesTheIconAsAPath,
// List_KindFile_GivesEachDistinctPathOneIdAndResolvesItBack and
// Column_KindFile_ResolvesEachRowsPath: every form asks for its path, a
// blank row asks nothing, and what comes back is drawn.
#[test]
fn every_file_icon_form_asks_for_its_path_and_draws_what_it_gets() {
    let view = window("icons/bound.png");
    let mut resolver = FileResolver::default();
    let textures = drawn(&view, &mut resolver);
    let mut asked = resolver.files.clone();
    asked.sort();
    assert_eq!(
        asked,
        [
            "icons/a.png",
            "icons/a.png",
            "icons/a.png",
            "icons/a.png",
            "icons/bound.png",
            "icons/bound.png",
            "icons/notes.png",
            "icons/sword.png",
        ]
    );
    // The two icons, the button, the tab and the rows with a path drew.
    assert!(textures.len() >= 6, "{textures:?}");
}

// A bound path read at the next sync: the new path is asked.
#[test]
fn a_bound_path_is_read_again_when_the_values_change() {
    let mut resolver = FileResolver::default();
    drawn(&window("icons/bound.png"), &mut resolver);
    drawn(&window("icons/other.jpg"), &mut resolver);
    assert!(resolver.files.iter().any(|p| p == "icons/other.jpg"));
}

// Icon_FileBindingWithNoPath_DrawsNothingWithoutAsking.
#[test]
fn a_blank_bound_path_draws_nothing_without_asking() {
    for blank in ["", "   "] {
        let mut resolver = FileResolver::default();
        drawn(&window(blank), &mut resolver);
        assert!(
            !resolver.files.iter().any(|p| p.trim().is_empty()),
            "{:?}",
            resolver.files
        );
        assert!(!resolver.files.iter().any(|p| p == "icons/bound.png"));
    }
}

// Icon_FileOnAResolverWithoutFileSupport_DrawsNothing and
// Icon_FileWithNoResolverWired_DrawsNothing.
#[test]
fn a_resolver_without_file_support_draws_no_file_icons() {
    assert!(drawn(&window("icons/bound.png"), &mut DidOnly).is_empty());
}
