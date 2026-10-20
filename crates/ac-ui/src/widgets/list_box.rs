//! The list box: rows stacked in a clipped viewport that scrolls a pixel at
//! a time. Rows are built from the list's authored row templates (dat
//! property 0x64) by whoever fills the list, then added here.

use std::collections::HashMap;
use std::sync::Arc;

use crate::draw::RenderContext;
use crate::element_info::{ElementInfo, TemplateEntry};
use crate::event::{UiEvent, kind};
use crate::geometry::AnchorEdges;
use crate::root::UiRoot;
use crate::text::Scrollable;
use crate::tree::{Element, ElementId, Widget};
use crate::widgets::DatElement;

/// Default row step for wheel scrolling, pixels.
pub const DEFAULT_LINE_HEIGHT: i32 = 16;

/// A clipped panel whose children are rows at fixed content offsets,
/// shifted by the scroll offset; rows wholly outside the view are hidden.
#[derive(Debug, Clone, Default)]
pub struct ScrollablePanel {
    /// The scroll model.
    pub scroll: Scrollable,
    /// Wheel step, pixels.
    pub line_height: i32,
    base_tops: HashMap<ElementId, f32>,
    content_height: i32,
}

impl ScrollablePanel {
    /// An empty panel with the default wheel step.
    #[must_use]
    pub fn new() -> Self {
        Self {
            line_height: DEFAULT_LINE_HEIGHT,
            ..Self::default()
        }
    }

    /// Height of the rows, pixels.
    #[must_use]
    pub fn content_height(&self) -> i32 {
        self.content_height
    }

    /// Add `row` at content offset `top` as the panel's last child.
    pub fn add_row(&mut self, ui: &mut UiRoot, me: ElementId, row: ElementId, top: f32) {
        if let Some(e) = ui.element_mut(row) {
            e.top = top;
        }
        ui.add_child(me, row);
        ui.set_anchors(row, AnchorEdges::NONE);
        self.base_tops.insert(row, top);
        self.recompute_content_height(ui);
    }

    /// Detach and free `row`.
    pub fn remove_row(&mut self, ui: &mut UiRoot, row: ElementId) {
        if self.base_tops.remove(&row).is_some() {
            ui.destroy(row);
            self.recompute_content_height(ui);
        }
    }

    /// Remove every row and scroll to the top.
    pub fn clear_content(&mut self, ui: &mut UiRoot, me: ElementId) {
        for child in ui.children(me) {
            ui.destroy(child);
        }
        self.base_tops.clear();
        self.content_height = 0;
        self.scroll.set_scroll_y(0);
    }

    fn recompute_content_height(&mut self, ui: &UiRoot) {
        let mut bottom: f32 = 0.0;
        for (row, top) in &self.base_tops {
            if let Some(e) = ui.element(*row) {
                bottom = bottom.max(top + e.height);
            }
        }
        self.content_height = bottom.ceil() as i32;
        let y = self.scroll.scroll_y();
        self.scroll.set_scroll_y(y);
    }

    /// Place the rows for the current scroll offset.
    pub fn layout_rows(&mut self, ui: &mut UiRoot, me: ElementId) {
        let height = ui.element(me).map_or(0.0, |e| e.height);
        self.scroll.line_height = self.line_height.max(1);
        self.scroll.content_height = self.content_height;
        self.scroll.view_height = (height.floor() as i32).max(0);
        let y = self.scroll.scroll_y();
        self.scroll.set_scroll_y(y);
        let scroll_y = self.scroll.scroll_y() as f32;
        for child in ui.children(me) {
            let Some(base) = self.base_tops.get(&child).copied() else {
                continue;
            };
            let top = base - scroll_y;
            let child_h = ui.element(child).map_or(0.0, |e| e.height);
            if let Some(e) = ui.element_mut(child) {
                e.top = top;
            }
            ui.set_visible(child, top + child_h > 0.5 && top < height - 0.5);
        }
    }
}

impl Widget for ScrollablePanel {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        if e.kind == kind::SCROLL {
            self.scroll.scroll_by_lines(-e.data0);
            self.layout_rows(ui, me);
            return true;
        }
        ui.dispatch_to_pointer_region(me, e)
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, _rc: &mut RenderContext<'_>) {
        self.layout_rows(ui, me);
    }

    fn scroll_model(&mut self) -> Option<&mut Scrollable> {
        Some(&mut self.scroll)
    }
}

/// A list box: its own dat look, a scrolling viewport of rows, and the
/// authored row templates and scrollbar link.
#[derive(Debug, Clone)]
pub struct ListBox {
    /// The element's dat look and states.
    pub inner: DatElement,
    /// The authored row templates, in authored order.
    pub templates: Vec<TemplateEntry>,
    /// The authored scrollbar element id (0: none).
    pub scrollbar_element_id: u32,
    viewport: Option<ElementId>,
    pending_line_height: i32,
}

impl ListBox {
    /// A list box over `info`.
    #[must_use]
    pub fn new(info: Arc<ElementInfo>) -> Self {
        Self {
            templates: info.template_list.clone(),
            scrollbar_element_id: info.scrollbar_element_id,
            inner: DatElement::new(info),
            viewport: None,
            pending_line_height: DEFAULT_LINE_HEIGHT,
        }
    }

    /// The viewport holding the rows, made on first use.
    pub fn viewport(&mut self, ui: &mut UiRoot, me: ElementId) -> ElementId {
        if let Some(v) = self.viewport.filter(|v| ui.exists(*v)) {
            return v;
        }
        let (w, h) = ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height));
        let mut panel = ScrollablePanel::new();
        panel.line_height = self.pending_line_height;
        let v = ui.add(
            me,
            Element::at(0.0, 0.0, w, h).with_anchors(AnchorEdges::ALL),
            panel,
        );
        // Capture the margins now (none), so a list box that shrinks
        // before its first layout takes the viewport with it.
        ui.apply_anchor(v, w, h);
        self.viewport = Some(v);
        v
    }

    /// The viewport, if made.
    #[must_use]
    pub fn viewport_id(&self) -> Option<ElementId> {
        self.viewport
    }

    /// Wheel step of the rows, pixels.
    pub fn set_line_height(&mut self, ui: &mut UiRoot, value: i32) {
        self.pending_line_height = value;
        if let Some(v) = self.viewport
            && let Some(p) = ui.widget_mut::<ScrollablePanel>(v)
        {
            p.line_height = value;
        }
    }

    /// Add `row` under the rows already there.
    pub fn add_prebuilt_row(
        &mut self,
        ui: &mut UiRoot,
        me: ElementId,
        row: ElementId,
    ) -> ElementId {
        let v = self.viewport(ui, me);
        ui.with::<ScrollablePanel, _>(v, |p, ui| {
            let top = p.content_height() as f32;
            if let Some(e) = ui.element_mut(row) {
                e.left = 0.0;
            }
            p.add_row(ui, v, row, top);
        });
        row
    }

    /// Number of rows.
    #[must_use]
    pub fn item_count(&self, ui: &UiRoot) -> usize {
        self.viewport.map_or(0, |v| ui.children(v).len())
    }

    /// Keep the first `retained` rows, freeing the rest. `false` (and
    /// nothing done) when there are fewer.
    pub fn remove_tail(&mut self, ui: &mut UiRoot, retained: usize) -> bool {
        let Some(v) = self.viewport else {
            return retained == 0;
        };
        let rows = ui.children(v);
        if retained > rows.len() {
            return false;
        }
        ui.with::<ScrollablePanel, _>(v, |p, ui| {
            for row in rows[retained..].iter().rev() {
                p.remove_row(ui, *row);
            }
        });
        true
    }

    /// Remove every row (scroll back to the top).
    pub fn flush(&mut self, ui: &mut UiRoot) {
        if let Some(v) = self.viewport {
            ui.with::<ScrollablePanel, _>(v, |p, ui| p.clear_content(ui, v));
        }
    }

    /// Remove every row, keeping the scroll offset for the rows that
    /// replace them.
    pub fn flush_preserving_scroll(&mut self, ui: &mut UiRoot) {
        if let Some(v) = self.viewport {
            ui.with::<ScrollablePanel, _>(v, |p, ui| {
                let saved = p.scroll.scroll_y();
                p.clear_content(ui, v);
                // The model still has the old extents until the next
                // layout, so the offset survives for the new rows.
                p.scroll.set_scroll_y(saved);
            });
        }
    }
}

impl Widget for ListBox {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        self.inner.on_event(ui, me, e)
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        self.inner.draw(ui, me, rc);
    }

    fn consumes_dat_children(&self) -> bool {
        true
    }

    fn active_cursor_state_name(&self) -> &str {
        self.inner.active_cursor_state_name()
    }

    fn retail_state(&self) -> Option<u32> {
        self.inner.retail_state()
    }

    fn set_retail_state(&mut self, ui: &mut UiRoot, me: ElementId, state: u32) -> bool {
        self.inner.set_retail_state(ui, me, state)
    }

    fn scroll_model_element(&self) -> Option<ElementId> {
        self.viewport
    }
}
