//! The outside-container window in the game window, as OpenAC's
//! `RetailUiRuntime.MountExternalContainer` mounts it
//! (`ExternalContainerController.CreateWindowOptions`), fed from the
//! session's open container and its range every frame one is open.

use ac_ui::font::DEFAULT_FONT_ID;
use ac_ui::geometry::ResizeEdges;
use ac_ui::import::{LayoutSource, import_rooted};
use ac_ui::panels::cell_template::empty_sprite;
use ac_ui::panels::external_container::{
    self, DEFAULT_CONTENT_WIDTH, ExternalContainerController, ExternalContainerFacts,
    ExternalContainerSprites, MINIMUM_CONTENT_WIDTH,
};
use ac_ui::panels::toolbar::ToolbarWindowController;
use ac_ui::widgets::NineSliceFrame;
use ac_ui::widgets::chrome::BORDER;

use super::mount::{Chrome, MountOptions, mount_window};
use super::window::{TOOLBAR_WINDOW, WindowUi};

/// The outside-container window's name.
pub const EXTERNAL_CONTAINER_WINDOW: &str = "external-container";

/// Where and how the window is mounted for a layout root at (`left`,
/// `top`) of `width` x `height` (OpenAC
/// `ExternalContainerController.CreateWindowOptions`).
#[must_use]
pub fn window_options(left: f32, top: f32, width: f32, height: f32) -> MountOptions {
    MountOptions {
        chrome: Chrome::NineSlice,
        left,
        top,
        content_width: Some(width.min(DEFAULT_CONTENT_WIDTH)),
        content_height: Some(height),
        min_width: Some(MINIMUM_CONTENT_WIDTH + 2.0 * BORDER as f32),
        visible: false,
        draggable: true,
        resizable: true,
        resize_x: true,
        resize_y: false,
        resizable_edges: ResizeEdges::LEFT | ResizeEdges::RIGHT,
        constrain_resize_to_parent: true,
        ..MountOptions::default()
    }
}

impl WindowUi {
    /// OpenAC `RetailUiRuntime.MountExternalContainer`.
    pub(super) fn mount_external_container(&mut self) {
        match self.mount_external_container_window() {
            Ok(()) => self.mounted.push(EXTERNAL_CONTAINER_WINDOW),
            Err(e) => self
                .notices
                .push(format!("ui: {EXTERNAL_CONTAINER_WINDOW}: {e}")),
        }
    }

    fn mount_external_container_window(&mut self) -> Result<(), String> {
        let sprites = ExternalContainerSprites {
            contents_empty: empty_sprite(
                &self.content,
                external_container::LAYOUT_ID,
                external_container::CONTENTS_LIST_ID,
            ),
            container_empty: empty_sprite(
                &self.content,
                external_container::LAYOUT_ID,
                external_container::CONTAINER_LIST_ID,
            ),
        };
        let font = self.content.font(DEFAULT_FONT_ID);
        let layout = import_rooted(
            &mut self.host.root,
            &self.content,
            external_container::LAYOUT_ID,
            external_container::ROOT_ID,
            font,
        )
        .ok_or("layout 0x21000008 root 0x10000063 is not in the dats")?;
        let ui = &mut self.host.root;
        let (left, top, width, height) = ui
            .element(layout.root)
            .map_or((0.0, 0.0, 0.0, 0.0), |e| (e.left, e.top, e.width, e.height));
        let frame = mount_window(
            ui,
            EXTERNAL_CONTAINER_WINDOW,
            layout.root,
            &window_options(left, top, width, height),
        )?;
        if let Some(f) = ui.widget_mut::<NineSliceFrame>(frame) {
            f.draw_center_fill = false;
        }
        let controller =
            ExternalContainerController::bind(ui, &layout, EXTERNAL_CONTAINER_WINDOW, sprites)
                .ok_or("required lists or the close button are missing")?;
        self.host
            .attach_controller(EXTERNAL_CONTAINER_WINDOW, Box::new(controller))
            .map_err(|e| e.to_string())
    }

    /// Feed the outside-container window while the session has one open
    /// (or the window still shows the last one).
    pub(super) fn update_external_container(&mut self) {
        let selected = self.player_view.and_then(|v| v.selected);
        let Self {
            host, facts, icons, ..
        } = self;
        let external = ExternalContainerFacts {
            container: facts.state.external_container,
            in_range: facts.state.external_container_in_range,
        };
        let shown = host
            .with_controller::<ExternalContainerController, _>(EXTERNAL_CONTAINER_WINDOW, |c, _| {
                c.open_container() != 0
            })
            .unwrap_or(false);
        if external.container == 0 && !shown {
            return;
        }
        let split = host.with_controller::<ToolbarWindowController, _>(TOOLBAR_WINDOW, |c, _| {
            c.split.quantity().clone()
        });
        let pending: Vec<u32> = facts.state.pending_item.into_iter().collect();
        let view = facts.items_view(selected, &pending, split.as_ref());
        host.with_controller::<ExternalContainerController, _>(
            EXTERNAL_CONTAINER_WINDOW,
            |c, ui| {
                c.update(ui, &view, external, icons);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ac_ui::root::UiRoot;
    use ac_ui::tree::Element;
    use ac_ui::widgets::Panel;

    /// OpenAC `ExternalContainerControllerTests.WindowPolicy_AddsOuterFrameAndAllowsHorizontalResize`.
    #[test]
    fn the_window_adds_its_frame_and_resizes_only_sideways() {
        let mut ui = UiRoot::new();
        ui.set_size(1280.0, 720.0);
        let r = ui.root();
        let root = ui.add(r, Element::at(0.0, 400.0, 800.0, 110.0), Panel::default());
        let frame = mount_window(
            &mut ui,
            EXTERNAL_CONTAINER_WINDOW,
            root,
            &window_options(0.0, 400.0, 800.0, 110.0),
        )
        .unwrap();
        let f = ui.element(frame).unwrap();
        assert_eq!((f.width, f.height), (710.0, 120.0));
        assert_eq!(f.min_width, 170.0);
        assert!(f.draggable && f.resizable && f.resize_x && !f.resize_y);
        assert_eq!(f.resizable_edges, ResizeEdges::LEFT | ResizeEdges::RIGHT);
        assert!(f.constrain_resize_to_parent);
        assert_eq!(ui.element(root).unwrap().width, 700.0);
        assert!(!ui.is_window_visible(EXTERNAL_CONTAINER_WINDOW));
    }
}
