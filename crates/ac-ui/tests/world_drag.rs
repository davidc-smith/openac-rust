//! What a drag started on the world lifts (OpenAC
//! `tests/AcDream.App.Tests/UI/WorldDragTests.cs`).

use ac_core::combat::CombatMode;
use ac_core::items::ItemType;
use ac_ui::draw::TextureRef;
use ac_ui::event::{ItemDragPayload, ItemDragSource};
use ac_ui::icons::IconSource;
use ac_ui::panels::inventory::PLAYER_PACK_BASE_ICON;
use ac_ui::world_drag::lift;

const PLAYER: u32 = 0x5000_0A01;

struct Icons {
    drag: bool,
}

impl IconSource for Icons {
    fn item_icon(&mut self, t: ItemType, icon: u32, _: u32, _: u32, _: u32) -> Option<TextureRef> {
        (t == ItemType::CONTAINER && icon == PLAYER_PACK_BASE_ICON)
            .then_some(TextureRef::Runtime(2))
    }
    fn drag_icon(&mut self, t: ItemType, icon: u32, _: u32, _: u32, _: u32) -> Option<TextureRef> {
        (self.drag && t == ItemType::CONTAINER && icon == PLAYER_PACK_BASE_ICON)
            .then_some(TextureRef::Runtime(1))
    }
    fn spell_icon(&mut self, _: u32) -> Option<TextureRef> {
        None
    }
    fn component_icon(&mut self, _: u32) -> Option<TextureRef> {
        None
    }
}

#[test]
fn your_own_character_lifts_your_main_pack_out_of_combat() {
    let (payload, ghost) = lift(
        PLAYER,
        PLAYER,
        CombatMode::NON_COMBAT,
        &mut Icons { drag: true },
    )
    .unwrap();
    let p = payload.get::<ItemDragPayload>().unwrap();
    assert_eq!(p.obj_id, PLAYER);
    assert_eq!(p.source_kind, ItemDragSource::Inventory);
    assert_eq!(p.source_slot, 0);
    let g = ghost.unwrap();
    assert_eq!(
        (g.texture, g.width, g.height),
        (TextureRef::Runtime(1), 32, 32)
    );
}

#[test]
fn without_a_drag_picture_the_lift_shows_the_pack_icon() {
    let (_, ghost) = lift(
        PLAYER,
        PLAYER,
        CombatMode::NON_COMBAT,
        &mut Icons { drag: false },
    )
    .unwrap();
    assert_eq!(ghost.unwrap().texture, TextureRef::Runtime(2));
}

#[test]
fn nothing_else_lifts_and_nothing_lifts_in_combat() {
    let icons = &mut Icons { drag: true };
    assert!(lift(PLAYER, PLAYER, CombatMode::MELEE, icons).is_none());
    assert!(lift(0x7000_0001, PLAYER, CombatMode::NON_COMBAT, icons).is_none());
    assert!(lift(0, PLAYER, CombatMode::NON_COMBAT, icons).is_none());
}
