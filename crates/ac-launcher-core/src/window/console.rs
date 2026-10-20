//! One windowless session's console: the end of what the session has
//! written (its log file, read again whenever it grew) and a line to send.

use std::io::{Read, Seek, SeekFrom};
use std::time::SystemTime;

use super::Orchestration;
use crate::launching::{SessionConsoleRun, parse_line};

/// How much of the end of the log is shown.
pub const TAIL_BYTES: u64 = 256 * 1024;

/// How many of the newest lines are drawn; the file keeps more.
pub const MOST_LINES_SHOWN: usize = 1500;

/// A console window's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConsole {
    /// The session.
    pub session_id: String,
    /// The window's title: whose console this is.
    pub title: String,
    /// The end of the log.
    pub output: String,
    /// The line being typed.
    pub input: String,
    /// Why the last line was not sent, or empty.
    pub notice: String,
    /// Open (closing the window clears it).
    pub is_open: bool,
    shown_length: Option<u64>,
    shown_written_at: Option<SystemTime>,
}

impl SessionConsole {
    /// A console for `session_id`.
    pub fn new(session_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            title: title.into(),
            output: String::new(),
            input: String::new(),
            notice: String::new(),
            is_open: true,
            shown_length: None,
            shown_written_at: None,
        }
    }

    /// Reads the log again if it changed; cheap when it has not. True when
    /// the output changed.
    pub fn refresh(&mut self, orchestrator: &dyn Orchestration) -> bool {
        let Some(path) = orchestrator.session_log_path(&self.session_id) else {
            return false;
        };
        let Ok(metadata) = std::fs::metadata(&path) else {
            return false;
        };
        let written = metadata.modified().ok();
        if self.shown_length == Some(metadata.len()) && self.shown_written_at == written {
            return false;
        }
        // The session is writing or rotating the file when this fails; the next poll reads it.
        let Ok(mut file) = std::fs::File::open(&path) else {
            return false;
        };
        let length = file.metadata().map_or(metadata.len(), |m| m.len());
        let start = length.saturating_sub(TAIL_BYTES);
        let mut bytes = Vec::new();
        if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut bytes).is_err() {
            return false;
        }
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        if start > 0 {
            // The cut fell inside a line; show whole lines only.
            if let Some(first_break) = text.find('\n') {
                text = text[first_break + 1..].to_owned();
            }
        }
        self.shown_length = Some(metadata.len());
        self.shown_written_at = written;
        self.output = text;
        true
    }

    /// The newest lines, split into coloured runs.
    pub fn lines(&self) -> Vec<Vec<SessionConsoleRun>> {
        let lines: Vec<&str> = self.output.split('\n').collect();
        let first = lines.len().saturating_sub(MOST_LINES_SHOWN);
        let mut out = Vec::new();
        for (index, line) in lines.iter().enumerate().skip(first) {
            let line = line.trim_end_matches('\r');
            if index == lines.len() - 1 && line.is_empty() {
                break;
            }
            out.push(parse_line(line));
        }
        out
    }

    /// Whether Send may run.
    pub fn can_send(&self) -> bool {
        !self.input.trim().is_empty()
    }

    /// Sends the typed line.
    pub fn send(&mut self, orchestrator: &dyn Orchestration) {
        let line = self.input.trim().to_owned();
        if line.is_empty() {
            return;
        }
        if orchestrator.try_send_console_line(&self.session_id, &line) {
            self.input.clear();
            self.notice.clear();
        } else {
            self.notice = "Not sent: the session is not running.".into();
        }
    }
}
