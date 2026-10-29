//! The file icons plugin windows draw (`<icon file="icons/sword.png"/>`),
//! held as session state (OpenAC `PluginMarkupIconResolver`). The window
//! says which files each plugin window draws ([`UiInput::FileIconsShown`]);
//! a file is read in the background the first time, and draws nothing until
//! it is ready. The image is then held in the plugin's own image table,
//! under the plugin's image budget, for as long as that window keeps
//! drawing it: a file the window has not drawn for [`RELEASE_AFTER_MS`] is
//! let go, checked at most once every [`RELEASE_CHECK_INTERVAL_MS`], and
//! read again if it is drawn later, so a bound icon that runs through many
//! files, or a list with many row icons, does not fill the plugin's budget
//! with images nobody sees. A file refused for want of room keeps its
//! decoded image here, outside the budget, for as long as the window draws
//! it, and is offered again, at most once every
//! [`RELEASE_CHECK_INTERVAL_MS`] and only once the table has let go of
//! something, without being read again unless the file on disk changed.
//! Time here is each window's own (see [`MAXIMUM_STEP_MS`]). A file that cannot be used is
//! refused (and logged once) until it is let go. Everything a window holds
//! is let go when the window goes (closed for good, its plugin unloaded or
//! reloaded) or the client window detaches. Two windows showing the same
//! file each hold it, so one window going does not take the image from the
//! other.
//!
//! [`UiInput::FileIconsShown`]: crate::UiInput::FileIconsShown

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::PluginUi;
use crate::file_images::{FileImage, FileImageLoader, FileImageRead};
use crate::model::UiChange;

/// A file the window has not drawn for this long, milliseconds, is let go.
pub const RELEASE_AFTER_MS: u64 = 5_000;

/// How often, at most, milliseconds, the session looks for files to let go.
pub const RELEASE_CHECK_INTERVAL_MS: u64 = 1_000;

/// A bound path can name anything; past this many, every hold of the
/// window is let go and the paths asked again.
const MAXIMUM_REMEMBERED: usize = 4096;

/// The most one tick moves a window's own time on, milliseconds.
pub const MAXIMUM_STEP_MS: u64 = 1_000;

#[derive(Debug, Clone)]
struct FileIcon {
    /// The read, until its result has been taken.
    reading: Option<FileImageRead>,
    /// The image held for the window, once it is (0: none).
    handle: u32,
    /// Set when the plugin's table had no room for the image: the image,
    /// kept (outside the budget) so a later offer need not read it again,
    /// and the table's release generation at that moment. Once it has
    /// moved there may be room.
    refused_for_room: Option<(Arc<FileImage>, u64)>,
    /// The window's time of the last offer to the table.
    offered_at: u64,
    /// The window's time of the last time it drew this file.
    drawn_at: u64,
}

/// What one plugin window holds of the file icons it draws.
#[derive(Debug, Clone)]
pub(super) struct PanelFiles {
    owner: String,
    directory: Option<PathBuf>,
    /// By the path as the markup gives it.
    icons: BTreeMap<String, FileIcon>,
    /// What the window draws now.
    shown: Vec<String>,
    /// The window's own time, milliseconds: each tick moves it on by the
    /// time since the last, but by no more than [`MAXIMUM_STEP_MS`], so a
    /// stall of the whole session does not count as time spent not drawing.
    time: u64,
    /// The session clock at the last tick.
    last_tick: u64,
    last_release_check: u64,
    /// How many times a file refused for room was offered to the table again.
    room_retries: usize,
    reported_no_folder: bool,
}

impl PluginUi {
    /// The host's reader of the image files plugin markup names; none: file
    /// icons draw nothing.
    pub fn set_file_images(&mut self, loader: Option<Arc<dyn FileImageLoader>>) {
        self.file_images = loader;
    }

    /// The install folder of plugin `owner` (none: it has none), where the
    /// file icons of the windows it registers from now on are read. The
    /// host sets it as the plugin starts; [`Self::release`] forgets it.
    pub fn set_plugin_folder(&mut self, owner: &str, folder: Option<PathBuf>) {
        match folder {
            Some(folder) => {
                self.plugin_folders.insert(owner.to_owned(), folder);
            }
            None => {
                self.plugin_folders.remove(owner);
            }
        }
    }

    /// The image window `panel` holds for file icon `path` (the path as its
    /// markup gives it): `(handle, width, height)`, none until it is ready.
    #[must_use]
    pub fn file_icon(&self, panel: u32, path: &str) -> Option<(u32, u32, u32)> {
        let files = self.panel_files.get(&panel)?;
        let icon = files.icons.get(path)?;
        let view = self.images.get(&files.owner)?.resolve(icon.handle)?;
        Some((view.handle, view.width, view.height))
    }

    /// How many file paths window `panel` remembers: held, being read or
    /// refused.
    #[must_use]
    pub fn remembered_file_count(&self, panel: u32) -> usize {
        self.panel_files.get(&panel).map_or(0, |f| f.icons.len())
    }

    /// How many times window `panel` offered a file refused for want of
    /// room to the table again.
    #[must_use]
    pub fn file_room_retries(&self, panel: u32) -> usize {
        self.panel_files.get(&panel).map_or(0, |f| f.room_retries)
    }

    /// The window draws `paths` in plugin window `panel` now (none while it
    /// is hidden).
    pub(super) fn file_icons_shown(&mut self, panel: u32, paths: Vec<String>) {
        if !self.window_attached {
            return;
        }
        let Some(p) = self.panels.iter().find(|p| p.view.id == panel) else {
            return;
        };
        let owner = p.view.owner.id.clone();
        let directory = p.directory.clone();
        let now = self.file_clock;
        let files = self.panel_files.entry(panel).or_insert_with(|| PanelFiles {
            owner,
            directory,
            icons: BTreeMap::new(),
            shown: Vec::new(),
            time: 0,
            last_tick: now,
            last_release_check: 0,
            room_retries: 0,
            reported_no_folder: false,
        });
        files.shown = paths;
        self.follow_file_icons(panel);
    }

    /// The session's clock moved to `now` (milliseconds): each window's own
    /// time moves on, files not drawn for [`RELEASE_AFTER_MS`] of it are let
    /// go, and then the files each window draws are drawn: reads that
    /// finished are held, and a file refused for want of room is offered
    /// again once the table let go of something.
    pub fn tick_file_icons(&mut self, now: u64) {
        self.file_clock = now;
        let panels: Vec<u32> = self.panel_files.keys().copied().collect();
        for panel in panels {
            if let Some(files) = self.panel_files.get_mut(&panel) {
                files.time += now.saturating_sub(files.last_tick).min(MAXIMUM_STEP_MS);
                files.last_tick = now;
            }
            self.release_undrawn(panel);
            self.follow_file_icons(panel);
        }
    }

    /// Every path window `panel` draws, drawn now.
    fn follow_file_icons(&mut self, panel: u32) {
        let Some(mut files) = self.panel_files.remove(&panel) else {
            return;
        };
        for path in files.shown.clone() {
            self.resolve_in(panel, &mut files, &path);
        }
        self.panel_files.insert(panel, files);
    }

    /// One draw of `path` in window `panel` (OpenAC `ResolveFile`).
    fn resolve_in(&mut self, panel: u32, files: &mut PanelFiles, path: &str) {
        let Some(directory) = files.directory.clone() else {
            // Said once per window, so drawing builds no text each time.
            if !files.reported_no_folder {
                files.reported_no_folder = true;
                self.report_file(
                    &files.owner,
                    "*",
                    "file icons need a plugin folder, and this window has no plugin folder",
                );
            }
            return;
        };
        let Some(loader) = self.file_images.clone() else {
            return;
        };
        let time = files.time;
        if !files.icons.contains_key(path) {
            if files.icons.len() >= MAXIMUM_REMEMBERED {
                self.release_all(panel, files);
            }
            files.icons.insert(
                path.to_owned(),
                FileIcon {
                    reading: Some(loader.load(&directory, path)),
                    handle: 0,
                    refused_for_room: None,
                    offered_at: 0,
                    drawn_at: time,
                },
            );
        }
        let generation = self.release_generation(&files.owner);
        let owner = files.owner.clone();
        let Some(icon) = files.icons.get_mut(path) else {
            return;
        };
        icon.drawn_at = time;
        if let Some((image, refused)) = icon.refused_for_room.clone()
            && time.saturating_sub(icon.offered_at) >= RELEASE_CHECK_INTERVAL_MS
            && generation != refused
        {
            // Something was let go since the table said no: there may be
            // room now. The kept image is offered as it is, unless the file
            // changed on disk since it was read.
            icon.refused_for_room = None;
            if loader.is_current(&image) {
                files.room_retries += 1;
                self.hold(panel, &owner, path, icon, image, time);
            } else {
                icon.reading = Some(loader.load(&directory, path));
            }
        }
        let Some(result) = icon.reading.as_ref().and_then(FileImageRead::result) else {
            return;
        };
        icon.reading = None;
        match result {
            Ok(image) => self.hold(panel, &owner, path, icon, image, time),
            Err(reason) => self.report_file(&owner, path, &reason),
        }
    }

    fn release_generation(&self, owner: &str) -> u64 {
        self.images
            .get(owner)
            .map_or(0, crate::images::ImageTable::release_generation)
    }

    /// Offers `image` to the plugin's table for window `panel`'s `path`.
    fn hold(
        &mut self,
        panel: u32,
        owner: &str,
        path: &str,
        icon: &mut FileIcon,
        image: Arc<FileImage>,
        time: u64,
    ) {
        icon.offered_at = time;
        let generation = self.release_generation(owner);
        match self.table(owner).acquire_file(owner, &image) {
            Ok(acquired) => {
                icon.handle = acquired.handle;
                if let Some(view) = acquired.added {
                    self.changes.push(UiChange::ImageAdded(Box::new(view)));
                }
                self.changes.push(UiChange::FileIcon {
                    panel,
                    path: path.to_owned(),
                    handle: acquired.handle,
                });
            }
            Err(report) => {
                icon.refused_for_room = Some((image, generation));
                if !report.is_empty() {
                    self.reports.push((owner.to_owned(), report));
                }
            }
        }
    }

    /// Lets go of every file window `panel` has not drawn for
    /// [`RELEASE_AFTER_MS`] of its time; a later draw reads it again. Looks
    /// at most once every [`RELEASE_CHECK_INTERVAL_MS`].
    fn release_undrawn(&mut self, panel: u32) {
        let Some(mut files) = self.panel_files.remove(&panel) else {
            return;
        };
        let time = files.time;
        if !files.icons.is_empty()
            && time.saturating_sub(files.last_release_check) >= RELEASE_CHECK_INTERVAL_MS
        {
            files.last_release_check = time;
            let undrawn: Vec<String> = files
                .icons
                .iter()
                .filter(|(_, icon)| time.saturating_sub(icon.drawn_at) >= RELEASE_AFTER_MS)
                .map(|(path, _)| path.clone())
                .collect();
            for path in undrawn {
                if let Some(icon) = files.icons.remove(&path) {
                    self.let_go(&files.owner, panel, &path, icon.handle);
                }
            }
        }
        self.panel_files.insert(panel, files);
    }

    fn release_all(&mut self, panel: u32, files: &mut PanelFiles) {
        for (path, icon) in std::mem::take(&mut files.icons) {
            self.let_go(&files.owner, panel, &path, icon.handle);
        }
    }

    fn let_go(&mut self, owner: &str, panel: u32, path: &str, handle: u32) {
        if handle == 0 {
            return;
        }
        self.image_release(owner, handle);
        self.changes.push(UiChange::FileIcon {
            panel,
            path: path.to_owned(),
            handle: 0,
        });
    }

    /// Everything window `panel` holds is let go (it went).
    pub(super) fn drop_panel_files(&mut self, panel: u32) {
        if let Some(mut files) = self.panel_files.remove(&panel) {
            for (_, icon) in std::mem::take(&mut files.icons) {
                if icon.handle != 0 {
                    self.image_release(&files.owner, icon.handle);
                }
            }
        }
    }

    /// Everything every window holds is let go (the client window left).
    pub(super) fn drop_all_panel_files(&mut self) {
        let panels: Vec<u32> = self.panel_files.keys().copied().collect();
        for panel in panels {
            self.drop_panel_files(panel);
        }
    }

    /// The file icons held, for a window that attaches.
    pub(super) fn file_icon_snapshot(&self) -> Vec<UiChange> {
        self.panel_files
            .iter()
            .flat_map(|(panel, files)| {
                files
                    .icons
                    .iter()
                    .filter(|(_, icon)| icon.handle != 0)
                    .map(|(path, icon)| UiChange::FileIcon {
                        panel: *panel,
                        path: path.clone(),
                        handle: icon.handle,
                    })
            })
            .collect()
    }

    /// Logs why a plugin's image is not drawn, once per file and reason.
    fn report_file(&mut self, owner: &str, path: &str, reason: &str) {
        let label = format!("{owner}/{}", path.replace('\\', "/"));
        if self.file_reported.insert(format!("{label}\n{reason}")) {
            self.reports.push((
                owner.to_owned(),
                format!("plugin image '{label}' ignored: {reason}"),
            ));
        }
    }
}

#[cfg(test)]
mod tests;
