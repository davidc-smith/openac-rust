//! The fonts a plugin draws canvas text with: the client's bundled Noto Sans
//! in either weight, or a TrueType or OpenType font the plugin ships, each
//! baked at one size into a coverage atlas. They are session state like
//! images: a plugin asks, is answered and can measure with or without a
//! window, and a window only uploads the atlases it is told about.
//!
//! Every request is counted once per distinct font, size and set of
//! characters and held as many times as asked. A plugin may hold
//! [`MAXIMUM_COUNT`] fonts, and its own fonts may take [`MAXIMUM_BYTES`],
//! counting the files and their atlases. The bundled font is shared between
//! plugins and costs nothing against that. On a high-density display each
//! font also gets a sharper bake, which draws in its place while every
//! measurement still comes from the font itself. Those are the client's
//! choice, so they count against [`MAXIMUM_SHARP_BYTES`] instead, and a plugin
//! that fits one display fits every display.
//!
//! A font file comes from a plugin and is read outside its sandbox, so a file
//! must never bring the client down. Its structure is checked before the
//! parsers see it, a glyph's size is checked before anything is allocated for
//! it, and a panic in the parsers is a refusal like any other. Handles are
//! never reused.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use ttf_parser::gpos::PositioningSubtable;
use ttf_parser::{Face, GlyphId};

/// The regular font file, compiled in.
pub static NOTO_SANS_REGULAR: &[u8] =
    include_bytes!("../../../assets/fonts/NotoSans/NotoSans-Regular.ttf");

/// The SemiBold font file, compiled in.
pub static NOTO_SANS_SEMIBOLD: &[u8] =
    include_bytes!("../../../assets/fonts/NotoSans/NotoSans-SemiBold.ttf");

/// The most distinct fonts one plugin may hold.
pub const MAXIMUM_COUNT: usize = 16;
/// The most memory one plugin's own fonts may take: the files and their
/// atlases, bytes.
pub const MAXIMUM_BYTES: u64 = 16 * 1024 * 1024;
/// The largest font file a plugin may hand over, bytes: a file this size
/// still leaves room for an atlas, and fits one request.
pub const MAXIMUM_FILE_BYTES: usize = 15 * 1024 * 1024;
/// The most memory the sharper bakes of one plugin's own fonts may take.
pub const MAXIMUM_SHARP_BYTES: u64 = 32 * 1024 * 1024;
/// The most characters one font may prepare.
pub const MAXIMUM_GLYPHS: usize = 2048;
/// The smallest size a font may be asked for, pixels.
pub const MINIMUM_PIXEL_SIZE: f32 = 6.0;
/// The largest size a font may be asked for, pixels.
pub const MAXIMUM_PIXEL_SIZE: f32 = 64.0;
/// The most code points a request's ranges may name.
pub const MAXIMUM_SCANNED_CODEPOINTS: u64 = 65_536;
/// The widest or tallest an atlas may be, pixels.
pub const MAXIMUM_ATLAS_SIDE: u32 = 2048;

/// The characters baked when a request names none: Latin, Greek and Cyrillic,
/// and general punctuation.
pub const DEFAULT_RANGES: [(u32, u32); 3] = [(0x20, 0x24F), (0x370, 0x52F), (0x2000, 0x206F)];

/// The atlas sizes tried, smallest first.
const ATLAS_SIZES: [(u32, u32); 7] = [
    (256, 256),
    (512, 256),
    (512, 512),
    (1024, 512),
    (1024, 1024),
    (2048, 1024),
    (2048, 2048),
];

/// A sharper bake's scale goes up in steps this size.
const SHARP_SCALE_STEP: f32 = 0.25;

/// The most kerning pairs a font remembers.
const KERNING_CACHE_LIMIT: usize = 4096;

/// A character without a glyph draws as this, when it was baked.
const FALLBACK: char = '?';

/// The weights of the bundled font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontWeight {
    /// Body text.
    #[default]
    Regular,
    /// The weight themed window titles are set in.
    SemiBold,
}

impl FontWeight {
    fn file(self) -> &'static [u8] {
        match self {
            Self::Regular => NOTO_SANS_REGULAR,
            Self::SemiBold => NOTO_SANS_SEMIBOLD,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Regular => "Regular",
            Self::SemiBold => "SemiBold",
        }
    }
}

/// One glyph as a font draws it: its box relative to the pen on the baseline
/// (y down), its advance and its place in the atlas, all at the size baked.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FontGlyph {
    /// The box's left edge from the pen.
    pub x: f32,
    /// The box's top edge from the baseline, y down.
    pub y: f32,
    /// The box's width.
    pub width: f32,
    /// The box's height.
    pub height: f32,
    /// How far the pen moves.
    pub advance: f32,
    /// The atlas area, left.
    pub u0: f32,
    /// The atlas area, top.
    pub v0: f32,
    /// The atlas area, right.
    pub u1: f32,
    /// The atlas area, bottom.
    pub v1: f32,
    /// The font's own number for it, which kerning is looked up by.
    pub index: u16,
}

#[derive(Debug, Clone)]
enum FontFile {
    Bundled(&'static [u8]),
    Own(Arc<Vec<u8>>),
}

impl FontFile {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Bundled(bytes) => bytes,
            Self::Own(bytes) => bytes,
        }
    }
}

/// A font baked at one size: its coverage atlas, one byte a pixel, drawn white
/// with that alpha, and what it takes to lay text out in it.
pub struct FontBake {
    /// The size baked, pixels from the highest ascender to the lowest
    /// descender.
    pub pixel_size: f32,
    /// How far apart two lines sit, pixels.
    pub line_height: f32,
    /// How far below the top of a line the baseline sits, pixels.
    pub ascent: f32,
    /// The atlas's width, pixels.
    pub atlas_width: u32,
    /// The atlas's height, pixels.
    pub atlas_height: u32,
    /// The atlas, rows from the top.
    pub coverage: Vec<u8>,
    glyphs: HashMap<char, FontGlyph>,
    file: FontFile,
    /// Font units to pixels.
    scale: f32,
    kerning: Mutex<HashMap<(u16, u16), f32>>,
}

impl fmt::Debug for FontBake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontBake")
            .field("pixel_size", &self.pixel_size)
            .field("atlas_width", &self.atlas_width)
            .field("atlas_height", &self.atlas_height)
            .field("glyphs", &self.glyphs.len())
            .finish_non_exhaustive()
    }
}

impl FontBake {
    /// The glyph for a character, or the font's `?` when it lacks one and `?`
    /// was baked. Control characters have none.
    #[must_use]
    pub fn glyph(&self, c: char) -> Option<&FontGlyph> {
        if (c as u32) < 0x20 {
            return None;
        }
        self.glyphs.get(&c).or_else(|| self.glyphs.get(&FALLBACK))
    }

    /// How many characters were baked.
    #[must_use]
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// How far the pen moves between two glyphs beyond their advances, pixels.
    #[must_use]
    pub fn kerning(&self, left: u16, right: u16) -> f32 {
        let mut known = self.kerning.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(kerning) = known.get(&(left, right)) {
            return *kerning;
        }
        if known.len() >= KERNING_CACHE_LIMIT {
            known.clear();
        }
        let units = Face::parse(self.file.bytes(), 0)
            .ok()
            .and_then(|face| pair_kerning(&face, GlyphId(left), GlyphId(right)))
            .unwrap_or(0);
        let kerning = f32::from(units) * self.scale;
        known.insert((left, right), kerning);
        kerning
    }

    /// Each glyph of a line of text with where its pen stands, and the pen's
    /// travel over the whole line: advances plus kerning. A control character,
    /// or one with no glyph and no `?`, is skipped and breaks the kerning.
    pub fn lay_out(&self, text: &str, mut each: impl FnMut(char, &FontGlyph, f32)) -> f32 {
        let mut pen = 0.0;
        let mut previous = None;
        for c in text.chars() {
            let Some(glyph) = self.glyph(c) else {
                previous = None;
                continue;
            };
            if let Some(left) = previous {
                pen += self.kerning(left, glyph.index);
            }
            each(c, glyph, pen);
            pen += glyph.advance;
            previous = Some(glyph.index);
        }
        pen
    }

    /// The width of a line of text.
    #[must_use]
    pub fn measure_width(&self, text: &str) -> f32 {
        self.lay_out(text, |_, _, _| {})
    }

    fn atlas_bytes(&self) -> u64 {
        u64::from(self.atlas_width) * u64::from(self.atlas_height)
    }
}

/// How far a pair of glyphs is kerned, font units: the `kern` table's value, or
/// else the first pair adjustment in `GPOS` that names them.
fn pair_kerning(face: &Face<'_>, left: GlyphId, right: GlyphId) -> Option<i16> {
    if let Some(kern) = face.tables().kern {
        let found = kern
            .subtables
            .into_iter()
            .filter(|s| s.horizontal && !s.variable)
            .find_map(|s| s.glyphs_kerning(left, right));
        if found.is_some() {
            return found;
        }
    }
    let gpos = face.tables().gpos?;
    for lookup in gpos.lookups {
        for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
            let PositioningSubtable::Pair(pair) = subtable else {
                continue;
            };
            use ttf_parser::gpos::PairAdjustment as Pair;
            let advance = match pair {
                Pair::Format1 { coverage, sets } => coverage
                    .get(left)
                    .and_then(|at| sets.get(at))
                    .and_then(|set| set.get(right))
                    .map(|(first, _)| first.x_advance),
                Pair::Format2 {
                    coverage,
                    classes,
                    matrix,
                } => {
                    if coverage.contains(left) {
                        matrix
                            .get((classes.0.get(left), classes.1.get(right)))
                            .map(|(first, _)| first.x_advance)
                    } else {
                        None
                    }
                }
            };
            if advance.is_some() {
                return advance;
            }
        }
    }
    None
}

/// Whether a file's header, table directory and every table's extent are
/// sound, and it has the tables a font needs, or why not. Runs before the
/// parsers see the file.
fn validate_sfnt(bytes: &[u8]) -> Result<(), String> {
    const NOT_A_FONT: &str = "the file is not a TrueType or OpenType font";
    let word = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let version = word(0).ok_or(NOT_A_FONT)?;
    if version == u32::from_be_bytes(*b"ttcf") {
        return Err("font collections are not supported".to_owned());
    }
    if version != 0x0001_0000
        && version != u32::from_be_bytes(*b"OTTO")
        && version != u32::from_be_bytes(*b"true")
    {
        return Err(NOT_A_FONT.to_owned());
    }
    let tables = bytes
        .get(4..6)
        .map_or(0, |b| usize::from(u16::from_be_bytes([b[0], b[1]])));
    if tables == 0 || 12 + 16 * tables > bytes.len() {
        return Err(NOT_A_FONT.to_owned());
    }
    let mut tags = BTreeSet::new();
    for table in 0..tables {
        let record = 12 + 16 * table;
        let (Some(tag), Some(offset), Some(length)) =
            (word(record), word(record + 8), word(record + 12))
        else {
            return Err(NOT_A_FONT.to_owned());
        };
        if u64::from(offset) + u64::from(length) > bytes.len() as u64 {
            return Err(NOT_A_FONT.to_owned());
        }
        tags.insert(tag.to_be_bytes());
    }
    for required in [b"cmap", b"head", b"hhea", b"hmtx"] {
        if !tags.contains(required) {
            let name = String::from_utf8_lossy(required).into_owned();
            return Err(format!("the font is missing its '{name}' table"));
        }
    }
    let has_outlines = tags.contains(b"CFF ") || (tags.contains(b"glyf") && tags.contains(b"loca"));
    if !has_outlines {
        return Err("the font is missing its outlines ('glyf' and 'loca', or 'CFF ')".to_owned());
    }
    Ok(())
}

/// The characters to bake: those the ranges name that the font has.
fn collect_characters(
    face: &Face<'_>,
    ranges: &[(u32, u32)],
    maximum_glyphs: usize,
) -> Result<Vec<(char, GlyphId)>, String> {
    let mut scanned = 0u64;
    let mut present = BTreeSet::new();
    for &(first, last) in ranges {
        if last > 0x10_FFFF || last < first {
            return Err(format!(
                "the range U+{first:04X}-U+{last:04X} is not a range of code points"
            ));
        }
        scanned += u64::from(last - first) + 1;
        if scanned > MAXIMUM_SCANNED_CODEPOINTS {
            return Err(format!(
                "the ranges name more than {MAXIMUM_SCANNED_CODEPOINTS} code points"
            ));
        }
        for codepoint in first..=last {
            // A surrogate is no character, and no text can name it.
            let Some(c) = char::from_u32(codepoint) else {
                continue;
            };
            if let Some(glyph) = face.glyph_index(c).filter(|g| g.0 != 0) {
                present.insert((c, glyph.0));
            }
        }
    }
    if present.is_empty() {
        return Err("the font has none of the requested characters".to_owned());
    }
    if present.len() > maximum_glyphs {
        return Err(format!(
            "the font has {} of the requested characters; the most a font may prepare is {maximum_glyphs}",
            present.len()
        ));
    }
    Ok(present.into_iter().map(|(c, g)| (c, GlyphId(g))).collect())
}

/// Where each box goes in an atlas: left to right in rows, tallest first, with
/// a pixel of space; none when they do not all fit.
fn pack(boxes: &[(u32, u32)], width: u32, height: u32) -> Option<Vec<(u32, u32)>> {
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by_key(|&at| std::cmp::Reverse(boxes[at].1));
    let mut places = vec![(0, 0); boxes.len()];
    let (mut x, mut y, mut row) = (1, 1, 0);
    for at in order {
        let (w, h) = boxes[at];
        if w == 0 || h == 0 {
            continue;
        }
        if x + w + 1 > width {
            x = 1;
            y += row + 1;
            row = 0;
        }
        if x + w + 1 > width || y + h + 1 > height {
            return None;
        }
        places[at] = (x, y);
        x += w + 1;
        row = row.max(h);
    }
    Some(places)
}

fn bake_file(
    file: FontFile,
    pixel_size: f32,
    ranges: &[(u32, u32)],
    maximum_glyphs: usize,
    maximum_atlas_bytes: u64,
) -> Result<FontBake, String> {
    let bytes = file.bytes();
    validate_sfnt(bytes)?;
    let face = Face::parse(bytes, 0)
        .map_err(|_| "the file is not a TrueType or OpenType font".to_owned())?;
    let characters = collect_characters(&face, ranges, maximum_glyphs)?;
    let hhea = face.tables().hhea;
    let (ascent, descent, gap) = (
        i32::from(hhea.ascender),
        i32::from(hhea.descender),
        i32::from(hhea.line_gap),
    );
    if ascent - descent <= 0 {
        return Err("the font's line metrics are not usable".to_owned());
    }
    let scale = pixel_size / (ascent - descent) as f32;
    let em = scale * f32::from(face.units_per_em());
    let raster = fontdue::Font::from_bytes(
        bytes,
        fontdue::FontSettings {
            collection_index: 0,
            // Outlines are flattened for the size they are drawn at.
            scale: em,
            load_substitutions: false,
        },
    )
    .map_err(|_| "the file is not a TrueType or OpenType font".to_owned())?;
    let mut boxes = Vec::with_capacity(characters.len());
    let mut area = 0u64;
    for (c, glyph) in &characters {
        // Checked first: a hostile outline can claim a glyph bigger than
        // memory, and rasterizing allocates what it claims.
        let metrics = raster.metrics_indexed(glyph.0, em);
        let side = MAXIMUM_ATLAS_SIDE as usize;
        if metrics.width > side || metrics.height > side {
            return Err(format!(
                "U+{:04X} is {} x {} pixels, larger than an atlas",
                *c as u32, metrics.width, metrics.height
            ));
        }
        area += (metrics.width as u64 + 1) * (metrics.height as u64 + 1);
        boxes.push((metrics.width as u32, metrics.height as u32));
    }
    let mut chosen = None;
    let mut largest = None;
    for (width, height) in ATLAS_SIZES {
        let atlas = u64::from(width) * u64::from(height);
        if atlas > maximum_atlas_bytes {
            break;
        }
        largest = Some((width, height));
        if atlas >= area
            && let Some(places) = pack(&boxes, width, height)
        {
            chosen = Some((width, height, places));
            break;
        }
    }
    let Some((width, height, places)) = chosen else {
        return Err(match largest {
            Some((w, h)) => format!(
                "{} glyphs at {pixel_size} px do not fit a {w}x{h} atlas",
                characters.len()
            ),
            None => format!("no atlas fits in {maximum_atlas_bytes} bytes"),
        });
    };
    let mut coverage = vec![0u8; (width as usize) * (height as usize)];
    let mut glyphs = HashMap::with_capacity(characters.len());
    for (at, (c, glyph)) in characters.iter().enumerate() {
        let (metrics, pixels) = raster.rasterize_indexed(glyph.0, em);
        let (x, y) = places[at];
        let (w, h) = boxes[at];
        // What it draws must be what was measured and packed.
        if (metrics.width as u32, metrics.height as u32) != (w, h) {
            return Err(format!("U+{:04X} could not be drawn", *c as u32));
        }
        for row in 0..h as usize {
            let start = (y as usize + row) * width as usize + x as usize;
            coverage[start..start + w as usize]
                .copy_from_slice(&pixels[row * w as usize..(row + 1) * w as usize]);
        }
        let advance = f32::from(face.glyph_hor_advance(*glyph).unwrap_or(0)) * scale;
        let (fw, fh) = (width as f32, height as f32);
        glyphs.insert(
            *c,
            FontGlyph {
                x: metrics.xmin as f32,
                y: -(metrics.ymin + metrics.height as i32) as f32,
                width: w as f32,
                height: h as f32,
                advance,
                u0: x as f32 / fw,
                v0: y as f32 / fh,
                u1: (x + w) as f32 / fw,
                v1: (y + h) as f32 / fh,
                index: glyph.0,
            },
        );
    }
    let line_height = ((ascent - descent + gap) as f32 * scale).ceil();
    let baseline = (ascent as f32 * scale).round_ties_even();
    drop(raster);
    Ok(FontBake {
        pixel_size,
        line_height,
        ascent: baseline,
        atlas_width: width,
        atlas_height: height,
        coverage,
        glyphs,
        file,
        scale,
        kerning: Mutex::new(HashMap::new()),
    })
}

/// [`bake_file`] with a panic in the parsers as a refusal.
fn bake_guarded(
    file: FontFile,
    pixel_size: f32,
    ranges: &[(u32, u32)],
    maximum_glyphs: usize,
    maximum_atlas_bytes: u64,
) -> Result<FontBake, String> {
    catch_unwind(AssertUnwindSafe(|| {
        bake_file(
            file,
            pixel_size,
            ranges,
            maximum_glyphs,
            maximum_atlas_bytes,
        )
    }))
    .unwrap_or_else(|_| Err("the file could not be read as a font".to_owned()))
}

/// Bakes the font in `bytes` at `pixel_size`: the characters `ranges` names
/// that it has, into the smallest atlas that holds them.
///
/// # Errors
/// Why the file cannot be used, never a panic, whatever the bytes are.
pub fn bake(
    bytes: Vec<u8>,
    pixel_size: f32,
    ranges: &[(u32, u32)],
    maximum_glyphs: usize,
) -> Result<FontBake, String> {
    if !pixel_size.is_finite() || pixel_size <= 0.0 {
        return Err("the size is not a positive number".to_owned());
    }
    bake_guarded(
        FontFile::Own(Arc::new(bytes)),
        pixel_size,
        ranges,
        maximum_glyphs,
        u64::MAX,
    )
}

/// A font's glyphs baked again at a multiple of its size.
#[derive(Debug, Clone)]
pub struct SharpBake {
    /// How many times the font's size it was baked at.
    pub scale: f32,
    /// The bake. Only its glyph images are drawn from; the font itself sets
    /// every advance, kerning and line metric, so text measures the same at
    /// every scale.
    pub font: Arc<FontBake>,
}

/// One font a plugin holds, as a window draws it.
#[derive(Debug, Clone)]
pub struct FontView {
    /// The plugin.
    pub owner: String,
    /// The plugin's handle (never reused).
    pub handle: u32,
    /// The font.
    pub font: Arc<FontBake>,
    /// Its sharper bake for the display in use, if it has one.
    pub sharp: Option<SharpBake>,
}

impl PartialEq for FontView {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner
            && self.handle == other.handle
            && Arc::ptr_eq(&self.font, &other.font)
            && match (&self.sharp, &other.sharp) {
                (Some(a), Some(b)) => a.scale == b.scale && Arc::ptr_eq(&a.font, &b.font),
                (None, None) => true,
                _ => false,
            }
    }
}

/// What a font request is answered with; all zero when it was refused.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FontInfo {
    /// The handle.
    pub handle: u32,
    /// The size prepared: the size asked for, to the nearest quarter pixel.
    pub pixel_size: f32,
    /// How far apart two lines sit, pixels.
    pub line_height: f32,
    /// How far below the top of a line the baseline sits, pixels.
    pub ascent: f32,
}

/// A size in quarter pixels: how sizes are told apart, and what is prepared.
fn quarters(pixel_size: f32) -> u32 {
    (pixel_size * 4.0).round_ties_even() as u32
}

/// The scale a sharper bake is made at on a display with `density` pixels a
/// unit: that density rounded down to a quarter step.
fn sharp_scale(density: f32) -> f32 {
    if !density.is_finite() {
        return 1.0;
    }
    ((density / SHARP_SCALE_STEP + 1e-3).floor() * SHARP_SCALE_STEP).max(1.0)
}

/// The bundled font's bakes, which every plugin shares: each stays while a
/// plugin holds it.
#[derive(Debug, Default)]
pub(crate) struct BundledBakes {
    bakes: HashMap<(FontWeight, u32), Weak<FontBake>>,
    /// The sharper bakes, by weight, size and scale in quarter steps.
    sharp: HashMap<(FontWeight, u32, u32), Weak<FontBake>>,
}

impl BundledBakes {
    fn acquire(
        &mut self,
        weight: FontWeight,
        quarter_pixels: u32,
    ) -> Result<Arc<FontBake>, String> {
        if let Some(bake) = self
            .bakes
            .get(&(weight, quarter_pixels))
            .and_then(Weak::upgrade)
        {
            return Ok(bake);
        }
        let bake = Arc::new(bake_guarded(
            FontFile::Bundled(weight.file()),
            quarter_pixels as f32 / 4.0,
            &DEFAULT_RANGES,
            MAXIMUM_GLYPHS,
            u64::MAX,
        )?);
        self.bakes.retain(|_, held| held.strong_count() > 0);
        self.bakes
            .insert((weight, quarter_pixels), Arc::downgrade(&bake));
        Ok(bake)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Bundled {
        weight: FontWeight,
        quarter_pixels: u32,
    },
    Own {
        name: String,
        quarter_pixels: u32,
        ranges: Vec<(u32, u32)>,
    },
}

#[derive(Debug)]
struct Entry {
    key: Key,
    handle: u32,
    font: Arc<FontBake>,
    sharp: Option<SharpBake>,
    /// The density [`Self::sharp`] was prepared for.
    prepared_for: f32,
    /// Zero for the shared bundled font; file and atlas for the plugin's own.
    owned_bytes: u64,
    holds: u32,
}

/// What a request came to.
#[derive(Debug)]
pub(crate) struct AcquiredFont {
    pub(crate) info: FontInfo,
    /// The font when this request made it (not a repeat).
    pub(crate) added: Option<FontView>,
}

/// One plugin's fonts. See the module documentation.
#[derive(Debug)]
pub(crate) struct FontTable {
    entries: Vec<Entry>,
    next_handle: u32,
    owned_bytes: u64,
    sharp_bytes: u64,
    reported: BTreeSet<String>,
}

impl FontTable {
    /// An empty table whose handles start after `last`.
    pub(crate) fn after(last: u32) -> Self {
        Self {
            entries: Vec::new(),
            next_handle: last,
            owned_bytes: 0,
            sharp_bytes: 0,
            reported: BTreeSet::new(),
        }
    }

    pub(crate) fn last_handle(&self) -> u32 {
        self.next_handle
    }

    pub(crate) fn count(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn owned_bytes(&self) -> u64 {
        self.owned_bytes
    }

    /// A reason to log, or nothing when it was logged before.
    fn once(&mut self, key: String, message: String) -> String {
        if self.reported.insert(key) {
            message
        } else {
            String::new()
        }
    }

    fn size_refused(&mut self, pixel_size: f32) -> Option<String> {
        if pixel_size.is_finite() && (MINIMUM_PIXEL_SIZE..=MAXIMUM_PIXEL_SIZE).contains(&pixel_size)
        {
            return None;
        }
        Some(self.once(
            format!("size:{pixel_size}"),
            format!(
                "a font size of {pixel_size} px was refused; sizes run from {MINIMUM_PIXEL_SIZE} to {MAXIMUM_PIXEL_SIZE} px"
            ),
        ))
    }

    fn hold_again(&mut self, key: &Key) -> Option<FontInfo> {
        let entry = self.entries.iter_mut().find(|e| e.key == *key)?;
        entry.holds += 1;
        Some(info_of(entry))
    }

    fn room_refused(&mut self, what: &str) -> Option<String> {
        if self.entries.len() < MAXIMUM_COUNT {
            return None;
        }
        Some(self.once(
            "count".to_owned(),
            format!("{what} refused: the plugin already holds {MAXIMUM_COUNT} fonts, which is the most it may"),
        ))
    }

    fn add(
        &mut self,
        owner: &str,
        key: Key,
        font: Arc<FontBake>,
        owned_bytes: u64,
    ) -> AcquiredFont {
        self.next_handle += 1;
        let entry = Entry {
            key,
            handle: self.next_handle,
            font,
            sharp: None,
            prepared_for: 1.0,
            owned_bytes,
            holds: 1,
        };
        let info = info_of(&entry);
        let added = view_of(owner, &entry);
        self.owned_bytes += owned_bytes;
        self.entries.push(entry);
        AcquiredFont {
            info,
            added: Some(added),
        }
    }

    /// The bundled font at a size and weight.
    pub(crate) fn acquire_bundled(
        &mut self,
        owner: &str,
        pixel_size: f32,
        weight: FontWeight,
        bundled: &mut BundledBakes,
    ) -> Result<AcquiredFont, String> {
        if let Some(report) = self.size_refused(pixel_size) {
            return Err(report);
        }
        let quarter_pixels = quarters(pixel_size);
        let key = Key::Bundled {
            weight,
            quarter_pixels,
        };
        if let Some(info) = self.hold_again(&key) {
            return Ok(AcquiredFont { info, added: None });
        }
        let what = format!(
            "the bundled font's {} weight at {pixel_size} px",
            weight.name()
        );
        if let Some(report) = self.room_refused(&what) {
            return Err(report);
        }
        match bundled.acquire(weight, quarter_pixels) {
            Ok(font) => Ok(self.add(owner, key, font, 0)),
            Err(why) => Err(self.once(
                format!("bundled:{}:{quarter_pixels}", weight.name()),
                format!("{what} could not be prepared: {why}"),
            )),
        }
    }

    /// A font the plugin ships, whose file it hands over.
    pub(crate) fn acquire_own(
        &mut self,
        owner: &str,
        name: &str,
        bytes: &[u8],
        pixel_size: f32,
        ranges: &[(u32, u32)],
    ) -> Result<AcquiredFont, String> {
        if let Some(report) = self.size_refused(pixel_size) {
            return Err(report);
        }
        let quarter_pixels = quarters(pixel_size);
        let ranges: Vec<(u32, u32)> = if ranges.is_empty() {
            DEFAULT_RANGES.to_vec()
        } else {
            ranges.to_vec()
        };
        let key = Key::Own {
            name: name.to_owned(),
            quarter_pixels,
            ranges: ranges.clone(),
        };
        if let Some(info) = self.hold_again(&key) {
            return Ok(AcquiredFont { info, added: None });
        }
        let what = format!("font '{name}'");
        if let Some(report) = self.room_refused(&what) {
            return Err(report);
        }
        if bytes.len() > MAXIMUM_FILE_BYTES {
            return Err(self.once(
                format!("file:{name}"),
                format!("{what} is larger than the {MAXIMUM_FILE_BYTES} bytes a font file may be"),
            ));
        }
        let room = MAXIMUM_BYTES.saturating_sub(self.owned_bytes);
        if bytes.len() as u64 > room {
            return Err(self.once(
                format!("bytes:{name}"),
                format!("{what} would take the plugin's own fonts past the budget of {MAXIMUM_BYTES} bytes"),
            ));
        }
        let size = quarter_pixels as f32 / 4.0;
        let file = FontFile::Own(Arc::new(bytes.to_vec()));
        let font = match bake_guarded(file, size, &ranges, MAXIMUM_GLYPHS, u64::MAX) {
            Ok(font) => font,
            Err(why) => {
                return Err(self.once(
                    format!("bake:{name}:{quarter_pixels}:{ranges:?}"),
                    format!("{what} at {size} px could not be prepared: {why}"),
                ));
            }
        };
        let owned = bytes.len() as u64 + font.atlas_bytes();
        if self.owned_bytes + owned > MAXIMUM_BYTES {
            return Err(self.once(
                format!("budget:{name}:{quarter_pixels}:{ranges:?}"),
                format!(
                    "{what} at {size} px would take the plugin's own fonts to {} bytes; the budget is {MAXIMUM_BYTES}",
                    self.owned_bytes + owned
                ),
            ));
        }
        Ok(self.add(owner, key, Arc::new(font), owned))
    }

    /// Lets go of one hold: whether the plugin held it, and whether that was
    /// the last.
    pub(crate) fn release(&mut self, handle: u32) -> Option<bool> {
        let at = self.entries.iter().position(|e| e.handle == handle)?;
        self.entries[at].holds -= 1;
        if self.entries[at].holds > 0 {
            return Some(false);
        }
        let entry = self.entries.swap_remove(at);
        self.owned_bytes -= entry.owned_bytes;
        if !matches!(entry.key, Key::Bundled { .. }) {
            self.sharp_bytes -= entry.sharp.as_ref().map_or(0, |s| s.font.atlas_bytes());
        }
        Some(true)
    }

    /// The width of `text` in a font and its line height: nothing for a font
    /// the plugin does not hold, and for empty text.
    pub(crate) fn measure(&self, handle: u32, text: &str) -> (f32, f32) {
        match self.entries.iter().find(|e| e.handle == handle) {
            Some(entry) if !text.is_empty() => {
                (entry.font.measure_width(text), entry.font.line_height)
            }
            _ => (0.0, 0.0),
        }
    }

    /// Readies every font for a display of `density` pixels a unit: each gets a
    /// sharper bake at that multiple of its size, or the largest lower quarter
    /// step that fits, and the bake for any other density goes. A font already
    /// prepared for it is left alone. The views that changed, and what to log
    /// for a font drawn less sharply than it could be.
    pub(crate) fn prepare_density(
        &mut self,
        owner: &str,
        density: f32,
        bundled: &mut BundledBakes,
    ) -> (Vec<FontView>, Vec<String>) {
        let wanted = sharp_scale(density);
        let mut changed = Vec::new();
        let mut reports = Vec::new();
        for at in 0..self.entries.len() {
            if self.entries[at].prepared_for == wanted {
                continue;
            }
            let shared = matches!(self.entries[at].key, Key::Bundled { .. });
            let held = self.entries[at]
                .sharp
                .take()
                .map_or(0, |s| s.font.atlas_bytes());
            if !shared {
                self.sharp_bytes -= held;
            }
            let (sharp, shortfall) = self.sharpen(at, wanted, bundled);
            let entry = &mut self.entries[at];
            entry.prepared_for = wanted;
            entry.sharp = sharp;
            if !shared {
                self.sharp_bytes += entry.sharp.as_ref().map_or(0, |s| s.font.atlas_bytes());
            }
            let entry = &self.entries[at];
            changed.push(view_of(owner, entry));
            if let Some(why) = shortfall {
                let drawn = entry.sharp.as_ref().map_or(1.0, |s| s.scale);
                let message = format!(
                    "a font at {} px is drawn at {drawn}x on a display of {wanted}x, so it is less sharp than it could be: {why}",
                    entry.font.pixel_size
                );
                let report = self.once(
                    format!("sharp:{}:{wanted}", self.entries[at].handle),
                    message,
                );
                if !report.is_empty() {
                    reports.push(report);
                }
            }
        }
        (changed, reports)
    }

    /// The sharper bake of entry `at` at `scale`, or the largest lower step
    /// that fits, and why it is not at `scale` itself.
    fn sharpen(
        &self,
        at: usize,
        scale: f32,
        bundled: &mut BundledBakes,
    ) -> (Option<SharpBake>, Option<String>) {
        let entry = &self.entries[at];
        let size = entry.font.pixel_size;
        let mut shortfall = None;
        let mut step = scale;
        while step > 1.0 {
            let baked = match &entry.key {
                Key::Bundled { weight, .. } => {
                    let key = (*weight, quarters(size), quarters(step));
                    match bundled.sharp.get(&key).and_then(Weak::upgrade) {
                        Some(bake) => Ok(bake),
                        None => bake_guarded(
                            entry.font.file.clone(),
                            size * step,
                            &DEFAULT_RANGES,
                            MAXIMUM_GLYPHS,
                            u64::MAX,
                        )
                        .map(|bake| {
                            let bake = Arc::new(bake);
                            bundled.sharp.retain(|_, held| held.strong_count() > 0);
                            bundled.sharp.insert(key, Arc::downgrade(&bake));
                            bake
                        }),
                    }
                }
                Key::Own { ranges, .. } => bake_guarded(
                    entry.font.file.clone(),
                    size * step,
                    ranges,
                    MAXIMUM_GLYPHS,
                    MAXIMUM_SHARP_BYTES.saturating_sub(self.sharp_bytes),
                )
                .map(Arc::new),
            };
            match baked {
                Ok(font) => return (Some(SharpBake { scale: step, font }), shortfall),
                Err(why) => {
                    shortfall.get_or_insert(why);
                }
            }
            step -= SHARP_SCALE_STEP;
        }
        (None, shortfall)
    }

    pub(crate) fn views(&self, owner: &str) -> Vec<FontView> {
        self.entries.iter().map(|e| view_of(owner, e)).collect()
    }

    pub(crate) fn handles(&self) -> Vec<u32> {
        self.entries.iter().map(|e| e.handle).collect()
    }
}

fn info_of(entry: &Entry) -> FontInfo {
    FontInfo {
        handle: entry.handle,
        pixel_size: entry.font.pixel_size,
        line_height: entry.font.line_height,
        ascent: entry.font.ascent,
    }
}

fn view_of(owner: &str, entry: &Entry) -> FontView {
    FontView {
        owner: owner.to_owned(),
        handle: entry.handle,
        font: Arc::clone(&entry.font),
        sharp: entry.sharp.clone(),
    }
}

#[cfg(test)]
mod tests;
