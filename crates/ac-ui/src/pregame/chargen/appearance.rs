//! The wizard's appearance page: gender, the face and clothes choices,
//! one spin per part (its left zone steps back, its middle zone forward,
//! the rest picks the part), the colour swatches and the shade slider of
//! the picked part, the gradient disc, and the 3-D preview's rotate and
//! zoom buttons.

use ac_core::chargen::{
    CLOTHING_SAMPLE_INDEX, ChargenClothingTableSource, ChargenGearOption, ChargenGenderOptions,
    ChargenHeritageGroup as H, ChargenPalSetSource, ChargenPaletteColorSource, ChargenSwatchRgb,
    EYE_SAMPLE_INDEX, HAIR_SAMPLE_INDEX, SKIN_FAMILY_SAMPLE_INDEX, clothing_swatch_pal_set_id,
    direct_color, pal_set_average_color,
};
use ac_runtime::session::creation::{
    AppearanceSlot, CreationAppearance, CreationCommand, ShadeSlot, UNSET,
};

use super::PageContext;
use crate::draw::{Color, TextureRef};
use crate::root::{ElementMessage, UiRoot};
use crate::states;
use crate::tree::ElementId;
use crate::widgets::{Button, DatElement, ScrollSource, Scrollbar, Text};

/// Female.
pub const FEMALE_ID: u32 = 0x1000_03A7;
/// Male.
pub const MALE_ID: u32 = 0x1000_03A8;
/// The face choices button.
pub const FACE_ID: u32 = 0x1000_03A9;
/// The clothes choices button.
pub const CLOTHES_ID: u32 = 0x1000_03AA;
/// The face spins.
pub const FACE_CHOICES_ID: u32 = 0x1000_03AE;
/// The clothes spins.
pub const CLOTHES_CHOICES_ID: u32 = 0x1000_03B4;
/// Rotate clockwise.
pub const ROTATE_CLOCKWISE_ID: u32 = 0x1000_0323;
/// Rotate counter-clockwise.
pub const ROTATE_COUNTER_CLOCKWISE_ID: u32 = 0x1000_0324;
/// Zoom in.
pub const ZOOM_IN_ID: u32 = 0x1000_0325;
/// Zoom out.
pub const ZOOM_OUT_ID: u32 = 0x1000_0326;
/// The gradient disc.
pub const GRAD_CIRCLE_ID: u32 = 0x1000_030E;
/// The shade slider.
pub const SHADE_SCROLL_ID: u32 = 0x1000_0321;
/// The 3-D preview.
pub const VIEWPORT_ID: u32 = 0x1000_03BB;
/// The help text.
pub const HELP_TEXT_ID: u32 = 0x1000_03AB;
/// The help text's own scrollbar.
pub const HELP_SCROLL_ID: u32 = 0x1000_02E7;
/// The nine swatches.
pub const SWATCH_IDS: [u32; 9] = [
    0x1000_030F,
    0x1000_0310,
    0x1000_0311,
    0x1000_0312,
    0x1000_0313,
    0x1000_0314,
    0x1000_0315,
    0x1000_0316,
    0x1000_0317,
];
/// The marks over the chosen swatch.
pub const SWATCH_OVERLAY_IDS: [u32; 9] = [
    0x1000_0318,
    0x1000_0319,
    0x1000_031A,
    0x1000_031B,
    0x1000_031C,
    0x1000_031D,
    0x1000_031E,
    0x1000_031F,
    0x1000_0320,
];
/// Where a spin's step-back zone starts, pixels from its left.
pub const DECREMENT_ZONE_START: i32 = 80;
/// Where its step-forward zone starts.
pub const INCREMENT_ZONE_START: i32 = 127;
/// Where that zone ends.
pub const INCREMENT_ZONE_END: i32 = 174;

/// A part a spin picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// Hair.
    Hair,
    /// Eyes.
    Eyes,
    /// Nose.
    Nose,
    /// Mouth.
    Mouth,
    /// Skin.
    Skin,
    /// Headgear.
    Headgear,
    /// Shirt.
    Shirt,
    /// Trousers.
    Trousers,
    /// Footwear.
    Footwear,
}

/// The spins and the part each picks.
pub const SPINS: [(u32, Part); 9] = [
    (0x1000_03AF, Part::Hair),
    (0x1000_03B0, Part::Eyes),
    (0x1000_03B1, Part::Nose),
    (0x1000_03B2, Part::Mouth),
    (0x1000_03B3, Part::Skin),
    (0x1000_03B5, Part::Headgear),
    (0x1000_03B6, Part::Shirt),
    (0x1000_03B7, Part::Trousers),
    (0x1000_03B8, Part::Footwear),
];

/// Which choices show: the face's or the clothes'.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Hair, eyes, nose, mouth, skin.
    Face,
    /// Headgear, shirt, trousers, footwear.
    Clothes,
}

/// What the rotate and zoom buttons ask of the 3-D preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewRequest {
    /// Turn the figure clockwise.
    RotateClockwise,
    /// Turn it counter-clockwise.
    RotateCounterClockwise,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
}

/// The step a spin takes from `current` through `count` styles by
/// `delta`: plain styles wrap around (from none, forward is the first and
/// back the last); headgear's ring includes "none" after the last style.
#[must_use]
pub fn cycle_index(current: u32, delta: i32, count: i32, allow_unset: bool) -> u32 {
    if count <= 0 {
        return UNSET;
    }
    if allow_unset {
        let cur = if current == UNSET {
            count
        } else {
            current as i32
        };
        let next = (cur + delta).rem_euclid(count + 1);
        return if next == count { UNSET } else { next as u32 };
    }
    if current == UNSET {
        let from_unset = -1 + delta;
        if from_unset < 0 {
            return (count - 1) as u32;
        }
        if from_unset >= count {
            return 0;
        }
        return from_unset as u32;
    }
    (current as i32 + delta).rem_euclid(count) as u32
}

/// A part's style slot (skin has none).
#[must_use]
pub fn style_slot(part: Part) -> Option<AppearanceSlot> {
    Some(match part {
        Part::Hair => AppearanceSlot::HairStyle,
        Part::Eyes => AppearanceSlot::EyesStrip,
        Part::Nose => AppearanceSlot::NoseStrip,
        Part::Mouth => AppearanceSlot::MouthStrip,
        Part::Headgear => AppearanceSlot::HeadgearStyle,
        Part::Shirt => AppearanceSlot::ShirtStyle,
        Part::Trousers => AppearanceSlot::TrousersStyle,
        Part::Footwear => AppearanceSlot::FootwearStyle,
        Part::Skin => return None,
    })
}

/// A part's colour slot (nose, mouth and skin have none).
#[must_use]
pub fn color_slot(part: Part) -> Option<AppearanceSlot> {
    Some(match part {
        Part::Hair => AppearanceSlot::HairColor,
        Part::Eyes => AppearanceSlot::EyeColor,
        Part::Headgear => AppearanceSlot::HeadgearColor,
        Part::Shirt => AppearanceSlot::ShirtColor,
        Part::Trousers => AppearanceSlot::TrousersColor,
        Part::Footwear => AppearanceSlot::FootwearColor,
        _ => return None,
    })
}

/// A part's shade slider (nose and mouth shade the skin; eyes have none).
#[must_use]
pub fn shade_slot(part: Part) -> Option<ShadeSlot> {
    Some(match part {
        Part::Hair => ShadeSlot::Hair,
        Part::Nose | Part::Mouth | Part::Skin => ShadeSlot::Skin,
        Part::Headgear => ShadeSlot::Headgear,
        Part::Shirt => ShadeSlot::Shirt,
        Part::Trousers => ShadeSlot::Trousers,
        Part::Footwear => ShadeSlot::Footwear,
        Part::Eyes => return None,
    })
}

fn style_count(part: Part, g: &ChargenGenderOptions) -> i32 {
    let n = match part {
        Part::Hair => g.hair_styles.len(),
        Part::Eyes => g.eye_strips.len(),
        Part::Nose => g.nose_strips.len(),
        Part::Mouth => g.mouth_strips.len(),
        Part::Headgear => g.headgears.len(),
        Part::Shirt => g.shirts.len(),
        Part::Trousers => g.pants.len(),
        Part::Footwear => g.footwear.len(),
        Part::Skin => 0,
    };
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn color_count(part: Part, g: &ChargenGenderOptions) -> usize {
    match part {
        Part::Hair => g.hair_colors.len(),
        Part::Eyes => g.eye_colors.len(),
        Part::Headgear | Part::Shirt | Part::Trousers | Part::Footwear => g.clothing_colors.len(),
        _ => 0,
    }
}

fn shade_of(slot: ShadeSlot, a: &CreationAppearance) -> f64 {
    match slot {
        ShadeSlot::Skin => a.skin_shade,
        ShadeSlot::Hair => a.hair_shade,
        ShadeSlot::Headgear => a.headgear_shade,
        ShadeSlot::Shirt => a.shirt_shade,
        ShadeSlot::Trousers => a.trousers_shade,
        ShadeSlot::Footwear => a.footwear_shade,
    }
}

/// Gearknights and the Olthoi choose no clothes, nose or mouth.
#[must_use]
pub fn clothes_hidden(heritage: u32) -> bool {
    heritage == H::GEARKNIGHT.0 || heritage == H::OLTHOI.0 || heritage == H::OLTHOI_ACID.0
}

/// The snapshot's gender options.
#[must_use]
pub fn gender<'a>(ctx: &'a PageContext<'_>) -> Option<&'a ChargenGenderOptions> {
    let s = ctx.snapshot;
    if s.heritage_id == 0 || s.gender_key == 0 {
        return None;
    }
    ctx.options
        .try_get_heritage(s.heritage_id)?
        .genders_by_key
        .get(&i32::try_from(s.gender_key).ok()?)
}

/// Where swatch colours come from (the dats' palette sets, palettes and
/// clothing tables).
pub trait SwatchSources:
    ChargenPalSetSource + ChargenClothingTableSource + ChargenPaletteColorSource
{
}

impl<T: ChargenPalSetSource + ChargenClothingTableSource + ChargenPaletteColorSource> SwatchSources
    for T
{
}

/// The art the swatches and the disc draw with, made from the dats'
/// colour-spot art.
pub trait SwatchTextures {
    /// The spot of a colour the part does not offer.
    fn blank_spot(&mut self) -> Option<TextureRef>;
    /// The gradient disc.
    fn grad_disk(&mut self) -> Option<TextureRef>;
    /// The eyes' plug.
    fn grad_plug(&mut self) -> Option<TextureRef>;
    /// The spot painted in `rgb`.
    fn active_spot(&mut self, rgb: ChargenSwatchRgb) -> Option<TextureRef>;
}

/// The swatch colours of `part`: the hair colours' palette sets, the eye
/// colours' palettes, one skin colour for nose, mouth and skin, the
/// garment's dyes for clothing (none for no garment).
#[must_use]
pub fn swatch_colors(
    part: Part,
    gender: &ChargenGenderOptions,
    appearance: &CreationAppearance,
    sources: &dyn SwatchSourcesDyn,
) -> [Option<ChargenSwatchRgb>; 9] {
    let mut result = [None; 9];
    match part {
        Part::Hair => {
            for (slot, &id) in result.iter_mut().zip(&gender.hair_colors) {
                *slot = sources.average(id, HAIR_SAMPLE_INDEX);
            }
        }
        Part::Eyes => {
            for (slot, &id) in result.iter_mut().zip(&gender.eye_colors) {
                *slot = sources.direct(id, EYE_SAMPLE_INDEX);
            }
        }
        Part::Nose | Part::Mouth | Part::Skin => {
            result[0] = sources.average(gender.skin_pal_set_id, SKIN_FAMILY_SAMPLE_INDEX);
        }
        Part::Headgear => clothing(
            &mut result,
            gender,
            &gender.headgears,
            appearance.headgear_style,
            sources,
        ),
        Part::Shirt => clothing(
            &mut result,
            gender,
            &gender.shirts,
            appearance.shirt_style,
            sources,
        ),
        Part::Trousers => clothing(
            &mut result,
            gender,
            &gender.pants,
            appearance.trousers_style,
            sources,
        ),
        Part::Footwear => clothing(
            &mut result,
            gender,
            &gender.footwear,
            appearance.footwear_style,
            sources,
        ),
    }
    result
}

fn clothing(
    result: &mut [Option<ChargenSwatchRgb>; 9],
    gender: &ChargenGenderOptions,
    gear: &[ChargenGearOption],
    style: u32,
    sources: &dyn SwatchSourcesDyn,
) {
    if style == UNSET {
        return;
    }
    let Some(garment) = gear.get(style as usize) else {
        return;
    };
    for (slot, &color) in result.iter_mut().zip(&gender.clothing_colors) {
        if let Some(pal_set) = sources.clothing_pal_set(garment.clothing_table_id, color) {
            *slot = sources.average(pal_set, CLOTHING_SAMPLE_INDEX);
        }
    }
}

/// [`SwatchSources`] as an object.
pub trait SwatchSourcesDyn {
    /// A palette set's average colour.
    fn average(&self, pal_set: u32, sample: i32) -> Option<ChargenSwatchRgb>;
    /// One colour of a palette.
    fn direct(&self, palette: u32, sample: i32) -> Option<ChargenSwatchRgb>;
    /// The palette set a garment's dye shows.
    fn clothing_pal_set(&self, clothing_table: u32, template: u32) -> Option<u32>;
}

impl<T: SwatchSources> SwatchSourcesDyn for T {
    fn average(&self, pal_set: u32, sample: i32) -> Option<ChargenSwatchRgb> {
        pal_set_average_color(self, self, pal_set, sample)
    }

    fn direct(&self, palette: u32, sample: i32) -> Option<ChargenSwatchRgb> {
        direct_color(self, palette, sample)
    }

    fn clothing_pal_set(&self, clothing_table: u32, template: u32) -> Option<u32> {
        clothing_swatch_pal_set_id(self, clothing_table, template)
    }
}

fn tint(rgb: ChargenSwatchRgb) -> Color {
    Color::rgba(
        f32::from(rgb.r) / 255.0,
        f32::from(rgb.g) / 255.0,
        f32::from(rgb.b) / 255.0,
        1.0,
    )
}

/// The appearance page. See the module documentation.
#[derive(Debug, Clone)]
pub struct AppearancePage {
    female: Option<ElementId>,
    male: Option<ElementId>,
    face: Option<ElementId>,
    clothes: Option<ElementId>,
    face_choices: Option<ElementId>,
    clothes_choices: Option<ElementId>,
    spins: Vec<(ElementId, Part)>,
    swatches: [Option<ElementId>; 9],
    overlays: [Option<ElementId>; 9],
    shade: Option<ElementId>,
    grad: Option<ElementId>,
    rotate_clockwise: Option<ElementId>,
    rotate_counter_clockwise: Option<ElementId>,
    zoom_in: Option<ElementId>,
    zoom_out: Option<ElementId>,
    viewport: Option<ElementId>,
    choice: Choice,
    part: Part,
    eyes_arrows_disabled: bool,
}

impl AppearancePage {
    /// Bind the page under `root`.
    pub fn bind(ui: &mut UiRoot, root: ElementId) -> Self {
        let button = |ui: &mut UiRoot, id| {
            let b = ui
                .find_descendant(root, id)
                .filter(|b| ui.widget::<Button>(*b).is_some())?;
            if let Some(w) = ui.widget_mut::<Button>(b) {
                w.listeners.click = true;
            }
            Some(b)
        };
        let mut spins = Vec::new();
        for (id, part) in SPINS {
            if let Some(b) = button(ui, id) {
                spins.push((b, part));
            }
        }
        let mut swatches = [None; 9];
        for (slot, id) in swatches.iter_mut().zip(SWATCH_IDS) {
            *slot = button(ui, id);
        }
        let mut overlays = [None; 9];
        for (slot, id) in overlays.iter_mut().zip(SWATCH_OVERLAY_IDS) {
            *slot = ui.find_descendant(root, id);
        }
        let help = ui
            .find_descendant(root, HELP_TEXT_ID)
            .filter(|t| ui.widget::<Text>(*t).is_some());
        if let Some(h) = help {
            if let Some(t) = ui.widget_mut::<Text>(h) {
                t.preserve_end_on_layout = false;
            }
            if let Some(bar) = ui.find_descendant(h, HELP_SCROLL_ID)
                && let Some(s) = ui.widget_mut::<Scrollbar>(bar)
            {
                s.model = ScrollSource::Element(h);
            }
        }
        let page = Self {
            female: button(ui, FEMALE_ID),
            male: button(ui, MALE_ID),
            face: button(ui, FACE_ID),
            clothes: button(ui, CLOTHES_ID),
            face_choices: ui.find_descendant(root, FACE_CHOICES_ID),
            clothes_choices: ui.find_descendant(root, CLOTHES_CHOICES_ID),
            spins,
            swatches,
            overlays,
            shade: ui
                .find_descendant(root, SHADE_SCROLL_ID)
                .filter(|s| ui.widget::<Scrollbar>(*s).is_some()),
            grad: ui
                .find_descendant(root, GRAD_CIRCLE_ID)
                .filter(|g| ui.widget::<DatElement>(*g).is_some()),
            rotate_clockwise: button(ui, ROTATE_CLOCKWISE_ID),
            rotate_counter_clockwise: button(ui, ROTATE_COUNTER_CLOCKWISE_ID),
            zoom_in: button(ui, ZOOM_IN_ID),
            zoom_out: button(ui, ZOOM_OUT_ID),
            viewport: ui.find_descendant(root, VIEWPORT_ID),
            choice: Choice::Face,
            part: Part::Hair,
            eyes_arrows_disabled: false,
        };
        page.apply_choice_visibility(ui);
        page
    }

    /// The part picked.
    #[must_use]
    pub fn part(&self) -> Part {
        self.part
    }

    /// The choices shown.
    #[must_use]
    pub fn choice(&self) -> Choice {
        self.choice
    }

    /// A spin's button.
    #[must_use]
    pub fn spin(&self, part: Part) -> Option<ElementId> {
        self.spins.iter().find(|(_, p)| *p == part).map(|(b, _)| *b)
    }

    /// The swatches and their marks.
    #[must_use]
    pub fn swatches(&self) -> ([Option<ElementId>; 9], [Option<ElementId>; 9]) {
        (self.swatches, self.overlays)
    }

    /// The 3-D preview's element.
    #[must_use]
    pub fn viewport(&self) -> Option<ElementId> {
        self.viewport
    }

    /// The gradient disc and the shade slider.
    #[must_use]
    pub fn grad_and_shade(&self) -> (Option<ElementId>, Option<ElementId>) {
        (self.grad, self.shade)
    }

    /// Show the snapshot: gender, which parts the heritage offers, the
    /// spins' captions, the swatches and the shade.
    pub fn refresh(
        &mut self,
        ui: &mut UiRoot,
        ctx: &PageContext<'_>,
        sources: Option<&dyn SwatchSourcesDyn>,
        textures: Option<&mut dyn SwatchTextures>,
    ) {
        let s = ctx.snapshot;
        for (b, key) in [(self.female, 2), (self.male, 1)] {
            if let Some(b) = b {
                ui.with::<Button, _>(b, |w, ui| w.set_selected(ui, b, s.gender_key == key));
            }
        }
        let hidden = clothes_hidden(s.heritage_id);
        if let Some(c) = self.clothes {
            ui.set_visible(c, !hidden);
        }
        for part in [Part::Nose, Part::Mouth] {
            if let Some(spin) = self.spin(part) {
                ui.set_visible(spin, !hidden);
            }
        }
        self.eyes_arrows_disabled = hidden;
        if hidden {
            self.choice = Choice::Face;
            self.part = Part::Hair;
        }
        self.apply_choice_visibility(ui);
        let (hair, eyes, skin) = match H(s.heritage_id) {
            H::GEARKNIGHT => (
                "ID_CharGen_GearText_HairButton",
                "ID_CharGen_GearText_EyesButton",
                "ID_CharGen_GearText_SkinButton",
            ),
            H::OLTHOI | H::OLTHOI_ACID => (
                "ID_CharGen_OlthoiText_HairButton",
                "ID_CharGen_OlthoiText_EyesButton",
                "ID_CharGen_OlthoiText_SkinButton",
            ),
            _ => ("ID_CharGen_HairStyle", "ID_CharGen_Eyes", "ID_CharGen_Skin"),
        };
        for (part, key) in [(Part::Hair, hair), (Part::Eyes, eyes), (Part::Skin, skin)] {
            if let Some(spin) = self.spin(part)
                && let Some(text) = (ctx.text)(key)
                && let Some(b) = ui.widget_mut::<Button>(spin)
            {
                b.label = Some(text);
            }
        }
        self.refresh_colors(ui, ctx, sources, textures);
    }

    /// The swatches, marks, disc and shade slider for the picked part.
    pub fn refresh_colors(
        &self,
        ui: &mut UiRoot,
        ctx: &PageContext<'_>,
        sources: Option<&dyn SwatchSourcesDyn>,
        mut textures: Option<&mut dyn SwatchTextures>,
    ) {
        let a = &ctx.snapshot.appearance;
        for &(spin, part) in &self.spins {
            let state = if part == self.part {
                states::HIGHLIGHT
            } else {
                states::NORMAL
            };
            ui.set_retail_state(spin, state);
        }
        let slot = color_slot(self.part);
        let current = slot.map_or(UNSET, |s| a.get(s));
        for (i, overlay) in self.overlays.iter().enumerate() {
            if let Some(o) = overlay {
                ui.set_visible(*o, slot.is_some() && current == i as u32);
            }
        }
        let gender = gender(ctx);
        let count = match (slot, gender) {
            (Some(_), Some(g)) => color_count(self.part, g),
            _ => 0,
        };
        let shown = if slot.is_some() { count } else { 1 };
        let colors = match (gender, sources) {
            (Some(g), Some(src)) => swatch_colors(self.part, g, a, src),
            _ => [None; 9],
        };
        for (i, swatch) in self.swatches.iter().enumerate() {
            let Some(b) = *swatch else {
                continue;
            };
            ui.set_visible(b, true);
            let visible = i < shown;
            let rgb = if visible { colors[i] } else { None };
            let face = match (&mut textures, visible, rgb) {
                (Some(t), false, _) => t.blank_spot(),
                (Some(t), true, Some(c)) => t.active_spot(c),
                _ => None,
            };
            if let Some(w) = ui.widget_mut::<Button>(b) {
                w.tint = rgb.map_or(Color::WHITE, tint);
                w.color_key_face = face;
            }
        }
        if let Some(g) = self.grad {
            ui.set_visible(g, true);
            let eyes = self.part == Part::Eyes;
            let index = if eyes {
                None
            } else if slot.is_none() {
                Some(0)
            } else {
                Some(current as usize)
            };
            let color = index.and_then(|i| colors.get(i).copied().flatten());
            let texture = textures
                .as_mut()
                .and_then(|t| if eyes { t.grad_plug() } else { t.grad_disk() });
            if let Some(d) = ui.widget_mut::<DatElement>(g) {
                d.tint = color.map_or(Color::WHITE, tint);
                d.runtime_image = texture.map(Some);
            }
        }
        let Some(bar) = self.shade else {
            return;
        };
        let shade = shade_slot(self.part);
        ui.set_visible(bar, shade.is_some());
        if let Some(slot) = shade
            && let Some(s) = ui.widget_mut::<Scrollbar>(bar)
        {
            let value = shade_of(slot, a);
            s.set_scalar_position(if value < 0.0 {
                0.0
            } else {
                value.clamp(0.0, 1.0) as f32
            });
        }
    }

    fn apply_choice_visibility(&self, ui: &mut UiRoot) {
        if let Some(f) = self.face_choices {
            ui.set_visible(f, self.choice == Choice::Face);
        }
        if let Some(c) = self.clothes_choices {
            ui.set_visible(c, self.choice == Choice::Clothes);
        }
        for (b, choice) in [(self.face, Choice::Face), (self.clothes, Choice::Clothes)] {
            if let Some(b) = b {
                ui.with::<Button, _>(b, |w, ui| w.set_selected(ui, b, self.choice == choice));
            }
        }
    }

    /// What a message on the page asks, and the preview request it makes;
    /// `None` when it is not the page's. A message that only changes the
    /// page's own state answers with no command and wants the swatches
    /// redrawn (the caller does it with the latest snapshot).
    pub fn on_message(
        &mut self,
        ui: &mut UiRoot,
        element: ElementId,
        message: &ElementMessage,
        ctx: &PageContext<'_>,
    ) -> Option<(Vec<CreationCommand>, Option<PreviewRequest>)> {
        if let ElementMessage::ScalarChanged(scalar) = message {
            if Some(element) != self.shade {
                return None;
            }
            let commands = shade_slot(self.part)
                .map(|slot| CreationCommand::SetShade(slot, f64::from(*scalar)))
                .into_iter()
                .collect();
            return Some((commands, None));
        }
        let ElementMessage::Clicked { x, .. } = message else {
            return None;
        };
        let x = *x;
        if Some(element) == self.female {
            return Some((vec![CreationCommand::SelectGender(2)], None));
        }
        if Some(element) == self.male {
            return Some((vec![CreationCommand::SelectGender(1)], None));
        }
        if Some(element) == self.face || Some(element) == self.clothes {
            self.choice = if Some(element) == self.face {
                Choice::Face
            } else {
                Choice::Clothes
            };
            self.part = if self.choice == Choice::Face {
                Part::Hair
            } else {
                Part::Headgear
            };
            self.apply_choice_visibility(ui);
            return Some((Vec::new(), None));
        }
        if let Some(&(_, part)) = self.spins.iter().find(|(b, _)| *b == element) {
            return Some((self.spin_clicked(part, x, ctx), None));
        }
        if let Some(i) = self.swatches.iter().position(|s| *s == Some(element)) {
            let command = color_slot(self.part).and_then(|slot| {
                let g = gender(ctx)?;
                (i < color_count(self.part, g))
                    .then_some(CreationCommand::SetAppearance(slot, i as u32))
            });
            return Some((command.into_iter().collect(), None));
        }
        let preview = if Some(element) == self.rotate_clockwise {
            PreviewRequest::RotateClockwise
        } else if Some(element) == self.rotate_counter_clockwise {
            PreviewRequest::RotateCounterClockwise
        } else if Some(element) == self.zoom_in || Some(element) == self.zoom_out {
            let zoom_in = Some(element) == self.zoom_in;
            let (on, off) = if zoom_in {
                (self.zoom_in, self.zoom_out)
            } else {
                (self.zoom_out, self.zoom_in)
            };
            if let Some(b) = on {
                ui.set_retail_state(b, states::HIGHLIGHT);
            }
            if let Some(b) = off {
                ui.set_retail_state(b, states::NORMAL);
            }
            if zoom_in {
                PreviewRequest::ZoomIn
            } else {
                PreviewRequest::ZoomOut
            }
        } else {
            return None;
        };
        Some((Vec::new(), Some(preview)))
    }

    fn spin_clicked(&mut self, part: Part, x: i32, ctx: &PageContext<'_>) -> Vec<CreationCommand> {
        if part == Part::Skin {
            return self.select_part(part, ctx);
        }
        let zone = if (DECREMENT_ZONE_START..INCREMENT_ZONE_START).contains(&x) {
            -1
        } else if (INCREMENT_ZONE_START..INCREMENT_ZONE_END).contains(&x) {
            1
        } else {
            0
        };
        if zone == 0 || (part == Part::Eyes && self.eyes_arrows_disabled) {
            return self.select_part(part, ctx);
        }
        let Some(g) = gender(ctx) else {
            return Vec::new();
        };
        let Some(slot) = style_slot(part) else {
            return self.select_part(part, ctx);
        };
        let current = ctx.snapshot.appearance.get(slot);
        let next = cycle_index(current, zone, style_count(part, g), part == Part::Headgear);
        // The spin then picks its part (its index is set already).
        self.part = part;
        vec![CreationCommand::SetAppearance(slot, next)]
    }

    /// Pick `part`: a style outside the list (or none, for a part that
    /// needs one) is put back in range first.
    fn select_part(&mut self, part: Part, ctx: &PageContext<'_>) -> Vec<CreationCommand> {
        let mut commands = Vec::new();
        if let (Some(slot), Some(g)) = (style_slot(part), gender(ctx)) {
            let count = style_count(part, g);
            if count > 0 {
                let current = ctx.snapshot.appearance.get(slot);
                let normalized = if part == Part::Headgear {
                    (current != UNSET && current >= count as u32).then_some(UNSET)
                } else if current != UNSET && current >= count as u32 {
                    Some(0)
                } else if current == UNSET {
                    Some((count - 1) as u32)
                } else {
                    None
                };
                if let Some(n) = normalized {
                    commands.push(CreationCommand::SetAppearance(slot, n));
                }
            }
        }
        self.part = part;
        commands
    }

    /// The random button: the face or the clothes, as shown.
    #[must_use]
    pub fn randomize(&self) -> CreationCommand {
        match self.choice {
            Choice::Clothes => CreationCommand::RandomizeClothing,
            Choice::Face => CreationCommand::RandomizeAppearance,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spin_wraps_and_headgear_passes_through_none() {
        // OpenAC `CharacterCreationUiControllerTests.CycleIndex_MatchesTheReferenceWrap`.
        let cases = [
            (0, 1, 3, false, 1),
            (2, 1, 3, false, 0),
            (1, -1, 3, false, 0),
            (0, -1, 3, false, 2),
            (UNSET, 1, 3, false, 0),
            (UNSET, -1, 3, false, 2),
            (0, -1, 3, true, UNSET),
            (UNSET, 1, 3, true, 0),
            (2, 1, 3, true, UNSET),
            (UNSET, -1, 3, true, 2),
        ];
        for (current, delta, count, unset, expected) in cases {
            assert_eq!(
                cycle_index(current, delta, count, unset),
                expected,
                "{current} {delta} {count} {unset}"
            );
        }
        assert_eq!(cycle_index(0, 1, 0, false), UNSET);
    }

    #[test]
    fn nose_and_mouth_shade_the_skin_and_eyes_have_no_slider() {
        // OpenAC `AppearanceShadeScroll_ForNoseOrMouth_RoutesToSkinShade`.
        assert_eq!(shade_slot(Part::Nose), Some(ShadeSlot::Skin));
        assert_eq!(shade_slot(Part::Mouth), Some(ShadeSlot::Skin));
        assert_eq!(shade_slot(Part::Eyes), None);
        assert_eq!(color_slot(Part::Skin), None);
        assert_eq!(style_slot(Part::Skin), None);
    }

    #[test]
    fn gearknights_and_olthoi_hide_the_clothes() {
        assert!(clothes_hidden(H::GEARKNIGHT.0));
        assert!(clothes_hidden(H::OLTHOI.0));
        assert!(clothes_hidden(H::OLTHOI_ACID.0));
        assert!(!clothes_hidden(H::LUGIAN.0));
    }
}
