//! Plan-only path previews (OpenAC `NavigationWalkController.Preview`): a
//! route to an object or a place searched over the collision loaded now,
//! without moving the character, owning navigation or touching a walk
//! under way. What the world holds is captured on the calling thread; the
//! grid build and the route search run like a walk's (on a worker, or at
//! once inline). The answer is a detached list of route points from the
//! character toward the goal, each with its cell, measured from the corner
//! of the first landblock.

use ac_physics::{CellSurface, TerrainSurface};
use glam::Vec3;

use super::{
    DEFAULT_CELL_SIZE, Job, LANDBLOCK_SIZE, MAXIMUM_DUNGEON_REGION, NavAvoidance, NavGrid,
    NavRouteOutcome, NavRouteRequest, WalkController, WalkWorld, around_obstacles, flat,
    planning_radius, radius_of, route, try_choose_region, try_choose_square,
};

/// What a preview plans toward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreviewTarget {
    /// An object, ending beside it.
    Object(u32),
    /// A place: a point in the landblock-local frame of `cell_id` (for a
    /// cell of zero, measured from the corner of the first landblock),
    /// ending on the floor it stands on.
    Place {
        /// The cell.
        cell_id: u32,
        /// The point.
        local: Vec3,
    },
}

/// How a preview ended (OpenAC `PluginNavigationPlanStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanStatus {
    /// A path to the destination was found.
    Routed,
    /// No path to the destination could be planned.
    #[default]
    NoRoute,
    /// The character or the navigation world is unavailable.
    Unavailable,
    /// The destination or the arrival distance is invalid.
    InvalidTarget,
    /// The grid build or the route search failed.
    Failed,
}

/// One point of a previewed path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanPoint {
    /// The cell it stands in.
    pub cell_id: u32,
    /// Where, metres from the corner of the first landblock.
    pub global: Vec3,
}

/// A previewed path (OpenAC `PluginNavigationPlan`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NavPlan {
    /// How it ended.
    pub status: PlanStatus,
    /// The route's points from the character toward the goal; empty
    /// unless routed.
    pub path: Vec<PlanPoint>,
    /// Why, in words.
    pub reason: String,
    /// The route's length, metres.
    pub length_meters: f32,
}

impl NavPlan {
    /// A plan with no path.
    #[must_use]
    pub fn failed(status: PlanStatus, reason: &str) -> Self {
        Self {
            status,
            path: Vec::new(),
            reason: reason.to_owned(),
            length_meters: 0.0,
        }
    }
}

/// A preview under way.
pub struct PathPreview(Job<NavPlan>);

impl std::fmt::Debug for PathPreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathPreview")
            .field("finished", &self.is_finished())
            .finish()
    }
}

impl PathPreview {
    /// An answer known at once.
    #[must_use]
    pub fn ready(plan: NavPlan) -> Self {
        Self(Job::Ready(plan))
    }

    /// The search has ended.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    /// The plan (waiting for the search when it is still running); a
    /// search that broke down is `Failed`.
    #[must_use]
    pub fn take(self) -> NavPlan {
        self.0.take().unwrap_or_else(|error| {
            NavPlan::failed(PlanStatus::Failed, &format!("the search failed: {error}"))
        })
    }
}

impl WalkController {
    /// Plans a path to `target` without walking it, ending within
    /// `arrival_meters` (OpenAC `NavigationWalkController.PreviewPathAsync`).
    pub fn preview_path(
        &mut self,
        cx: &WalkWorld<'_>,
        target: PreviewTarget,
        arrival_meters: f32,
    ) -> PathPreview {
        match target {
            PreviewTarget::Object(object_id) => {
                let Some(goal) = cx.goals.try_locate(object_id) else {
                    return PathPreview::ready(NavPlan::failed(
                        PlanStatus::NoRoute,
                        "the client has no position for the object",
                    ));
                };
                let radius = cx
                    .goals
                    .try_get_surfaces(object_id)
                    .map_or(0.0, |s| radius_of(&s, goal));
                self.preview_to(cx, goal, object_id, arrival_meters, false, radius)
            }
            PreviewTarget::Place { cell_id, local } => {
                let Some(goal) = cx.goals.try_locate_place(cell_id, local) else {
                    return PathPreview::ready(NavPlan::failed(
                        PlanStatus::NoRoute,
                        "the client cannot place the destination",
                    ));
                };
                self.preview_to(cx, goal, 0, arrival_meters, true, 0.0)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn preview_to(
        &mut self,
        cx: &WalkWorld<'_>,
        goal: Vec3,
        object_id: u32,
        arrival_meters: f32,
        on_goal_floor: bool,
        goal_radius: f32,
    ) -> PathPreview {
        let standing = cx
            .body
            .try_sample()
            .filter(|s| !s.in_portal_space && !s.airborne)
            .and_then(|s| cx.goals.try_global_of(s.position).map(|g| (s, g)));
        let Some((sample, global_start)) = standing else {
            return PathPreview::ready(NavPlan::failed(
                PlanStatus::Unavailable,
                "the character is not standing in the world",
            ));
        };
        let dungeon_cells = self.try_measure_dungeon(cx, sample.cell_id);
        let region = match dungeon_cells {
            Some((minimum, maximum)) => {
                let near = flat(sample.position).min(flat(goal));
                let far = flat(sample.position).max(flat(goal));
                try_choose_square(minimum.min(near), maximum.max(far), MAXIMUM_DUNGEON_REGION)
            }
            None => try_choose_region(sample.position, goal),
        };
        let Some((origin_x, origin_y, size)) = region else {
            return PathPreview::ready(NavPlan::failed(
                PlanStatus::NoRoute,
                "the destination is outside one planning region",
            ));
        };
        let stands_on = |id: u32| cx.goals.stands_still(id);
        let dungeon = (sample.cell_id & 0xFFFF_0000) | 0xFFFF;
        let captured = if dungeon_cells.is_some() {
            super::geometry::capture_dungeon(
                cx.physics,
                dungeon,
                origin_x,
                origin_y,
                size,
                Some(&stands_on),
            )
        } else {
            super::geometry::capture(cx.physics, origin_x, origin_y, size, Some(&stands_on))
        };
        let Some(geometry) = captured else {
            return PathPreview::ready(NavPlan::failed(
                PlanStatus::NoRoute,
                "no collision is loaded around the character",
            ));
        };
        // Cell surfaces do not change once loaded: they go with the
        // geometry so the worker can name each point's floor.
        let cells: Vec<CellSurface> = geometry
            .landblock_ids
            .iter()
            .filter_map(|&id| cx.physics.landblock(id))
            .flat_map(|landblock| landblock.cells.iter().cloned())
            .collect();
        let crowd = self.crowd(cx, &sample, object_id);
        let body = sample.body;
        let start = sample.position;
        let leaps = sample.leaps;
        let half = size * 0.5;
        let centre = Vec3::new(origin_x + half, origin_y + half, 0.0);
        let reach = half * std::f32::consts::SQRT_2;
        let widen = |spot: NavAvoidance| NavAvoidance::new(spot.centre, spot.radius + body.radius);
        let portals: Vec<NavAvoidance> = cx
            .goals
            .find_portals(centre, reach, object_id, goal)
            .into_iter()
            .map(widen)
            .collect();
        let obstacles: Vec<NavAvoidance> = cx
            .goals
            .find_obstacles(centre, reach, object_id, Some(&geometry.object_ids))
            .into_iter()
            .map(widen)
            .collect();
        let sealed = dungeon_cells.is_some();
        let start_cell = sample.cell_id;
        PathPreview(Job::start(self.mode, move || {
            let grid = match NavGrid::build(&geometry, body, DEFAULT_CELL_SIZE) {
                Ok(grid) => grid,
                Err(error) => {
                    return NavPlan::failed(
                        PlanStatus::Failed,
                        &format!("the search failed: {error:?}"),
                    );
                }
            };
            let found = around_obstacles(goal, &portals, &obstacles, |spots| {
                route::find(
                    &grid,
                    start,
                    goal,
                    planning_radius(arrival_meters),
                    &NavRouteRequest {
                        avoid: spots,
                        leaps,
                        crowd: &crowd,
                        arrive_on_goal_floor: on_goal_floor,
                        goal_radius,
                    },
                )
            });
            if found.outcome != NavRouteOutcome::Routed || !found.ends_in_sight {
                return NavPlan::failed(PlanStatus::NoRoute, &found.reason);
            }
            NavPlan {
                status: PlanStatus::Routed,
                path: found
                    .legs
                    .iter()
                    .map(|&point| {
                        plan_point(point, start, global_start, start_cell, &cells, sealed)
                    })
                    .collect(),
                reason: found.reason,
                length_meters: found.length,
            }
        }))
    }
}

/// A route point as a preview reports it: measured from the corner of the
/// first landblock, in the cell whose floor lies nearest its height, else
/// the start's cell in a sealed dungeon, else the outdoor cell under it
/// (OpenAC `PreviewPosition`).
fn plan_point(
    point: Vec3,
    start: Vec3,
    global_start: Vec3,
    start_cell: u32,
    cells: &[CellSurface],
    sealed_dungeon: bool,
) -> PlanPoint {
    let global = global_start + point - start;
    let mut cell_id = 0;
    let mut nearest = f32::INFINITY;
    for cell in cells {
        let Some(z) = cell.sample_floor_z(point.x, point.y) else {
            continue;
        };
        if (z - point.z).abs() >= nearest {
            continue;
        }
        nearest = (z - point.z).abs();
        cell_id = cell.cell_id();
    }
    if cell_id == 0 && sealed_dungeon {
        cell_id = start_cell;
    }
    if cell_id == 0 {
        let bx = (global.x / LANDBLOCK_SIZE).floor().clamp(0.0, 255.0);
        let by = (global.y / LANDBLOCK_SIZE).floor().clamp(0.0, 255.0);
        let landblock = ((bx as u32) << 24) | ((by as u32) << 16);
        cell_id = TerrainSurface::compute_outdoor_cell_id_in(
            landblock,
            global.x - bx * LANDBLOCK_SIZE,
            global.y - by * LANDBLOCK_SIZE,
        );
    }
    PlanPoint { cell_id, global }
}
