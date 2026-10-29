//! The outside-container window.
//!
//! Ported from OpenAC `tests/AcDream.App.Tests/UI/Layout/ExternalContainerControllerTests.cs`
//! on a synthetic window shaped like the retail one (layout 0x21000008; the
//! real one is imported in `real_dats`). OpenAC's `sendUse`,
//! `sendPutItemInContainer`, `sendSplitToContainer` and the item policy's
//! pickup are `SessionCommand::Item` outputs here (`CloseContainer`,
//! `DropInto`, `Use`); the open container and its range come in
//! [`ExternalContainerFacts`]. The pending-pickup gate cases
//! (`PendingPickupRejects*`, `ExternalPutOwnsGlobalGate*`) are the
//! runtime's one-request gate (`ItemsView::can_make_request`).

mod common;

use std::collections::HashMap;

use ac_core::items::{ClientObject, ItemType, ItemUseable};
use ac_runtime::inventory::{ItemCommand, ObjectRef, PendingPlacement};
use ac_runtime::session::SessionCommand;
use ac_ui::ElementId;
use ac_ui::event::{DragAcceptance, DragPayload, ItemDragPayload, ItemDragSource, kind};
use ac_ui::host::UiHost;
use ac_ui::import::ImportedLayout;
use ac_ui::panels::PanelOutput;
use ac_ui::panels::external_container::{
    self, ExternalContainerController, ExternalContainerFacts, ExternalContainerSprites,
};
use ac_ui::panels::items::{ItemsView, NameResolver};
use ac_ui::widgets::{ItemList, ItemSlot, Scrollbar};
use common::items::*;
use common::panels::*;
use common::*;

const CHEST: u32 = 0x7000_0001;
const ITEM: u32 = 0x6000_0001;
const WINDOW: &str = "external-container";

macro_rules! assert_dbg_eq {
    ($a:expr, $b:expr $(,)?) => {
        assert_eq!(format!("{:?}", $a), format!("{:?}", $b))
    };
}

struct Strip {
    host: UiHost,
    ids: HashMap<u32, ElementId>,
    objects: Objects,
    icons: TestIcons,
    facts: ExternalContainerFacts,
    can_make_request: bool,
    placement: Option<PendingPlacement>,
}

fn strip() -> Strip {
    let mut host = UiHost::new();
    host.root.set_size(800.0, 600.0);
    let ui = &mut host.root;
    let r = ui.root();
    let root = panel(ui, r, at(0.0, 0.0, 800.0, 110.0));
    let mut ids = HashMap::new();
    ids.insert(
        external_container::TOP_CONTAINER_ID,
        list(ui, root, 0.0, 0.0, 36.0, 36.0),
    );
    ids.insert(
        external_container::CONTAINER_LIST_ID,
        list(ui, root, 40.0, 0.0, 680.0, 36.0),
    );
    ids.insert(
        external_container::CONTENTS_LIST_ID,
        list(ui, root, 0.0, 40.0, 784.0, 32.0),
    );
    add_std_button(ui, &mut ids, root, external_container::CLOSE_BUTTON_ID);
    let bar = ui.add(root, at(0.0, 80.0, 784.0, 16.0), Scrollbar::default());
    ids.insert(external_container::CONTENTS_SCROLLBAR_ID, bar);
    let layout = ImportedLayout::from_parts(root, ids.clone());
    ui.register_window(WINDOW, root, None, 0).unwrap();
    let c =
        ExternalContainerController::bind(ui, &layout, WINDOW, ExternalContainerSprites::default())
            .expect("binds");
    host.attach_controller(WINDOW, Box::new(c)).unwrap();
    let mut objects = Objects::new();
    objects.player(102, 7);
    Strip {
        host,
        ids,
        objects,
        icons: TestIcons::default(),
        facts: ExternalContainerFacts::default(),
        can_make_request: true,
        placement: None,
    }
}

impl Strip {
    /// The session opened `id` with these loose items in it.
    fn open(&mut self, id: u32, contents: &[u32]) {
        let mut chest = ClientObject::new(id);
        chest.item_type = ItemType::CONTAINER;
        chest.items_capacity = 24;
        chest.useability = Some(ItemUseable::REMOTE);
        self.objects.add(chest);
        for (i, &g) in contents.iter().enumerate() {
            if self.objects.t.get(g).is_none() {
                self.objects.add(plain_item(g));
            }
            self.objects.mv(g, id, i as i32);
        }
        self.facts = ExternalContainerFacts {
            container: id,
            in_range: true,
        };
        self.update();
    }

    fn update(&mut self) {
        self.update_selected(None);
    }

    fn update_selected(&mut self, selected: Option<u32>) {
        let names = |o: &ClientObject| composed_name(o);
        let resolver: NameResolver<'_> = &names;
        let view = ItemsView {
            names: Some(resolver),
            selected,
            can_make_request: self.can_make_request,
            pending_placement: self.placement,
            ..ItemsView::new(&self.objects.t, PLAYER)
        };
        let (facts, icons) = (self.facts, &mut self.icons);
        self.host
            .with_controller::<ExternalContainerController, _>(WINDOW, |c, ui| {
                c.update(ui, &view, facts, icons);
            })
            .unwrap();
    }

    fn cell(&self, list: u32, i: usize) -> ElementId {
        self.host
            .root
            .widget::<ItemList>(self.ids[&list])
            .unwrap()
            .item(i)
            .unwrap()
    }

    fn slot(&self, list: u32, i: usize) -> &ItemSlot {
        self.host
            .root
            .widget::<ItemSlot>(self.cell(list, i))
            .unwrap()
    }

    fn fire(&mut self, list: u32, i: usize, k: i32, p: Option<ItemDragPayload>) {
        let c = self.cell(list, i);
        self.host.root.fire_event(k, c, p.map(DragPayload::new));
        self.host.pump();
    }

    fn outputs(&mut self) -> Vec<PanelOutput> {
        panel_outputs(&mut self.host)
    }

    fn visible(&self) -> bool {
        self.host.root.is_window_visible(WINDOW)
    }

    fn closes(outputs: &[PanelOutput]) -> Vec<u32> {
        sessions(outputs)
            .into_iter()
            .filter_map(|c| match c {
                SessionCommand::Item(ItemCommand::CloseContainer(ObjectRef::Guid(g))) => Some(g),
                _ => None,
            })
            .collect()
    }
}

const TOP: u32 = external_container::TOP_CONTAINER_ID;
const PACKS: u32 = external_container::CONTAINER_LIST_ID;
const CONTENTS: u32 = external_container::CONTENTS_LIST_ID;

#[test]
fn the_containers_meter_hides_while_empty_then_follows_its_loose_items() {
    let mut s = strip();
    s.open(CHEST, &[]);
    assert_eq!(s.slot(TOP, 0).capacity_fill, -1.0);
    s.open(CHEST, &[ITEM]);
    assert_eq!(s.slot(TOP, 0).capacity_fill, 1.0 / 24.0);
}

#[test]
fn the_hover_caption_carries_the_material_prefix_and_stays_plain_without_one() {
    let mut s = strip();
    s.objects.add(material_item(ITEM));
    s.objects.add(plain_item(ITEM + 1));
    s.open(CHEST, &[ITEM, ITEM + 1]);
    assert_eq!(
        s.slot(CONTENTS, 0).tooltip.as_deref(),
        Some("Pyreal Scarab")
    );
    assert_eq!(s.slot(CONTENTS, 1).tooltip.as_deref(), Some("Bread Loaf"));
}

#[test]
fn an_update_with_nothing_moved_keeps_the_cells() {
    let mut s = strip();
    s.open(CHEST, &[ITEM]);
    let (cell, chest) = (s.cell(CONTENTS, 0), s.cell(TOP, 0));
    s.can_make_request = false;
    s.update();
    s.objects.get_mut(ITEM).properties.ints.insert(1, 42);
    s.update();
    assert_eq!(s.cell(CONTENTS, 0), cell);
    assert_eq!(s.cell(TOP, 0), chest);
}

#[test]
fn an_open_container_shows_the_strip_and_fills_its_rows() {
    let mut s = strip();
    assert!(!s.visible(), "hidden until a container opens");
    s.open(CHEST, &[ITEM]);
    assert!(s.visible());
    assert_eq!(s.slot(TOP, 0).object_id, CHEST);
    assert_eq!(s.slot(CONTENTS, 0).object_id, ITEM);
    assert_eq!(s.slot(CONTENTS, 0).source_kind, ItemDragSource::Ground);
    let contents = s.host.root.widget::<ItemList>(s.ids[&CONTENTS]).unwrap();
    assert!(contents.horizontal_scroll && contents.single_row);
    let bar = s
        .host
        .root
        .widget::<Scrollbar>(s.ids[&external_container::CONTENTS_SCROLLBAR_ID])
        .unwrap();
    assert_eq!(
        bar.model,
        ac_ui::widgets::ScrollSource::Element(s.ids[&CONTENTS])
    );
    assert!(bar.horizontal);
    assert_eq!(bar.track_sprite, 0x0600_4C7F);
}

#[test]
fn a_double_click_on_loot_uses_it() {
    let mut s = strip();
    s.open(CHEST, &[ITEM]);
    s.outputs();
    s.fire(CONTENTS, 0, kind::DOUBLE_CLICK, None);
    assert_eq!(uses(&s.outputs()), vec![ITEM]);
}

#[test]
fn a_right_click_on_loot_selects_and_examines_without_taking_it() {
    let mut s = strip();
    s.open(CHEST, &[ITEM]);
    s.outputs();
    s.fire(CONTENTS, 0, kind::RIGHT_CLICK, None);
    assert_dbg_eq!(
        s.outputs(),
        vec![
            PanelOutput::SelectObject(ITEM),
            PanelOutput::Session(SessionCommand::Item(ItemCommand::Appraise(
                ObjectRef::Guid(ITEM)
            ))),
        ]
    );
}

#[test]
fn a_press_on_loot_selects_before_the_click_completes() {
    let mut s = strip();
    s.open(CHEST, &[ITEM]);
    s.outputs();
    s.fire(CONTENTS, 0, kind::MOUSE_DOWN, None);
    assert!(s.slot(CONTENTS, 0).selected);
    let out = s.outputs();
    assert_dbg_eq!(out, vec![PanelOutput::SelectObject(ITEM)]);
    assert!(uses(&out).is_empty());
}

#[test]
fn closing_or_leaving_range_uses_the_container_once() {
    let mut s = strip();
    s.open(CHEST, &[]);
    s.outputs();
    s.facts.in_range = false;
    s.update();
    s.update();
    assert_eq!(Strip::closes(&s.outputs()), vec![CHEST]);
    assert!(!s.visible());
    // The server's close.
    s.facts = ExternalContainerFacts::default();
    s.update();
    assert!(Strip::closes(&s.outputs()).is_empty());
}

#[test]
fn the_close_button_and_a_double_click_on_the_container_close_it() {
    let mut s = strip();
    s.open(CHEST, &[]);
    s.outputs();
    let close = s.ids[&external_container::CLOSE_BUTTON_ID];
    s.host.root.fire_event(kind::CLICK, close, None);
    s.host.pump();
    assert_eq!(Strip::closes(&s.outputs()), vec![CHEST]);
    assert!(!s.visible());

    let mut s = strip();
    s.open(CHEST, &[]);
    s.outputs();
    s.fire(TOP, 0, kind::DOUBLE_CLICK, None);
    assert_eq!(Strip::closes(&s.outputs()), vec![CHEST]);
}

#[test]
fn a_replacement_shows_the_new_container() {
    let mut s = strip();
    s.open(CHEST, &[]);
    s.open(0x7000_0002, &[]);
    assert!(s.visible());
    assert_eq!(s.slot(TOP, 0).object_id, 0x7000_0002);
    assert!(Strip::closes(&s.outputs()).is_empty());
}

#[test]
fn a_replacement_request_hides_the_old_view_without_a_close() {
    let mut s = strip();
    s.open(CHEST, &[]);
    s.outputs();
    // Another container was asked for: none is open until it arrives.
    s.facts = ExternalContainerFacts {
        container: 0,
        in_range: false,
    };
    s.update();
    assert!(!s.visible());
    assert!(Strip::closes(&s.outputs()).is_empty());
    s.open(0x7000_0002, &[]);
    assert!(s.visible());
}

#[test]
fn a_part_of_an_owned_stack_dropped_in_splits_into_the_chest() {
    let mut s = strip();
    s.open(CHEST, &[]);
    let mut item = ClientObject::new(ITEM);
    item.stack_size = 5;
    item.item_type = ItemType::MISC;
    s.objects.add(item);
    s.objects.mv(ITEM, PLAYER, 0);
    let mut split = ac_core::items::StackSplitQuantity::new();
    split.reset(5, Some(2));
    let names = |o: &ClientObject| composed_name(o);
    let resolver: NameResolver<'_> = &names;
    s.outputs();
    s.fire(CONTENTS, 0, kind::DROP_RELEASED, Some(payload(ITEM)));
    let view = ItemsView {
        names: Some(resolver),
        selected: Some(ITEM),
        split: Some(&split),
        ..ItemsView::new(&s.objects.t, PLAYER)
    };
    let (facts, icons) = (s.facts, &mut s.icons);
    s.host
        .with_controller::<ExternalContainerController, _>(WINDOW, |c, ui| {
            c.update(ui, &view, facts, icons);
        });
    assert_eq!(
        moves(&s.outputs()),
        vec![(ITEM, ObjectRef::Guid(CHEST), 0, 2)]
    );
}

#[test]
fn a_request_in_flight_refuses_a_drop_into_the_chest() {
    let mut s = strip();
    s.open(CHEST, &[]);
    s.objects.contained(ITEM, PLAYER, 0, ItemType::MISC);
    s.can_make_request = false;
    s.outputs();
    s.fire(CONTENTS, 0, kind::DROP_RELEASED, Some(payload(ITEM)));
    s.update();
    let out = s.outputs();
    assert!(moves(&out).is_empty());
    assert_eq!(notices(&out).len(), 1, "{out:?}");
}

#[test]
fn an_item_from_the_pack_lands_left_of_the_chest_item_dropped_on() {
    // OpenAC #250: the hovered cell's slot whatever the source.
    let mut s = strip();
    s.objects.add(plain_item(0x7000_0101));
    s.objects.add(plain_item(0x7000_0102));
    s.open(CHEST, &[0x7000_0101, 0x7000_0102]);
    s.objects.contained(ITEM, PLAYER, 0, ItemType::MISC);
    s.outputs();
    s.fire(CONTENTS, 1, kind::DROP_RELEASED, Some(payload(ITEM)));
    s.update();
    assert_eq!(
        moves(&s.outputs()),
        vec![(ITEM, ObjectRef::Guid(CHEST), 1, 0)]
    );
}

#[test]
fn a_reorder_inside_the_chest_lands_left_of_the_item_dropped_on() {
    // OpenAC #249: the server closes the old slot up first, so an item
    // moving later asks for one slot less; onto its right-hand neighbour
    // it is already in place and nothing is sent.
    let mut s = strip();
    s.open(CHEST, &[0x7000_0101, 0x7000_0102, 0x7000_0103]);
    s.outputs();
    let p = ItemDragPayload::new(0x7000_0101, ItemDragSource::Ground, 0);
    s.fire(CONTENTS, 1, kind::DROP_RELEASED, Some(p.clone()));
    s.update();
    assert!(moves(&s.outputs()).is_empty());
    s.fire(CONTENTS, 2, kind::DROP_RELEASED, Some(p));
    s.update();
    assert_eq!(
        moves(&s.outputs()),
        vec![(0x7000_0101, ObjectRef::Guid(CHEST), 1, 0)]
    );
}

#[test]
fn a_pack_is_refused_over_the_contents_and_shortcuts_are_ignored() {
    let mut s = strip();
    s.open(CHEST, &[]);
    s.objects.bag(0xB0, 1, 24);
    s.fire(CONTENTS, 0, kind::DRAG_ENTER, Some(payload(0xB0)));
    s.update();
    assert_eq!(
        s.slot(CONTENTS, 0).drag_acceptance(),
        DragAcceptance::Reject
    );
    s.objects.contained(ITEM, PLAYER, 0, ItemType::MISC);
    s.fire(CONTENTS, 0, kind::DRAG_ENTER, Some(payload(ITEM)));
    s.update();
    assert_eq!(
        s.slot(CONTENTS, 0).drag_acceptance(),
        DragAcceptance::Accept
    );
    let shortcut = ItemDragPayload::new(ITEM, ItemDragSource::ShortcutBar, 0);
    s.fire(CONTENTS, 0, kind::DRAG_ENTER, Some(shortcut));
    s.update();
    assert_eq!(s.slot(CONTENTS, 0).drag_acceptance(), DragAcceptance::None);
}

#[test]
fn a_pack_inside_the_container_opens_on_a_press_without_a_request() {
    let mut s = strip();
    let mut pack = ClientObject::new(0x7000_0200);
    pack.item_type = ItemType::CONTAINER;
    pack.items_capacity = 10;
    s.objects.add(pack);
    s.open(CHEST, &[0x7000_0200, ITEM]);
    s.objects
        .contained(0x7000_0201, 0x7000_0200, 0, ItemType::MISC);
    s.update();
    assert_eq!(s.slot(PACKS, 0).object_id, 0x7000_0200);
    assert_eq!(s.slot(CONTENTS, 0).object_id, ITEM);
    s.outputs();
    s.fire(PACKS, 0, kind::MOUSE_DOWN, None);
    let out = s.outputs();
    assert_dbg_eq!(out, vec![PanelOutput::SelectObject(0x7000_0200)]);
    s.update();
    assert_eq!(s.slot(CONTENTS, 0).object_id, 0x7000_0201);
    assert!(s.slot(PACKS, 0).is_open_container);
}

#[test]
fn an_item_put_in_the_chest_waits_at_its_slot_until_the_session_says_it_ended() {
    // ExternalContainerController.OnPendingPlacementRequested (kept only
    // when it names the pack open then), Populate (the item inserted at its
    // slot) and IsWaiting; ResolvePendingPlacement drops it.
    let mut s = strip();
    s.objects.add(plain_item(0x7000_0101));
    s.open(CHEST, &[0x7000_0101]);
    s.objects.contained(ITEM, PLAYER, 0, ItemType::MISC);
    let pending = |token, container| PendingPlacement {
        token,
        item_id: ITEM,
        container_id: container,
        placement: 0,
    };
    s.placement = Some(pending(3, CHEST));
    s.update();
    assert_eq!(s.slot(CONTENTS, 0).object_id, ITEM);
    assert!(s.slot(CONTENTS, 0).waiting());
    assert_eq!(s.slot(CONTENTS, 1).object_id, 0x7000_0101);
    assert!(!s.slot(CONTENTS, 1).waiting());
    s.placement = None;
    s.update();
    assert_eq!(s.slot(CONTENTS, 0).object_id, 0x7000_0101);
    // One made for another pack is never shown here.
    s.placement = Some(pending(4, 0x7000_0300));
    s.update();
    assert_eq!(s.slot(CONTENTS, 0).object_id, 0x7000_0101);
}

// ── Real dats ───────────────────────────────────────────────────────────

mod real_dats {
    use super::*;
    use ac_ui::content::DatContent;
    use ac_ui::font::DEFAULT_FONT_ID;
    use ac_ui::import::{LayoutSource, import_rooted};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn content() -> DatContent {
        let dir = std::env::var_os("OPENAC_DAT_DIR")
            .map(PathBuf::from)
            .or_else(ac_dat::default_dat_dir)
            .expect("no dat directory: set OPENAC_DAT_DIR");
        DatContent::new(Arc::new(
            ac_dat::DatCollection::open(dir).expect("open the retail dats"),
        ))
        .expect("master property table")
    }

    #[test]
    #[ignore = "needs the retail dat files"]
    fn the_strip_imports_binds_and_shows_a_chest() {
        let c = content();
        let mut host = UiHost::new();
        host.root.set_size(1024.0, 768.0);
        let layout = import_rooted(
            &mut host.root,
            &c,
            external_container::LAYOUT_ID,
            external_container::ROOT_ID,
            c.font(DEFAULT_FONT_ID),
        )
        .expect("layout 0x21000008");
        let r = host.root.root();
        host.root.add_child(r, layout.root);
        host.root
            .register_window(WINDOW, layout.root, None, 0)
            .unwrap();
        let sprites = ExternalContainerSprites {
            contents_empty: ac_ui::panels::cell_template::empty_sprite(
                &c,
                external_container::LAYOUT_ID,
                external_container::CONTENTS_LIST_ID,
            ),
            container_empty: ac_ui::panels::cell_template::empty_sprite(
                &c,
                external_container::LAYOUT_ID,
                external_container::CONTAINER_LIST_ID,
            ),
        };
        assert_ne!(sprites.contents_empty, 0);
        let ctl = ExternalContainerController::bind(&mut host.root, &layout, WINDOW, sprites)
            .expect("binds");
        host.attach_controller(WINDOW, Box::new(ctl)).unwrap();
        let mut objects = Objects::new();
        objects.player(102, 7);
        let mut chest = ClientObject::new(CHEST);
        chest.item_type = ItemType::CONTAINER;
        chest.items_capacity = 24;
        objects.add(chest);
        objects.contained(ITEM, CHEST, 0, ItemType::MISC);
        let view = ItemsView::new(&objects.t, PLAYER);
        let mut icons = TestIcons::default();
        host.with_controller::<ExternalContainerController, _>(WINDOW, |w, ui| {
            w.update(
                ui,
                &view,
                ExternalContainerFacts {
                    container: CHEST,
                    in_range: true,
                },
                &mut icons,
            );
        });
        assert!(host.root.is_window_visible(WINDOW));
        let contents = layout.find(external_container::CONTENTS_LIST_ID).unwrap();
        let first = host
            .root
            .widget::<ItemList>(contents)
            .unwrap()
            .item(0)
            .unwrap();
        assert_eq!(host.root.widget::<ItemSlot>(first).unwrap().object_id, ITEM);
        host.tick(0.016, 16, None);
        assert!(!host.draw((1024.0, 768.0), &c).main.is_empty());
        assert!(c.take_errors().is_empty());
    }
}

/// More loose items than the row shows at once (24 cells fit): every item
/// keeps its cell, and the next update leaves the cells alone, so a press
/// on one is not lost to a rebuild.
#[test]
fn a_container_with_more_items_than_fit_keeps_a_cell_per_item_across_updates() {
    let mut s = strip();
    let items: Vec<u32> = (0..30).map(|i| ITEM + i).collect();
    s.open(CHEST, &items);
    let list = s.ids[&CONTENTS];
    let cells = |s: &Strip| {
        s.host
            .root
            .widget::<ItemList>(list)
            .unwrap()
            .cells()
            .to_vec()
    };
    let first = cells(&s);
    assert_eq!(first.len(), items.len());
    for (i, &g) in items.iter().enumerate() {
        assert_eq!(s.slot(CONTENTS, i).object_id, g);
    }
    s.update();
    assert_eq!(cells(&s), first);
}
