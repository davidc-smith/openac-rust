//! Landblocks leaving the window: released over frames, never shown with
//! the window that replaced them, and released whole before they load
//! again.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ac_host::settings::DisplaySettings;

use super::*;
use crate::camera::FlyInput;
use crate::content::ContentSlot;

/// A resident whose landblock the cell dat lacks: it counts as arrived,
/// and it needs no GPU.
fn missing() -> Resident {
    Resident {
        tier: Tier::Near,
        tile: None,
        missing: true,
        near: None,
        near_requested: false,
    }
}

#[test]
fn a_destination_is_not_ready_while_the_last_window_is_being_released() {
    let block = LandblockCoord { x: 0xA9, y: 0xB4 };
    let residents = HashMap::from([(block, missing())]);
    let models = crate::IdMap::default();
    let walk = FrameWalk::new();
    let facts = |retiring| RevealState {
        residents: &residents,
        models: &models,
        walk: &walk,
        server_models_ready: true,
        retiring,
        unavailable: false,
        window: RevealWindow {
            near_radius: 0,
            far_radius: 0,
        },
    };
    let cell = 0xA9B4_0001;
    assert!(facts(false).render_neighborhood_ready(cell, 0, 0));
    assert!(
        !facts(true).render_neighborhood_ready(cell, 0, 0),
        "what the window left behind is released before the destination shows"
    );
}

fn dat_dir() -> PathBuf {
    std::env::var_os("OPENAC_DAT_DIR")
        .map(PathBuf::from)
        .or_else(ac_dat::default_dat_dir)
        .expect("no dat directory: set OPENAC_DAT_DIR")
}

fn renderer(device: &wgpu::Device, queue: &wgpu::Queue) -> WorldRenderer {
    let content = Arc::new(ContentSlot::new(dat_dir()));
    content.get().expect("open the retail dats");
    let settings = WorldSettings {
        content,
        quality_overrides: crate::quality::QualityOverrides {
            near_radius: Some(0),
            far_radius: Some(0),
            msaa_samples: Some(0),
            ..Default::default()
        },
        residency: Default::default(),
        orbit: Default::default(),
        reveal_radius: None,
        sky: Default::default(),
        audio: Default::default(),
    };
    WorldRenderer::new(
        device,
        queue,
        wgpu::TextureFormat::Rgba8Unorm,
        96,
        64,
        &settings,
        &DisplaySettings::default(),
        &[1],
    )
}

/// Prepare frames until `done` holds (at most a minute).
fn frames_until(
    world: &mut WorldRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    what: &str,
    mut done: impl FnMut(&mut WorldRenderer) -> bool,
) {
    let started = Instant::now();
    loop {
        world.prepare(device, queue, None, FlyInput::default());
        if done(world) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "never: {what} ({:?})",
            world.failure()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Holtburg: terrain, statics and buildings with interior cells.
const HOLTBURG: LandblockCoord = LandblockCoord { x: 0xA9, y: 0xB4 };

/// Load Holtburg near and wait until all of it has arrived.
fn load_holtburg(world: &mut WorldRenderer, device: &wgpu::Device, queue: &wgpu::Queue) {
    let _ = world
        .origin
        .try_initialize(i32::from(HOLTBURG.x), i32::from(HOLTBURG.y));
    world.load(HOLTBURG, Tier::Near);
    frames_until(world, device, queue, "Holtburg arrives", |w| {
        w.residents
            .get(&HOLTBURG)
            .is_some_and(|r| r.tile.is_some() && r.near.is_some())
    });
}

fn holds_holtburg(world: &WorldRenderer) -> (bool, bool) {
    let cell = HOLTBURG.terrain_id() | 0x0001;
    (
        world.collision.terrain_origin(cell).is_some(),
        !world.walk.is_empty(),
    )
}

#[test]
#[ignore = "needs the retail dat files"]
fn a_landblock_leaving_the_window_goes_at_once_and_is_released_on_a_later_frame() {
    let Some((device, queue)) = crate::test_gpu::device() else {
        eprintln!("no GPU adapter on this machine");
        return;
    };
    let mut world = renderer(&device, &queue);
    load_holtburg(&mut world, &device, &queue);
    assert_eq!(holds_holtburg(&world), (true, true), "terrain and cells in");

    world.unload(HOLTBURG);
    // Out of the window at once; its collision and cells wait their turn.
    assert!(!world.residents.contains_key(&HOLTBURG));
    assert_eq!(world.retiring.len(), 1);
    assert_eq!(holds_holtburg(&world), (true, true));

    world.prepare(&device, &queue, None, FlyInput::default());
    assert!(
        world.retiring.is_empty(),
        "released within a frame's budget"
    );
    assert_eq!(holds_holtburg(&world), (false, false), "released whole");
}

#[test]
#[ignore = "needs the retail dat files"]
fn a_landblock_wanted_again_before_its_release_is_released_whole_first() {
    let Some((device, queue)) = crate::test_gpu::device() else {
        eprintln!("no GPU adapter on this machine");
        return;
    };
    let mut world = renderer(&device, &queue);
    load_holtburg(&mut world, &device, &queue);

    // A short hop away and back: Holtburg leaves the window and is asked
    // for again before any frame released it.
    world.unload(HOLTBURG);
    world.load(HOLTBURG, Tier::Near);
    assert!(world.retiring.is_empty(), "no old copy is left to release");
    assert_eq!(
        holds_holtburg(&world),
        (false, false),
        "the old copy is gone before the new one arrives"
    );
    let fresh = &world.residents[&HOLTBURG];
    assert!(fresh.tile.is_none() && fresh.near.is_none() && fresh.near_requested);

    // The new copy arrives whole, and later frames release nothing of it.
    load_holtburg(&mut world, &device, &queue);
    assert_eq!(holds_holtburg(&world), (true, true));
    world.prepare(&device, &queue, None, FlyInput::default());
    assert_eq!(holds_holtburg(&world), (true, true));
}
