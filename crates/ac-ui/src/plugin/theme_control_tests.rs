//! Themed windows and controls draw the modern shapes and states.

use super::style::tests::{corners, has_color, same_rgb};
use super::style::{self};
use super::theme_tests::{CLASSIC_TEXTURE, Gallery, SEMIBOLD_TEXTURE, find_named};
use super::title_bar::{self, TitleBar};
use super::*;
use crate::draw::{DrawCmd, DrawList, TextureRef};

const P: Palette = Palette::MOSS;

fn window(attributes: &str, body: &str) -> Gallery {
    Gallery::of(
        &format!(
            r#"<panel x="100" y="100" w="200" h="120" theme="plugin" {attributes}>{body}</panel>"#
        ),
        PluginTheme::Moss,
    )
}

/// The atlases text was drawn from, with each string's lowest edge.
fn text_runs(list: &DrawList) -> Vec<(TextureRef, f32, f32)> {
    list.commands()
        .filter_map(|c| match c {
            DrawCmd::Glyphs { texture, quads, .. } => {
                let top = quads.iter().map(|q| q.y).fold(f32::MAX, f32::min);
                let left = quads.iter().map(|q| q.x).fold(f32::MAX, f32::min);
                Some((*texture, left, top))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_titled_themed_window_draws_a_shadow_a_header_and_a_separator() {
    let mut g = window(r#"title="Buff Bot""#, "");
    let v = corners(&g.draw());
    assert!(v.iter().any(|(x, _, _)| *x < 100.0), "a shadow outside it");
    assert!(has_color(&v, style::mix(P.background, P.text, 0.06)));
    assert!(
        v.iter()
            .any(|(_, y, c)| (*y - 124.0).abs() < 0.01 && same_rgb(*c, P.border))
    );
}

#[test]
fn a_themed_window_has_round_corners() {
    let mut g = window(r#"title="Buff Bot""#, "");
    let v = corners(&g.draw());
    assert!(
        !v.iter()
            .any(|(x, y, c)| *x == 100.0 && *y == 100.0 && same_rgb(*c, P.background)),
        "no square corner"
    );
}

#[test]
fn an_untitled_themed_window_has_no_header() {
    let mut g = window("", "");
    assert!(!has_color(
        &corners(&g.draw()),
        style::mix(P.background, P.text, 0.06)
    ));
}

#[test]
fn a_title_moves_into_the_header_in_the_title_weight_and_back() {
    let mut g = window(r#"title="Buff Bot""#, "");
    // 14-pixel lines centre 5 down a 24-pixel band; each glyph starts 1 lower.
    let themed = text_runs(&g.draw());
    assert_eq!(
        themed,
        [(TextureRef::Surface(SEMIBOLD_TEXTURE), 112.0, 106.0)]
    );
    g.set_theme(PluginTheme::Classic);
    // Classic's title is outlined, so a pass a pixel out comes first.
    let classic = text_runs(&g.draw());
    assert!(classic.contains(&(TextureRef::Surface(CLASSIC_TEXTURE), 108.0, 105.0)));
}

#[test]
fn a_title_bars_title_is_in_the_title_weight_and_its_close_button_a_ghost_x() {
    let mut g = window(r#"title="Buff Bot" titlebar="true""#, "");
    let list = g.draw();
    assert_eq!(text_runs(&list)[0].0, TextureRef::Surface(SEMIBOLD_TEXTURE));
    let ui = &g.host.root;
    let bar = find_named(ui, ui.root(), "PluginTitleBar").unwrap();
    let close = ui.widget::<TitleBar>(bar).unwrap().close;
    let (x, y) = ui.screen_position(close);
    let e = ui.element(close).unwrap();
    let (w, h) = (e.width, e.height);
    let v = corners(&list);
    let glyph = |c: Color| {
        v.iter().any(|(vx, vy, vc)| {
            *vx >= x && *vx <= x + w && *vy >= y && *vy <= y + h && same_rgb(*vc, c)
        })
    };
    assert!(glyph(P.muted), "the X at rest");
    assert!(!glyph(style::hover(&P, P.field)), "no face at rest");
    assert_eq!(e.top, title_bar::THEMED_CLOSE_TOP);

    let (cx, cy) = g.centre("PluginClose");
    g.host.mouse_move(cx, cy);
    let v = corners(&g.draw());
    assert!(has_color(&v, style::hover(&P, P.field)), "a face on hover");
}

#[test]
fn a_resizable_themed_window_has_a_grip_of_dots() {
    let mut g = window(r#"resizable="true""#, "");
    let v = corners(&g.draw());
    assert!(
        v.iter()
            .any(|(x, y, c)| { *x > 280.0 && *y > 210.0 && same_rgb(*c, P.muted) && c.a > 0.9 })
    );
}

#[test]
fn a_window_that_does_not_opt_in_stays_classic_under_a_theme() {
    let markup = r#"<panel x="100" y="100" w="200" h="120" title="Plain"></panel>"#;
    let mut plain = Gallery::of(markup, PluginTheme::Moss);
    let mut classic = Gallery::of(markup, PluginTheme::Classic);
    assert_eq!(plain.draw(), classic.draw());
}
