//! Themed windows and controls draw the modern shapes and states.

use super::style::tests::{corners, has_color, same_rgb};
use super::style::{self};
use super::theme_tests::{CLASSIC_TEXTURE, Gallery, SEMIBOLD_TEXTURE, find_named};
use super::title_bar::{self, TitleBar};
use super::*;
use crate::draw::{DrawCmd, DrawList, TextureRef};
use crate::event::MouseButton;

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

fn control(body: &str) -> Gallery {
    Gallery::of(
        &format!(r#"<panel x="0" y="0" w="500" h="400" theme="plugin">{body}</panel>"#),
        PluginTheme::Moss,
    )
}

/// What the named element draws, in its parent's coordinates.
fn element_corners(g: &mut Gallery, name: &str) -> Vec<(f32, f32, Color)> {
    corners(&element_list(g, name))
}

fn element_list(g: &mut Gallery, name: &str) -> DrawList {
    g.tick();
    let ui = &mut g.host.root;
    let id = find_named(ui, ui.root(), name).unwrap();
    let mut list = DrawList::default();
    let mut rc =
        crate::draw::RenderContext::new(&mut list, (1024.0, 768.0), &crate::draw::NoSprites);
    ui.draw_tree(id, &mut rc);
    drop(rc);
    list
}

fn opaque(v: &[(f32, f32, Color)], color: Color) -> Vec<(f32, f32)> {
    v.iter()
        .filter(|(_, _, c)| c.a > 0.99 && same_rgb(*c, color))
        .map(|(x, y, _)| (*x, *y))
        .collect()
}

fn span(values: impl Iterator<Item = f32> + Clone) -> (f32, f32) {
    (
        values.clone().fold(f32::MAX, f32::min),
        values.fold(f32::MIN, f32::max),
    )
}

#[test]
fn a_themed_button_shows_hover_and_press() {
    let mut g =
        control(r#"<button name="b" x="10" y="10" w="80" h="24" text="Go" onclick="{Click}"/>"#);
    assert!(has_color(&element_corners(&mut g, "b"), P.field));
    let (x, y) = g.centre("b");
    g.host.mouse_move(x, y);
    assert!(has_color(
        &element_corners(&mut g, "b"),
        style::hover(&P, P.field)
    ));
    g.host.mouse_down(MouseButton::Left, x, y, 0);
    assert!(has_color(
        &element_corners(&mut g, "b"),
        style::pressed(P.field)
    ));
    g.host.mouse_up(MouseButton::Left, x, y, 0);
    g.host.mouse_move(450, 390);
    assert!(has_color(&element_corners(&mut g, "b"), P.field));
}

#[test]
fn a_disabled_themed_button_fades_its_caption_too() {
    let mut g = control(
        r#"<button name="on" x="10" y="10" w="80" h="24" text="Start" onclick="{Click}"/>
           <button name="off" x="100" y="10" w="80" h="24" text="Stop" enabled="{TabOff}" onclick="{Click}"/>"#,
    );
    let tint = |list: &DrawList| {
        list.commands()
            .find_map(|c| match c {
                DrawCmd::Glyphs { tint, .. } => Some(tint.a),
                _ => None,
            })
            .expect("a caption")
    };
    assert_eq!(tint(&element_list(&mut g, "on")), 1.0);
    assert!(tint(&element_list(&mut g, "off")) <= style::DISABLED_ALPHA + 1e-4);
}

#[test]
fn a_selected_themed_tab_is_a_pill_without_an_underline() {
    let mut g = control(
        r#"<tab name="a" x="10" y="10" w="80" h="24" text="Status" selected="{TabOn}" onclick="{Click}"/>
           <tab name="b" x="100" y="10" w="80" h="24" text="Spells" selected="{TabOff}" onclick="{Click}"/>"#,
    );
    let selected = element_corners(&mut g, "a");
    assert!(has_color(&selected, P.selected));
    assert!(!has_color(&selected, P.accent));
    assert!(!has_color(&element_corners(&mut g, "b"), P.selected));
    let tint = |list: &DrawList| {
        list.commands()
            .find_map(|c| match c {
                DrawCmd::Glyphs { tint, .. } => Some(*tint),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(tint(&element_list(&mut g, "a")), P.text);
    assert_eq!(tint(&element_list(&mut g, "b")), P.muted);
}

#[test]
fn a_themed_field_glows_when_focused() {
    let mut g = control(r#"<field name="f" x="10" y="10" w="160" h="24" text="{Text}"/>"#);
    let blurred = element_corners(&mut g, "f");
    assert!(has_color(&blurred, P.border));
    assert!(!has_color(&blurred, P.accent));
    let at = g.centre("f");
    g.click(MouseButton::Left, at);
    let focused = element_corners(&mut g, "f");
    assert!(has_color(&focused, P.accent));
}

#[test]
fn a_themed_menu_has_a_rounded_face_and_a_shadowed_popup() {
    let mut g = control(
        r#"<menu name="m" x="10" y="10" w="160" h="24" items="{Items}" selected="{Selected}" rows="3"/>"#,
    );
    let at = g.centre("m");
    g.host.mouse_move(at.0, at.1);
    assert!(has_color(
        &element_corners(&mut g, "m"),
        style::hover(&P, P.field)
    ));
    g.click(MouseButton::Left, at);
    let v = corners(&g.draw());
    assert!(
        v.iter()
            .any(|(_, y, c)| *y > 34.0 && c.a > 0.0 && same_rgb(*c, Color::BLACK)),
        "a shadow below the face"
    );
    assert!(has_color(&v, P.accent), "the open edge");
    assert!(has_color(&v, P.selected), "the chosen row");
}

#[test]
fn a_themed_menu_hovers_in_a_colour_distinct_from_selected_and_classic_gets_its_own_back() {
    let mut g = control(
        r#"<menu name="m" x="10" y="10" w="160" h="24" items="{Items}" selected="{Selected}" rows="3"/>"#,
    );
    let hover_color = |g: &Gallery| {
        let ui = &g.host.root;
        let id = find_named(ui, ui.root(), "m").unwrap();
        ui.widget::<Menu>(id).unwrap().plain_hover_color
    };
    assert_eq!(hover_color(&g), style::hover(&P, P.field));
    assert_ne!(hover_color(&g), P.selected);
    g.set_theme(PluginTheme::Classic);
    assert_eq!(hover_color(&g), Menu::default().plain_hover_color);
}

#[test]
fn a_themed_list_has_inset_rows_and_a_slim_thumb() {
    let mut g = control(
        r#"<list name="l" x="0" y="0" w="200" h="54" rowheight="18" items="{Items}" selected="{Index}" selectionband="true"/>"#,
    );
    let v = element_corners(&mut g, "l");
    let band = opaque(&v, P.selected);
    assert!(!band.is_empty());
    assert!(span(band.iter().map(|p| p.0)).0 >= style::ROW_INSET - 0.51);
    let thumb = opaque(&v, P.muted);
    let (left, right) = span(thumb.iter().map(|p| p.0));
    assert!(
        right - left <= style::SCROLL_THUMB_WIDTH + 1.01,
        "{left} {right}"
    );
}

#[test]
fn a_themed_list_bands_the_selected_row_of_its_columns_too() {
    let mut g = control(
        r#"<list name="l" x="0" y="0" w="200" h="54" selected="{Index}">
             <column type="text" width="*" items="{Items}"/></list>"#,
    );
    assert!(has_color(&element_corners(&mut g, "l"), P.selected));
}

#[test]
fn a_themed_list_header_sits_inside_the_rounded_container() {
    let mut g = control(
        r#"<list name="l" x="0" y="0" w="200" h="80" header="true" selected="{Index}" onchange="{Choose}">
             <column type="text" header="Name" items="{Items}"/></list>"#,
    );
    let v = element_corners(&mut g, "l");
    assert!(
        !v.iter()
            .any(|(x, y, c)| *x == 0.0 && *y == 0.0 && same_rgb(*c, P.field)),
        "no square band at the corner"
    );
    let line = super::list_ext::HEADER_HEIGHT - 1.0;
    let underline: Vec<f32> = v
        .iter()
        .filter(|(_, y, c)| (*y - line).abs() < 0.01 && same_rgb(*c, P.border))
        .map(|(x, _, _)| *x)
        .collect();
    assert_eq!(
        span(underline.iter().copied()),
        (style::ROW_INSET, 200.0 - style::ROW_INSET)
    );
}

#[test]
fn a_themed_slider_has_a_fill_and_a_round_thumb_where_it_is_grabbed() {
    let mut g = control(
        r#"<slider name="s" x="0" y="0" w="200" h="16" value="{Value}" onchange="{Changed}"/>"#,
    );
    let v = element_corners(&mut g, "s");
    assert!(has_color(&v, P.accent));
    let thumb = opaque(&v, P.text);
    let (left, right) = span(thumb.iter().map(|p| p.0));
    // The hit test's 16-pixel thumb, at 0.6 of the 184 it can travel.
    let centre = 184.0 * 0.6 + 8.0;
    assert!(
        ((left + right) / 2.0 - centre).abs() < 1.0,
        "{left} {right}"
    );
}

#[test]
fn a_themed_meter_is_rounded_in_its_own_colour() {
    let mut g = control(
        r##"<meter name="m" x="0" y="0" w="200" h="12" fill="{Fill}" color="#FFCC3333"/>"##,
    );
    let v = element_corners(&mut g, "m");
    assert!(has_color(&v, P.field));
    assert!(has_color(
        &v,
        Color::rgba(
            0xCC as f32 / 255.0,
            0x33 as f32 / 255.0,
            0x33 as f32 / 255.0,
            1.0
        )
    ));
    assert!(v.len() > 12, "rounded, not two rectangles");
}

#[test]
fn a_themed_log_sits_in_a_rounded_container_with_a_slim_thumb() {
    let mut g = control(r#"<log name="l" x="0" y="0" w="200" h="80" items="{Log}"/>"#);
    let v = element_corners(&mut g, "l");
    assert!(has_color(&v, P.field));
    assert!(v.len() > 12);
    let (left, right) = span(opaque(&v, P.muted).iter().map(|p| p.0));
    assert!(
        right - left <= style::SCROLL_THUMB_WIDTH + 1.01,
        "{left} {right}"
    );
}

#[test]
fn a_themed_scrolling_groups_bar_is_a_slim_thumb() {
    let mut g = control(
        r#"<group name="g" x="0" y="0" w="200" h="100" scroll="y"><label x="0" y="500" text="Far"/></group>"#,
    );
    let v = corners(&g.draw());
    let thumb = opaque(&v, P.muted);
    assert!(!thumb.is_empty());
    let (left, right) = span(thumb.iter().map(|p| p.0));
    assert!(
        right - left <= style::SCROLL_THUMB_WIDTH + 1.01,
        "{left} {right}"
    );
}

#[test]
fn switching_to_classic_clears_every_controls_palette() {
    let mut g = control(
        r#"<slider name="s" x="0" y="0" w="200" h="16" value="{Value}" onchange="{Changed}"/>
           <field name="f" x="0" y="20" w="200" h="24"/>
           <menu name="m" x="0" y="50" w="200" h="24" items="{Items}" selected="{Selected}"/>
           <meter name="e" x="0" y="80" w="200" h="12" fill="{Fill}"/>"#,
    );
    let set = |g: &Gallery| {
        let ui = &g.host.root;
        let id = |n| find_named(ui, ui.root(), n).unwrap();
        [
            ui.widget::<Scrollbar>(id("s")).unwrap().theme_palette,
            ui.widget::<MarkupField>(id("f"))
                .unwrap()
                .inner
                .theme_palette,
            ui.widget::<Menu>(id("m")).unwrap().theme_palette,
            ui.widget::<Meter>(id("e")).unwrap().theme_palette,
        ]
    };
    assert_eq!(set(&g), [Some(P); 4]);
    g.set_theme(PluginTheme::Classic);
    assert_eq!(set(&g), [None; 4]);
}
