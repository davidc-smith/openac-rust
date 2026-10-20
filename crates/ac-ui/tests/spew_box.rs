//! The status box (spew box).
//! Ported from OpenAC `tests/AcDream.App.Tests/UI/SpewBoxControllerTests.cs`
//! and `tests/AcDream.UI.Abstractions.Tests/Panels/SpewBox/SpewBoxVMTests.cs`.
//! OpenAC's `isGameplayActive` closure is the flag the owner sets; its
//! `LinesProvider` is the text's content lines; `Dispose` is `dispose`.

use std::sync::Arc;

use ac_core::chat::{DEFAULT_LIFETIME_SECONDS, MAX_CONCURRENT_ITEMS, SpewBoxState};
use ac_dat::objects::FontCharDesc;
use ac_ui::draw::{DEFAULT_OUTLINE_COLOR, TextureRef};
use ac_ui::font::{Atlas, UiFont};
use ac_ui::panels::spew_box::{self, RETAIL_FONT_ID, SpewBoxController};
use ac_ui::root::UiRoot;
use ac_ui::widgets::Text;

fn root(w: f32, h: f32) -> UiRoot {
    let mut ui = UiRoot::new();
    ui.set_size(w, h);
    ui
}

fn tick(ui: &mut UiRoot, now_ms: i64) {
    ui.tick(0.0, now_ms);
}

fn texts(c: &SpewBoxController, ui: &UiRoot) -> Vec<String> {
    c.shown(ui).into_iter().map(|l| l.text).collect()
}

fn visible(c: &SpewBoxController, ui: &UiRoot) -> bool {
    ui.element(c.text()).is_some_and(|e| e.visible())
}

fn count(c: &SpewBoxController, ui: &UiRoot) -> usize {
    c.state(ui).map_or(0, SpewBoxState::len)
}

fn font(line_height: f32, background: bool) -> Arc<UiFont> {
    let atlas = |s| Atlas {
        texture: TextureRef::Surface(s),
        width: 64,
        height: 64,
    };
    Arc::new(UiFont::new(
        RETAIL_FONT_ID,
        atlas(1),
        background.then(|| atlas(2)),
        line_height,
        line_height - 4.0,
        std::iter::empty::<FontCharDesc>(),
        0,
        0,
    ))
}

#[test]
fn returning_to_character_selection_hides_gameplay_notices_before_the_viewport_reflows() {
    let mut ui = root(1760.0, 990.0);
    let c = SpewBoxController::mount(&mut ui, None);
    c.enqueue(&mut ui, "In Portal Space - Please Wait...");
    tick(&mut ui, 1000);
    assert!(visible(&c, &ui));

    c.set_gameplay_active(&mut ui, false);
    tick(&mut ui, 1016);
    assert!(!visible(&c, &ui));
    assert!(c.shown(&ui).is_empty());
    assert_eq!(count(&c, &ui), 0);
    ui.set_size(800.0, 600.0);
    c.enqueue(&mut ui, "A late message from the previous world.");
    tick(&mut ui, 1017);
    assert!(!visible(&c, &ui));
    assert_eq!(count(&c, &ui), 0);

    c.set_gameplay_active(&mut ui, true);
    tick(&mut ui, 1018);
    assert!(!visible(&c, &ui));
    c.enqueue(&mut ui, "Welcome back.");
    tick(&mut ui, 1019);
    assert!(visible(&c, &ui));
    assert_eq!(texts(&c, &ui), ["Welcome back."]);
}

#[test]
fn the_box_is_a_click_through_overlay_and_disposes_from_the_root() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    let e = ui.element(c.text()).unwrap();
    assert!(!e.visible());
    assert!(e.click_through);
    assert_eq!(e.z_order(), i32::MAX);
    assert_eq!(ui.pick(640, 10), None, "nothing of the box is hit");

    c.enqueue(&mut ui, "You can't jump while in the air");
    // The frame's time pulse drains the queue; no draw is needed.
    tick(&mut ui, 1000);
    assert_eq!(texts(&c, &ui), ["You can't jump while in the air"]);
    assert!(visible(&c, &ui));

    c.dispose(&mut ui);
    assert!(ui.children(ui.root()).is_empty());
}

#[test]
fn no_content_leaves_the_text_hidden_and_empty() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    tick(&mut ui, 1000);
    assert!(c.shown(&ui).is_empty());
    assert!(!visible(&c, &ui));
}

#[test]
fn it_shows_without_any_draw_having_happened_first() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    assert!(!visible(&c, &ui));
    c.enqueue(&mut ui, "Out of Range!");
    tick(&mut ui, 500);
    assert!(visible(&c, &ui));
    assert_eq!(texts(&c, &ui), ["Out of Range!"]);
}

#[test]
fn the_queue_drains_without_a_draw_pass() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    c.enqueue(&mut ui, "first");
    assert_eq!(count(&c, &ui), 0, "still queued");
    tick(&mut ui, 100);
    assert_eq!(count(&c, &ui), 1);
    assert_eq!(c.state(&ui).unwrap().visible()[0].text, "first");
}

#[test]
fn the_queue_stays_bounded_across_many_ticks() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    for i in 0..50 {
        c.enqueue(&mut ui, &format!("line {i}"));
        tick(&mut ui, 1000 + i);
        assert!(count(&c, &ui) <= MAX_CONCURRENT_ITEMS);
    }
    let shown = texts(&c, &ui);
    assert!(shown.len() <= MAX_CONCURRENT_ITEMS);
    assert_eq!(shown[0], "line 49");
}

#[test]
fn the_newer_message_is_the_top_line() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    c.enqueue(&mut ui, "older message");
    tick(&mut ui, 1000);
    c.enqueue(&mut ui, "newer message");
    tick(&mut ui, 1001);
    assert_eq!(texts(&c, &ui), ["newer message", "older message"]);
}

#[test]
fn dispose_removes_both_the_text_and_the_time_sink() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    assert_eq!(
        ui.children(ui.root()).len(),
        2,
        "the text and the time sink"
    );
    c.dispose(&mut ui);
    assert!(ui.children(ui.root()).is_empty());
}

#[test]
fn it_mounts_flush_with_the_top() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    assert_eq!(ui.element(c.text()).unwrap().top, 0.0);
}

#[test]
fn a_tick_keeps_the_top_and_recentres_across_a_resize() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    c.enqueue(&mut ui, "resize me");
    tick(&mut ui, 1000);
    let e = ui.element(c.text()).unwrap();
    assert_eq!(e.left, (1280.0 - 450.0) / 2.0);
    assert_eq!(e.top, 0.0);

    ui.set_size(1920.0, 720.0);
    tick(&mut ui, 1016);
    let e = ui.element(c.text()).unwrap();
    assert_eq!(e.left, (1920.0 - 450.0) / 2.0);
    assert_eq!(e.top, 0.0);
}

#[test]
fn the_resolved_retail_font_is_the_texts_font() {
    let mut ui = root(1280.0, 720.0);
    let f = font(11.0, false);
    let c = SpewBoxController::mount(&mut ui, Some(f.clone()));
    let t = ui.widget::<Text>(c.text()).unwrap();
    assert!(Arc::ptr_eq(t.font.as_ref().unwrap(), &f));
    assert_eq!(t.font.as_ref().unwrap().line_height, 11.0);
}

#[test]
fn without_a_font_the_text_has_none() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    assert!(ui.widget::<Text>(c.text()).unwrap().font.is_none());
}

#[test]
fn the_retail_font_is_the_authored_eighteen_pixel_face() {
    assert_eq!(RETAIL_FONT_ID, 0x4000_0001);
}

#[test]
fn the_text_is_outlined_as_the_authored_line_template() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, None);
    let t = ui.widget::<Text>(c.text()).unwrap();
    assert!(t.outline);
    assert_eq!(t.outline_color, DEFAULT_OUTLINE_COLOR);
}

#[test]
fn with_the_authored_font_the_line_height_is_the_eighteen_pixel_box() {
    let mut ui = root(1280.0, 720.0);
    let c = SpewBoxController::mount(&mut ui, Some(font(18.0, true)));
    let t = ui.widget::<Text>(c.text()).unwrap();
    assert_eq!(t.font.as_ref().unwrap().line_height, 18.0);
}

// ── SpewBoxVMTests ──────────────────────────────────────────────────────

#[test]
fn no_content_gives_no_lines() {
    let mut state = SpewBoxState::new();
    assert!(spew_box::lines(&mut state, 0.0).is_empty());
    assert!(state.is_empty());
}

#[test]
fn lines_drain_the_queue_with_their_remaining_lifetime() {
    let mut state = SpewBoxState::new();
    state.enqueue("You can't jump while in the air");
    let lines = spew_box::lines(&mut state, 10.0);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "You can't jump while in the air");
    assert!((lines[0].remaining_lifetime_seconds - DEFAULT_LIFETIME_SECONDS).abs() < 1e-3);
    assert!(!state.is_empty());
}

#[test]
fn the_remaining_lifetime_counts_down_and_floors_at_zero() {
    let mut state = SpewBoxState::new();
    state.enqueue("fading");
    spew_box::lines(&mut state, 0.0);
    let halfway = DEFAULT_LIFETIME_SECONDS / 2.0;
    let mid = spew_box::lines(&mut state, halfway);
    assert_eq!(mid.len(), 1);
    assert!((mid[0].remaining_lifetime_seconds - halfway).abs() < 1e-3);
    assert!(spew_box::lines(&mut state, DEFAULT_LIFETIME_SECONDS + 1.0).is_empty());
}

#[test]
fn lines_come_newest_first() {
    let mut state = SpewBoxState::new();
    state.enqueue("older");
    spew_box::lines(&mut state, 0.0);
    state.enqueue("newer");
    let lines: Vec<String> = spew_box::lines(&mut state, 0.0)
        .into_iter()
        .map(|l| l.text)
        .collect();
    assert_eq!(lines, ["newer", "older"]);
}

#[test]
fn the_revision_follows_the_state() {
    let mut state = SpewBoxState::new();
    let initial = state.revision();
    state.enqueue("a line");
    spew_box::lines(&mut state, 0.0);
    assert!(state.revision() > initial);
}
