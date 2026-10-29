//! Ported from OpenAC PR #271's `tests/AcDream.App.Tests/Plugins/PluginFileImageLoaderTests.cs`:
//! plugin markup's image files are read, decoded and mipmapped away from
//! the session's thread, once however many windows ask while the read is
//! under way, and every refusal comes back as a reason. With the session's
//! plugin UI: two spellings of one file are one image, and a window rebuilt
//! after the file changed gets the new one (`PluginMarkupIconResolverTests`).

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use super::tests::{JPEG_HEIGHT, JPEG_WIDTH, TemporaryDirectory, png, red_jpeg};
use super::*;

fn folder() -> TemporaryDirectory {
    let root = TemporaryDirectory::new("loader");
    std::fs::create_dir_all(root.path().join("icons")).unwrap();
    root
}

fn write(root: &TemporaryDirectory, relative: &str, bytes: &[u8]) {
    std::fs::write(root.path().join(relative), bytes).unwrap();
}

fn wait(read: &FileImageRead) -> FileImageResult {
    for _ in 0..1000 {
        if let Some(result) = read.result() {
            return result;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the read did not finish");
}

/// Runs each read where the caller is.
fn inline() -> PluginFileImageLoader {
    PluginFileImageLoader::with_runner(Box::new(|job| FileImageRead::finished(job())))
}

type Held = Arc<Mutex<Vec<(FileImageJob, FileImageRead)>>>;

/// Runs each read only when the test says so, and counts them.
#[derive(Clone, Default)]
struct HeldReads(Held, Arc<Mutex<usize>>);

impl HeldReads {
    fn loader(&self) -> PluginFileImageLoader {
        let held = self.clone();
        PluginFileImageLoader::with_runner(Box::new(move |job| {
            let read = FileImageRead::pending();
            held.0.lock().unwrap().push((job, read.clone()));
            *held.1.lock().unwrap() += 1;
            read
        }))
    }

    fn started(&self) -> usize {
        *self.1.lock().unwrap()
    }

    fn finish_all(&self) {
        let jobs: Vec<_> = self.0.lock().unwrap().drain(..).collect();
        for (job, read) in jobs {
            read.finish(job());
        }
    }
}

// AFileIsReadOnAWorkerNotOnTheThreadThatAsks and
// TheDefaultLoaderDoesNotReadOnTheCallersThread.
#[test]
fn the_default_loader_reads_on_a_worker() {
    let root = folder();
    write(&root, "icons/sword.png", &png(24, 16));
    let asking = std::thread::current().id();
    let reading = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&reading);
    let loader = PluginFileImageLoader::with_runner(Box::new(move |job| {
        let read = FileImageRead::pending();
        let done = read.clone();
        let seen = Arc::clone(&seen);
        std::thread::spawn(move || {
            *seen.lock().unwrap() = Some(std::thread::current().id());
            done.finish(job());
        });
        read
    }));
    let image = wait(&loader.load(root.path(), "icons/sword.png")).unwrap();
    let worker = *reading.lock().unwrap();
    assert!(worker.is_some() && worker != Some(asking));
    assert_eq!(image.size(), (24, 16));

    let default = PluginFileImageLoader::new();
    let image = wait(&default.load(root.path(), "icons/sword.png")).unwrap();
    assert_eq!(image.size(), (24, 16));
}

// AnImageComesWithEverySmallerLevel.
#[test]
fn an_image_comes_with_every_smaller_level() {
    let root = folder();
    write(&root, "icons/sword.png", &png(8, 4));
    let image = wait(&inline().load(root.path(), "icons/sword.png")).unwrap();
    let sizes: Vec<(u32, u32)> = image.levels.iter().map(|l| (l.width, l.height)).collect();
    assert_eq!(sizes, [(8, 4), (4, 2), (2, 1), (1, 1)]);
    assert_eq!(image.path, "icons/sword.png");
}

// WindowsAskingWhileAFileIsReadShareTheOneRead.
#[test]
fn windows_asking_while_a_file_is_read_share_the_one_read() {
    let root = folder();
    write(&root, "icons/sword.png", &png(8, 8));
    let reads = HeldReads::default();
    let loader = reads.loader();
    let first = loader.load(root.path(), "icons/sword.png");
    let second = loader.load(root.path(), r"icons\sword.png");
    reads.finish_all();
    assert!(first.same(&second));
    assert_eq!(reads.started(), 1);
    assert!(wait(&first).is_ok());
}

// AFileAskedForAgainAfterItsReadIsReadAgain.
#[test]
fn a_file_asked_for_again_after_its_read_is_read_again() {
    let root = folder();
    write(&root, "icons/sword.png", &png(8, 8));
    let reads = HeldReads::default();
    let loader = reads.loader();
    let first = loader.load(root.path(), "icons/sword.png");
    reads.finish_all();
    wait(&first).unwrap();
    let again = loader.load(root.path(), "icons/sword.png");
    assert!(!first.same(&again));
    assert_eq!(reads.started(), 2);
}

// AJpegIsRead.
#[test]
fn a_jpeg_is_read() {
    let root = folder();
    write(&root, "icons/red.jpg", &red_jpeg());
    let image = wait(&inline().load(root.path(), "icons/red.jpg")).unwrap();
    assert_eq!(image.size(), (JPEG_WIDTH, JPEG_HEIGHT));
}

fn touch_later(root: &TemporaryDirectory, relative: &str) {
    let file = std::fs::File::options()
        .write(true)
        .open(root.path().join(relative))
        .unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
}

// AChangedFileComesBackAsANewVersion, and a kept image knows it is stale
// (OpenAC `VersionOf`).
#[test]
fn a_changed_file_comes_back_as_a_new_version() {
    let root = folder();
    write(&root, "icons/sword.png", &png(8, 8));
    let loader = inline();
    let old = wait(&loader.load(root.path(), "icons/sword.png")).unwrap();
    assert!(loader.is_current(&old));
    write(&root, "icons/sword.png", &png(16, 16));
    touch_later(&root, "icons/sword.png");
    assert!(!loader.is_current(&old));
    let fresh = wait(&loader.load(root.path(), "icons/sword.png")).unwrap();
    assert_ne!(old.version, fresh.version);
    assert_eq!(fresh.size().0, 16);
    assert!(loader.is_current(&fresh));
    std::fs::remove_file(root.path().join("icons/sword.png")).unwrap();
    assert!(!loader.is_current(&fresh));
}

// AFileThatCannotBeUsedComesBackWithItsReason.
#[test]
fn a_file_that_cannot_be_used_comes_back_with_its_reason() {
    let root = folder();
    write(&root, "icons/fake.png", b"not an image");
    let loader = inline();
    let missing = wait(&loader.load(root.path(), "icons/missing.png")).unwrap_err();
    let fake = wait(&loader.load(root.path(), "icons/fake.png")).unwrap_err();
    let outside = wait(&loader.load(root.path(), "../secret.png")).unwrap_err();
    assert!(missing.contains("does not exist"), "{missing}");
    assert!(fake.contains("not a PNG or JPEG"), "{fake}");
    assert!(outside.contains("outside the plugin folder"), "{outside}");
}

fn register(ui: &mut ac_plugin_ui::PluginUi, id: &str) -> u32 {
    ui.register_panel(
        &ac_plugin_ui::UiOwner::new("acme.hello", "Hello"),
        ac_plugin_ui::PanelDescriptor::new(id, id),
        "<panel/>",
        ac_plugin_ui::BindingSchema::new("Binding"),
    )
    .unwrap()
}

fn session_ui(root: &TemporaryDirectory) -> ac_plugin_ui::PluginUi {
    let mut ui = ac_plugin_ui::PluginUi::new();
    ui.set_window_attached(true);
    ui.set_file_images(Some(Arc::new(inline())));
    ui.set_plugin_folder("acme.hello", Some(root.path().to_path_buf()));
    ui
}

fn draw(ui: &mut ac_plugin_ui::PluginUi, panel: u32, paths: &[&str]) {
    ui.input(ac_plugin_ui::UiInput::FileIconsShown {
        panel,
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
    });
}

// TwoSpellingsOfOneFileAreOneImage.
#[test]
fn two_spellings_of_one_file_are_one_image() {
    let root = folder();
    write(&root, "icons/sword.png", &png(20, 10));
    let mut ui = session_ui(&root);
    let panel = register(&mut ui, "hello");
    let spellings = ["icons/sword.png", "./icons/sword.png", r"icons\sword.png"];
    draw(&mut ui, panel, &spellings);
    let first = ui.file_icon(panel, spellings[0]).unwrap();
    assert_eq!((first.1, first.2), (20, 10));
    for spelling in spellings {
        assert_eq!(ui.file_icon(panel, spelling), Some(first), "{spelling}");
    }
    assert_eq!(ui.image_count("acme.hello"), 1);
}

// TwoWindowsShowingAFileThatChangesEachKeepDrawingUntilRebuilt, on disk.
#[test]
fn a_window_rebuilt_after_its_file_changed_gets_the_new_image() {
    let root = folder();
    write(&root, "icons/sword.png", &png(20, 10));
    let mut ui = session_ui(&root);
    let first = register(&mut ui, "first");
    let second = register(&mut ui, "second");
    draw(&mut ui, first, &["icons/sword.png"]);
    draw(&mut ui, second, &["icons/sword.png"]);
    let old = ui.file_icon(second, "icons/sword.png").unwrap();
    write(&root, "icons/sword.png", &png(40, 20));
    touch_later(&root, "icons/sword.png");
    ui.remove_panel("acme.hello", first);
    let rebuilt = register(&mut ui, "first");
    draw(&mut ui, rebuilt, &["icons/sword.png"]);
    let fresh = ui.file_icon(rebuilt, "icons/sword.png").unwrap();
    assert_eq!((fresh.1, fresh.2), (40, 20));
    assert_eq!(ui.file_icon(second, "icons/sword.png"), Some(old));
    assert_eq!(ui.image_count("acme.hello"), 2);
}
