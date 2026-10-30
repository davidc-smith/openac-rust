//! The shapes plugins lay on the ground, cut into see-through triangles for
//! the world pass: rings, discs and wedges around an object's feet or a
//! fixed position.
//!
//! Bearings are compass bearings: 0 is north (+Y), 90 east (+X), increasing
//! clockwise. A shape follows the land only when its anchor is on the
//! landscape and no more than [`ON_THE_LAND_METRES`] above the land at its
//! centre. Anything else (a floor, a roof, a bridge, a dungeon) gets a flat
//! shape at the anchor's height.
//!
//! The draw is bounded here, not by the plugin: a shape whose centre is more
//! than [`DRAW_RANGE_METRES`] from the camera, or that lies wholly outside
//! its view, is skipped, and the shapes of one frame never exceed
//! [`VERTEX_BUDGET`]. When the shapes in range and in view would, the nearest
//! are drawn and the farthest are left out.

use ac_plugin_ui::GroundShape;
use glam::{Vec2, Vec3};

use crate::frustum::FrustumPlanes;
use crate::world_triangles::{DEPTH_NUDGE, WorldTriangleVertex};

/// Shapes further than this from the camera are not drawn, metres: the same
/// reach as the plugins' world lines.
pub const DRAW_RANGE_METRES: f32 = crate::world_lines::DRAW_RANGE_METRES;

/// The most vertices one frame's shapes take: 2 MiB of them.
pub const VERTEX_BUDGET: usize = 2 * 1024 * 1024 / size_of::<WorldTriangleVertex>();

/// How far the land under a shape that follows it may rise or fall per metre
/// out from its centre, for the box the view is tested against: steeper than
/// any slope the land has, other than a cliff.
pub const LAND_SLOPE_ALLOWANCE: f32 = 2.0;

/// About how long each piece of a shape's edge is, metres.
pub const SEGMENT_METRES: f32 = 0.5;

/// The fewest pieces a full turn is cut into.
pub const MINIMUM_SEGMENTS: usize = 16;

/// The most pieces a full turn is cut into.
pub const MAXIMUM_SEGMENTS: usize = 128;

/// About how deep each ring of a filled shape is, metres, so a large disc
/// still has points on the land every few metres.
pub const FILL_RING_METRES: f32 = 4.0;

/// The most rings a filled shape is cut into.
pub const MAXIMUM_FILL_RINGS: usize = 8;

/// How far above the land, or the anchor's height, a shape lies, metres.
pub const LIFT_METRES: f32 = crate::world_lines::GROUND_LIFT_METRES;

/// How far above the land its anchor may be for a shape to follow it, metres
/// (at or below the land counts).
pub const ON_THE_LAND_METRES: f32 = 1.0;

/// Where a shape's anchor is this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeAnchor {
    /// Its base, drawing frame.
    pub centre: Vec3,
    /// The way it faces, as a compass bearing in degrees.
    pub heading_degrees: f32,
    /// On the landscape, not inside a building or dungeon.
    pub is_outdoor: bool,
}

/// A plugin's ground shape placed in the drawing frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedShape {
    /// Its centre, drawing frame.
    pub centre: Vec3,
    /// The bearing its arc starts at, the object's heading already added.
    pub start_degrees: f32,
    /// Each point lies on the land under it, rather than flat at the
    /// centre's height.
    pub follow_terrain: bool,
    /// The land's height at the centre when it follows the land, else the
    /// centre's height.
    pub ground_z: f32,
    /// The outer radius, metres.
    pub radius: f32,
    /// How wide a band is, metres, inward from the radius.
    pub width: f32,
    /// How far round it goes, clockwise from its start, degrees.
    pub sweep_degrees: f32,
    /// Filled from the centre, rather than a band.
    pub filled: bool,
    /// Red, green, blue, alpha, 0..1.
    pub color: [f32; 4],
}

/// Where `shape` lies around `at`, or `None` when the anchor is not a finite
/// position. `ground` is the land's height at a drawing-frame (x, y), where
/// it is known.
#[must_use]
pub fn place(
    shape: &GroundShape,
    at: ShapeAnchor,
    ground: impl Fn(f32, f32) -> Option<f32>,
) -> Option<PlacedShape> {
    if !at.centre.is_finite() {
        return None;
    }
    let land = ground(at.centre.x, at.centre.y)
        .filter(|z| at.is_outdoor && at.centre.z - z <= ON_THE_LAND_METRES);
    let start_degrees = if shape.faces_object {
        shape.start_degrees + at.heading_degrees
    } else {
        shape.start_degrees
    };
    Some(PlacedShape {
        centre: at.centre,
        start_degrees,
        follow_terrain: land.is_some(),
        ground_z: land.unwrap_or(at.centre.z),
        radius: shape.radius,
        width: shape.width,
        sweep_degrees: shape.sweep_degrees,
        filled: shape.filled,
        color: shape
            .color_rgba
            .to_be_bytes()
            .map(|channel| f32::from(channel) / 255.0),
    })
}

/// How many pieces an edge of `radius` metres is cut into over
/// `sweep_degrees`: about every half metre, between [`MINIMUM_SEGMENTS`] and
/// [`MAXIMUM_SEGMENTS`] for a full turn, an arc taking its share and at least
/// one.
#[must_use]
pub fn segments_for(radius: f32, sweep_degrees: f32) -> usize {
    let full = (2.0 * std::f64::consts::PI * f64::from(radius) / f64::from(SEGMENT_METRES))
        .ceil()
        .clamp(MINIMUM_SEGMENTS as f64, MAXIMUM_SEGMENTS as f64);
    let share = f64::from(sweep_degrees.clamp(0.0, 360.0)) / 360.0;
    (full * share).ceil().max(1.0) as usize
}

/// How many rings a filled shape of `radius` metres is cut into.
#[must_use]
pub fn fill_rings_for(radius: f32) -> usize {
    (radius / FILL_RING_METRES)
        .ceil()
        .clamp(1.0, MAXIMUM_FILL_RINGS as f32) as usize
}

/// How many points [`tessellate`] emits for `shape`.
#[must_use]
pub fn vertex_count(shape: &PlacedShape) -> usize {
    let segments = segments_for(shape.radius, shape.sweep_degrees);
    if shape.filled {
        3 * segments * (2 * fill_rings_for(shape.radius) - 1)
    } else {
        6 * segments
    }
}

/// Whether any of `shape` may be in the view: a box round it, as deep as the
/// land under it could rise or fall, touches the frustum. A shape on the land
/// is drawn at the land's height, which may be far from its anchor's, and at
/// the anchor's where the land is not known, so the box spans both.
#[must_use]
pub fn is_in_view(frustum: &FrustumPlanes, shape: &PlacedShape) -> bool {
    let rise = if shape.follow_terrain {
        shape.radius * LAND_SLOPE_ALLOWANCE + LIFT_METRES
    } else {
        LIFT_METRES
    };
    let (centre, radius) = (shape.centre, shape.radius);
    let min = Vec3::new(
        centre.x - radius,
        centre.y - radius,
        centre.z.min(shape.ground_z) - rise,
    );
    let max = Vec3::new(
        centre.x + radius,
        centre.y + radius,
        centre.z.max(shape.ground_z) + rise,
    );
    frustum.is_aabb_visible(min, max)
}

/// Appends this frame's vertices to `out`: the shapes with their centres
/// within [`DRAW_RANGE_METRES`] of `camera` and some part inside `frustum`
/// (when there is one), nearest first once they do not all fit in
/// [`VERTEX_BUDGET`]. Each point moves [`DEPTH_NUDGE`] of the way toward the
/// camera, so the land it lies on doesn't flicker through it.
pub fn build(
    shapes: &[PlacedShape],
    camera: Vec3,
    frustum: Option<&FrustumPlanes>,
    ground: impl Fn(f32, f32) -> Option<f32>,
    out: &mut Vec<WorldTriangleVertex>,
) {
    let range_squared = DRAW_RANGE_METRES * DRAW_RANGE_METRES;
    let mut planned: Vec<(&PlacedShape, f32)> = shapes
        .iter()
        .filter_map(|shape| {
            let distance_squared = camera.distance_squared(shape.centre);
            let wanted = distance_squared <= range_squared
                && frustum.is_none_or(|frustum| is_in_view(frustum, shape));
            wanted.then_some((shape, distance_squared))
        })
        .collect();
    let total: usize = planned.iter().map(|(s, _)| vertex_count(s)).sum();
    // Only when the shapes do not all fit is the order worth a sort.
    if total > VERTEX_BUDGET {
        planned.sort_by(|a, b| a.1.total_cmp(&b.1));
    }
    let mut remaining = VERTEX_BUDGET;
    let mut points = Vec::new();
    for (shape, _) in planned {
        let count = vertex_count(shape);
        if count > remaining {
            break;
        }
        remaining -= count;
        points.clear();
        tessellate(shape, &ground, &mut points);
        out.extend(points.iter().map(|point| WorldTriangleVertex {
            position: (*point + (camera - *point) * DEPTH_NUDGE).to_array(),
            color: shape.color,
        }));
    }
}

/// One shape's edge points, enough for a full turn at the most pieces.
type Row = [Vec3; MAXIMUM_SEGMENTS + 1];

/// Appends the triangles of `shape` to `out`, three points each. A band runs
/// from the radius less the width to the radius; a filled shape is a fan
/// round the centre and then bands out to the radius. Following the land,
/// each point lies [`LIFT_METRES`] above `ground`'s height under it (or the
/// centre's height where it has none); otherwise every point lies that far
/// above the centre's height.
pub fn tessellate(
    shape: &PlacedShape,
    ground: &impl Fn(f32, f32) -> Option<f32>,
    out: &mut Vec<Vec3>,
) {
    let segments = segments_for(shape.radius, shape.sweep_degrees);
    let mut directions = [Vec2::ZERO; MAXIMUM_SEGMENTS + 1];
    for (i, direction) in directions.iter_mut().enumerate().take(segments) {
        let radians =
            (shape.start_degrees + shape.sweep_degrees * i as f32 / segments as f32).to_radians();
        *direction = Vec2::new(radians.sin(), radians.cos());
    }
    // A full turn ends exactly where it began, so it closes without a seam.
    directions[segments] = if shape.sweep_degrees >= 360.0 {
        directions[0]
    } else {
        let radians = (shape.start_degrees + shape.sweep_degrees).to_radians();
        Vec2::new(radians.sin(), radians.cos())
    };
    let directions = &directions[..=segments];

    let mut inner: Row = [Vec3::ZERO; MAXIMUM_SEGMENTS + 1];
    let mut outer: Row = [Vec3::ZERO; MAXIMUM_SEGMENTS + 1];
    if !shape.filled {
        row(
            shape,
            shape.radius - shape.width,
            directions,
            ground,
            &mut inner,
        );
        row(shape, shape.radius, directions, ground, &mut outer);
        band(&inner[..=segments], &outer[..=segments], out);
        return;
    }
    let rings = fill_rings_for(shape.radius);
    let middle = lift(shape, shape.centre.x, shape.centre.y, ground);
    row(
        shape,
        shape.radius / rings as f32,
        directions,
        ground,
        &mut outer,
    );
    for i in 0..segments {
        out.extend([middle, outer[i], outer[i + 1]]);
    }
    for ring in 2..=rings {
        inner = outer;
        let radius = shape.radius * ring as f32 / rings as f32;
        row(shape, radius, directions, ground, &mut outer);
        band(&inner[..=segments], &outer[..=segments], out);
    }
}

fn row(
    shape: &PlacedShape,
    radius: f32,
    directions: &[Vec2],
    ground: &impl Fn(f32, f32) -> Option<f32>,
    out: &mut Row,
) {
    for (point, direction) in out.iter_mut().zip(directions) {
        let x = shape.centre.x + direction.x * radius;
        let y = shape.centre.y + direction.y * radius;
        *point = lift(shape, x, y, ground);
    }
}

fn lift(shape: &PlacedShape, x: f32, y: f32, ground: &impl Fn(f32, f32) -> Option<f32>) -> Vec3 {
    let land = if shape.follow_terrain {
        ground(x, y)
    } else {
        None
    };
    Vec3::new(x, y, land.unwrap_or(shape.centre.z) + LIFT_METRES)
}

fn band(inner: &[Vec3], outer: &[Vec3], out: &mut Vec<Vec3>) {
    for i in 0..inner.len() - 1 {
        out.extend([
            inner[i],
            outer[i],
            outer[i + 1],
            inner[i],
            outer[i + 1],
            inner[i + 1],
        ]);
    }
}

#[cfg(test)]
mod tests;
