//! Routing from the wire to the owners, through the one entry every server
//! message takes (`SessionState::apply`), as OpenAC's
//! `tests/AcDream.Runtime.Tests/Session/LiveSessionEventRouterTests.cs` drives
//! its router. The router's delegate bookkeeping (attach and detach,
//! nested routers, throwing sinks, subscription sets and binding
//! declarations) has no counterpart: here routing is one match per owner
//! and nothing subscribes.

use std::time::Duration;

use ac_proto::fellowship::{FellowMember, FellowshipFullUpdate};
use ac_proto::turbine_chat::{BlobType, DispatchType, Payload, TurbineChat};
use ac_proto::{GameEventType, Inbound, PrivateUpdateAttribute};

use super::{Clocks, Outbox, SessionState};
use crate::communication::CommunicationPaths;
use crate::world::tests::{empty_description, game_event};

const SELF: u32 = 0x5000_000A;
const OTHER: u32 = 0x5000_0002;
const THIRD: u32 = 0x5000_0003;

const CLOCKS: Clocks = Clocks {
    wall_unix_ms: 1_790_000_000_000,
    session: Duration::from_secs(30),
};

fn in_world() -> (SessionState, Outbox) {
    let mut state = SessionState::new(CommunicationPaths::default());
    state.world.set_player(SELF);
    state
        .communication
        .entered_world(SELF, "Router", "Self", CLOCKS.wall_unix_ms);
    (state, Outbox::new())
}

fn apply(state: &mut SessionState, outbox: &mut Outbox, message: &Inbound<'_>) {
    state.apply(message, outbox, true, CLOCKS);
}

fn texts(state: &SessionState) -> Vec<String> {
    state
        .communication
        .transcript_lines()
        .into_iter()
        .map(|l| l.text)
        .collect()
}

#[test]
fn a_server_message_is_routed_by_its_wire_chat_type() {
    // ServerMessage_RoutesThroughAddText_WithWireChatTypeVerbatim_WhenWired.
    let (mut state, mut outbox) = in_world();
    let before = state.communication.transcript_lines().len();
    apply(
        &mut state,
        &mut outbox,
        &Inbound::ServerMessage(ac_proto::chat::ServerMessage {
            message: "You are too encumbered to carry that!".into(),
            chat_type: 0x1A,
        }),
    );
    // 0x1A is the client-local type: the text goes to the interface (the
    // spew box), not the chat, as OpenAC's AddText route does.
    assert_eq!(state.communication.transcript_lines().len(), before);
    assert!(state.drain_host_events().iter().any(|e| matches!(
        e,
        super::HostEvent::InterfaceText(t) if t == "You are too encumbered to carry that!"
    )));

    // ServerMessage_FallsBackToChatDirectly_WhenNoRouterWired: any other
    // type is a chat line of that type.
    apply(
        &mut state,
        &mut outbox,
        &Inbound::ServerMessage(ac_proto::chat::ServerMessage {
            message: "fallback path".into(),
            chat_type: 0x00,
        }),
    );
    let lines = state.communication.transcript_lines();
    assert_eq!(lines.len(), before + 1);
    let line = lines.last().unwrap();
    assert_eq!(line.text, "fallback path");
    assert_eq!(line.log_text_type, 0x00);
}

#[test]
fn repeated_server_messages_publish_distinct_entries() {
    // RepeatedServerMessagePackets_PublishDistinctEntriesThroughAddText
    // (OpenAC v0.1.25, #198): CombatSelf, CombatEnemy, Magic and
    // Default each keep both copies with their own sequence numbers.
    for chat_type in [0x16, 0x15, 0x07, 0x00] {
        let (mut state, mut outbox) = in_world();
        let before = state.communication.transcript_lines();
        for _ in 0..2 {
            apply(
                &mut state,
                &mut outbox,
                &Inbound::ServerMessage(ac_proto::chat::ServerMessage {
                    message: "Additional damage: 5".into(),
                    chat_type,
                }),
            );
        }
        let lines = state.communication.transcript_lines();
        assert_eq!(lines.len(), before.len() + 2, "type {chat_type:#x}");
        let new = &lines[before.len()..];
        assert!(
            new.iter()
                .all(|l| l.text == "Additional damage: 5" && l.log_text_type == chat_type)
        );
        assert_eq!(new[1].sequence, new[0].sequence + 1);
    }
}

#[test]
fn a_death_line_naming_the_local_player_is_not_echoed_but_a_bystanders_is() {
    // PlayerKilled_SuppressesLocalParticipantsAndKeepsBystanderLine.
    let (mut state, mut outbox) = in_world();
    let before = state.communication.transcript_lines().len();
    for (text, victim, killer) in [
        ("self died", SELF, OTHER),
        ("self killed", OTHER, SELF),
        ("bystander", OTHER, THIRD),
    ] {
        apply(
            &mut state,
            &mut outbox,
            &Inbound::PlayerKilled(ac_proto::combat::PlayerKilled {
                death_message: text.into(),
                victim_guid: victim,
                killer_guid: killer,
            }),
        );
    }
    let lines = state.communication.transcript_lines();
    assert_eq!(lines.len(), before + 1);
    let line = lines.last().unwrap();
    assert_eq!(line.text, "bystander");
    assert_eq!(line.sender_guid, OTHER);
}

fn turbine(blob_type: BlobType, dispatch_type: DispatchType, payload: Payload) -> Inbound<'static> {
    Inbound::TurbineChat(Box::new(TurbineChat {
        blob_type,
        dispatch_type,
        target_type: 0,
        target_id: 0,
        transport_type: 0,
        transport_id: 0,
        cookie: 0,
        payload,
    }))
}

fn response(hresult: i32) -> Inbound<'static> {
    turbine(
        BlobType::ResponseBinary,
        DispatchType::Unknown,
        Payload::Response {
            context_id: 5,
            response_id: 2,
            method_id: 2,
            hresult,
        },
    )
}

#[test]
fn a_rejected_turbine_chat_send_is_said_in_the_chat() {
    // TurbineChat_ResponseWithNonZeroHResult_SurfacesAsSystemMessage.
    let (mut state, mut outbox) = in_world();
    let before = texts(&state).len();
    apply(&mut state, &mut outbox, &response(-1));
    let texts = texts(&state);
    assert_eq!(texts.len(), before + 1);
    assert!(texts.last().unwrap().to_lowercase().contains("rejected"));
}

#[test]
fn an_accepted_turbine_chat_send_stays_silent() {
    // TurbineChat_ResponseWithZeroHResult_StaysSilent.
    let (mut state, mut outbox) = in_world();
    let before = texts(&state).len();
    apply(&mut state, &mut outbox, &response(0));
    assert_eq!(texts(&state).len(), before);
}

#[test]
fn a_turbine_chat_room_message_is_a_channel_line() {
    // TurbineChat_EventSendToRoom_StillRoutesToChannelBroadcast.
    let (mut state, mut outbox) = in_world();
    let before = texts(&state).len();
    apply(
        &mut state,
        &mut outbox,
        &turbine(
            BlobType::EventBinary,
            DispatchType::SendToRoomByName,
            Payload::EventSendToRoom {
                room_id: 2,
                sender_name: "Someone".into(),
                message: "hi".into(),
                extra_data_size: 0x0C,
                sender_id: 0x5000_0001,
                hresult: 0,
                chat_type: 2,
            },
        ),
    );
    let texts = texts(&state);
    assert_eq!(texts.len(), before + 1);
    assert!(texts.last().unwrap().contains("hi"));
}

fn fellow(guid: u32, name: &str) -> FellowMember {
    FellowMember {
        guid,
        cp_cache: 0,
        lum_cache: 0,
        level: 1,
        max_health: 100,
        max_stamina: 100,
        max_mana: 100,
        current_health: 100,
        current_stamina: 100,
        current_mana: 100,
        share_loot: 0,
        name: name.into(),
    }
}

#[test]
fn a_fellowship_quit_removes_another_member_or_clears_on_the_players_own() {
    // FellowshipQuit_RoutesSelfGuidToClearAndOtherGuidToRemove.
    let (mut state, mut outbox) = in_world();
    state
        .gameplay
        .fellowship_mut()
        .apply_full_update(&FellowshipFullUpdate {
            members: vec![fellow(SELF, "Self"), fellow(OTHER, "Other")],
            name: "The Fellows".into(),
            leader_guid: SELF,
            share_xp: true,
            even_xp_split: false,
            open_fellow: true,
            locked: false,
            departed: Vec::new(),
        });

    let quit = game_event(GameEventType::FELLOWSHIP_QUIT, &[OTHER]);
    apply(&mut state, &mut outbox, &Inbound::parse(&quit).unwrap());
    let fellowship = state.gameplay.fellowship();
    assert!(fellowship.is_in_fellowship());
    assert_eq!(fellowship.member_count(), 1);
    assert!(fellowship.member(OTHER).is_none());
    assert!(fellowship.member(SELF).is_some());

    let quit = game_event(GameEventType::FELLOWSHIP_QUIT, &[SELF]);
    apply(&mut state, &mut outbox, &Inbound::parse(&quit).unwrap());
    let fellowship = state.gameplay.fellowship();
    assert!(!fellowship.is_in_fellowship());
    assert_eq!(fellowship.member_count(), 0);

    // OpenAC v0.1.25 (`FellowshipParityTests`): each departure
    // reaches the shared chat as a default-class line.
    let lines = state.communication.transcript_lines();
    let said: Vec<(&str, u32)> = lines
        .iter()
        .rev()
        .take(2)
        .rev()
        .map(|l| (l.text.as_str(), l.log_text_type))
        .collect();
    assert_eq!(
        said,
        [
            ("Other has left your Fellowship.", 0),
            (
                "You are no longer a member of the The Fellows Fellowship.",
                0
            )
        ]
    );
}

#[test]
fn an_attribute_update_reaches_the_players_numbers() {
    // AttributeAndSkillUpdates_RouteToPlayerStateAndMovementSeam (the
    // player-state half: Quickness 100 start + 60 ranks).
    let (mut state, mut outbox) = in_world();
    state.world.apply_player_description(&empty_description());
    apply(
        &mut state,
        &mut outbox,
        &Inbound::PrivateUpdateAttribute(PrivateUpdateAttribute {
            sequence: 1,
            attribute: 3,
            ranks: 60,
            start: 100,
            xp: 500,
        }),
    );
    let quickness = state
        .world
        .player()
        .unwrap()
        .numbers()
        .get_attribute(ac_core::player::AttributeKind::Quickness)
        .unwrap();
    assert_eq!(quickness.ranks + quickness.start, 160);
    assert_eq!(quickness.xp, 500);
}

#[test]
fn a_message_before_the_world_is_entered_reaches_no_player_state() {
    // The router publishes nothing to a player that is not there yet
    // (CreatedRouterPublishesNoHandlersUntilExplicitAttach, in its Rust
    // form: a private update with no description changes nothing).
    let mut state = SessionState::new(CommunicationPaths::default());
    let mut outbox = Outbox::new();
    state.apply(
        &Inbound::PrivateUpdateAttribute(PrivateUpdateAttribute {
            sequence: 1,
            attribute: 3,
            ranks: 60,
            start: 100,
            xp: 500,
        }),
        &mut outbox,
        false,
        CLOCKS,
    );
    assert!(state.world.player().is_none());
    assert_eq!(outbox.drain().count(), 0);
}
