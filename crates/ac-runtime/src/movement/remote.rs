//! Other objects' bodies: an object the server animates gets a motion
//! driver once its motion is accepted, and from then on its accepted
//! motion and positions drive the body, which moves by its animation
//! between the server's positions, collides with the resident world, and
//! catches up to (or is placed at) each accepted position.
//!
//! What the window and `/objects` see as the object's position is its body
//! (the world's live position), not the last position the server sent.

use std::collections::HashMap;

use ac_physics::movement::ObjectToken;
use ac_physics::movement::{
    InterpreterHost, RemoteMotionApplied, RemoteMotionDriver, RemoteMotionUpdate,
    RemotePositionOutcome, RemotePositionRoute, RemotePositionUpdate, StickyTarget, TargetWorld,
    WireMotionItem, WireMotionState,
};
use ac_physics::settle::try_settle;
use ac_physics::{
    CellPosition, ObjectInfoState, PhysicsEngine, PhysicsSetPositionResult, PhysicsState, Quat,
    Vec3,
};
use ac_proto::{Movement, MovementData, Position};

use super::content::WorldContent;
use super::route::{PositionDispositionRoute, TeleportHookPhase};
use super::server_move;
use crate::world::{ObjectKey, World, WorldObject};

/// The width and height a body is settled with when its setup has none,
/// metres (an ordinary person).
const FALLBACK_RADIUS: f32 = 0.48;
const FALLBACK_HEIGHT: f32 = 1.835;
/// Bodies narrower than this settle as an ordinary person, metres.
const MINIMUM_SETTLE_RADIUS: f32 = 0.05;

/// One remote object's body.
#[derive(Debug)]
pub struct RemoteBody {
    /// The object.
    pub guid: u32,
    /// Its presentation key.
    pub key: ObjectKey,
    /// Its motion.
    pub driver: RemoteMotionDriver,
    /// How many server interpreted states its motion took up (see
    /// [`crate::world::MotionView::serial`]).
    pub motions_applied: u32,
}

/// A position in the engine's frame as the wire carries it: the world
/// origin minus the corner of the cell's landblock.
#[must_use]
pub fn wire_from_world(p: &CellPosition, landblock_origin: Vec3) -> Position {
    let o = p.frame.origin - landblock_origin;
    let q = p.frame.orientation;
    Position {
        cell_id: p.obj_cell_id,
        origin: ac_proto::Vector3 {
            x: o.x,
            y: o.y,
            z: o.z,
        },
        orientation: ac_proto::Quaternion {
            w: q.w,
            x: q.x,
            y: q.y,
            z: q.z,
        },
    }
}

/// The orientation a wire position carries.
#[must_use]
pub fn wire_orientation(p: &Position) -> Quat {
    let q = p.orientation;
    Quat::from_xyzw(q.x, q.y, q.z, q.w)
}

/// The wire motion fields of a motion record.
#[must_use]
pub fn wire_motion_state(data: &MovementData) -> WireMotionState {
    let mut state = WireMotionState {
        stance: data.stance(),
        ..WireMotionState::default()
    };
    if let Some(s) = data.interpreted() {
        state.forward_command = s.forward_command;
        state.forward_speed = s.forward_speed;
        state.side_step_command = s.sidestep_command;
        state.side_step_speed = s.sidestep_speed;
        state.turn_command = s.turn_command;
        state.turn_speed = s.turn_speed;
        state.commands = s
            .commands
            .iter()
            .map(|c| WireMotionItem {
                command: c.command,
                packed_sequence: c.packed_sequence,
                speed: c.speed,
            })
            .collect();
    }
    state
}

/// The driver update a motion record asks for: its interpreted state, or
/// the move-to or turn-to it orders (resolved against the other bodies),
/// and the object it sticks to with that object's body (`sticky_body`:
/// `None` when the object is not in the world to follow, which sticks to
/// nothing).
#[must_use]
pub fn remote_motion_update(
    data: &MovementData,
    target_body: impl Fn(u32) -> Option<server_move::BodyShape>,
    sticky_body: impl Fn(u32) -> Option<server_move::BodyShape>,
    frame_offset: impl Fn(u32) -> Option<Vec3>,
    cell_id: u32,
) -> RemoteMotionUpdate {
    let sticky_object = match &data.movement {
        Movement::Interpreted { sticky_object, .. } => {
            sticky_object.filter(|&id| id != 0).and_then(|id| {
                sticky_body(id).map(|(radius, height)| StickyTarget { id, radius, height })
            })
        }
        _ => None,
    };
    RemoteMotionUpdate {
        state: wire_motion_state(data),
        movement_type: data.movement_type,
        move_to: server_move::resolve(data, target_body, frame_offset, cell_id),
        sticky_object,
        standing_long_jump: data.standing_long_jump(),
        fallback_forward_class: 0x4000_0000,
    }
}

impl RemoteBody {
    /// Arm a body for `object` where its entry into the world `placed` it
    /// (a committed placement), else at its accepted position (in the
    /// engine's frame at `origin`, the corner of its landblock); settled
    /// onto the ground when the placement left it touching nothing,
    /// animated when its setup and motion table are known.
    #[must_use]
    pub fn arm(
        object: &WorldObject,
        engine: &PhysicsEngine,
        content: &WorldContent,
        origin: Vec3,
        placed: Option<&PhysicsSetPositionResult>,
    ) -> Option<Self> {
        let accepted = object.accepted_position()?;
        let physics = object.physics();
        let (cell_id, world_pos, orientation) = match placed {
            Some(placed) => (placed.cell_id, placed.position, placed.orientation),
            None => (
                accepted.cell_id,
                Vec3::new(accepted.origin.x, accepted.origin.y, accepted.origin.z) + origin,
                wire_orientation(accepted),
            ),
        };
        let key = object.key();
        let token = ObjectToken {
            id: object.guid(),
            incarnation: u64::from(key.incarnation),
        };
        let mut driver = RemoteMotionDriver::new(token, cell_id, world_pos, orientation);
        let scale = physics.scale.filter(|s| *s > 0.0).unwrap_or(1.0);
        driver.object_scale = scale;
        driver.sphere_scale = scale;
        driver.local_entity_id = key.local_id;
        driver.apply_stale_velocity_cycle = true;
        driver.obj.body.state = PhysicsState::from(physics.state);
        driver.hidden = physics.state & PhysicsState::HIDDEN.bits() != 0;
        let (mut radius, mut height) = (0.0, 0.0);
        if let Some(setup) = physics.setup_id.and_then(|id| content.setup(id)) {
            radius = setup.radius * scale;
            height = setup.height * scale;
            driver.step_up_height = setup.step_up_height * scale;
            driver.step_down_height = setup.step_down_height * scale;
            driver.sphere_list = setup
                .spheres
                .iter()
                .map(|s| {
                    ac_physics::CollisionSphere::new(
                        Vec3::new(s.origin.x, s.origin.y, s.origin.z),
                        s.radius,
                    )
                })
                .collect();
            if let Some(mut sequencer) = physics
                .motion_table_id
                .filter(|id| *id != 0)
                .and_then(|table| content.sequencer(setup.id, table))
            {
                // The table's default state first; the motion the object
                // was created in is taken up on it (see `enter_world`).
                sequencer.initialize_state();
                driver.attach_sequencer(sequencer);
            }
        }
        driver.obj.radius = radius;
        driver.obj.height = height;

        // A standing body gains its ground contact from one short sweep
        // down (retail gains it from its first gravity frame).
        let (settle_radius, settle_height) = if radius < MINIMUM_SETTLE_RADIUS {
            (FALLBACK_RADIUS, FALLBACK_HEIGHT)
        } else {
            (radius, height)
        };
        let mover = if driver.is_player {
            ObjectInfoState(ObjectInfoState::IS_PLAYER | ObjectInfoState::EDGE_SLIDE)
        } else {
            ObjectInfoState(ObjectInfoState::EDGE_SLIDE)
        };
        if let Some(placed) = placed {
            super::commit_placement(&mut driver.obj.body, placed, orientation);
            driver.airborne = !driver.obj.body.on_walkable();
        }
        let mut body = std::mem::take(&mut driver.obj.body);
        if !body.in_contact()
            && try_settle(
                engine,
                &mut body,
                world_pos,
                cell_id,
                settle_radius,
                settle_height,
                mover,
                key.local_id,
                &mut |_| {},
                &mut |_| {},
            )
        {
            driver.airborne = !body.on_walkable();
        }
        driver.obj.body = body;
        driver.arm_leash();
        Some(Self {
            guid: object.guid(),
            key,
            driver,
            motions_applied: 0,
        })
    }

    /// The armed body enters the world: it takes up the motion it was
    /// described in (`described`, when the server gave one), then drops the
    /// links into that motion and abandons what was queued, so it is in
    /// that motion's cycle at once. A corpse described as dead lies dead;
    /// it does not fall again. A creature first seen walking walks.
    pub fn enter_world(&mut self, world: &dyn TargetWorld, described: Option<&RemoteMotionUpdate>) {
        if let Some(update) = described {
            self.apply_motion(world, 0.0, update);
        }
        InterpreterHost::remove_link_animations(&mut self.driver.cx(world));
    }

    /// Apply an accepted motion record, counting it when its interpreted
    /// state was taken up.
    pub fn apply_motion(
        &mut self,
        world: &dyn TargetWorld,
        now: f64,
        update: &RemoteMotionUpdate,
    ) -> RemoteMotionApplied {
        let applied = self.driver.apply_motion(world, now, update);
        if applied.applied_interpreted_state {
            self.motions_applied = self.motions_applied.wrapping_add(1);
        }
        applied
    }

    /// Apply an accepted position (already routed). A teleport, or a far
    /// snap of a body in contact, returns
    /// [`RemotePositionOutcome::PlacementRequired`] with the catch-up
    /// cleared: the caller places the body at the position with the
    /// route's set-position flags (`RemoteMotionDriver::commit_placement`)
    /// and then puts its leash on (`RemoteMotionDriver::arm_leash`).
    #[allow(clippy::too_many_arguments)]
    pub fn apply_position(
        &mut self,
        accepted: &Position,
        origin: Vec3,
        disposition: PositionDispositionRoute,
        teleport_hook: TeleportHookPhase,
        velocity: Option<Vec3>,
        is_grounded: bool,
        will_be_advanced: bool,
        now: f64,
    ) -> RemotePositionOutcome {
        let route = match disposition {
            PositionDispositionRoute::SetPosition => RemotePositionRoute::Teleport,
            PositionDispositionRoute::SetPositionSimple => RemotePositionRoute::FarSnap,
            PositionDispositionRoute::Interpolate => RemotePositionRoute::NearInterpolate,
            PositionDispositionRoute::NoPositionOperation => {
                RemotePositionRoute::AirborneNoOperation
            }
            PositionDispositionRoute::RejectedAuthority
            | PositionDispositionRoute::RejectedData
            | PositionDispositionRoute::AwaitFreshPosition => {
                return RemotePositionOutcome::Nothing;
            }
        };
        let local = Vec3::new(accepted.origin.x, accepted.origin.y, accepted.origin.z);
        let world_position = local + origin;
        let orientation = wire_orientation(accepted);
        // A teleport first cancels what the body was doing: its move-to,
        // its sticky target, its interpolation, its leash, its target.
        if teleport_hook == TeleportHookPhase::BeforePositionOperation {
            self.driver.teleport_hook();
        }
        let outcome = self.driver.apply_server_position(&RemotePositionUpdate {
            world_position,
            orientation,
            cell_id: accepted.cell_id,
            wire_velocity: velocity,
            route,
            will_be_advanced,
            is_grounded,
            now,
        });
        if outcome == RemotePositionOutcome::PlacementRequired {
            self.driver.interpolation = ac_physics::InterpolationManager::new();
        }
        outcome
    }
}

/// Where every simulated body is, for the move-to and target managers.
#[derive(Debug, Default)]
pub struct BodiesView {
    entries: HashMap<u32, (ObjectToken, CellPosition)>,
}

impl BodiesView {
    /// Record a body.
    pub fn insert(&mut self, token: ObjectToken, position: CellPosition) {
        self.entries.insert(token.id, (token, position));
    }
}

impl TargetWorld for BodiesView {
    fn resolve(&self, id: u32) -> Option<ObjectToken> {
        self.entries.get(&id).map(|(t, _)| *t)
    }

    fn position_of(&self, token: ObjectToken) -> Option<CellPosition> {
        self.entries
            .get(&token.id)
            .filter(|(t, _)| *t == token)
            .map(|(_, p)| *p)
    }
}

/// The world's live position of a body (its cell and landblock-relative
/// origin), given the corner of its landblock.
#[must_use]
pub fn live_position(body: &RemoteBody, landblock_origin: Vec3) -> Position {
    wire_from_world(&body.driver.position(), landblock_origin)
}

/// The width and height of an object's body for a move-to's distance.
#[must_use]
pub fn body_shape_of(
    world: &World,
    content: &WorldContent,
    guid: u32,
) -> Option<server_move::BodyShape> {
    let object = world.object(guid)?;
    object.position()?;
    let physics = object.physics();
    let scale = physics.scale.filter(|s| *s > 0.0).unwrap_or(1.0);
    let setup = physics.setup_id.and_then(|id| content.setup(id))?;
    Some((setup.radius * scale, setup.height * scale))
}

/// The body a sticky follow keeps its gap to: an object in the world and
/// not hidden, with its width and height (0 when its shape is unknown);
/// `None` when there is nothing to follow.
#[must_use]
pub fn sticky_body_of(
    world: &World,
    content: &WorldContent,
    guid: u32,
) -> Option<server_move::BodyShape> {
    let object = world.object(guid)?;
    if object.physics().state & PhysicsState::HIDDEN.bits() != 0 {
        return None;
    }
    object.position()?;
    Some(body_shape_of(world, content, guid).unwrap_or((0.0, 0.0)))
}
