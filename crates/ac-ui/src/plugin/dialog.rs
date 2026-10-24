//! Plugin dialogs and modal windows in the window (OpenAC
//! `PluginDialogHost`, `UiPluginDialog`, `UiDrawnPluginDialog`): the
//! session queues the dialogs ([`ac_plugin_ui::DialogView`]); the window
//! shows the first of each owner window's queue (the ownerless ones share
//! one), centred on its owner, which it blocks until answered, and reports
//! the answer. A window shown modal blocks its owner while it is shown.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use ac_plugin_ui::{DialogView, UiInput};

use super::fold::{MENU_BACKGROUND, MENU_BORDER, MENU_HIGHLIGHT, MENU_TEXT};
use super::{Palette, PluginOutbox};
use crate::draw::{Color, RenderContext};
use crate::event::{Key, UiEvent, kind};
use crate::font::UiFont;
use crate::root::UiRoot;
use crate::tree::{Element, ElementId, Widget};

/// The box's margin.
pub const PAD: f32 = 10.0;
/// The title bar's height.
pub const TITLE_HEIGHT: f32 = 20.0;
/// The close button's side.
pub const CLOSE_SIZE: f32 = 16.0;
/// A button's height.
pub const BUTTON_HEIGHT: f32 = 22.0;
/// A button's least width.
pub const BUTTON_MIN_WIDTH: f32 = 64.0;
/// Room either side of a button's caption.
pub const BUTTON_TEXT_PAD: f32 = 8.0;
/// Between buttons.
pub const BUTTON_GAP: f32 = 8.0;
/// The narrowest box.
pub const BOX_MIN_WIDTH: f32 = 260.0;
/// A markup button's own background.
pub const CLASSIC_BUTTON: Color = Color::rgba(0.1, 0.1, 0.15, 0.8);
/// The default button's border in Classic.
pub const CLASSIC_DEFAULT_BORDER: Color = Color::rgba(1.0, 0.82, 0.25, 1.0);

fn measure(font: Option<&Arc<UiFont>>, s: &str) -> f32 {
    font.map_or(s.chars().count() as f32 * 7.0, |f| f.measure_width(s))
}

/// `text` wrapped to `width` at spaces (a word longer than the width
/// stands on a line of its own).
#[must_use]
pub fn wrap_words(text: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && measure(&candidate) > width {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

/// A plugin dialog drawn with primitives (OpenAC `UiDrawnPluginDialog`): a
/// title bar with a close button, the wrapped message and a centred row of
/// buttons, the first marked as the default. Enter picks the first button
/// and Escape the last; a press anywhere on it gives it the keyboard.
#[derive(Debug)]
pub struct PluginDialogBox {
    /// The dialog's handle.
    pub handle: u32,
    /// The owner window's theme, when it has one.
    pub palette: Option<Palette>,
    title: String,
    lines: Vec<String>,
    buttons: Vec<(String, f32, f32)>,
    font: Option<Arc<UiFont>>,
    line_height: f32,
    answered: bool,
    outbox: PluginOutbox,
}

impl PluginDialogBox {
    /// The box for `dialog`, and its size.
    #[must_use]
    pub fn new(
        dialog: &DialogView,
        font: Option<Arc<UiFont>>,
        outbox: PluginOutbox,
    ) -> (Self, f32, f32) {
        let f = font.as_ref();
        let widths: Vec<f32> = dialog
            .buttons
            .iter()
            .map(|b| BUTTON_MIN_WIDTH.max(measure(f, b).ceil() + 2.0 * BUTTON_TEXT_PAD))
            .collect();
        let row = widths.iter().sum::<f32>() + BUTTON_GAP * (widths.len().saturating_sub(1)) as f32;
        let width = BOX_MIN_WIDTH
            .max(row + 2.0 * PAD)
            .max(measure(f, &dialog.title).ceil() + 2.0 * PAD + CLOSE_SIZE + 4.0);
        let lines = if dialog.message.is_empty() {
            Vec::new()
        } else {
            wrap_words(&dialog.message, width - 2.0 * PAD, |s| measure(f, s))
        };
        let line_height = f.map_or(14.0, |f| f.line_height);
        let message = lines.len() as f32 * line_height;
        let height = TITLE_HEIGHT
            + PAD
            + if message > 0.0 { message + PAD } else { 0.0 }
            + BUTTON_HEIGHT
            + PAD;
        let mut x = ((width - row) * 0.5).round();
        let buttons = dialog
            .buttons
            .iter()
            .zip(&widths)
            .map(|(caption, w)| {
                let b = (caption.clone(), x, *w);
                x += w + BUTTON_GAP;
                b
            })
            .collect();
        (
            Self {
                handle: dialog.handle,
                palette: None,
                title: dialog.title.clone(),
                lines,
                buttons,
                font,
                line_height,
                answered: false,
                outbox,
            },
            width,
            height,
        )
    }

    fn cancel(&self) -> i32 {
        self.buttons.len() as i32 - 1
    }

    fn answer(&mut self, button: i32) {
        if std::mem::replace(&mut self.answered, true) {
            return;
        }
        self.outbox.borrow_mut().push(UiInput::DialogAnswered {
            handle: self.handle,
            button,
        });
    }

    /// The button at a local point (the close button is the last), or −1.
    #[must_use]
    pub fn button_at(&self, w: f32, h: f32, x: f32, y: f32) -> i32 {
        let close_x = w - CLOSE_SIZE - 2.0;
        let close_y = ((TITLE_HEIGHT - CLOSE_SIZE) * 0.5).round();
        if x >= close_x && x < close_x + CLOSE_SIZE && y >= close_y && y < close_y + CLOSE_SIZE {
            return self.cancel();
        }
        let top = h - PAD - BUTTON_HEIGHT;
        if y < top || y >= top + BUTTON_HEIGHT {
            return -1;
        }
        self.buttons
            .iter()
            .position(|(_, bx, bw)| x >= *bx && x < bx + bw)
            .map_or(-1, |i| i as i32)
    }
}

impl Widget for PluginDialogBox {
    fn on_event(&mut self, ui: &mut UiRoot, me: ElementId, e: &UiEvent) -> bool {
        if e.target != Some(me) {
            return false;
        }
        match e.kind {
            kind::KEY_DOWN => match e.key {
                Key::Enter | Key::KeypadEnter => {
                    ui.suppress_physical_key_until_release(e.key);
                    self.answer(0);
                    true
                }
                Key::Escape => {
                    ui.suppress_physical_key_until_release(e.key);
                    let cancel = self.cancel();
                    self.answer(cancel);
                    true
                }
                _ => false,
            },
            kind::MOUSE_DOWN | kind::RIGHT_DOWN => {
                if ui.keyboard_focus() != Some(me) {
                    ui.set_keyboard_focus(Some(me));
                }
                true
            }
            kind::CLICK => {
                let (w, h) = ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height));
                let button = self.button_at(w, h, e.data1 as f32, e.data2 as f32);
                if button >= 0 {
                    self.answer(button);
                }
                true
            }
            _ => true,
        }
    }

    fn handles_click(&self, _element: &Element) -> bool {
        true
    }

    fn draw(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height));
        let p = self.palette.as_ref();
        let text = p.map_or(MENU_TEXT, |p| p.text);
        let outline = p.is_none();
        rc.fill(0.0, 0.0, w, h, p.map_or(MENU_BACKGROUND, |p| p.background));
        rc.fill(
            0.0,
            0.0,
            w,
            TITLE_HEIGHT,
            p.map_or(MENU_HIGHLIGHT, |p| p.field),
        );
        let caption =
            |rc: &mut RenderContext<'_>, s: &str, x: f32, y: f32, h: f32, color: Color| {
                if let Some(font) = &self.font {
                    let top = y + ((h - font.line_height) * 0.5).floor();
                    rc.string_dat(
                        font,
                        s,
                        x,
                        top,
                        color,
                        outline,
                        crate::draw::DEFAULT_OUTLINE_COLOR,
                    );
                }
            };
        caption(rc, &self.title, PAD, 0.0, TITLE_HEIGHT, text);
        let mut y = TITLE_HEIGHT + PAD;
        for line in &self.lines {
            caption(rc, line, PAD, y, self.line_height, text);
            y += self.line_height;
        }
        let border = p.map_or(MENU_BORDER, |p| p.border);
        let face = p.map_or(CLASSIC_BUTTON, |p| p.field);
        let top = h - PAD - BUTTON_HEIGHT;
        for (i, (label, x, bw)) in self.buttons.iter().enumerate() {
            rc.fill(*x, top, *bw, BUTTON_HEIGHT, face);
            let edge = if i == 0 {
                p.map_or(CLASSIC_DEFAULT_BORDER, |p| p.accent)
            } else {
                border
            };
            rc.rect_outline(*x, top, *bw, BUTTON_HEIGHT, edge, 1.0);
            let tx = x + ((bw - measure(self.font.as_ref(), label)) * 0.5).floor();
            caption(rc, label, tx, top, BUTTON_HEIGHT, text);
        }
        let cx = w - CLOSE_SIZE - 2.0;
        let cy = ((TITLE_HEIGHT - CLOSE_SIZE) * 0.5).round();
        rc.fill(cx, cy, CLOSE_SIZE, CLOSE_SIZE, face);
        rc.rect_outline(cx, cy, CLOSE_SIZE, CLOSE_SIZE, border, 1.0);
        let tx = cx + ((CLOSE_SIZE - measure(self.font.as_ref(), "x")) * 0.5).floor();
        caption(rc, "x", tx, cy, CLOSE_SIZE, text);
    }

    fn draw_after_children(&mut self, ui: &mut UiRoot, me: ElementId, rc: &mut RenderContext<'_>) {
        let (w, h) = ui.element(me).map_or((0.0, 0.0), |e| (e.width, e.height));
        rc.rect_outline(
            0.0,
            0.0,
            w,
            h,
            self.palette.map_or(MENU_BORDER, |p| p.border),
            1.0,
        );
    }
}

/// The dialogs and modal blocks on screen (OpenAC `PluginDialogHost`).
#[derive(Debug, Default)]
pub struct DialogPresenter {
    /// Each dialog shown: its box and the window it blocks.
    shown: HashMap<u32, (ElementId, Option<ElementId>)>,
    /// What had the keyboard when each dialog took it.
    previous_focus: HashMap<u32, Option<ElementId>>,
    /// The modal blocks: (window, blocker).
    modals: HashSet<(ElementId, ElementId)>,
}

impl DialogPresenter {
    /// Follows the session's dialogs: the first of each owner's queue is
    /// shown, centred on its owner (`frames` maps a panel number to its
    /// window frame, `palettes` to its theme), others wait; answered ones
    /// go and focus they took goes back.
    pub fn sync(
        &mut self,
        ui: &mut UiRoot,
        dialogs: &[DialogView],
        frames: &HashMap<u32, ElementId>,
        palettes: &HashMap<u32, Palette>,
        font: Option<&Arc<UiFont>>,
        outbox: &PluginOutbox,
    ) {
        let live: HashSet<u32> = dialogs.iter().map(|d| d.handle).collect();
        let gone: Vec<u32> = self
            .shown
            .keys()
            .copied()
            .filter(|h| !live.contains(h))
            .collect();
        for handle in gone {
            let Some((element, owner)) = self.shown.remove(&handle) else {
                continue;
            };
            let had_focus = ui.keyboard_focus() == Some(element);
            if let Some(owner) = owner {
                ui.unblock_window(owner, element);
            }
            ui.destroy(element);
            let previous = self.previous_focus.remove(&handle).flatten();
            if had_focus && let Some(previous) = previous.filter(|p| ui.can_focus(*p)) {
                ui.set_keyboard_focus(Some(previous));
            }
        }
        let mut heads: Vec<&DialogView> = Vec::new();
        for dialog in dialogs {
            if !heads.iter().any(|h| h.owner_panel == dialog.owner_panel) {
                heads.push(dialog);
            }
        }
        let (screen_w, screen_h) = ui.effective_canvas_size();
        for dialog in heads {
            let owner = dialog.owner_panel.and_then(|p| frames.get(&p).copied());
            if !self.shown.contains_key(&dialog.handle) {
                let (widget, w, h) =
                    PluginDialogBox::new(dialog, font.cloned(), std::rc::Rc::clone(outbox));
                let mut e = Element::at(0.0, 0.0, w, h).with_z_order(10);
                e.accepts_focus = true;
                let element = ui.insert(e, widget);
                let top = ui.root();
                ui.add_child(top, element);
                ui.bring_to_front(element);
                if let Some(owner) = owner {
                    ui.block_window(owner, element);
                }
                self.previous_focus
                    .insert(dialog.handle, ui.keyboard_focus());
                if ui.keyboard_focus().is_none() {
                    ui.set_keyboard_focus(Some(element));
                }
                self.shown.insert(dialog.handle, (element, owner));
            }
            let Some((element, _)) = self.shown.get(&dialog.handle).copied() else {
                continue;
            };
            if let Some(b) = ui.widget_mut::<PluginDialogBox>(element) {
                b.palette = dialog.owner_panel.and_then(|p| palettes.get(&p).copied());
            }
            let (cx, cy) = match owner.and_then(|o| ui.element(o).map(|e| (o, e.width, e.height))) {
                Some((o, ow, oh)) => {
                    let (ox, oy) = ui.screen_position(o);
                    (ox + ow * 0.5, oy + oh * 0.5)
                }
                None => (screen_w * 0.5, screen_h * 0.5),
            };
            if let Some(el) = ui.element_mut(element) {
                el.left = (cx - el.width * 0.5)
                    .clamp(0.0, (screen_w - el.width).max(0.0))
                    .round();
                el.top = (cy - el.height * 0.5)
                    .clamp(0.0, (screen_h - el.height).max(0.0))
                    .round();
            }
        }
    }

    /// Follows the modal windows: each `(panel, owner)` blocks the owner's
    /// frame with the panel's while both are mounted.
    pub fn sync_modals(
        &mut self,
        ui: &mut UiRoot,
        modals: &[(u32, u32)],
        frames: &HashMap<u32, ElementId>,
    ) {
        let wanted: HashSet<(ElementId, ElementId)> = modals
            .iter()
            .filter_map(|(panel, owner)| Some((*frames.get(owner)?, *frames.get(panel)?)))
            .collect();
        for gone in self.modals.difference(&wanted).copied().collect::<Vec<_>>() {
            ui.unblock_window(gone.0, gone.1);
            self.modals.remove(&gone);
        }
        for new in wanted.difference(&self.modals).copied().collect::<Vec<_>>() {
            ui.block_window(new.0, new.1);
            ui.bring_to_front(new.1);
            self.modals.insert(new);
        }
    }
}

#[cfg(test)]
mod tests {
    //! OpenAC `tests/AcDream.App.Tests/UI/PluginDialogHostTests.cs` and
    //! `UiPluginDialogTests.cs`: what the window does with the session's
    //! dialogs.
    use std::cell::RefCell;
    use std::rc::Rc;

    use ac_plugin_ui::UiOwner;

    use super::*;
    use crate::event::{Modifiers, MouseButton};

    fn dialog(handle: u32, owner_panel: Option<u32>) -> DialogView {
        DialogView {
            handle,
            owner: UiOwner::new("p", "P"),
            title: "Delete".into(),
            message: "Delete the profile?".into(),
            buttons: vec!["Yes".into(), "No".into()],
            owner_panel,
        }
    }

    fn setup() -> (UiRoot, ElementId, HashMap<u32, ElementId>, PluginOutbox) {
        let mut ui = UiRoot::new();
        ui.set_size(1024.0, 768.0);
        let mut e = Element::at(100.0, 100.0, 200.0, 100.0);
        e.accepts_focus = true;
        e.focus_on_mouse_click = true;
        let owner = ui.insert(e, crate::widgets::Panel::default());
        let top = ui.root();
        ui.add_child(top, owner);
        let frames = HashMap::from([(7, owner)]);
        (ui, owner, frames, Rc::new(RefCell::new(Vec::new())))
    }

    #[test]
    fn a_dialog_centres_on_its_owner_blocks_it_and_answers_once() {
        // AConfirmShowsCentredOnItsOwnerBlocksItAndTakesFocus,
        // AButtonAnswersOnceAfterTheDialogIsGone
        let (mut ui, owner, frames, outbox) = setup();
        let mut presenter = DialogPresenter::default();
        presenter.sync(
            &mut ui,
            &[dialog(1, Some(7))],
            &frames,
            &HashMap::new(),
            None,
            &outbox,
        );
        assert!(ui.is_window_blocked(owner));
        let (element, _) = presenter.shown[&1];
        let el = ui.element(element).unwrap();
        assert_eq!(el.left + el.width * 0.5, 200.0);
        assert_eq!(ui.keyboard_focus(), Some(element));
        // A press on the blocked owner goes nowhere.
        ui.on_mouse_down(MouseButton::Left, 110, 110, 0);
        ui.on_mouse_up(MouseButton::Left, 110, 110, 0);
        assert_eq!(ui.keyboard_focus(), Some(element));
        ui.on_key_down(Key::Escape, Modifiers::NONE);
        ui.on_key_down(Key::Enter, Modifiers::NONE);
        assert_eq!(
            outbox.borrow().as_slice(),
            [UiInput::DialogAnswered {
                handle: 1,
                button: 1
            }]
        );
        presenter.sync(&mut ui, &[], &frames, &HashMap::new(), None, &outbox);
        assert!(!ui.is_window_blocked(owner));
        assert!(!ui.exists(element));
    }

    #[test]
    fn dialogs_for_one_owner_queue_and_others_show_at_once() {
        // DialogsForOneOwnerQueueAndOthersShowAtOnce
        let (mut ui, _owner, frames, outbox) = setup();
        let mut presenter = DialogPresenter::default();
        let all = [dialog(1, Some(7)), dialog(2, Some(7)), dialog(3, None)];
        presenter.sync(&mut ui, &all, &frames, &HashMap::new(), None, &outbox);
        assert!(presenter.shown.contains_key(&1));
        assert!(!presenter.shown.contains_key(&2));
        assert!(presenter.shown.contains_key(&3));
        presenter.sync(&mut ui, &all[1..], &frames, &HashMap::new(), None, &outbox);
        assert!(presenter.shown.contains_key(&2));
    }

    #[test]
    fn focus_elsewhere_stays_and_comes_back_when_the_dialog_closes() {
        // FocusOutsideTheOwnerStaysWhereItIs, FocusComesBackWhenTheDialogCloses
        let (mut ui, _owner, frames, outbox) = setup();
        let mut e = Element::at(500.0, 500.0, 50.0, 20.0);
        e.accepts_focus = true;
        let chat = ui.insert(e, crate::widgets::Panel::default());
        let top = ui.root();
        ui.add_child(top, chat);
        ui.set_keyboard_focus(Some(chat));
        let mut presenter = DialogPresenter::default();
        presenter.sync(
            &mut ui,
            &[dialog(1, Some(7))],
            &frames,
            &HashMap::new(),
            None,
            &outbox,
        );
        assert_eq!(ui.keyboard_focus(), Some(chat));
        presenter.sync(&mut ui, &[], &frames, &HashMap::new(), None, &outbox);
        assert_eq!(ui.keyboard_focus(), Some(chat));
    }

    #[test]
    fn a_modal_window_blocks_its_owner_while_listed() {
        // AModalPanelBlocksItsOwnerUntilItHides
        let (mut ui, owner, mut frames, _outbox) = setup();
        let modal = ui.insert(
            Element::at(0.0, 0.0, 50.0, 50.0),
            crate::widgets::Panel::default(),
        );
        let top = ui.root();
        ui.add_child(top, modal);
        frames.insert(8, modal);
        let mut presenter = DialogPresenter::default();
        presenter.sync_modals(&mut ui, &[(8, 7)], &frames);
        assert!(ui.is_window_blocked(owner));
        presenter.sync_modals(&mut ui, &[], &frames);
        assert!(!ui.is_window_blocked(owner));
    }

    #[test]
    fn long_messages_wrap_at_spaces() {
        let lines = wrap_words("one two three four", 50.0, |s| s.len() as f32 * 7.0);
        assert_eq!(lines, ["one two", "three", "four"]);
    }
}
