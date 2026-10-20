//! Callbacks: the client asking a plugin something it registered to
//! answer (a chat filter or input interceptor, a loot classifier), through
//! the plugin's `openac_callback` export
//! ([`crate::exports::CALLBACK`]). The client never calls a plugin back
//! while that plugin is inside a call of its own.

use crate::items::{LootClassification, LootContext, LootedItem};
use crate::types::{ChatLine, CommandCall, CommandCompletion};
use crate::{wire_record, wire_union};

wire_record! {
    /// Ask a loot classifier.
    pub struct LootQuestion {
        /// The plugin's handle for the classifier.
        pub handle: u32,
        /// A stored profile to classify with; empty for the current one.
        pub profile: String,
        /// What to classify.
        pub context: LootContext,
    }
}

wire_union! {
    /// What the client asks.
    pub enum Callback: "callback" {
        /// Should this chat line (or status notice, kind 100) be dropped?
        /// (filter handle, line); `Bool(true)` drops it.
        0 => ChatFilter((u32, ChatLine)),
        /// What becomes of this typed line? (interceptor handle, line)
        1 => ChatIntercept((u32, String)),
        /// Classify an item.
        2 => LootClassify(LootQuestion),
        /// Does the classifier need the item appraised first?
        3 => LootNeedsIdentification(LootQuestion),
        /// An item was looted under an action (classifier handle, item).
        4 => LootLooted((u32, LootedItem)),
        /// An item went away (classifier handle, object).
        5 => LootRemoved((u32, u32)),
        /// Offer completions for a partly typed command of one of the
        /// plugin's verbs; answer [`CallbackAnswer::Completions`].
        6 => CommandComplete(CommandCall),
    }
}

wire_union! {
    /// What becomes of a typed line an interceptor saw.
    pub enum InterceptDecision: "intercept decision" {
        /// Leave it as it is.
        0 => Pass,
        /// Replace it with this text (the pipeline runs on with it).
        1 => Rewrite(String),
        /// Drop it.
        2 => Suppress,
    }
}

#[allow(clippy::derivable_impls)]
impl Default for InterceptDecision {
    fn default() -> Self {
        Self::Pass
    }
}

wire_union! {
    /// A plugin's answer to a [`Callback`].
    pub enum CallbackAnswer: "callback answer" {
        /// Nothing to say (a notice, or a callback it does not have).
        0 => Nothing,
        /// Yes or no.
        1 => Bool(bool),
        /// An interceptor's decision.
        2 => Intercept(InterceptDecision),
        /// A classifier's verdict, none when it could not classify.
        3 => Classification(Option<LootClassification>),
        /// Completions for a command, best first.
        4 => Completions(Vec<CommandCompletion>),
    }
}

#[allow(clippy::derivable_impls)]
impl Default for CallbackAnswer {
    fn default() -> Self {
        Self::Nothing
    }
}
