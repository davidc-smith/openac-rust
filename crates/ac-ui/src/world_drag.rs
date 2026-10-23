//! The world side of a drag: what the object under a press point lifts.
//! Only the player lifts, and only out of combat, as the game does:
//! dragging your own character carries your main pack, so it can go on the
//! hotbar as a shortcut that targets yourself (healing kits, mana stones).
//! Other world objects lift nothing yet.

use ac_core::combat::CombatMode;
use ac_core::items::ItemType;

use crate::event::{DragPayload, ItemDragPayload, ItemDragSource};
use crate::icons::IconSource;
use crate::panels::inventory::PLAYER_PACK_BASE_ICON;
use crate::tree::DragGhost;

/// Where the main pack sits in the pack row: lifting the player off the
/// world is the same lift as one started on that slot, so every drop
/// target that knows that payload treats it the same.
pub const MAIN_PACK_SLOT_INDEX: i32 = 0;

/// The drag ghost's size in pixels, the paperdoll figure's.
pub const GHOST_SIZE: i32 = 32;

/// The lift for the object `hit` under the press point, or `None` when it
/// lifts nothing.
pub fn lift(
    hit: u32,
    player: u32,
    combat_mode: CombatMode,
    icons: &mut dyn IconSource,
) -> Option<(DragPayload, Option<DragGhost>)> {
    if hit == 0 || hit != player || combat_mode != CombatMode::NON_COMBAT {
        return None;
    }
    let payload = ItemDragPayload::new(hit, ItemDragSource::Inventory, MAIN_PACK_SLOT_INDEX);
    // The player has no icon of their own; the pack row draws the main pack
    // with a fixed one, and this lift shows the same picture.
    let ghost = icons
        .drag_icon(ItemType::CONTAINER, PLAYER_PACK_BASE_ICON, 0, 0, 0)
        .or_else(|| icons.item_icon(ItemType::CONTAINER, PLAYER_PACK_BASE_ICON, 0, 0, 0))
        .map(|texture| DragGhost {
            texture,
            width: GHOST_SIZE,
            height: GHOST_SIZE,
        });
    Some((DragPayload::new(payload), ghost))
}
