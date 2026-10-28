//! Mounting the indicator bar and the panels it opens (the positive and
//! negative effects, link status, vitae and mini-game panels) in the game
//! window, as OpenAC's `RetailUiRuntime` mounts them (`MountEffects`,
//! `MountIndicatorDetailPanels`, `MountIndicators`), and feeding them each
//! frame from the window's copy of the session's facts.

use ac_runtime::session::LinkStatus;
use ac_ui::font::DEFAULT_FONT_ID;
use ac_ui::geometry::{AnchorEdges, ResizeEdges};
use ac_ui::import::{LayoutSource, import, import_infos_rooted, import_rooted};
use ac_ui::panels::indicators::{
    self as ind, EffectRowTemplate, EffectsController, EffectsView, EffectsWindowController,
    IndicatorBarController, IndicatorsView, LinkStatusController, LinkStatusStrings,
    MiniGameController, VitaeController, VitaeStrings,
};
use ac_ui::panels::main_panels::RESTORE_PREVIOUS_PROPERTY;
use ac_ui::panels::{DatRowTemplates, panel_id};
use ac_ui::strings::compute_hash;
use ac_ui::widgets::NineSliceFrame;

use super::mount::{Chrome, MountOptions, mount_window};
use super::social_panels::authors_full_panel_center;
use super::window::{UI_STRING_TABLE, WindowUi};

/// The indicator bar's window.
pub const INDICATORS_WINDOW: &str = "indicators";
/// The positive effects panel's window.
pub const POSITIVE_EFFECTS_WINDOW: &str = "effects-positive";
/// The negative effects panel's window.
pub const NEGATIVE_EFFECTS_WINDOW: &str = "effects-negative";
/// The link-status panel's window.
pub const LINK_STATUS_WINDOW: &str = "link-status";
/// The vitae panel's window.
pub const VITAE_WINDOW: &str = "vitae";
/// The mini-game panel's window.
pub const MINI_GAME_WINDOW: &str = "mini-game";

impl WindowUi {
    /// Mount the effects panels, the detail panels and the indicator bar
    /// (OpenAC's order); a panel whose layout is missing is named in the
    /// notices.
    pub(super) fn mount_indicator_panels(&mut self) {
        let results = [
            (POSITIVE_EFFECTS_WINDOW, self.mount_effects(true)),
            (NEGATIVE_EFFECTS_WINDOW, self.mount_effects(false)),
            (LINK_STATUS_WINDOW, self.mount_link_status()),
            (VITAE_WINDOW, self.mount_vitae()),
            (MINI_GAME_WINDOW, self.mount_mini_game()),
            (INDICATORS_WINDOW, self.mount_indicator_bar()),
        ];
        for (name, result) in results {
            match result {
                Ok(()) => self.mounted.push(name),
                Err(e) => self.notices.push(format!("ui: {name}: {e}")),
            }
        }
    }

    fn ui_string(&self, key: &str) -> Option<String> {
        self.content
            .strings()
            .resolve(UI_STRING_TABLE, compute_hash(key), 0)
    }

    /// A detail panel: the shared nine-piece frame at its place, hidden,
    /// resizable at the bottom, registered as main panel `panel`
    /// (OpenAC `RegisterIndicatorDetailPanel`, `MountEffectsInstance`).
    fn mount_detail_panel(
        &mut self,
        name: &str,
        layout_id: u32,
        root_id: u32,
        left: f32,
        panel: u32,
    ) -> Result<ac_ui::import::ImportedLayout, String> {
        let missing =
            || format!("layout {layout_id:#010x} root {root_id:#010x} is not in the dats");
        let info = import_infos_rooted(&self.content, layout_id, root_id).ok_or_else(missing)?;
        let font = self.content.font(DEFAULT_FONT_ID);
        let layout = import_rooted(&mut self.host.root, &self.content, layout_id, root_id, font)
            .ok_or_else(missing)?;
        let ui = &mut self.host.root;
        let frame = mount_window(
            ui,
            name,
            layout.root,
            &MountOptions {
                chrome: Chrome::NineSlice,
                left,
                top: 18.0,
                visible: false,
                resize_x: false,
                resize_y: true,
                resizable_edges: ResizeEdges::BOTTOM,
                constrain_resize_to_parent: true,
                content_anchors: AnchorEdges::ALL,
                content_click_through: Some(false),
                ..MountOptions::default()
            },
        )?;
        if let Some(f) = ui.widget_mut::<NineSliceFrame>(frame) {
            f.draw_center_fill = !authors_full_panel_center(&info);
        }
        let restore = info.effective_bool(RESTORE_PREVIOUS_PROPERTY, None) == Some(true);
        self.panels
            .register(&mut self.host.root, panel, name, restore);
        Ok(layout)
    }

    /// OpenAC `RetailUiRuntime.MountEffectsInstance`.
    fn mount_effects(&mut self, positive: bool) -> Result<(), String> {
        let (name, root_id, panel) = if positive {
            (
                POSITIVE_EFFECTS_WINDOW,
                ind::POSITIVE_ROOT_ID,
                panel_id::POSITIVE_EFFECTS,
            )
        } else {
            (
                NEGATIVE_EFFECTS_WINDOW,
                ind::NEGATIVE_ROOT_ID,
                panel_id::NEGATIVE_EFFECTS,
            )
        };
        let row = import_infos_rooted(&self.content, ind::EFFECTS_LAYOUT_ID, ind::ROW_TEMPLATE_ID)
            .ok_or("the effect row template is not in the dats")?;
        let template = EffectRowTemplate {
            layout: ind::EFFECTS_LAYOUT_ID,
            element: ind::ROW_TEMPLATE_ID,
            width: row.width,
            height: row.height,
        };
        let prompt = self
            .ui_string("ID_Effects_Info_SelectASpell")
            .unwrap_or_else(|| "SELECT A SPELL".to_owned());
        let screen_width = self.host.root.size().0;
        let width = import_infos_rooted(&self.content, ind::EFFECTS_LAYOUT_ID, root_id)
            .map_or(0.0, |i| i.width);
        let layout = self.mount_detail_panel(
            name,
            ind::EFFECTS_LAYOUT_ID,
            root_id,
            (screen_width - width - 12.0).max(0.0),
            panel,
        )?;
        let effects =
            EffectsController::bind(&mut self.host.root, &layout, positive, template, prompt)
                .ok_or("the effect list is missing")?;
        self.host
            .attach_controller(name, Box::new(EffectsWindowController::new(effects, panel)))
            .map_err(|e| e.to_string())
    }

    /// OpenAC `RetailUiRuntime.MountLinkStatusPanel`.
    fn mount_link_status(&mut self) -> Result<(), String> {
        let strings = self.content.strings();
        let english = LinkStatusStrings::english();
        let s = |hash: u32, fallback: String| {
            strings
                .resolve(UI_STRING_TABLE, hash, 0)
                .unwrap_or(fallback)
        };
        let link_strings = LinkStatusStrings {
            description: s(0x0D63_2F7F, english.description),
            legend: s(0x038F_ABF3, english.legend),
            disconnect_warning: s(0x0548_1084, english.disconnect_warning),
            packet_loss_prefix: s(0x0CAD_BA93, english.packet_loss_prefix),
            ping_prefix: s(0x0D64_85F7, english.ping_prefix),
        };
        let layout = self.mount_detail_panel(
            LINK_STATUS_WINDOW,
            ind::LINK_LAYOUT_ID,
            ind::LINK_ROOT_ID,
            18.0,
            panel_id::LINK_STATUS,
        )?;
        let c = LinkStatusController::bind(&mut self.host.root, &layout, link_strings)
            .ok_or("the link-status text is missing")?;
        self.host
            .attach_controller(LINK_STATUS_WINDOW, Box::new(c))
            .map_err(|e| e.to_string())
    }

    /// OpenAC `RetailUiRuntime.MountVitaePanel`.
    fn mount_vitae(&mut self) -> Result<(), String> {
        let strings = self.content.strings();
        let english = VitaeStrings::english();
        let s = |key: &str, token: i32, fallback: String| {
            strings
                .resolve(UI_STRING_TABLE, compute_hash(key), token)
                .unwrap_or(fallback)
        };
        let vitae_strings = VitaeStrings {
            full_strength: s("ID_Vitae_Text_Full", 0, english.full_strength),
            lost_prefix: s("ID_Vitae_Text_Vitae", 0, english.lost_prefix),
            lost_suffix: s("ID_Vitae_Text_Vitae", 1, english.lost_suffix),
            skills_prefix: s("ID_Vitae_Text_Skills", 0, english.skills_prefix),
            skills_suffix: s("ID_Vitae_Text_Skills", 1, english.skills_suffix),
            recovery_prefix: s("ID_Vitae_Text_Experience", 0, english.recovery_prefix),
            recovery_suffix: s("ID_Vitae_Text_Experience", 1, english.recovery_suffix),
        };
        let layout = self.mount_detail_panel(
            VITAE_WINDOW,
            ind::VITAE_LAYOUT_ID,
            ind::VITAE_ROOT_ID,
            18.0,
            panel_id::VITAE,
        )?;
        let c = VitaeController::bind(&mut self.host.root, &layout, vitae_strings)
            .ok_or("the vitae text is missing")?;
        self.host
            .attach_controller(VITAE_WINDOW, Box::new(c))
            .map_err(|e| e.to_string())
    }

    /// OpenAC `RetailUiRuntime.MountMiniGamePanel`.
    fn mount_mini_game(&mut self) -> Result<(), String> {
        let layout = self.mount_detail_panel(
            MINI_GAME_WINDOW,
            ind::MINI_GAME_LAYOUT_ID,
            ind::MINI_GAME_ROOT_ID,
            18.0,
            panel_id::MINI_GAME,
        )?;
        let c = MiniGameController::bind(&mut self.host.root, &layout);
        self.host
            .attach_controller(MINI_GAME_WINDOW, Box::new(c))
            .map_err(|e| e.to_string())
    }

    /// OpenAC `RetailUiRuntime.MountIndicators`.
    fn mount_indicator_bar(&mut self) -> Result<(), String> {
        let font = self.content.font(DEFAULT_FONT_ID);
        let layout = import(&mut self.host.root, &self.content, ind::BAR_LAYOUT_ID, font)
            .ok_or("layout 0x21000071 is not in the dats")?;
        let message = self
            .ui_string("ID_Client_EndCharacterSessionConfirm")
            .unwrap_or_else(|| "Are you sure you want to end this character session?".to_owned());
        let now = self.status.clock.elapsed().as_secs_f64();
        let bar = IndicatorBarController::bind(&mut self.host.root, &layout, message, now)
            .ok_or("one or more indicator buttons are missing")?;
        if let Some(infos) = ac_ui::import::import_infos(&self.content, ind::BAR_LAYOUT_ID) {
            let ctx = ac_ui::import::BuildContext {
                source: &self.content,
                default_font: self.content.font(DEFAULT_FONT_ID),
                source_layout_did: ind::BAR_LAYOUT_ID,
                media_seconds: self.host.root.media_seconds(),
            };
            bar.attach_press_highlights(&mut self.host.root, &infos, &ctx);
        }
        // OpenAC's mount takes the window's own size as its limits.
        let (w, h) = self
            .host
            .root
            .element(layout.root)
            .map_or((0.0, 0.0), |e| (e.width, e.height));
        mount_window(
            &mut self.host.root,
            INDICATORS_WINDOW,
            layout.root,
            &MountOptions {
                left: 10.0,
                top: 96.0,
                min_width: Some(w),
                min_height: Some(h),
                resizable: false,
                resize_x: false,
                resize_y: false,
                content_click_through: Some(false),
                ..MountOptions::default()
            },
        )?;
        self.host
            .attach_controller(INDICATORS_WINDOW, Box::new(bar))
            .map_err(|e| e.to_string())
    }

    /// One frame of the indicator bar and its panels.
    pub(super) fn update_indicator_panels(&mut self, link: LinkStatus) {
        let now = self.status.clock.elapsed().as_secs_f64();
        let session_now = self.facts.session_now().unwrap_or(0.0);
        let show_duration = self.player_view.is_some_and(|v| {
            ac_runtime::character::option(ac_runtime::character::option_id::SPELL_DURATION)
                .is_some_and(|e| {
                    let word = if e.in_options1 {
                        v.options1
                    } else {
                        v.options2
                    };
                    word & e.mask != 0
                })
        });
        let Self {
            host,
            facts,
            icons,
            content,
            ..
        } = self;
        let book = facts.numbers().and_then(|n| n.spellbook());
        let strength = facts.strength();
        let player = facts.state.player;
        let view = IndicatorsView {
            spellbook: book,
            objects: &facts.objects,
            player,
            strength,
            link,
            now,
        };
        host.with_controller::<IndicatorBarController, _>(INDICATORS_WINDOW, |c, ui| {
            c.update(ui, &view);
        });
        host.with_controller::<LinkStatusController, _>(LINK_STATUS_WINDOW, |c, ui| {
            c.update(ui, &link, now);
        });
        if host.root.is_window_visible(VITAE_WINDOW) {
            host.with_controller::<VitaeController, _>(VITAE_WINDOW, |c, ui| {
                c.update(ui, book, &facts.objects, player);
            });
        }
        let Some(book) = book else {
            return;
        };
        for name in [POSITIVE_EFFECTS_WINDOW, NEGATIVE_EFFECTS_WINDOW] {
            if !host.root.is_window_visible(name) {
                continue;
            }
            let mut rows = DatRowTemplates::new(&*content, content.font(DEFAULT_FONT_ID));
            let view = EffectsView {
                spellbook: book,
                now: session_now,
                show_duration,
            };
            host.with_controller::<EffectsWindowController, _>(name, |c, ui| {
                c.update(ui, &view, &mut rows, icons);
            });
        }
    }
}
