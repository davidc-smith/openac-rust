use ac_plugin_ui::{GroundShapeKind, MarkerAnchor};

use super::*;

const TOLERANCE: f32 = 0.001;

fn no_land(_: f32, _: f32) -> Option<f32> {
    None
}

fn slope(x: f32, _: f32) -> Option<f32> {
    Some(x * 0.5)
}

fn shape(kind: GroundShapeKind, radius: f32, width: f32, sweep: f32, filled: bool) -> GroundShape {
    GroundShape {
        anchor: MarkerAnchor::Object(1),
        kind,
        radius,
        width,
        start_degrees: 0.0,
        sweep_degrees: sweep,
        filled,
        faces_object: false,
        color_rgba: 0xFF00_0080,
    }
}

fn ring(radius: f32, width: f32) -> GroundShape {
    shape(GroundShapeKind::Ring, radius, width, 360.0, false)
}

fn disc(radius: f32) -> GroundShape {
    shape(GroundShapeKind::Disc, radius, 0.0, 360.0, true)
}

fn arc(radius: f32, start: f32, sweep: f32, filled: bool, width: f32) -> GroundShape {
    GroundShape {
        start_degrees: start,
        ..shape(GroundShapeKind::Arc, radius, width, sweep, filled)
    }
}

fn outdoors(centre: Vec3) -> ShapeAnchor {
    ShapeAnchor {
        centre,
        heading_degrees: 0.0,
        is_outdoor: true,
    }
}

/// The shape's points, at `centre`, flat at its height.
fn cut(shape: &GroundShape, centre: Vec3) -> Vec<Vec3> {
    cut_over(shape, outdoors(centre), no_land)
}

fn cut_over(
    shape: &GroundShape,
    at: ShapeAnchor,
    ground: impl Fn(f32, f32) -> Option<f32>,
) -> Vec<Vec3> {
    let placed = place(shape, at, &ground).unwrap();
    let mut out = Vec::new();
    tessellate(&placed, &ground, &mut out);
    assert_eq!(out.len(), vertex_count(&placed));
    out
}

fn across(point: Vec3, centre: Vec3) -> f32 {
    point.truncate().distance(centre.truncate())
}

#[test]
fn edges_are_cut_into_half_metre_segments_within_bounds() {
    assert_eq!(segments_for(0.5, 360.0), 16); // a small ring still gets the minimum
    assert_eq!(segments_for(5.0, 360.0), 63); // 2π·5 m / 0.5 m = 62.8
    assert_eq!(segments_for(20.0, 360.0), 128); // a large ring is capped
    assert_eq!(segments_for(5.0, 90.0), 16); // a quarter of 63, rounded up
    assert_eq!(segments_for(0.5, 10.0), 1); // a sliver is at least one segment
}

#[test]
fn filled_shapes_are_cut_into_rings_about_four_metres_deep() {
    assert_eq!(fill_rings_for(1.0), 1);
    assert_eq!(fill_rings_for(4.0), 1);
    assert_eq!(fill_rings_for(6.0), 2);
    assert_eq!(fill_rings_for(100.0), 8);
}

#[test]
fn a_ring_is_a_band_from_its_inner_to_its_outer_radius() {
    let centre = Vec3::new(10.0, 20.0, 3.0);
    let points = cut(&ring(2.0, 0.5), centre);
    assert!(points.iter().all(|p| {
        let r = across(*p, centre);
        (1.5 - TOLERANCE..=2.0 + TOLERANCE).contains(&r)
    }));
    assert!(
        points
            .iter()
            .any(|p| (across(*p, centre) - 1.5).abs() < TOLERANCE)
    );
    assert!(
        points
            .iter()
            .any(|p| (across(*p, centre) - 2.0).abs() < TOLERANCE)
    );
}

#[test]
fn a_disc_is_filled_from_its_centre_through_its_rings() {
    let centre = Vec3::new(10.0, 20.0, 3.0);
    let points = cut(&disc(6.0), centre);
    assert!(points.iter().any(|p| across(*p, centre) < TOLERANCE));
    // Two rings: the boundary between them is half way out.
    assert!(
        points
            .iter()
            .any(|p| (across(*p, centre) - 3.0).abs() < TOLERANCE)
    );
    assert!(
        points
            .iter()
            .any(|p| (across(*p, centre) - 6.0).abs() < TOLERANCE)
    );
    assert!(points.iter().all(|p| across(*p, centre) <= 6.0 + TOLERANCE));
}

#[test]
fn an_arc_sweeps_clockwise_from_its_start_bearing() {
    // From east (90°) a quarter turn clockwise ends at south.
    let points = cut(&arc(5.0, 90.0, 90.0, true, 0.0), Vec3::ZERO);
    assert!(points.iter().all(|p| p.x >= -TOLERANCE && p.y <= TOLERANCE));
    assert!(
        points
            .iter()
            .any(|p| (p.x - 5.0).abs() < TOLERANCE && p.y.abs() < TOLERANCE)
    );
    assert!(
        points
            .iter()
            .any(|p| p.x.abs() < TOLERANCE && (p.y + 5.0).abs() < TOLERANCE)
    );
}

#[test]
fn a_filled_full_turn_arc_closes_without_a_seam() {
    let points = cut(&arc(4.0, 37.0, 360.0, true, 0.0), Vec3::ZERO);
    let segments = segments_for(4.0, 360.0);
    // One ring: a fan of (centre, rim i, rim i + 1). The last rim point is
    // the first one exactly, not one turn of rounding away from it.
    assert_eq!(points.len(), 3 * segments);
    assert_eq!(points[1], points[points.len() - 1]);
    assert!(
        points
            .iter()
            .all(|p| across(*p, Vec3::ZERO) <= 4.0 + TOLERANCE)
    );
    assert!(points.iter().any(|p| p.x > 3.9));
    assert!(points.iter().any(|p| p.x < -3.9));
    assert!(points.iter().any(|p| p.y > 3.9));
    assert!(points.iter().any(|p| p.y < -3.9));
}

#[test]
fn a_filled_sliver_is_one_triangle_from_the_centre_across_its_sweep() {
    let points = cut(&arc(0.5, 0.0, 10.0, true, 0.0), Vec3::ZERO);
    assert_eq!(points.len(), 3);
    assert!(across(points[0], Vec3::ZERO) < TOLERANCE);
    // North, then 10° clockwise of north.
    assert!(points[1].x.abs() < TOLERANCE && (points[1].y - 0.5).abs() < TOLERANCE);
    let end = 10f32.to_radians();
    assert!((points[2].x - 0.5 * end.sin()).abs() < TOLERANCE);
    assert!((points[2].y - 0.5 * end.cos()).abs() < TOLERANCE);
}

#[test]
fn an_unfilled_sliver_is_one_quad_across_its_band() {
    let points = cut(&arc(0.5, 0.0, 10.0, false, 0.1), Vec3::ZERO);
    assert_eq!(points.len(), 6);
    assert!(points.iter().all(|p| {
        let r = across(*p, Vec3::ZERO);
        (0.4 - TOLERANCE..=0.5 + TOLERANCE).contains(&r) && p.x >= -TOLERANCE
    }));
}

#[test]
fn an_unfilled_arc_is_a_band_over_its_sweep_only() {
    // North round through east to south: the eastern half.
    let points = cut(&arc(5.0, 0.0, 180.0, false, 1.0), Vec3::ZERO);
    assert!(points.iter().all(|p| {
        let r = across(*p, Vec3::ZERO);
        (4.0 - TOLERANCE..=5.0 + TOLERANCE).contains(&r) && p.x >= -TOLERANCE
    }));
}

#[test]
fn on_the_land_every_point_sits_just_above_the_ground_under_it() {
    let points = cut_over(&disc(6.0), outdoors(Vec3::ZERO), slope);
    assert!(
        points
            .iter()
            .all(|p| (p.z - (p.x * 0.5 + LIFT_METRES)).abs() < TOLERANCE)
    );
}

#[test]
fn where_the_land_is_unknown_a_point_sits_at_the_centres_height() {
    // On the land at the centre, and unknown everywhere else.
    let ground = |x: f32, y: f32| (x == 0.0 && y == 0.0).then_some(7.0);
    let points = cut_over(&ring(2.0, 0.2), outdoors(Vec3::new(0.0, 0.0, 7.0)), ground);
    assert!(
        points
            .iter()
            .all(|p| (p.z - (7.0 + LIFT_METRES)).abs() < TOLERANCE)
    );
}

#[test]
fn off_the_land_the_shape_is_flat_at_the_anchors_height() {
    let points = cut_over(&disc(6.0), outdoors(Vec3::new(0.0, 0.0, 100.0)), slope);
    assert!(
        points
            .iter()
            .all(|p| (p.z - (100.0 + LIFT_METRES)).abs() < TOLERANCE)
    );
}

/// Whether a shape at this height, over flat land at 0, follows the land.
fn follows(z: f32, is_outdoor: bool) -> bool {
    let at = ShapeAnchor {
        centre: Vec3::new(0.0, 0.0, z),
        heading_degrees: 0.0,
        is_outdoor,
    };
    place(&disc(2.0), at, |_, _| Some(0.0))
        .unwrap()
        .follow_terrain
}

#[test]
fn an_object_follows_the_land_only_when_it_stands_on_it() {
    assert!(follows(0.3, true)); // standing on the land
    assert!(follows(-0.9, true)); // a little sunk into it
    assert!(!follows(6.0, true)); // up on a roof or a bridge
}

#[test]
fn an_object_inside_a_building_is_flat_even_on_the_land() {
    assert!(!follows(0.0, false));
}

#[test]
fn the_one_metre_boundary_is_inclusive() {
    assert!(follows(1.0, true));
    assert!(!follows(1.01, true));
}

#[test]
fn an_anchor_with_no_land_under_it_is_flat() {
    let placed = place(&disc(2.0), outdoors(Vec3::new(0.0, 0.0, 4.0)), no_land).unwrap();
    assert!(!placed.follow_terrain);
    assert_eq!(placed.ground_z, 4.0);
}

#[test]
fn a_spot_far_below_the_land_outdoors_follows_it() {
    assert!(follows(-20.0, true));
}

#[test]
fn a_wedge_that_faces_its_object_turns_with_it_and_others_do_not() {
    // A thin wedge pointing north, its object facing east: it points east.
    let wedge = arc(5.0, -5.0, 10.0, true, 0.0);
    let facing = GroundShape {
        faces_object: true,
        ..wedge
    };
    let east = ShapeAnchor {
        heading_degrees: 90.0,
        ..outdoors(Vec3::ZERO)
    };
    let points = cut_over(&facing, east, no_land);
    assert!(points.iter().all(|p| p.x >= -TOLERANCE));
    assert!(points.iter().any(|p| p.x > 4.9));
    let points = cut_over(&wedge, east, no_land);
    assert!(points.iter().any(|p| p.y > 4.9));
}

#[test]
fn an_anchor_that_is_not_a_finite_position_places_nothing() {
    let at = outdoors(Vec3::new(f32::NAN, 0.0, 0.0));
    assert!(place(&disc(2.0), at, no_land).is_none());
}

#[test]
fn the_shapes_colour_and_opacity_are_kept() {
    let placed = place(&disc(2.0), outdoors(Vec3::ZERO), no_land).unwrap();
    assert_eq!(placed.color, [1.0, 0.0, 0.0, 128.0 / 255.0]);
}
