//! Housing: the house the player owns or rents, its rent, its access list,
//! the houses for sale, and the guest and storage commands.
//!
//! Game-action builders take the session's next action sequence number
//! and are sent on the UI queue. The house-status query itself is
//! [`crate::build_house_query`].

use ac_bits::Reader;

use crate::client_commands::string_u32_action;
use crate::wire::{bare_action, read_bool32, read_count, read_string, string_action, u32_action};
use crate::{GameActionType, HouseRestrictions, Position, Result};

/// One item of a purchase or rent price.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HousePayment {
    /// How many are due.
    pub num: i32,
    /// How many are paid.
    pub paid: i32,
    /// The item's weenie class.
    pub weenie_id: u32,
    /// Its name.
    pub name: String,
    /// Its plural name.
    pub plural_name: String,
}

fn read_payments(r: &mut Reader<'_>) -> Result<Vec<HousePayment>> {
    // Each entry takes at least three words and two string lengths.
    let count = read_count(r, 16, "house payments")?;
    (0..count)
        .map(|_| {
            Ok(HousePayment {
                num: r.i32()?,
                paid: r.i32()?,
                weenie_id: r.u32()?,
                name: read_string(r)?,
                plural_name: read_string(r)?,
            })
        })
        .collect()
}

/// The player's house ([`crate::GameEventType::HOUSE_DATA`]).
#[derive(Debug, Clone, PartialEq)]
pub struct HouseData {
    /// When it was bought, server time in seconds.
    pub buy_time: u32,
    /// When rent was last paid.
    pub rent_time: u32,
    /// House type (1 cottage, 2 villa, 3 mansion, 4 apartment; 0 none).
    pub house_type: u32,
    /// No upkeep is due.
    pub maintenance_free: bool,
    /// The purchase price.
    pub buy: Vec<HousePayment>,
    /// The rent.
    pub rent: Vec<HousePayment>,
    /// Where the house is.
    pub position: Position,
}

impl HouseData {
    /// Parse the event payload: buy time, rent time, type, upkeep-free
    /// word, the buy list, the rent list, the position.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let mut r = Reader::new(payload);
        Ok(Self {
            buy_time: r.u32()?,
            rent_time: r.u32()?,
            house_type: r.u32()?,
            maintenance_free: read_bool32(&mut r)?,
            buy: read_payments(&mut r)?,
            rent: read_payments(&mut r)?,
            position: Position::read(&mut r)?,
        })
    }
}

/// The rent now due ([`crate::GameEventType::UPDATE_RENT_PAYMENT`]).
pub fn parse_rent_payment(payload: &[u8]) -> Result<Vec<HousePayment>> {
    read_payments(&mut Reader::new(payload))
}

/// The single word of a house-status event
/// ([`crate::GameEventType::HOUSE_STATUS`], an error code) or a rent-time
/// event ([`crate::GameEventType::UPDATE_RENT_TIME`], server time).
pub fn parse_house_word(payload: &[u8]) -> Result<u32> {
    Ok(Reader::new(payload).u32()?)
}

/// A house's access list changed
/// ([`crate::GameEventType::HOUSE_UPDATE_RESTRICTIONS`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HouseUpdateRestrictions {
    /// Update sequence.
    pub sequence: u8,
    /// The house object.
    pub sender_id: u32,
    /// The access list.
    pub restrictions: HouseRestrictions,
}

impl HouseUpdateRestrictions {
    /// Parse the event payload: a sequence byte, the house, then the access
    /// list as in an object description.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let mut r = Reader::new(payload);
        Ok(Self {
            sequence: r.u8()?,
            sender_id: r.u32()?,
            restrictions: HouseRestrictions::read(&mut r)?,
        })
    }
}

/// Houses for sale ([`crate::GameEventType::AVAILABLE_HOUSES`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableHouses {
    /// The house type asked about.
    pub house_type: u32,
    /// Landblock ids of the first houses for sale.
    pub locations: Vec<u32>,
    /// How many are for sale in all.
    pub total_available: i32,
}

impl AvailableHouses {
    /// Parse the event payload: type, location count and ids, total.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let mut r = Reader::new(payload);
        let house_type = r.u32()?;
        let count = read_count(&mut r, 4, "house locations")?;
        let locations = (0..count)
            .map(|_| r.u32())
            .collect::<ac_bits::Result<_>>()?;
        Ok(Self {
            house_type,
            locations,
            total_available: r.i32()?,
        })
    }
}

/// List the houses of one type for sale.
#[must_use]
pub fn build_list_available_houses(sequence: u32, house_type: u32) -> Vec<u8> {
    u32_action(sequence, GameActionType::LIST_AVAILABLE_HOUSES, house_type)
}

/// Give a player access to the house.
pub fn build_add_player_permission(sequence: u32, player_name: &str) -> Result<Vec<u8>> {
    string_action(sequence, GameActionType::ADD_PLAYER_PERMISSION, player_name)
}

/// Take a player's access away.
pub fn build_remove_player_permission(sequence: u32, player_name: &str) -> Result<Vec<u8>> {
    string_action(
        sequence,
        GameActionType::REMOVE_PLAYER_PERMISSION,
        player_name,
    )
}

/// Abandon the house.
#[must_use]
pub fn build_abandon_house(sequence: u32) -> Vec<u8> {
    bare_action(sequence, GameActionType::ABANDON_HOUSE)
}

/// Open the house to the public or close it.
#[must_use]
pub fn build_set_open_house_status(sequence: u32, open: bool) -> Vec<u8> {
    u32_action(
        sequence,
        GameActionType::SET_OPEN_HOUSE_STATUS,
        u32::from(open),
    )
}

/// Add a permanent guest.
pub fn build_add_permanent_guest(sequence: u32, player_name: &str) -> Result<Vec<u8>> {
    string_action(sequence, GameActionType::ADD_PERMANENT_GUEST, player_name)
}

/// Remove a permanent guest.
pub fn build_remove_permanent_guest(sequence: u32, player_name: &str) -> Result<Vec<u8>> {
    string_action(
        sequence,
        GameActionType::REMOVE_PERMANENT_GUEST,
        player_name,
    )
}

/// Remove every permanent guest.
#[must_use]
pub fn build_remove_all_permanent_guests(sequence: u32) -> Vec<u8> {
    bare_action(sequence, GameActionType::REMOVE_ALL_PERMANENT_GUESTS)
}

/// Give (`true`) or take a guest's storage access: name, then 1 or 0.
pub fn build_change_storage_permission(
    sequence: u32,
    player_name: &str,
    enabled: bool,
) -> Result<Vec<u8>> {
    string_u32_action(
        sequence,
        GameActionType::CHANGE_STORAGE_PERMISSION,
        player_name,
        u32::from(enabled),
    )
}

/// Give every guest storage access.
#[must_use]
pub fn build_add_all_storage_permission(sequence: u32) -> Vec<u8> {
    bare_action(sequence, GameActionType::ADD_ALL_STORAGE_PERMISSION)
}

/// Take storage access from everyone.
#[must_use]
pub fn build_remove_all_storage_permission(sequence: u32) -> Vec<u8> {
    bare_action(sequence, GameActionType::REMOVE_ALL_STORAGE_PERMISSION)
}

/// Ask for the full guest list.
#[must_use]
pub fn build_request_full_guest_list(sequence: u32) -> Vec<u8> {
    bare_action(sequence, GameActionType::REQUEST_FULL_GUEST_LIST)
}

/// Boot one guest.
pub fn build_boot_specific_house_guest(sequence: u32, player_name: &str) -> Result<Vec<u8>> {
    string_action(
        sequence,
        GameActionType::BOOT_SPECIFIC_HOUSE_GUEST,
        player_name,
    )
}

/// Boot everyone.
#[must_use]
pub fn build_boot_everyone(sequence: u32) -> Vec<u8> {
    bare_action(sequence, GameActionType::BOOT_EVERYONE)
}

/// Show or hide the house hooks.
#[must_use]
pub fn build_set_hooks_visibility(sequence: u32, visible: bool) -> Vec<u8> {
    u32_action(
        sequence,
        GameActionType::SET_HOOKS_VISIBILITY,
        u32::from(visible),
    )
}

/// Give or take the allegiance's guest access.
#[must_use]
pub fn build_modify_allegiance_guest_permission(sequence: u32, enabled: bool) -> Vec<u8> {
    u32_action(
        sequence,
        GameActionType::MODIFY_ALLEGIANCE_GUEST_PERMISSION,
        u32::from(enabled),
    )
}

/// Give or take the allegiance's storage access.
#[must_use]
pub fn build_modify_allegiance_storage_permission(sequence: u32, enabled: bool) -> Vec<u8> {
    u32_action(
        sequence,
        GameActionType::MODIFY_ALLEGIANCE_STORAGE_PERMISSION,
        u32::from(enabled),
    )
}

#[cfg(test)]
mod tests {
    //! Cases ported from OpenAC `tests/AcDream.Core.Net.Tests/Messages/`
    //! `HouseEventsTests.cs`, the available-houses cases of
    //! `ClientCommandResponsesTests.cs`, the house cases of
    //! `ClientCommandRequestsTests.cs`, and the restriction layout of
    //! OpenAC `GameEvents.ParseHouseUpdateRestrictions`.

    use ac_bits::Writer;

    use super::*;
    use crate::test_util::{BareBuilder, assert_every_prefix_fails, words};

    fn payment(w: &mut Writer, num: i32, paid: i32, weenie: u32, name: &str, plural: &str) {
        w.i32(num);
        w.i32(paid);
        w.u32(weenie);
        w.string16l(name).unwrap();
        w.string16l(plural).unwrap();
    }

    #[test]
    fn rent_payment_lists() {
        assert_eq!(parse_rent_payment(&words(&[0])), Ok(vec![]));
        let mut w = Writer::new();
        w.u32(1);
        payment(&mut w, 400, 150, 273, "Pyreal", "Pyreals");
        assert_eq!(
            parse_rent_payment(w.as_slice()),
            Ok(vec![HousePayment {
                num: 400,
                paid: 150,
                weenie_id: 273,
                name: "Pyreal".into(),
                plural_name: "Pyreals".into()
            }])
        );
        assert!(parse_rent_payment(&words(&[1000])).is_err());
    }

    #[test]
    fn house_words() {
        assert_eq!(parse_house_word(&words(&[0])), Ok(0));
        assert_eq!(
            parse_house_word(&words(&[1_700_000_000])),
            Ok(1_700_000_000)
        );
        assert!(parse_house_word(&[]).is_err());
    }

    fn house_position(w: &mut Writer, cell: u32, values: [f32; 7]) {
        w.u32(cell);
        for v in values {
            w.f32(v);
        }
    }

    #[test]
    fn house_data_without_a_house() {
        let mut w = Writer::new();
        for word in [0, 0, 0, 0, 0, 0] {
            w.u32(word);
        }
        house_position(&mut w, 0x0012_0001, [10.0, 20.0, 30.0, 1.0, 0.0, 0.0, 0.0]);
        let data = HouseData::parse(w.as_slice()).unwrap();
        assert_eq!(data.buy_time, 0);
        assert!(data.buy.is_empty() && data.rent.is_empty());
        assert_eq!(data.position.cell_id, 0x0012_0001);
        assert_eq!(data.position.origin.x, 10.0);
        assert_eq!(data.position.origin.z, 30.0);
        assert_eq!(data.position.orientation.w, 1.0);
        assert_every_prefix_fails(w.as_slice(), HouseData::parse);
    }

    #[test]
    fn house_data_with_buy_and_rent() {
        let mut w = Writer::new();
        for word in [1_650_000_000, 1_699_000_000, 1, 0] {
            w.u32(word);
        }
        w.u32(1);
        payment(&mut w, 1, 1, 273, "Pyreal", "Pyreals");
        w.u32(2);
        payment(&mut w, 300, 300, 273, "Pyreal", "Pyreals");
        payment(
            &mut w,
            1,
            0,
            1049,
            "Writ of the Chosen",
            "Writs of the Chosen",
        );
        house_position(&mut w, 0x0034_0002, [-15.0, 45.0, 0.0, 0.5, 0.0, 0.0, 0.5]);
        let data = HouseData::parse(w.as_slice()).unwrap();
        assert_eq!(data.buy_time, 1_650_000_000);
        assert_eq!(data.rent_time, 1_699_000_000);
        assert_eq!(data.house_type, 1);
        assert!(!data.maintenance_free);
        assert_eq!(data.buy.len(), 1);
        assert_eq!(data.rent.len(), 2);
        assert_eq!(data.rent[1].name, "Writ of the Chosen");
        assert_eq!(data.position.cell_id, 0x0034_0002);
        assert!(HouseData::parse(&words(&[0, 0])).is_err());
    }

    #[test]
    fn house_restrictions_update() {
        let mut w = Writer::new();
        w.u8(4);
        w.u32(0x7000_0001);
        w.u32(2);
        w.u32(1);
        w.u32(0x5000_0009);
        w.u32(1);
        w.u32(0x5000_0002);
        w.u32(1);
        let update = HouseUpdateRestrictions::parse(w.as_slice()).unwrap();
        assert_eq!(update.sequence, 4);
        assert_eq!(update.sender_id, 0x7000_0001);
        assert!(update.restrictions.open_to_public());
        assert_eq!(update.restrictions.monarch_id, 0x5000_0009);
        assert_eq!(update.restrictions.guests.len(), 1);
        assert_eq!(update.restrictions.guests[0].guest_id, 0x5000_0002);
        assert_every_prefix_fails(w.as_slice(), HouseUpdateRestrictions::parse);
    }

    #[test]
    fn available_houses() {
        let wire = words(&[2, 1, 0x0A0A_0001, 5]);
        assert_eq!(
            AvailableHouses::parse(&wire),
            Ok(AvailableHouses {
                house_type: 2,
                locations: vec![0x0A0A_0001],
                total_available: 5
            })
        );
        assert_every_prefix_fails(&wire, AvailableHouses::parse);
    }

    #[test]
    fn house_requests() {
        for (body, action) in [
            (build_set_open_house_status(13, true), 0x0247),
            (build_set_hooks_visibility(13, true), 0x0266),
            (build_modify_allegiance_guest_permission(13, true), 0x0267),
            (build_modify_allegiance_storage_permission(13, true), 0x0268),
        ] {
            assert_eq!(body, words(&[0xF7B1, 13, action, 1]));
        }
        assert_eq!(
            build_list_available_houses(1, 2),
            words(&[0xF7B1, 1, 0x0270, 2])
        );
        let bare: [(BareBuilder, u32); 6] = [
            (build_abandon_house, 0x021F),
            (build_remove_all_permanent_guests, 0x025E),
            (build_add_all_storage_permission, 0x025C),
            (build_remove_all_storage_permission, 0x024C),
            (build_request_full_guest_list, 0x024D),
            (build_boot_everyone, 0x025F),
        ];
        for (build, action) in bare {
            assert_eq!(build(0x1234), words(&[0xF7B1, 0x1234, action]));
        }
        const BJORN: [u8; 8] = [5, 0, 0x42, 0x6A, 0xF8, 0x72, 0x6E, 0];
        for (body, action) in [
            (build_add_permanent_guest(11, "Bjørn").unwrap(), 0x0245),
            (build_remove_permanent_guest(11, "Bjørn").unwrap(), 0x0246),
            (
                build_boot_specific_house_guest(11, "Bjørn").unwrap(),
                0x024A,
            ),
            (build_add_player_permission(11, "Bjørn").unwrap(), 0x0219),
            (build_remove_player_permission(11, "Bjørn").unwrap(), 0x021A),
        ] {
            assert_eq!(body[..12], words(&[0xF7B1, 11, action]));
            assert_eq!(body[12..], BJORN);
        }
        let storage = build_change_storage_permission(16, "Bjørn", true).unwrap();
        assert_eq!(storage.len(), 24);
        assert_eq!(storage[8..12], 0x0249u32.to_le_bytes());
        assert_eq!(storage[12..20], BJORN);
        assert_eq!(storage[20..], 1u32.to_le_bytes());
    }
}
