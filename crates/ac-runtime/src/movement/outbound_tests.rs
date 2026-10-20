//! Ported from OpenAC `tests/AcDream.Runtime.Tests/Gameplay/LocalPlayerImmediatePositionTests.cs`
//! (the bytes of the forced-position answer) and the outbound cadence of
//! `RuntimeLocalPlayerFrameControllerTests.cs` / `HeadlessLocalPlayerFrameHost`
//! (movement event and jump before the network pass, the autonomous
//! position after it). The position-event gate itself is the controller's
//! and is tested in `ac-physics` (`PlayerOutboundPositionTests`).

use ac_physics::movement::{MovementEnv, MovementInput, NoTargets, PlayerMovementController};
use ac_physics::{PhysicsEngine, Quat, TerrainSurface, Vec3};
use ac_proto::movement_out::MotionTimestamps;

use super::*;
use crate::session::Outbox;

fn u32_at(body: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(body[at..at + 4].try_into().unwrap())
}

fn u16_at(body: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(body[at..at + 2].try_into().unwrap())
}

fn f32_at(body: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(body[at..at + 4].try_into().unwrap())
}

fn flat_engine() -> PhysicsEngine {
    let table: [f32; 256] = std::array::from_fn(|i| i as f32);
    let mut engine = PhysicsEngine::new();
    engine.add_landblock(
        0xA9B4_FFFF,
        TerrainSurface::new(&[50; 81], &table, 0xA9, 0xB4, None),
        Vec::new(),
        Vec::new(),
        0.0,
        0.0,
    );
    engine
}

fn grounded_player(engine: &mut PhysicsEngine) -> PlayerMovementController {
    let mut player = PlayerMovementController::new();
    let mut env = MovementEnv::new(engine, &NoTargets);
    player.seed_placement(
        &mut env,
        Vec3::new(96.0, 97.0, 50.0),
        0xA9B4_0001,
        Vec3::new(96.0, 97.0, 50.0),
    );
    assert!(player.can_send_position_event());
    player
}

const STAMPS: MotionTimestamps = MotionTimestamps {
    instance: 11,
    server_control: 12,
    teleport: 13,
    force_position: 14,
};

#[test]
fn the_forced_position_answer_is_one_exact_autonomous_position_and_is_stamped() {
    let mut engine = flat_engine();
    let mut player = grounded_player(&mut engine);
    let rotation = Quat::from_rotation_z(0.7);
    player.set_body_orientation(rotation);
    let outbound_position = player.try_get_outbound_position().unwrap();
    let mut outbox = Outbox::new();
    let mut sender = LocalPlayerOutbound::new(false);

    sender.send_immediate_position(&mut outbox, &mut player, &STAMPS);

    let bodies: Vec<Vec<u8>> = outbox.drain().map(|o| o.body).collect();
    assert_eq!(bodies.len(), 1);
    let body = &bodies[0];
    assert_eq!(body.len(), 56);
    assert_eq!(u32_at(body, 0), 0xF7B1);
    assert_eq!(u32_at(body, 4), 1);
    assert_eq!(u32_at(body, 8), 0xF753);
    assert_eq!(u32_at(body, 12), outbound_position.obj_cell_id);
    assert_eq!(f32_at(body, 16), outbound_position.frame.origin.x);
    assert_eq!(f32_at(body, 20), outbound_position.frame.origin.y);
    assert_eq!(f32_at(body, 24), outbound_position.frame.origin.z);
    assert!((f32_at(body, 28) - rotation.w).abs() < 1e-6);
    assert!((f32_at(body, 40) - rotation.z).abs() < 1e-6);
    assert_eq!(
        [
            u16_at(body, 44),
            u16_at(body, 46),
            u16_at(body, 48),
            u16_at(body, 50)
        ],
        [11, 12, 13, 14]
    );
    assert_eq!(body[52], 1);
    let current = player.try_get_outbound_position().unwrap();
    assert!(!player.should_send_position_event(
        current,
        player.contact_plane(),
        player.sim_time_seconds()
    ));
}

#[test]
fn the_immediate_answer_needs_a_body_on_the_ground() {
    let mut outbox = Outbox::new();
    let mut sender = LocalPlayerOutbound::new(false);
    let mut unplaced = PlayerMovementController::new();
    sender.send_immediate_position(&mut outbox, &mut unplaced, &STAMPS);
    assert!(outbox.is_empty());
}

#[test]
fn walking_sends_a_movement_event_before_and_a_position_after_the_network_pass() {
    let mut engine = flat_engine();
    let mut player = grounded_player(&mut engine);
    let mut outbox = Outbox::new();
    let mut sender = LocalPlayerOutbound::new(true);
    let forward = MovementInput {
        forward: true,
        run: true,
        ..MovementInput::default()
    };

    let movement = {
        let mut env = MovementEnv::new(&mut engine, &NoTargets);
        player.update(&mut env, 0.05, forward)
    };
    assert!(movement.should_send_movement_event);
    sender.send_pre_network_actions(&mut outbox, &mut player, &movement, &STAMPS, false);
    sender.send_post_network_position(&mut outbox, &mut player, &STAMPS, false);
    let records = sender.take_records();
    let kinds: Vec<OutboundKind> = records.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds,
        [OutboundKind::MoveToState, OutboundKind::AutonomousPosition]
    );
    // Run forward on the wire is walk forward with the run key held.
    assert_eq!(records[0].forward_command, 0x4500_0005);
    assert_eq!(records[0].sequence, 1);
    assert_eq!(records[1].sequence, 2);

    // Held keys do not repeat the movement event; the position follows the
    // gate (a cell or plane change within the second, else a moved frame).
    let movement = {
        let mut env = MovementEnv::new(&mut engine, &NoTargets);
        player.update(&mut env, 0.05, forward)
    };
    assert!(!movement.should_send_movement_event);
    sender.send_pre_network_actions(&mut outbox, &mut player, &movement, &STAMPS, false);
    assert!(sender.take_records().is_empty());
}

#[test]
fn a_hidden_player_sends_nothing() {
    let mut engine = flat_engine();
    let mut player = grounded_player(&mut engine);
    let mut outbox = Outbox::new();
    let mut sender = LocalPlayerOutbound::new(false);
    let movement = {
        let mut env = MovementEnv::new(&mut engine, &NoTargets);
        player.update(
            &mut env,
            0.05,
            MovementInput {
                forward: true,
                ..MovementInput::default()
            },
        )
    };
    sender.send_pre_network_actions(&mut outbox, &mut player, &movement, &STAMPS, true);
    sender.send_post_network_position(&mut outbox, &mut player, &STAMPS, true);
    assert!(outbox.is_empty());
}
