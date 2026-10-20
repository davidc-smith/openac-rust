//! Relaunching checked accounts' clients automatically: after a crash or a
//! Stop, once the process and its start-up work have ended and the delay
//! has passed, the account is launched again with its current choice.
//!
//! The controller owns no process and runs no launch itself: [`poll`]
//! hands out at most one due attempt, the caller runs the ordinary launch,
//! and [`finish`] records how it went.
//!
//! [`poll`]: LauncherRelaunchController::poll
//! [`finish`]: LauncherRelaunchController::finish

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use super::capabilities::LauncherCapability;
use super::orchestrator::OrchestratorError;
use super::snapshot::{LauncherActivityKind, LauncherSessionSnapshot, LauncherStateSnapshot};
use crate::cancel::CancelToken;
use crate::profiles::LaunchMode;

/// The default delay before a relaunch, in seconds.
pub const DEFAULT_RELAUNCH_DELAY_SECONDS: u32 = 180;
/// The shortest delay allowed, in seconds.
pub const MINIMUM_RELAUNCH_DELAY_SECONDS: u32 = 5;
/// The longest delay allowed, in seconds.
pub const MAXIMUM_RELAUNCH_DELAY_SECONDS: u32 = 3600;

/// A monotonic clock: the time since some fixed start.
pub type MonotonicClock = Arc<dyn Fn() -> Duration + Send + Sync>;

/// A checked account's current saved launch choice, whatever the visual
/// filters show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherRelaunchSelection {
    /// The server.
    pub server: String,
    /// The account.
    pub account: String,
    /// The character, or `None` for character select.
    pub character: Option<String>,
    /// The launch mode.
    pub mode: LaunchMode,
}

impl LauncherRelaunchSelection {
    /// A choice for `account` on `server`.
    pub fn new(server: &str, account: &str, character: Option<&str>, mode: LaunchMode) -> Self {
        Self {
            server: server.to_owned(),
            account: account.to_owned(),
            character: character.map(str::to_owned),
            mode,
        }
    }
}

/// A delay outside 5..=3600 seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("The relaunch delay must be a whole number from 5 to 3600 seconds.")]
pub struct RelaunchDelayOutOfRange;

/// One due relaunch, handed to the caller to run.
#[derive(Debug, Clone)]
pub struct RelaunchAttempt {
    /// What to launch.
    pub selection: LauncherRelaunchSelection,
    /// Cancelled when the account's relaunch is no longer wanted.
    pub cancel: CancelToken,
    entry: u64,
}

type Key = (String, String);

#[derive(Debug)]
struct Entry {
    id: u64,
    selection: LauncherRelaunchSelection,
    seen_session: Option<String>,
    seen_created_at: Option<SystemTime>,
    tracked_session: Option<String>,
    terminal_since: Option<Duration>,
    cancellation: Option<CancelToken>,
}

/// Schedules ordinary launches for the play sessions of checked accounts.
pub struct LauncherRelaunchController {
    clock: MonotonicClock,
    entries: Vec<(Key, Entry)>,
    next_entry: u64,
    enabled: bool,
    disposed: bool,
    launching: bool,
    delay_seconds: u32,
}

impl std::fmt::Debug for LauncherRelaunchController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LauncherRelaunchController")
            .field("enabled", &self.enabled)
            .field("launching", &self.launching)
            .field("delay_seconds", &self.delay_seconds)
            .finish_non_exhaustive()
    }
}

impl Default for LauncherRelaunchController {
    fn default() -> Self {
        let start = Instant::now();
        Self::new(Arc::new(move || start.elapsed()))
    }
}

impl LauncherRelaunchController {
    /// A controller timing its delays on `clock`.
    pub fn new(clock: MonotonicClock) -> Self {
        Self {
            clock,
            entries: Vec::new(),
            next_entry: 0,
            enabled: false,
            disposed: false,
            launching: false,
            delay_seconds: DEFAULT_RELAUNCH_DELAY_SECONDS,
        }
    }

    /// An attempt is running.
    pub fn is_launching(&self) -> bool {
        self.launching
    }

    /// The delay, in seconds.
    pub fn delay_seconds(&self) -> u32 {
        self.delay_seconds
    }

    /// Sets the delay; refused outside 5..=3600 seconds.
    pub fn set_delay_seconds(&mut self, seconds: u32) -> Result<(), RelaunchDelayOutOfRange> {
        if !(MINIMUM_RELAUNCH_DELAY_SECONDS..=MAXIMUM_RELAUNCH_DELAY_SECONDS).contains(&seconds) {
            return Err(RelaunchDelayOutOfRange);
        }
        self.delay_seconds = seconds;
        Ok(())
    }

    /// Updates the checked accounts and follows only their active or newly
    /// seen play sessions; an account that has only old history is not
    /// launched just by being checked.
    pub fn observe(
        &mut self,
        snapshot: &LauncherStateSnapshot,
        checked_accounts: &[LauncherRelaunchSelection],
        enabled: bool,
    ) {
        if self.disposed {
            return;
        }
        if !enabled {
            self.cancel_entries();
            self.enabled = false;
            return;
        }
        self.enabled = true;
        let configured: HashSet<(&str, &str)> = snapshot
            .servers
            .iter()
            .flat_map(|server| {
                server
                    .accounts
                    .iter()
                    .map(move |account| (server.name.as_str(), account.account_name.as_str()))
            })
            .collect();
        let mut selected: Vec<(Key, LauncherRelaunchSelection)> = Vec::new();
        for selection in checked_accounts {
            if !configured.contains(&(selection.server.as_str(), selection.account.as_str())) {
                continue;
            }
            let key = (selection.server.clone(), selection.account.clone());
            match selected.iter_mut().find(|(known, _)| *known == key) {
                Some((_, choice)) => *choice = selection.clone(),
                None => selected.push((key, selection.clone())),
            }
        }
        let mut sessions: HashMap<&str, &LauncherSessionSnapshot> = HashMap::new();
        let mut latest_plays: HashMap<(&str, &str), &LauncherSessionSnapshot> = HashMap::new();
        for session in &snapshot.sessions {
            sessions.insert(session.session_id.as_str(), session);
            if session.kind != LauncherActivityKind::Play {
                continue;
            }
            let key = (session.server_name.as_str(), session.account_name.as_str());
            if latest_plays
                .get(&key)
                .is_none_or(|previous| session.created_at > previous.created_at)
            {
                latest_plays.insert(key, session);
            }
        }
        self.entries.retain(|(key, entry)| {
            let keep = selected.iter().any(|(wanted, _)| wanted == key);
            if !keep && let Some(cancel) = &entry.cancellation {
                cancel.cancel();
            }
            keep
        });
        let now = (self.clock)();
        for (key, choice) in selected {
            let latest = latest_plays.get(&(key.0.as_str(), key.1.as_str())).copied();
            let index = match self.entries.iter().position(|(known, _)| *known == key) {
                Some(index) => {
                    let entry = &mut self.entries[index].1;
                    entry.selection = choice;
                    if let Some(latest) = latest
                        && entry.seen_session.as_deref() != Some(latest.session_id.as_str())
                        && entry
                            .seen_created_at
                            .is_none_or(|seen| latest.created_at >= seen)
                    {
                        entry.seen_session = Some(latest.session_id.clone());
                        entry.seen_created_at = Some(latest.created_at);
                        entry.tracked_session = Some(latest.session_id.clone());
                        entry.terminal_since = None;
                    }
                    index
                }
                None => {
                    let id = self.next_entry;
                    self.next_entry += 1;
                    self.entries.push((
                        key,
                        Entry {
                            id,
                            selection: choice,
                            seen_session: latest.map(|session| session.session_id.clone()),
                            seen_created_at: latest.map(|session| session.created_at),
                            tracked_session: latest
                                .filter(|session| session.is_process_or_start_active())
                                .map(|session| session.session_id.clone()),
                            terminal_since: None,
                            cancellation: None,
                        },
                    ));
                    self.entries.len() - 1
                }
            };
            let entry = &mut self.entries[index].1;
            let Some(tracked) = &entry.tracked_session else {
                continue;
            };
            if sessions
                .get(tracked.as_str())
                .is_some_and(|session| session.is_process_or_start_active())
            {
                entry.terminal_since = None;
            } else if entry.terminal_since.is_none() {
                entry.terminal_since = Some(now);
            }
        }
    }

    /// Hands out at most one due relaunch. `may_launch` and each account's
    /// `capability` (a reconnect hold, a running session) are checked again
    /// on every poll; nothing is handed out while an attempt runs.
    pub fn poll(
        &mut self,
        may_launch: bool,
        capability: &mut dyn FnMut(
            &LauncherRelaunchSelection,
        ) -> Result<LauncherCapability, OrchestratorError>,
    ) -> Result<Option<RelaunchAttempt>, OrchestratorError> {
        if self.disposed || !self.enabled || self.launching || !may_launch {
            return Ok(None);
        }
        let now = (self.clock)();
        let delay = Duration::from_secs(u64::from(self.delay_seconds));
        for (_, entry) in &mut self.entries {
            let Some(since) = entry.terminal_since else {
                continue;
            };
            if now.saturating_sub(since) < delay {
                continue;
            }
            let choice = &entry.selection;
            if choice.mode == LaunchMode::Headless && choice.character.is_none() {
                continue;
            }
            if !capability(choice)?.is_available {
                continue;
            }
            let cancel = CancelToken::new();
            entry.cancellation = Some(cancel.clone());
            self.launching = true;
            return Ok(Some(RelaunchAttempt {
                selection: choice.clone(),
                cancel,
                entry: entry.id,
            }));
        }
        Ok(None)
    }

    /// Records how `attempt` went. A failure (not a cancellation) is
    /// returned for the caller to show; the next attempt waits a full delay.
    pub fn finish(
        &mut self,
        attempt: &RelaunchAttempt,
        result: Result<LauncherSessionSnapshot, OrchestratorError>,
    ) -> Option<OrchestratorError> {
        let now = (self.clock)();
        let mut failure = None;
        match result {
            Ok(launched) => {
                if let Some(entry) = self.live_entry(attempt) {
                    entry.tracked_session = Some(launched.session_id.clone());
                    entry.seen_session = Some(launched.session_id.clone());
                    entry.seen_created_at = Some(launched.created_at);
                    entry.terminal_since = (!launched.is_process_or_start_active()).then_some(now);
                }
            }
            Err(OrchestratorError::Cancelled) if attempt.cancel.is_cancelled() => {}
            Err(error) => {
                if !self.disposed {
                    failure = Some(error);
                }
            }
        }
        if let Some(entry) = self.live_entry(attempt)
            && entry.terminal_since.is_some()
        {
            entry.terminal_since = Some(now);
        }
        if let Some((_, entry)) = self
            .entries
            .iter_mut()
            .find(|(_, entry)| entry.id == attempt.entry)
        {
            entry.cancellation = None;
        }
        self.launching = false;
        failure
    }

    /// Cancels every pending relaunch and stops scheduling.
    pub fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.enabled = false;
        self.cancel_entries();
    }

    fn live_entry(&mut self, attempt: &RelaunchAttempt) -> Option<&mut Entry> {
        if self.disposed || !self.enabled {
            return None;
        }
        self.entries
            .iter_mut()
            .map(|(_, entry)| entry)
            .find(|entry| entry.id == attempt.entry)
    }

    fn cancel_entries(&mut self) {
        for (_, entry) in &self.entries {
            if let Some(cancel) = &entry.cancellation {
                cancel.cancel();
            }
        }
        self.entries.clear();
    }
}
