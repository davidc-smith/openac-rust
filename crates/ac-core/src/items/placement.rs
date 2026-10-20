//! Putting an item into a container: whether the move is legal, which pack
//! an item picked up goes into, and the notice when it cannot go anywhere.

use std::collections::BTreeSet;

use super::{ClientObject, ClientObjectTable, ItemType};

/// Why an item may not go into a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementRejection {
    /// The item is unknown.
    InvalidItem,
    /// The item is the player.
    CannotMovePlayer,
    /// The item is a creature.
    CannotMoveCreature,
    /// The item is in the trade window.
    SourceBeingTraded,
    /// The destination is unknown or not a container.
    InvalidDestination,
    /// The destination is in the trade window.
    DestinationBeingTraded,
    /// The destination is the item or inside it.
    RecursiveContainment,
    /// No free item slot.
    ItemCapacityFull,
    /// No free container slot.
    ContainerCapacityFull,
}

/// The container-type value of a spell focus.
const FOCUS_CONTAINER_TYPE: u32 = 2;

/// Whether the object takes a container slot rather than an item slot. A
/// spell focus does, although it holds nothing; ask
/// [`is_focus`] before putting anything into one.
#[must_use]
pub fn is_container(item: &ClientObject) -> bool {
    item.container_type_hint != 0
        || item.item_type.intersects(ItemType::CONTAINER)
        || item.items_capacity != 0
        || item.containers_capacity != 0
}

/// A spell focus: counted as a container, but never somewhere an item can
/// go (the server refuses any request naming one as the destination).
#[must_use]
pub fn is_focus(item: &ClientObject) -> bool {
    item.container_type_hint == FOCUS_CONTAINER_TYPE
}

fn is_container_id(objects: &ClientObjectTable, id: u32) -> bool {
    objects.get(id).is_some_and(is_container)
}

/// Whether `item_id` may go into `destination_id`: a known item that is
/// not the player, not a creature and not being traded; a destination
/// that is the player or a known container not being traded, and not the
/// item or inside it; and a free slot of the right kind unless the item is
/// already directly inside (a reorder).
pub fn evaluate_container_placement(
    objects: &ClientObjectTable,
    item_id: u32,
    destination_id: u32,
    player_id: u32,
) -> Result<(), PlacementRejection> {
    let item = match objects.get(item_id) {
        Some(item) if item_id != 0 => item,
        _ => return Err(PlacementRejection::InvalidItem),
    };
    if item_id == player_id {
        return Err(PlacementRejection::CannotMovePlayer);
    }
    if item.item_type.intersects(ItemType::CREATURE) {
        return Err(PlacementRejection::CannotMoveCreature);
    }
    if item.trade_state == 1 {
        return Err(PlacementRejection::SourceBeingTraded);
    }
    let destination = objects.get(destination_id);
    let invalid = match destination {
        _ if destination_id == 0 => true,
        None => destination_id != player_id,
        Some(destination) => !is_container(destination) && destination_id != player_id,
    };
    if invalid {
        return Err(PlacementRejection::InvalidDestination);
    }
    if destination.is_some_and(|destination| destination.trade_state == 1) {
        return Err(PlacementRejection::DestinationBeingTraded);
    }
    if item_id == destination_id || is_contained_by(objects, destination_id, item_id) {
        return Err(PlacementRejection::RecursiveContainment);
    }

    let already_directly_contained = item.container_id == destination_id;
    let moving_container = is_container(item);
    let capacity = destination.map_or(0, |destination| {
        if moving_container {
            destination.containers_capacity
        } else {
            destination.items_capacity
        }
    });
    let occupied = objects
        .get_contents(destination_id)
        .iter()
        .filter(|&&child| {
            objects
                .get(child)
                .is_some_and(|child| is_container(child) == moving_container)
        })
        .count();
    if !already_directly_contained
        && capacity > 0
        && occupied >= usize::try_from(capacity).unwrap_or(usize::MAX)
    {
        return Err(if moving_container {
            PlacementRejection::ContainerCapacityFull
        } else {
            PlacementRejection::ItemCapacityFull
        });
    }
    Ok(())
}

fn is_contained_by(
    objects: &ClientObjectTable,
    candidate_id: u32,
    possible_ancestor_id: u32,
) -> bool {
    let mut visited = BTreeSet::new();
    let mut current = candidate_id;
    while current != 0 && visited.insert(current) {
        if current == possible_ancestor_id {
            return true;
        }
        current = objects.get(current).map_or(0, |item| item.container_id);
    }
    false
}

/// The notice the client prints itself for a refused placement. The
/// player's own inventory is called by the player object's name (the
/// "Backpack" notices).
#[must_use]
pub fn placement_rejection_text(
    rejection: PlacementRejection,
    item: Option<&ClientObject>,
    destination: Option<&ClientObject>,
    player_id: u32,
) -> String {
    let item_name = item.map_or_else(|| "item".into(), |item| item.appropriate_name());
    let destination_name = destination.map_or_else(
        || "container".into(),
        |destination| destination.appropriate_name(),
    );
    let destination_is_player = destination.is_some_and(|d| d.object_id() == player_id);
    match rejection {
        PlacementRejection::InvalidItem => "That item is not valid!".into(),
        PlacementRejection::CannotMovePlayer => {
            "You cannot place yourself within another object!".into()
        }
        PlacementRejection::CannotMoveCreature => "You cannot pick up creatures!".into(),
        PlacementRejection::SourceBeingTraded => format!("The {item_name} is being traded"),
        PlacementRejection::InvalidDestination => "The destination container is not valid!".into(),
        PlacementRejection::DestinationBeingTraded => {
            format!("The {destination_name} is being traded")
        }
        PlacementRejection::RecursiveContainment => {
            "You cannot place an object within itself!".into()
        }
        PlacementRejection::ItemCapacityFull if destination_is_player => {
            "Backpack is completely full!".into()
        }
        PlacementRejection::ItemCapacityFull => {
            format!("The {destination_name} is completely full!")
        }
        PlacementRejection::ContainerCapacityFull if destination_is_player => {
            format!("{destination_name} can carry no more containers!")
        }
        PlacementRejection::ContainerCapacityFull => {
            format!("The {destination_name} can fit no more containers!")
        }
    }
}

/// Whether `container_id` can take `item_id`: the container must be the
/// player or in the player's inventory, not a spell focus and not being
/// traded. A plain item needs a free item slot (a capacity of 0 or -1 is
/// unbounded, and an item already inside fits); a pack needs a free
/// container slot (-1 is unbounded, 0 means none). An undescribed player
/// object fits anything.
#[must_use]
pub fn will_item_fit_in_container(
    objects: &ClientObjectTable,
    item_id: u32,
    container_id: u32,
    player_id: u32,
) -> bool {
    if container_id == 0 || item_id == container_id {
        return false;
    }
    let Some(container) = objects.get(container_id) else {
        return container_id == player_id;
    };
    if is_focus(container) {
        return false;
    }
    let owned_by_player = container_id == player_id
        || (player_id != 0 && objects.is_owned_by_object(container_id, player_id));
    if !owned_by_player || container.trade_state == 1 {
        return false;
    }

    let contents = objects.get_contents(container_id);
    let moving_container = is_container_id(objects, item_id);
    let capacity = if moving_container {
        if container.containers_capacity < 0 {
            return true;
        }
        container.containers_capacity
    } else {
        if container.items_capacity <= 0 {
            return true;
        }
        container.items_capacity
    };
    let mut occupied = 0;
    for &id in contents {
        if id == item_id {
            return true;
        }
        if is_container_id(objects, id) == moving_container {
            occupied += 1;
        }
    }
    occupied < capacity
}

/// The container to ask the server for when `item_id` goes into the
/// inventory: the named `target_id` if it fits, else the root (the main
/// pack), else the first of the root's packs with room, in slot order.
/// When nothing has room the error is the capacity refusal, whose notice
/// names the root.
pub fn choose_container(
    objects: &ClientObjectTable,
    item_id: u32,
    root_id: u32,
    target_id: u32,
    player_id: u32,
) -> Result<u32, PlacementRejection> {
    if target_id != 0 && will_item_fit_in_container(objects, item_id, target_id, player_id) {
        return Ok(target_id);
    }
    if root_id != 0 && will_item_fit_in_container(objects, item_id, root_id, player_id) {
        return Ok(root_id);
    }
    if root_id != 0 {
        let mut packs: Vec<&ClientObject> = objects
            .get_contents(root_id)
            .iter()
            .filter_map(|&id| objects.get(id))
            .filter(|contained| is_container(contained))
            .collect();
        packs.sort_by_key(|pack| pack.container_slot);
        if let Some(pack) = packs
            .iter()
            .find(|pack| will_item_fit_in_container(objects, item_id, pack.object_id(), player_id))
        {
            return Ok(pack.object_id());
        }
    }
    Err(if is_container_id(objects, item_id) {
        PlacementRejection::ContainerCapacityFull
    } else {
        PlacementRejection::ItemCapacityFull
    })
}

#[cfg(test)]
mod tests {
    //! Ported from OpenAC
    //! `tests/AcDream.Core.Tests/Items/InventoryContainerPlacementPolicyTests.cs`
    //! and `InventoryPlacementSearchTests.cs`.

    use super::*;
    use crate::items::{EquipMask, TableEvent};

    const PLAYER: u32 = 0x5000_0001;
    const SIDE_PACK_A: u32 = 0x6000_0010;
    const SIDE_PACK_B: u32 = 0x6000_0011;

    struct World {
        t: ClientObjectTable,
        ev: Vec<TableEvent>,
    }

    impl World {
        fn new() -> Self {
            Self {
                t: ClientObjectTable::new(),
                ev: Vec::new(),
            }
        }

        fn add(&mut self, item: ClientObject) {
            self.t.add_or_update(item, &mut self.ev);
        }

        fn mv(&mut self, id: u32, container: u32, slot: i32) {
            self.t
                .move_item(id, container, slot, EquipMask::NONE, None, &mut self.ev);
        }
    }

    fn object(id: u32, name: &str, item_type: ItemType) -> ClientObject {
        let mut item = ClientObject::new(id);
        item.name = name.into();
        item.item_type = item_type;
        item
    }

    #[test]
    fn full_item_capacity_rejects_new_item_but_allows_reorder() {
        let mut w = World::new();
        let mut player = object(PLAYER, "Player", ItemType::NONE);
        player.items_capacity = 1;
        player.containers_capacity = 7;
        w.add(player);
        w.add(ClientObject::new(2));
        w.mv(2, PLAYER, 0);
        w.add(ClientObject::new(3));

        assert_eq!(
            evaluate_container_placement(&w.t, 3, PLAYER, PLAYER),
            Err(PlacementRejection::ItemCapacityFull)
        );
        assert_eq!(
            evaluate_container_placement(&w.t, 2, PLAYER, PLAYER),
            Ok(())
        );
    }

    #[test]
    fn container_cycle_and_trade_are_rejected() {
        let mut w = World::new();
        for id in [10, 11] {
            let mut bag = object(id, "Bag", ItemType::CONTAINER);
            bag.items_capacity = 24;
            w.add(bag);
        }
        w.mv(11, 10, 0);
        assert_eq!(
            evaluate_container_placement(&w.t, 10, 11, PLAYER),
            Err(PlacementRejection::RecursiveContainment)
        );
        w.t.get_mut(10).expect("present").trade_state = 1;
        assert_eq!(
            evaluate_container_placement(&w.t, 10, PLAYER, PLAYER),
            Err(PlacementRejection::SourceBeingTraded)
        );
    }

    #[test]
    fn full_message_matches_container_type_branches() {
        let player = object(PLAYER, "Backpack", ItemType::NONE);
        let bag = object(2, "Pack", ItemType::NONE);
        assert_eq!(
            placement_rejection_text(
                PlacementRejection::ItemCapacityFull,
                None,
                Some(&player),
                PLAYER
            ),
            "Backpack is completely full!"
        );
        assert_eq!(
            placement_rejection_text(
                PlacementRejection::ContainerCapacityFull,
                None,
                Some(&bag),
                PLAYER
            ),
            "The Pack can fit no more containers!"
        );
    }

    fn player_with_packs(main_capacity: i32, containers_capacity: i32) -> World {
        let mut w = World::new();
        let mut player = object(PLAYER, "Tester", ItemType::CREATURE);
        player.items_capacity = main_capacity;
        player.containers_capacity = containers_capacity;
        w.add(player);
        for (guid, name, slot) in [(SIDE_PACK_A, "Pack A", 0), (SIDE_PACK_B, "Pack B", 1)] {
            let mut pack = object(guid, name, ItemType::CONTAINER);
            pack.items_capacity = 2;
            w.add(pack);
            w.mv(guid, PLAYER, slot);
        }
        w
    }

    fn add_loose(w: &mut World, guid: u32, container: u32) -> u32 {
        w.add(object(guid, "Item", ItemType::MISC));
        let slot = w.t.get_contents(container).len() as i32;
        w.mv(guid, container, slot);
        guid
    }

    fn world_item(w: &mut World) -> u32 {
        w.add(object(0x7000_0001, "Loot", ItemType::MISC));
        0x7000_0001
    }

    #[test]
    fn named_container_with_room_is_used_as_is() {
        let mut w = player_with_packs(3, 2);
        let loot = world_item(&mut w);
        assert_eq!(
            choose_container(&w.t, loot, PLAYER, SIDE_PACK_B, PLAYER),
            Ok(SIDE_PACK_B)
        );
    }

    #[test]
    fn full_main_pack_falls_through_to_the_first_side_pack_with_room() {
        let mut w = player_with_packs(1, 2);
        add_loose(&mut w, 0x7000_0100, PLAYER);
        add_loose(&mut w, 0x7000_0101, SIDE_PACK_A);
        add_loose(&mut w, 0x7000_0102, SIDE_PACK_A);
        let loot = world_item(&mut w);
        assert_eq!(
            choose_container(&w.t, loot, PLAYER, PLAYER, PLAYER),
            Ok(SIDE_PACK_B)
        );
    }

    #[test]
    fn a_focus_is_never_chosen_as_a_placement_destination() {
        let mut w = player_with_packs(1, 2);
        add_loose(&mut w, 0x7000_0100, PLAYER);
        add_loose(&mut w, 0x7000_0101, SIDE_PACK_A);
        add_loose(&mut w, 0x7000_0102, SIDE_PACK_A);
        add_loose(&mut w, 0x7000_0103, SIDE_PACK_B);
        add_loose(&mut w, 0x7000_0104, SIDE_PACK_B);
        let mut focus = object(0x6000_0020, "Foci of Verdancy", ItemType::MISC);
        focus.container_type_hint = 2;
        w.add(focus);
        w.mv(0x6000_0020, PLAYER, 2);
        let loot = world_item(&mut w);
        assert_eq!(
            choose_container(&w.t, loot, PLAYER, PLAYER, PLAYER),
            Err(PlacementRejection::ItemCapacityFull)
        );
    }

    #[test]
    fn no_target_named_tries_the_main_pack_first() {
        let mut w = player_with_packs(3, 2);
        let loot = world_item(&mut w);
        assert_eq!(choose_container(&w.t, loot, PLAYER, 0, PLAYER), Ok(PLAYER));
    }

    #[test]
    fn everything_full_refuses_with_the_backpack_notice() {
        let mut w = player_with_packs(1, 2);
        add_loose(&mut w, 0x7000_0100, PLAYER);
        add_loose(&mut w, 0x7000_0101, SIDE_PACK_A);
        add_loose(&mut w, 0x7000_0102, SIDE_PACK_A);
        add_loose(&mut w, 0x7000_0103, SIDE_PACK_B);
        add_loose(&mut w, 0x7000_0104, SIDE_PACK_B);
        let loot = world_item(&mut w);
        let refusal = choose_container(&w.t, loot, PLAYER, PLAYER, PLAYER);
        assert_eq!(refusal, Err(PlacementRejection::ItemCapacityFull));
        assert_eq!(
            placement_rejection_text(
                PlacementRejection::ItemCapacityFull,
                w.t.get(loot),
                w.t.get(PLAYER),
                PLAYER
            ),
            "Backpack is completely full!"
        );
    }

    #[test]
    fn a_pack_needs_a_container_slot_and_the_notice_names_the_player() {
        let mut w = player_with_packs(3, 2);
        let mut pack = object(0x6000_0020, "New Pack", ItemType::CONTAINER);
        pack.items_capacity = 4;
        w.add(pack);
        assert_eq!(
            choose_container(&w.t, 0x6000_0020, PLAYER, PLAYER, PLAYER),
            Err(PlacementRejection::ContainerCapacityFull)
        );
        let expected = format!(
            "{} can carry no more containers!",
            w.t.get(PLAYER).expect("player").appropriate_name()
        );
        assert_eq!(
            placement_rejection_text(
                PlacementRejection::ContainerCapacityFull,
                w.t.get(0x6000_0020),
                w.t.get(PLAYER),
                PLAYER
            ),
            expected
        );
    }

    #[test]
    fn an_item_already_in_the_container_fits_even_when_it_is_full() {
        let mut w = player_with_packs(1, 2);
        let inside = add_loose(&mut w, 0x7000_0100, PLAYER);
        assert!(will_item_fit_in_container(&w.t, inside, PLAYER, PLAYER));
    }

    #[test]
    fn unbounded_capacity_always_fits() {
        let mut w = player_with_packs(-1, 2);
        for i in 0..10 {
            add_loose(&mut w, 0x7000_0200 + i, PLAYER);
        }
        let loot = world_item(&mut w);
        assert!(will_item_fit_in_container(&w.t, loot, PLAYER, PLAYER));
    }

    #[test]
    fn a_container_not_owned_by_the_player_never_fits() {
        let mut w = player_with_packs(3, 2);
        let mut chest = object(0x6000_0030, "Chest", ItemType::CONTAINER);
        chest.items_capacity = 10;
        w.add(chest);
        let loot = world_item(&mut w);
        assert!(!will_item_fit_in_container(&w.t, loot, 0x6000_0030, PLAYER));
    }
}
