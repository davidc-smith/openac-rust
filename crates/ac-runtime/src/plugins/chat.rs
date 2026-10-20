//! API group K: chat (OpenAC `IPluginChat` over `RuntimeAutomationSurface`
//! and `RuntimeChatEntryOwner`): the retained lines, the chat entry, and the
//! links clicked in the transcript. Input interceptors are the plugin
//! host's (`ac_plugin::interceptors`), asked by the runner about the lines
//! the chat path holds for them ([`super::PluginLines`]).

use std::collections::VecDeque;

use ac_plugin::abi::{ChatLine, ChatLinkClicked};

use super::PluginBinding;
use crate::session::{HostEvent, SessionCommand};

/// How many lines plugins can read back (OpenAC
/// `RuntimeAutomationSurface.MaximumPluginChatMessages`).
pub const RETAINED_LINES: usize = 512;

/// The lines plugins can read back, and the links clicked since the last
/// pass.
#[derive(Debug, Default)]
pub struct ChatWatch {
    lines: VecDeque<ChatLine>,
    pub(super) links: Vec<ChatLinkClicked>,
    /// The sequences of the lines the plugins already heard right after
    /// the call that wrote them, still queued for the host: the next
    /// publication does not hand them out again.
    early: Vec<u64>,
}

impl ChatWatch {
    /// Whether the plugins already heard the line `sequence` early.
    #[must_use]
    pub fn heard_early(&self, sequence: u64) -> bool {
        self.early.contains(&sequence)
    }

    /// The queued lines were published: nothing is pending early any more.
    pub fn published(&mut self) {
        self.early.clear();
    }

    /// A line arrived; the oldest goes past [`RETAINED_LINES`].
    pub(super) fn retain(&mut self, line: ChatLine) {
        self.lines.push_back(line);
        while self.lines.len() > RETAINED_LINES {
            self.lines.pop_front();
        }
    }

    /// A coordinate link was clicked in the transcript.
    pub(crate) fn link_clicked(&mut self, east_west: f64, north_south: f64, text: String) {
        self.links.push(ChatLinkClicked {
            kind: 0,
            display_text: text,
            coordinate: Some((east_west, north_south)),
        });
    }
}

impl PluginBinding<'_> {
    /// The chat lines queued for the host that the plugins have not heard
    /// yet, oldest first; each is retained and marked heard (the
    /// [`ac_plugin::SessionApi::new_chat_lines`] of the session).
    pub(super) fn chat_new_lines(&mut self) -> Vec<ChatLine> {
        // The owners' notices become lines first, as at publication.
        self.state.collect_host_events();
        let watch = &self.state.plugin_services.chat;
        let fresh: Vec<ChatLine> = self
            .state
            .queued_host_events()
            .iter()
            .filter_map(|event| match event {
                HostEvent::Chat(line) if !watch.heard_early(line.sequence) => {
                    Some(super::plugin_chat_line(line))
                }
                _ => None,
            })
            .collect();
        let watch = &mut self.state.plugin_services.chat;
        for line in &fresh {
            watch.early.push(line.sequence);
            watch.retain(line.clone());
        }
        fresh
    }

    /// Carries out the plugins' chat-box submits now, as typed lines,
    /// while a connection is up; without one they wait for the runner.
    pub(super) fn chat_submit_now(&mut self, submits: &mut Vec<String>) {
        if self.link.is_none() {
            return;
        }
        for line in std::mem::take(submits) {
            self.run(SessionCommand::Chat(line));
        }
    }

    /// Every retained line above `after` (OpenAC `CaptureMessages`).
    pub(super) fn chat_captured(&self, after: u64) -> Vec<ChatLine> {
        self.state
            .plugin_services
            .chat
            .lines
            .iter()
            .filter(|l| l.sequence > after)
            .cloned()
            .collect()
    }

    /// Stages text in the chat entry (OpenAC `RuntimeChatEntryOwner.
    /// Compose`): refused while the player is typing there.
    pub(super) fn chat_compose(&mut self, text: &str) -> bool {
        self.state.communication.chat_entry_mut().compose(text)
    }

    /// The keyboard goes into the chat entry.
    pub(super) fn chat_is_input_active(&self) -> bool {
        self.state.communication.chat_entry().is_input_active()
    }
}
