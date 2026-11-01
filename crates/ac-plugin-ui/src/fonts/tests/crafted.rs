//! Fonts made here byte by byte, each built to cost far more work than its
//! size suggests: they are refused, or bounded, before the rasterizer or a
//! frame pays for them.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::super::*;
use super::OWNER;

fn be16(out: &mut Vec<u8>, n: u16) {
    out.extend_from_slice(&n.to_be_bytes());
}

fn be32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_be_bytes());
}

/// A font file: the version tag and its tables, sorted for the directory.
fn sfnt(version: &[u8; 4], mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_by_key(|(tag, _)| *tag);
    let mut out = version.to_vec();
    be16(&mut out, tables.len() as u16);
    out.extend_from_slice(&[0; 6]);
    let mut offset = 12 + 16 * tables.len();
    for (tag, data) in &tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&[0; 4]);
        be32(&mut out, offset as u32);
        be32(&mut out, data.len() as u32);
        offset += data.len().next_multiple_of(4);
    }
    for (_, data) in &tables {
        out.extend_from_slice(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    out
}

/// The tables every font here has: header, line metrics, glyph count,
/// advances (600 units each) and the character map.
fn common_tables(
    units_per_em: u16,
    (ascent, descent): (i16, i16),
    glyphs: u16,
    map: &[(char, u16)],
) -> Vec<([u8; 4], Vec<u8>)> {
    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    head[18..20].copy_from_slice(&units_per_em.to_be_bytes());
    // Long glyph offsets.
    head[50..52].copy_from_slice(&1u16.to_be_bytes());
    let mut hhea = vec![0u8; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea[4..6].copy_from_slice(&ascent.to_be_bytes());
    hhea[6..8].copy_from_slice(&descent.to_be_bytes());
    hhea[34..36].copy_from_slice(&glyphs.to_be_bytes());
    let mut maxp = 0x0000_5000u32.to_be_bytes().to_vec();
    be16(&mut maxp, glyphs);
    let mut hmtx = Vec::new();
    for _ in 0..glyphs {
        be16(&mut hmtx, 600);
        be16(&mut hmtx, 0);
    }
    let mut characters: Vec<(char, GlyphId)> = map.iter().map(|(c, g)| (*c, GlyphId(*g))).collect();
    characters.sort();
    vec![
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"maxp", maxp),
        (*b"hmtx", hmtx),
        (*b"cmap", sfnt::character_map(&characters)),
    ]
}

/// A TrueType font of `glyphs` (their outline data), with `map` naming them
/// and any `extra` tables.
fn truetype(
    units_per_em: u16,
    line: (i16, i16),
    glyphs: &[Vec<u8>],
    map: &[(char, u16)],
    extra: Vec<([u8; 4], Vec<u8>)>,
) -> Vec<u8> {
    let mut tables = common_tables(units_per_em, line, glyphs.len() as u16, map);
    let mut loca = Vec::new();
    let mut glyf = Vec::new();
    for glyph in glyphs {
        be32(&mut loca, glyf.len() as u32);
        glyf.extend_from_slice(glyph);
    }
    be32(&mut loca, glyf.len() as u32);
    tables.push((*b"loca", loca));
    tables.push((*b"glyf", glyf));
    tables.extend(extra);
    sfnt(&[0, 1, 0, 0], tables)
}

/// A glyph with no outline.
fn empty() -> Vec<u8> {
    vec![0; 10]
}

/// A glyph of one contour through `points` (x, y, on the curve).
fn contour(points: &[(i16, i16, bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    be16(&mut out, 1);
    out.extend_from_slice(&[0; 8]);
    be16(&mut out, points.len() as u16 - 1);
    be16(&mut out, 0);
    for (_, _, on) in points {
        out.push(u8::from(*on));
    }
    let (mut x, mut y) = (0i16, 0i16);
    let mut ys = Vec::new();
    for (px, py, _) in points {
        out.extend_from_slice(&(px - x).to_be_bytes());
        ys.extend_from_slice(&(py - y).to_be_bytes());
        (x, y) = (*px, *py);
    }
    out.extend_from_slice(&ys);
    out
}

/// A square glyph, `side` units.
fn square(side: i16) -> Vec<u8> {
    contour(&[
        (0, 0, true),
        (side, 0, true),
        (side, side, true),
        (0, side, true),
    ])
}

/// A glyph built of `parts`, each placed at the origin.
fn composite(parts: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    be16(&mut out, 0xFFFF);
    out.extend_from_slice(&[0; 8]);
    for (at, part) in parts.iter().enumerate() {
        // Word arguments that are offsets, and more to come but for the last.
        let more = if at + 1 < parts.len() { 0x0020 } else { 0 };
        be16(&mut out, 0x0001 | 0x0002 | more);
        be16(&mut out, *part);
        be16(&mut out, 0);
        be16(&mut out, 0);
    }
    out
}

/// `bake`, which must answer within `limit` whatever it answers.
fn bake_within(bytes: Vec<u8>, pixel_size: f32, limit: Duration) -> Result<FontBake, String> {
    let start = Instant::now();
    let result = bake(bytes, pixel_size, &[(0x20, 0x7E)], MAXIMUM_GLYPHS);
    assert!(start.elapsed() < limit, "took {:?}", start.elapsed());
    result
}

/// Each glyph is two copies of the one before, 23 deep: eight million parts
/// for one character from a file of a few hundred bytes. The parser would
/// walk every one before drawing anything.
#[test]
fn a_glyph_that_doubles_at_every_level_is_refused_before_it_is_drawn() {
    let mut glyphs = vec![empty(), empty()];
    for below in 1..23u16 {
        glyphs.push(composite(&[below, below]));
    }
    let top = glyphs.len() as u16 - 1;
    let bytes = truetype(1000, (800, -200), &glyphs, &[('A', top)], Vec::new());
    assert!(bytes.len() < 2048, "{}", bytes.len());
    let why = bake_within(bytes, 16.0, Duration::from_secs(2)).unwrap_err();
    assert!(why.contains("too complex"), "{why}");
}

/// A glyph built of itself would be followed as deep as the parser goes.
#[test]
fn a_glyph_that_is_part_of_itself_is_refused() {
    let glyphs = vec![empty(), square(500), composite(&[1, 2])];
    let bytes = truetype(1000, (800, -200), &glyphs, &[('A', 2)], Vec::new());
    let why = bake_within(bytes, 16.0, Duration::from_secs(2)).unwrap_err();
    assert!(why.contains("too complex"), "{why}");
}

/// Thousands of wide curves in one glyph: at this size the rasterizer would
/// split them into a million and a half line segments.
#[test]
fn a_glyph_that_flattens_into_too_many_segments_is_refused() {
    let mut points = Vec::new();
    for _ in 0..3000 {
        points.extend([
            (0, 0, true),
            (15000, 15000, false),
            (30000, 0, true),
            (15000, -15000, false),
        ]);
    }
    let glyphs = vec![empty(), contour(&points)];
    let bytes = truetype(1000, (1000, 0), &glyphs, &[('A', 1)], Vec::new());
    let why = bake_within(bytes, 64.0, Duration::from_secs(5)).unwrap_err();
    assert!(why.contains("too complex"), "{why}");
}

/// A font whose line spans one unit of a thousand-unit em would draw every
/// glyph a thousand times the size asked for.
#[test]
fn a_font_whose_line_is_a_sliver_of_its_em_is_refused() {
    let glyphs = vec![empty(), square(500)];
    let bytes = truetype(1000, (1, 0), &glyphs, &[('A', 1)], Vec::new());
    let why = bake_within(bytes, 64.0, Duration::from_secs(2)).unwrap_err();
    assert!(why.contains("line metrics"), "{why}");
}

/// A glyph wider than an atlas at the size asked for.
#[test]
fn a_glyph_wider_than_an_atlas_is_refused_before_it_is_drawn() {
    let glyphs = vec![
        empty(),
        contour(&[
            (0, 0, true),
            (32000, 0, true),
            (32000, 100, true),
            (0, 100, true),
        ]),
    ];
    let bytes = truetype(1000, (1000, 0), &glyphs, &[('A', 1)], Vec::new());
    let why = bake_within(bytes, 64.0, Duration::from_secs(2)).unwrap_err();
    assert!(why.contains("larger than an atlas"), "{why}");
}

/// An INDEX of `items`, with four-byte offsets.
fn index(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    be16(&mut out, items.len() as u16);
    if items.is_empty() {
        return out;
    }
    out.push(4);
    let mut offset = 1u32;
    be32(&mut out, offset);
    for item in items {
        offset += item.len() as u32;
        be32(&mut out, offset);
    }
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

/// A dictionary operand written as five bytes, so offsets can be filled in
/// without moving anything.
fn dict_int(out: &mut Vec<u8>, n: u32) {
    out.push(29);
    be32(out, n);
}

/// An OpenType font with CFF outlines: one charstring a glyph, and local
/// subroutines.
fn cff_font(charstrings: &[Vec<u8>], subroutines: &[Vec<u8>], map: &[(char, u16)]) -> Vec<u8> {
    // Header, the name, the top dictionary (filled in below), no strings and
    // no global subroutines; then the charstrings, the private dictionary and
    // its subroutines.
    let top_len = 17;
    let fixed = 4 + index(&[b"A".to_vec()]).len() + index(&[vec![0; top_len]]).len() + 2 + 2;
    let char_strings = index(charstrings);
    let private_at = fixed + char_strings.len();
    let private_len = 6;
    let mut top = Vec::new();
    dict_int(&mut top, fixed as u32);
    top.push(17);
    dict_int(&mut top, private_len as u32);
    dict_int(&mut top, private_at as u32);
    top.push(18);
    let mut private = Vec::new();
    dict_int(&mut private, private_len as u32);
    private.push(19);
    let mut table = vec![1, 0, 4, 4];
    table.extend(index(&[b"A".to_vec()]));
    table.extend(index(&[top]));
    table.extend(index(&[]));
    table.extend(index(&[]));
    assert_eq!(table.len(), fixed);
    table.extend(char_strings);
    table.extend(private);
    table.extend(index(subroutines));
    let mut tables = common_tables(1000, (800, -200), charstrings.len() as u16, map);
    tables.push((*b"CFF ", table));
    sfnt(b"OTTO", tables)
}

/// A charstring number, -107..=107.
fn number(n: i16) -> u8 {
    (n + 139) as u8
}

const CALLSUBR: u8 = 10;
const RETURN: u8 = 11;
const ENDCHAR: u8 = 14;

/// Ten subroutines, each calling the next five times: one glyph runs two
/// million calls from a file of a few hundred bytes.
#[test]
fn a_cff_glyph_whose_subroutines_multiply_is_refused_before_it_is_drawn() {
    let bias = 107;
    let mut subroutines = Vec::new();
    for level in 0..9i16 {
        let mut program = Vec::new();
        for _ in 0..5 {
            program.extend([number(level + 1 - bias), CALLSUBR]);
        }
        program.push(RETURN);
        subroutines.push(program);
    }
    subroutines.push(vec![RETURN]);
    let glyph = vec![number(-bias), CALLSUBR, ENDCHAR];
    let bytes = cff_font(&[vec![ENDCHAR], glyph], &subroutines, &[('A', 1)]);
    assert!(bytes.len() < 1024, "{}", bytes.len());
    let why = bake_within(bytes, 16.0, Duration::from_secs(2)).unwrap_err();
    assert!(why.contains("too complex"), "{why}");
}

/// A glyph made of two others by the old accent composition is refused: its
/// parts would be run too, each able to do the same.
#[test]
fn a_cff_glyph_composed_from_others_is_refused() {
    let glyph = vec![number(0), number(0), number(65), number(66), ENDCHAR];
    let bytes = cff_font(&[vec![ENDCHAR], glyph], &[], &[('A', 1)]);
    let why = bake_within(bytes, 16.0, Duration::from_secs(2)).unwrap_err();
    assert!(why.contains("accented"), "{why}");
}

/// A pair adjustment subtable: `left` followed by `right` moves `units`.
fn pair_subtable(left: u16, right: u16, units: i16) -> Vec<u8> {
    let mut out = Vec::new();
    // Format 1, coverage at 12, the advance of the first glyph, one set at 18.
    for word in [1u16, 12, 0x0004, 0, 1, 18] {
        be16(&mut out, word);
    }
    // Coverage: format 1, one glyph.
    for word in [1u16, 1, left] {
        be16(&mut out, word);
    }
    // The set: one record.
    be16(&mut out, 1);
    be16(&mut out, right);
    out.extend_from_slice(&units.to_be_bytes());
    out
}

/// A `GPOS` table: `features` (tag and lookup numbers) over `lookups` (each
/// a list of pair adjustment subtables), and no scripts.
fn gpos(features: &[(&[u8; 4], Vec<u16>)], lookups: &[Vec<Vec<u8>>]) -> Vec<u8> {
    let mut feature_list = Vec::new();
    be16(&mut feature_list, features.len() as u16);
    let mut tables = Vec::new();
    let mut at = 2 + 6 * features.len();
    for (tag, indices) in features {
        feature_list.extend_from_slice(*tag);
        be16(&mut feature_list, at as u16);
        let mut table = Vec::new();
        be16(&mut table, 0);
        be16(&mut table, indices.len() as u16);
        for i in indices {
            be16(&mut table, *i);
        }
        at += table.len();
        tables.extend(table);
    }
    feature_list.extend(tables);

    let mut lookup_list = Vec::new();
    be16(&mut lookup_list, lookups.len() as u16);
    let mut bodies = Vec::new();
    let mut at = 2 + 2 * lookups.len();
    for subtables in lookups {
        be16(&mut lookup_list, at as u16);
        let mut lookup = Vec::new();
        for word in [2u16, 0, subtables.len() as u16] {
            be16(&mut lookup, word);
        }
        let mut sub_at = 6 + 2 * subtables.len();
        for subtable in subtables {
            be16(&mut lookup, sub_at as u16);
            sub_at += subtable.len();
        }
        for subtable in subtables {
            lookup.extend_from_slice(subtable);
        }
        at += lookup.len();
        bodies.extend(lookup);
    }
    lookup_list.extend(bodies);

    let mut out = Vec::new();
    let scripts = 10;
    let features_at = scripts + 2;
    let lookups_at = features_at + feature_list.len();
    for word in [
        1u16,
        0,
        scripts as u16,
        features_at as u16,
        lookups_at as u16,
    ] {
        be16(&mut out, word);
    }
    be16(&mut out, 0);
    out.extend(feature_list);
    out.extend(lookup_list);
    out
}

/// A legacy `kern` table with one pair.
fn kern_table(left: u16, right: u16, units: i16) -> Vec<u8> {
    let mut out = Vec::new();
    // Version 0, one subtable: version 0, 20 bytes, horizontal format 0, one
    // pair.
    for word in [0u16, 1, 0, 20, 0x0001, 1, 6, 0, 0, left, right] {
        be16(&mut out, word);
    }
    out.extend_from_slice(&units.to_be_bytes());
    out
}

/// 'A' is glyph 1 and 'V' glyph 2, each a square.
fn kerned_font(extra: Vec<([u8; 4], Vec<u8>)>) -> FontBake {
    let glyphs = vec![empty(), square(500), square(500)];
    let bytes = truetype(1000, (800, -200), &glyphs, &[('A', 1), ('V', 2)], extra);
    bake(bytes, 32.0, &[(0x41, 0x56)], MAXIMUM_GLYPHS).unwrap()
}

fn units_between(font: &FontBake, left: u16, right: u16) -> f32 {
    (font.kerning(left, right) / font.scale).round()
}

/// Only the lookups the kerning feature names kern: one another feature
/// names, listed first, is not applied.
#[test]
fn only_the_kerning_features_lookups_kern() {
    let table = gpos(
        &[(b"dist", vec![0]), (b"kern", vec![1])],
        &[
            vec![pair_subtable(1, 2, -100)],
            vec![pair_subtable(1, 2, -40)],
        ],
    );
    let font = kerned_font(vec![(*b"GPOS", table)]);
    assert_eq!(units_between(&font, 1, 2), -40.0);
    assert_eq!(units_between(&font, 2, 1), 0.0);
}

/// A font with both reads `GPOS`, the newer kerning, not the `kern` table.
#[test]
fn gpos_kerning_comes_before_the_kern_table() {
    let font = kerned_font(vec![
        (
            *b"GPOS",
            gpos(&[(b"kern", vec![0])], &[vec![pair_subtable(1, 2, -40)]]),
        ),
        (*b"kern", kern_table(1, 2, -100)),
    ]);
    assert_eq!(units_between(&font, 1, 2), -40.0);
    // Without GPOS kerning, the table is read.
    let font = kerned_font(vec![(*b"kern", kern_table(1, 2, -100))]);
    assert_eq!(units_between(&font, 1, 2), -100.0);
}

/// Each of the feature's lookups applies, and their adjustments add up.
#[test]
fn the_kerning_features_lookups_add_up() {
    let table = gpos(
        &[(b"kern", vec![0, 1])],
        &[
            vec![pair_subtable(1, 2, -40)],
            vec![pair_subtable(1, 2, -10)],
        ],
    );
    let font = kerned_font(vec![(*b"GPOS", table)]);
    assert_eq!(units_between(&font, 1, 2), -50.0);
}

/// Thirty thousand lookups that are one table, each listing one subtable
/// thirty thousand times: the pair it kerns is kerned once, and a pair it
/// does not know costs nothing to lay out.
#[test]
fn a_lookup_and_subtable_listed_over_and_over_are_read_once() {
    const TIMES: usize = 30_000;
    let subtable = pair_subtable(1, 2, -40);
    let mut lookup = Vec::new();
    for word in [2u16, 0, TIMES as u16] {
        be16(&mut lookup, word);
    }
    for _ in 0..TIMES {
        be16(&mut lookup, (6 + 2 * TIMES) as u16);
    }
    lookup.extend_from_slice(&subtable);
    let mut lookups = Vec::new();
    be16(&mut lookups, TIMES as u16);
    for _ in 0..TIMES {
        be16(&mut lookups, (2 + 2 * TIMES) as u16);
    }
    lookups.extend(lookup);
    let mut features = Vec::new();
    be16(&mut features, 1);
    features.extend_from_slice(b"kern");
    be16(&mut features, 8);
    be16(&mut features, 0);
    be16(&mut features, TIMES as u16);
    for i in 0..TIMES {
        be16(&mut features, i as u16);
    }
    let mut table = Vec::new();
    let features_at = 12;
    let lookups_at = features_at + features.len();
    for word in [1u16, 0, 10, features_at as u16, lookups_at as u16, 0] {
        be16(&mut table, word);
    }
    table.extend(features);
    table.extend(lookups);

    let start = Instant::now();
    let font = kerned_font(vec![(*b"GPOS", table)]);
    assert_eq!(units_between(&font, 1, 2), -40.0);
    let a = font.glyph('A').unwrap().advance;
    let line: String = "A".repeat(10_000);
    let advances: f32 = (0..10_000).map(|_| a).sum();
    assert_eq!(font.measure_width(&line), advances);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn a_font_name_or_range_list_past_the_limit_is_refused() {
    let mut ui = super::ui();
    let name = "f".repeat(MAXIMUM_NAME_BYTES + 1);
    let regular = NOTO_SANS_REGULAR;
    assert_eq!(ui.font_load(OWNER, &name, regular, 16.0, &[]).handle, 0);
    let ranges = vec![(0x41, 0x41); MAXIMUM_RANGES + 1];
    assert_eq!(ui.font_load(OWNER, "f", regular, 16.0, &ranges).handle, 0);
    let reports = ui.take_reports();
    assert!(reports[0].1.contains("font name of"), "{reports:?}");
    assert!(reports[1].1.contains("ranges was refused"), "{reports:?}");
    assert_ne!(
        ui.font_load(OWNER, "f", regular, 16.0, &ranges[1..]).handle,
        0
    );
}

/// Refusals are remembered by reason, so a plugin asking for a thousand
/// different wrong things is told once and leaves nothing growing behind.
#[test]
fn many_different_refusals_of_one_kind_are_reported_once() {
    let mut ui = super::ui();
    for i in 0..1000 {
        let size = 100.0 + i as f32;
        assert_eq!(
            ui.font_load(OWNER, &format!("{i}"), b"junk", size, &[])
                .handle,
            0
        );
        assert_eq!(
            ui.font_load(OWNER, &format!("{i}"), b"junk", 16.0, &[])
                .handle,
            0
        );
    }
    assert_eq!(ui.take_reports().len(), 2);
}

#[test]
fn handles_and_holds_stop_at_their_limits_without_overflowing() {
    let mut bundled = BundledBakes::default();
    let none = HashSet::new();
    let mut table = FontTable::after(u32::MAX);
    let refused = table.acquire_bundled(OWNER, 16.0, FontWeight::Regular, &mut bundled, &none);
    assert!(refused.unwrap_err().contains("every font handle"));
    assert_eq!(table.count(), 0);

    let mut table = FontTable::after(0);
    let first = table
        .acquire_bundled(OWNER, 16.0, FontWeight::Regular, &mut bundled, &none)
        .unwrap();
    table.entries[0].holds = u32::MAX;
    let again = table.acquire_bundled(OWNER, 16.0, FontWeight::Regular, &mut bundled, &none);
    assert!(again.unwrap_err().contains("as many times as it may"));
    assert_eq!(table.release(first.info.handle), Some(false));
}

/// Bundled bakes a plugin holds alone count against its budget; one another
/// plugin holds as well costs it nothing.
#[test]
fn bundled_fonts_a_plugin_holds_alone_count_against_its_budget() {
    let mut ui = super::ui();
    // Sixteen default sets near 64 px take a megabyte or two each: more than
    // the budget.
    let sizes: Vec<f32> = (0..16).map(|i| 64.0 - i as f32 * 0.25).collect();
    let held: Vec<u32> = sizes
        .iter()
        .map(|s| ui.font_bundled(OWNER, *s, FontWeight::Regular).handle)
        .collect();
    let refused = held
        .iter()
        .position(|h| *h == 0)
        .expect("the budget ends it");
    assert!(refused >= 1, "the first fits");
    let reports = ui.take_reports();
    assert!(reports[0].1.contains("the budget is"), "{reports:?}");
    // Another plugin's fonts are shared once it holds them too.
    for size in &sizes {
        assert_ne!(
            ui.font_bundled("acme.other", *size, FontWeight::Regular)
                .handle,
            0
        );
    }
    assert_ne!(
        ui.font_bundled(OWNER, sizes[refused], FontWeight::Regular)
            .handle,
        0
    );
}

/// A sharper bake that failed is not baked again for the same room: moving
/// between displays does not repeat the work.
#[test]
fn a_sharper_bake_that_failed_is_not_tried_again() {
    // At 64 px the bar is 1500 pixels wide: it fits an atlas, but not at
    // one and a half times the size.
    let width = (1500.0 / 0.064) as i16;
    let glyphs = vec![
        empty(),
        contour(&[
            (0, 0, true),
            (width, 0, true),
            (width, 150, true),
            (0, 150, true),
        ]),
    ];
    let bytes = truetype(1000, (1000, 0), &glyphs, &[('A', 1)], Vec::new());
    let mut bundled = BundledBakes::default();
    let none = HashSet::new();
    let mut table = FontTable::after(0);
    table
        .acquire_own(OWNER, "bar", &bytes, 64.0, &[(0x41, 0x41)], &none)
        .unwrap();
    table.prepare_density(OWNER, 1.5, &mut bundled, &none);
    let entry = &table.entries[0];
    assert_eq!(entry.sharp.as_ref().map(|s| s.scale), Some(1.25));
    assert_eq!(entry.sharp_failed.len(), 1);
    table.prepare_density(OWNER, 1.0, &mut bundled, &none);
    table.prepare_density(OWNER, 1.5, &mut bundled, &none);
    let entry = &table.entries[0];
    assert_eq!(entry.sharp.as_ref().map(|s| s.scale), Some(1.25));
    assert_eq!(entry.sharp_failed.len(), 1, "not tried again");
}
