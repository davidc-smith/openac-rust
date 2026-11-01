# Plugin dock and modern look: plan

Local to this fork. It never goes in the upstream PR, so never add it to `feat/plugin-dock-v2`.

Written 2026-10-31 and meant to be read by a fresh session with no other context. Work one phase per session: the user clears the context between phases.

## How to run a phase

1. Read this whole file, then the Progress table. Do the first phase that is not Done.
2. Read the "Notes for the next phase" section, and the memory notes `plugin-dock-redo`, `ui-shapes-f3`, `upstream-contributing-rules` and `mac-live-launch`.
3. Check the plan still holds: `git fetch upstream`, see whether upstream has moved from `57a8e81`. If the ABI position below has changed, tell the user before continuing.
4. Work in a worktree on `feat/plugin-dock-v2`. It sits on upstream `57a8e81` with the three F3 commits first. Do not rebase it onto `main`.
5. Make the commits listed. Each must build and pass on its own, with a clean message.
6. Run the targeted checks and the live check for that phase. Long checks wait for the MBP, see Checks.
7. Update this file in the main checkout (`/Users/davidsmith/Code/OpenAC/openac-rust/docs/plugin-dock-plan.md`): the Progress row, commit hashes, and anything the next phase must know. Then report to the user and stop. Do not start the next phase in the same session.

Commit only what the phase lists. Push only when the user says. The PR stays held until the user says to open it.

## Goal

One upstream PR to `eriknihlen/openac-rust`, from `davidc-smith:feat/plugin-dock-v2`, holding one ordered series of commits. The phases are checkpoints, not separate PRs. Erik's rule: work that builds on itself is one PR of logically ordered commits, with no stacked PRs.

Contents:

| Tracker | What |
| --- | --- |
| F3 | Anti-aliased shapes (done, three commits) |
| F4 (part) | SemiBold weight. The plugin font API part is P8 |
| P1, P13 | Modern look for themed plugin windows, themed Confirm close X, SemiBold list headers |
| P14 | Expander framed like a group box |
| P6 | Five more themes, `is_light`, light and dark variants |
| P7 | Theme metrics and SemiBold for canvases |
| P8 | Plugin fonts |
| P2, P5 | The dock, with its gear |
| P4 | Per-window PNG dock icons |

Out of scope: P3 (SVG dock icons) and with it F5 (`resvg`), which the user left out on 2026-11-01, P9 (anti-aliased shapes on canvases), P10, P11, P12, P15 to P18, P19 (samples). Live checks use a private test plugin instead of ThemeGallery, in the style of `../openac-markertest`.

## Erik's agreement

His reply about the dock, to quote in the PR:

> Yeah sounds good. Its a bit buggy how its working now. Using icons from the dats but it is missing the underlay so icons have white borders. Havent fixed it yet. That is in the c# client. I only wish it could be minimized

He agreed the dock replaces the shelf with no option, which waives the usual "non-retail features sit behind an option". That agreement covers the dock only. The modern look and the themes need no waiver, because they only apply to windows that opted in under a theme the player picked.

The user was going to message Erik about the wider scope: the modern look, themes and new icon types, the plugin font API, the raise to API 1.4, and what else he finds buggy in the dock. Ask the user for his answers at the start of each phase and record them in Notes. He had not answered at the start of phase 5. SVG icons and `resvg` are no longer part of this.

## Rules for this work

- **No client internals in code comments or test docs**: no `gmToolbarUI::...`, `DROPITEM_*`, `UIElement_*`. Grep the diff for `gm[A-Z]\w+UI::|DROPITEM_|UIElement_` before the PR.
- **Never name private fork classes as OpenAC.** The existing code's `(OpenAC \`X\`)` comments are fine only where upstream OpenAC has `X`. `PluginDockLayout`, `PluginUiStyle`, `CanvasGeometry` and similar are in the private C# fork only, so describe what the code does instead. Check before citing.
- **Fix known limits** in the same work rather than listing them in the PR.
- **Retail outranks C#** on behaviour questions. The decompile is `ac-client-src/symbols/acclient_2013_pseudo_c.txt` in the main checkout.
- Match the surrounding code's comment density, naming and idiom. Plain, full-sentence doc comments are the local style.
- **ABI**: follow `CONTRIBUTING.md` "The plugin ABI". New fields go at the end of a record, marked `since (1, 4)`. New imports get a test through a real plugin module. Pin the older shapes in the layout tests.

## Environment

- C# fork: `/Users/davidsmith/Code/OpenAC/OpenAC`. Read it for behaviour; it is the source of the port. Its `archive/*` tags and these merges show each feature: P1 `16b0db10` and `e239a759` (controls), P2 `7832a8e7`, P3 `e57192bb`, P4 `7f1262fe`, P5 `abfd3f9c`, P6 `773f0294`, P7 `a0c1408e`, P8 `7d6f7246` `2fde2783` `866257df`, P13 `2e0894ce`, P14 `451b6f9c`.
- Dats: `OPENAC_DAT_DIR=/Users/davidsmith/AsheronsCall`.
- `cargo` needs `export PATH="$HOME/.cargo/bin:$PATH"`. Outside the repo also set `RUSTUP_TOOLCHAIN=1.95.0`.
- The shell is zsh, so `$R:crates/...` applies a `:c` modifier and breaks. Write `"${R}:crates/..."` or the branch name in full.
- `target/debug/actool dat texture <hex> --out x.png` and `dat sprites <hex,hex> --out x.png` export dat art.
- Live runs: see memory `mac-live-launch`. Local ACE only, driven through a FIFO on stdin with `/window` and `/screenshot <path>`, stopped with SIGINT.

## Checks

- Every commit: `cargo fmt --check`, `cargo clippy -p <crate> -- -D warnings`, and the tests of the crates it touches, usually `cargo test -p ac-ui -p ac-render`. ABI commits also need `-p ac-plugin-abi -p ac-plugin -p openac-plugin -p plugincheck`.
- Real-dat tests: add `-- --include-ignored` with `OPENAC_DAT_DIR` set, when a commit touches dats, rendering or the interface.
- Full `scripts/check.sh`: on Davids-MBP only, in phase 6. The branch has to be pushed to reach it, which needs the user's go-ahead. See memory `heavy-checks-on-mbp`.
- Not testable here: Windows and Linux. The PR says so.

## Facts already established

Verified on 2026-10-31, so do not redo these unless the code has moved.

**White icon borders.** `0x06002C41` (the icon MossTank and MosswartMassacre use) is A8R8G8B8 32x32 with 102 opaque white outline pixels and a transparent white fill. `surface_icon` in `crates/ac-render/src/ui/plugin_ui.rs:339` hands out the raw surface. C# resolves through `GetKeyedIcon` (`IMarkupIconResolver.cs:62`). Rust already has `IconComposer::keyed_icon` (`crates/ac-ui/src/icons.rs:341`), with no callers. It swaps pure white for the no-effect tile `0x060011C5`, which is opaque black: 102 white pixels became 0 in a probe on real dats. The result looks clean on the dark dock with a black outline. The item underlay (`0x060011D4`, a brown tile) is not used: it looks like a box inside the dock, and C# does not use it either.

**The same bug in markup.** `MarkupIcons::surface` (`plugin_ui.rs:375`) goes through `surface_icon` too, and C# keys those as well. `ImageSource::ClientArt` and `SpellIcon` at `plugin_ui.rs:960` also use it. Those are not icons a plugin names for a slot, so read what C# does before changing them.

**Minimize today.** The Rust shelf collapses with a small `>`, but `shelf.rs` has no `capture_window_state`, so it forgets on restart. The C# toggle only draws while the pointer is over the dock, so porting it as is would not give Erik what he wants.

**C# sizing rule cannot be ported.** C# works out room as `parent.Height - Top - 8`. A full dock therefore always ends level with the bottom margin, so with a bottom edge it would count as touching that edge wherever it is dragged. Room must come from the screen size, with the position clamped to fit.

**F3 API.** `CornerRadii` is per corner. `fill_vertical_gradient`, `soft_shadow` (drops downward only), `stroke_rounded_rect`, `fill_ellipse`, `smooth_line` and `triangles` exist on `RenderContext`. There is no horizontal gradient.

**Window drag.** `UiRoot::on_mouse_move` (`crates/ac-ui/src/root.rs:2212`) sets the position from the pointer on every move and clamps it to the parent, and it has no hook for a window to change that. `UiHost::tick` runs `root.tick`, then each controller's `tick`. #58 touches `root.rs`, so stay out of it where possible.

**Verified since.** Input is handled before the tick in a frame, so a snap does not lag. `fontdue` with `ttf-parser` default features off reads CFF-based OTF fonts, with `opentype-layout` named in the workspace `Cargo.toml`.

## ABI position

Settled in phase 5. Upstream released API 1.3 in v0.1.9 with #51's world triangles and markers. This branch sits on that and raises to 1.4. Its additions are `since (1, 4)`: the five theme kinds, the font imports, the font on `PaintCmd::Text` and `icon_file`. Plugins built for 1.0 to 1.3 keep loading. The phase 3 and 5 sections below still say 1.3 where they were written before this.

## Phase 1: Foundations

Commits:

1. **SemiBold weight.** Copy `assets/fonts/NotoSans/NotoSans-SemiBold.ttf` from the C# repo. Add its source and checksum to `assets/fonts/NotoSans/README.md`, and update `NOTICE.md`, which names only Regular. `bake` in `crates/ac-ui/src/bundled_font.rs:109` always uses Regular, so give it a weight. Bake both in `mount_plugin_ui` (`plugin_ui.rs:431`) and carry both alongside `modern_font`. Port the relevant cases of C# `BundledUiFontTests`, whose goldens are in `crates/ac-ui/tests/data/bundled_font`.
2. **Sharper themed text.** A second bake at the display's density, drawn on high-density screens, with measurements and so layout the same on every display. C#: `BundledUiFont.cs`, `UiDatFont.cs`. The pixels per unit come from the window since F3 (`UiRoot`, `with_pixels_per_unit`).
3. **Horizontal gradient.** `fill_horizontal_gradient` on `RenderContext`, mirroring the vertical one, with tests including a GPU readback alongside the existing shape tests in `crates/ac-render`.

Live check: Moss and Brass windows show sharper text on the Retina Mac, layout does not move, and Classic is unchanged.

## Phase 2: Modern look (P1, P13, P14)

Rust's themes are flat palette swaps today. Palette: `crates/ac-ui/src/plugin/widgets.rs:71`. Targets: `widgets.rs`, `controls.rs`, `fold.rs`, `dialog.rs`, `title_bar.rs`, `list_ext.rs`, `color.rs` in `crates/ac-ui/src/plugin/`. The C# source for the shapes is `PluginUiStyle.cs`, and the files using it are listed by `grep -rl PluginUiStyle src/AcDream.App/UI`.

Commits:

1. **Shared style helpers**, a new module: surface, button, ghost button, close button, tab, field, switch, check, slider, scroll thumbs, row, chevron, meter, window, popup and shelf shadows, header, resize grip, focus ring. Plus the hover, pressed, faded and mix colour helpers.
2. **Window chrome**: rounded corners, soft shadow, header band, SemiBold title, title bar, plain X.
3. **Lists, menus, logs, scrollbars and tabs**: pill tabs, rounded fields, menus and lists, an accent edge while a menu is open, hover and press states.
4. **Markup controls**: toggle as a switch, checkbox, radio, number, hotkey, colour and its popup, expander, split, list header and tree, context menu. Commit `e239a759` is the guide.
5. **Confirm box (P13)**: ghost close X, SemiBold heading and list headers. `plugin/dialog.rs`.
6. **Expander frame (P14)**: parse `border`, draw the frame in `plugin/fold.rs`. C# is about 140 lines. No ABI change.

Must hold: **plugin windows in Classic draw exactly as they do today.** Port C# `PluginThemeClassicIdentityTests` first, and keep it passing through every commit. Also port `PluginThemeControlTests`, `PluginThemeWindowTests`, `PluginUiStyleTests`. The C# merge also touched tooltip expiry and pointer state in `UiRoot`; port those only if a test shows they are needed.

Retail windows must not change at all.

Live check: a test plugin with every control, screenshotted in Classic, Moss and Brass. Compare Classic against a screenshot from before the phase.

## Phase 3: Themes and plugin API (P6, P8, P7, ABI 1.3)

Order matters: P8 comes before P7, because SemiBold for canvases is a weight in the font API.

Commits:

1. **Five themes (P6)**: Lavender Dusk, Ember, Bloodmoon, Glacier, Parchment. `crates/ac-plugin-ui/src/theme.rs`, using the C# palette values in `PluginUiThemeSettings.cs:131-145`. `is_light`. Raises `API_CURRENT` to 1.3, and new `ThemeKind` variants and fields are `since (1, 3)`. A plugin on 1.2 or older must be sent a kind it knows, with the real colours, because the palette is in the record. Saved theme names must round-trip. The Appearance list comes from `ThemeKind::ALL`, so check it fits.
2. **Light and dark in markup**: `color="A|light:B"`, and `.light.png` / `.dark.png` lookups. C#: `PluginImageVariants.cs`, `MarkupDocument.cs`, `PluginMarkupIconResolver.cs`.
3. **Plugin fonts (P8), ABI and SDK**: imports to get bundled Noto Sans at a size and weight, to load the plugin's own TTF or OTF, to measure and to release. A font handle at the end of `PaintCmd::Text`, `since (1, 3)`. SDK surface in `crates/openac-plugin`. Docs in `docs/plugin-development.md`. C#'s docs are at `docs/plugin-api.md:1759`.
4. **Plugin fonts, client side**: a glyph atlas per font and size, with budgets. C# allows 16 fonts per plugin; keep that. Requests are counted, so asking twice takes two releases. `bake` only accepts 8 to 32 px today, so widen it to the C# limits. Rust canvases are a retained command list, so C#'s rule about asking inside a paint callback does not apply. Fonts are released when the plugin goes.
5. **Theme metrics and canvas SemiBold (P7)**: metric constants in the SDK, the bundled SemiBold weight through P8.

Must hold:

- **A bad font file must not crash the client**: it is parsed outside the WebAssembly sandbox. Test corrupt and hostile files, and put a size limit on loading. Check whether `fontdue` can panic on bad input, and contain it if so. A one-off randomised run like F3's is worth reporting in the PR.
- **OTF**: confirm CFF fonts read. If they do not, fix that here rather than ship TTF only.
- **Older plugins**: a plugin built for 1.2 still loads, gets a kind it knows, and has no font field. Test through a real plugin module. `CONTRIBUTING.md` requires this for every new import.
- **Typed bindings, asked for by the user so they are not missed**: `BoolAction`, `OptionalBool` and `UintAction` must work from a real plugin. `crates/openac-plugin/tests/typed_ui.rs` covers the first two; no test in `crates/openac-plugin/tests` or `crates/ac-plugin/tests` names `UintAction`, so add one that sends a colour pick back through a real plugin module. No ABI change is needed for this.

Live check: all eight themes through the Appearance window, a canvas drawing with its own TTF and a bundled SemiBold, a 1.2 plugin still loading, and the three typed bindings above answering clicks on a checkbox and a colour picker. That needs someone at the pointer, as the harness cannot move it.

## Phase 4: The dock (P2, P5)

C# sources: `PluginDockLayout.cs`, `PluginSidePanel.cs`, `PluginUiStyle.Dock.cs`, and the tests `PluginDockModeTests`, `PluginDockLayoutTests`, `PluginDockChromeTests`, `PluginDockStyleTests`, `PluginDockLabelTests`, `PluginDockSettingsTests`. The Rust shelf is `crates/ac-ui/src/plugin/shelf.rs`, used from `crates/ac-render/src/ui/plugin_ui.rs` and the tests in `crates/ac-render/tests/plugin_ui.rs` and `crates/ac-ui/src/plugin/tests.rs`.

### Decisions

| Area | Decision |
| --- | --- |
| Positions | One saved setting with six values: `floating`, `floating-horizontal`, `left`, `right`, `top`, `bottom`. Unknown values load as `floating`. Stored as `dock` beside `theme` in `PluginUiSettings` (`crates/ac-host/src/settings/sections.rs:323`). |
| Orientation | Left and right are vertical; top and bottom are horizontal. Pulling off an edge keeps the orientation: top or bottom gives `floating-horizontal`, left or right gives `floating`. |
| Snapping | Snaps when the dock's edge is within 8pt of a screen edge and that is the edge nearest the pointer. Stays docked until pulled 32pt away. A docked rail slides along its edge. Moving to another edge means floating first. The saved mode changes only when the drag ends. |
| Dock size | Room comes from the screen size, not the position (see Facts). What does not fit scrolls with the wheel, along the dock's main axis, with fades at the cut ends. |
| Minimize | **Replaced in phase 4b by the icon tile, and the grip now does the moving: see Notes.** The chevron is always visible, not only on hover. Floating shrinks to a small pill (48x24, or 24x48 when horizontal). A rail shrinks to a 12x44 tab, 44x12 on top and bottom. Saved per character through `capture_window_state` and `restore_window_state`. Plugin windows stay open. Toggle Plugin Manager (Shift+Ctrl+F1) still hides it fully, and showing it again also un-minimizes, as C# does. |
| Icons | Dat icons use `keyed_icon`, no underlay. |
| Hover labels | Beside the slot, on the side away from the screen edge, and kept on screen on both axes. C# does not clamp, which would run labels off screen on a horizontal dock. Replaces the slot tooltips. Drawn in the overlay pass. Title in SemiBold, plugin name under it in Regular unless the two match. |
| Other windows | The dock sits over retail windows and never pushes them aside. |
| Saved layouts | The saved window name stays `plugin-shelf` (`shelf::WINDOW`, `layout_persistence.rs:168`), so existing placements survive. |
| Naming | The module and type become `dock`. ABI names such as `show_in_side_panel` stay. Fix player-facing text that says "shelf" or "sidepanel": `chrome.rs:42` and the hidden message in `plugin_ui.rs`. |
| Monograms | Two letters at most, on one of eight hues picked by FNV-1a of the plugin id. The Rust `initials` takes three today, and its test changes. |
| Classic | The dock uses its own Classic palette (`ClassicDock`, `PluginUiThemeSettings.cs:152`). This is the one place Classic changes, as Erik agreed. |

### How snapping works

Do it in the controller's `tick`, not in `root.rs`. The root rewrites the position from the pointer on every move, so the tick can read it, decide the mode, resize and put the dock back on its edge. This keeps the PR out of #58's file. Confirm input runs before the tick in a frame, and report if not. Use `ui.is_window_move_active()` to see the drag end. Tell `plugin_ui.rs` about a mode change the way `take_appearance_request` does, and save through `settings_changes` with `Section::PluginUi`, as `pick_plugin_theme` does. Use `ui.mouse()` for the pointer. Grip dims when `ui.ui_locked()`.

Edge shapes: a rail rounds only its two inner corners and has no rim on the screen side. Top rail rounds the bottom corners, bottom rail the top ones. Edge pills and open dots sit on the screen-edge side, and pills grow along the main axis. `soft_shadow` only drops downward, which is fine for a bottom rail because its spread still shows.

### Commits

1. **Keyed dat icons** on the dock and in markup icons. Pass the composer into `WindowShelfIcons` and `WindowIcons`. Real-dat test that `0x06002C41` has no opaque white pixels left.
2. **Layout function**: pure, working on main and cross axes, with all six modes. Port the C# layout tests and add horizontal cases.
3. **Dock replaces the shelf**: body, wells, pills, dots, dividers, fades, gear, monograms, hover labels, floating vertical only. Rename here. This is where the look changes.
4. **Snapping and edges**: four edges, top and bottom shapes, horizontal mode. Port `PluginDockModeTests`, adding top and bottom, corners, a slow drag that changes mode only once per edge, and a dock long enough to reach both edges.
5. **Minimize**: always-visible chevron, saved state.
6. **Setting and menu**: the `dock` setting and a dock menu in Plugin appearance (`show_plugin_appearance`, `sync_appearance`, `follow_appearance` in `plugin_ui.rs`). Port `PluginDockSettingsTests`.

Live check: all six positions in all eight themes, minimized and expanded, with real plugins including a dat icon. Restart to confirm the position and minimize are remembered. Resize the window with a right and a bottom rail. Check overflow scrolling on a horizontal dock.

## Phase 5: Dock icons (P4)

Done. The user left SVG out on 2026-11-01, so the listed `resvg` work, and the installer and `plugincheck` commit that only existed to accept `.svg`, were not done. Order on the dock: the window's own PNG (`icon_file`), the plugin's `icon.png`, the dat icon (keyed), the monogram. A named `.svg` is refused and logged, and the next icon is drawn.

## Phase 6: Finish

1. Rebase check: does the branch still merge with upstream, #51 and #58? Say what conflicts.
2. Grep the diff for client internals and private C# class names.
3. Full `scripts/check.sh` on the MBP at the tip, and fmt, clippy and tests on each commit. The rebase rewrote every commit and only the tip has been built since, so this sweep matters more than it did: expect to fix an intermediate commit or two. Needs the user's go-ahead to push.
4. Fork `main` has not taken upstream `57a8e81` yet, and still holds #51 and #58 as its own merges, so sync it first and expect conflicts in the API version. Merge into fork `main` with `git merge --no-ff`, as a topic branch merge. Remove the worktree.
5. Update `docs/csharp-fork-port-tracker.md`: the rows above, plus the top and bottom edges and minimize, which are new beyond the C# dock. Memory `ui-shapes-f3` and `plugin-dock-redo` too.
6. Final live test with the user, of everything. Look together at **Classic expanders, which have no frame**. C# frames them in Classic and this port does not, because Classic windows must not change. The user will judge then whether that looks odd, and it can be adjusted. Drawing it would mean re-pinning the Classic hashes in `crates/ac-ui/tests/data/plugin_classic/pinned.txt`, on the user's say only.
7. Write the PR text for the user to review. The old F3 copy in `ui-shapes-f3` is out of date: its title, "Nothing draws with these yet", and the temporary-patch live test no longer hold. It must quote Erik's agreement, state the raise to 1.4 and that older plugins keep loading, say Classic windows are unchanged, and say it is not tested on Windows or Linux.

**Do not open the PR until the user says so.**

## Estimated size

About 10,000 to 13,000 added lines including tests, of which F3 is 2,700.

## Progress

| Phase | Status | Commits | Notes |
| --- | --- | --- | --- |
| Plan | Agreed 2026-10-31 | | Hashes below are from the rebase of 2026-11-01. The older notes further down use the old ones, and `feat/plugin-dock-pre-rebase` (`a664b20`) keeps them |
| F3 | Done | `7da4050`, `36f42b8`, `fcf7c82` | MBP check passed at `f9c403a`, on its own. Has to run again at the new tip |
| 1. Foundations | Done 2026-10-31 | `b6ae9a4`, `49d721c`, `350657d` | Tests and real-dat tests pass at the tip, 2931 in `ac-ui` and `ac-render`. Live on the Air's Retina screen. Nothing pushed |
| 2. Modern look | Done 2026-10-31 | `c26a003`, `29b440b`, `a870f0d`, `f7408c6`, `d96c086`, `da0b8d0` | Tests and real-dat tests pass at the tip, 2986 in `ac-ui` and `ac-render`, 264 in `ac-plugin-ui`. Live in Classic, Moss and Brass on the Air's Retina screen. Pushed to `origin` at `c3b81a9` |
| 3. Themes and plugin API | Done 2026-10-31 | `06985c5`, `0707529`, `58bd47c`, `efb70f6`, `7170b08`, `82acdf5` | Tests and real-dat tests pass, 4261 in the crates touched; after the last fold, which changed only a test and docs, `ac-ui` and clippy were rerun. Screenshots good in all eight themes. Pointer retest by the user passed. Nothing pushed |
| 4. Dock | Done 2026-10-31 | `31494a7`, `dbb1c6c`, `8e4b770`, `48b3970`, `2471292`, `9b8f42a` | fmt, clippy and tests with real dats pass at each commit, 3608 at the tip in `ac-ui`, `ac-render`, `ac-host` and `ac-plugin-ui`, 324 in the plugin crates. All six positions, minimized and expanded, in all eight themes on the Air's Retina screen, through the probe's pointer. By-hand retest with a real mouse passed. Nothing pushed |
| 4b. Closed design | Done 2026-10-31 | `0cfb7da`, `b6f704c`, `051caca`, `b3dbaa4`, `0f2edfd`, `fdc44fa` | The icon tile, three sizes, hover or click, then the clear crescent and the grip. Tests with real dats pass at the tip, 3328 in `ac-ui`, `ac-render` and `ac-host`; `2ef008b` ran clippy, the icon tests and the dock tests on its own. Seen live: Parchment at the default size, Classic bigger, Glacier smaller, through the lock, grip, tile, a closed drag, and left and top rails (top seen on Parchment). By-hand retest with a real mouse passed. Nothing pushed |
| 5. Dock icons | Done 2026-11-01, no SVG | `12b68d4`, `323425e` | Rebased onto upstream `57a8e81` and raised to 1.4 first, which rewrote every hash above. fmt, clippy over the workspace and tests with real dats pass at the tip, 4782 in the crates touched. Live on the Air's Retina screen: Classic, Moss and Parchment, and Moss at UI Scale 110% and 150%. Nothing pushed |
| 6. Finish | Steps 1 to 5 done 2026-11-01. Steps 6 and 7 wait on the user | | Tip is `3f62730`. See the first note below |

## Notes for the next phase

- **Start here: phase 6, step 6.** Steps 1 to 5 are done. What is left is the final live test with the user, then the PR text and the user's say to open it.
  - **Hashes.** The MBP passed `scripts/check.sh` (10849 tests) and fmt, clippy and tests at all 32 commits at `323425e` on 2026-11-01. Rewording the comments below rewrote 20 commits, so the tip is now `3f62730`, with only comments, one test name and one commit message changed. The Progress table's hashes from `06985c5` up are old; `git log upstream/main..feat/plugin-dock-v2` has the current ones. After the rewrite, fmt passes at all 32 and clippy and tests pass in `ac-plugin-ui` and `ac-ui`, the crates the changes touch (2312 tests, without the real-dat ones).
  - **Not pushed.** `origin/feat/plugin-dock-v2` is still `323425e`, so the fork needs a force-push or another new name before the PR, on the user's go-ahead. `upstream-main` and `main` were also changed locally only. `origin/feat/plugin-dock` holds the old `d5f6c52` and nothing uses it.
  - **Comments reworded.** Against upstream OpenAC `34d06e38`: `PluginImageVariants`, `PluginFontTable`, `CanvasFontBaker`, `CanvasFont`, their tests, `PluginThemeMetrics`, the `IsLight` members and two test-name citations are private, and the five new palettes are not upstream's, so `the_new_palettes_are_openacs_bytes` was renamed and the commit message that said so changed. `RetailUiRuntime.TogglePluginManager` stays: it is upstream Rust's own comment, with only the word shelf changed to dock. The grep is against every PascalCase name in an added comment, so a private name in plain words would still slip through.
  - **Fork `main`.** Synced with upstream `57a8e81` in `25bc6b2`, conflicts in `state.rs` and `batch.rs`, then the branch merged in `9c5bc4f`, conflicts in five files F3 touched. `crates/` on `main` matches the branch tip exactly: the only differences are `.gitignore`, `BRANCHING.md` and the two docs. So F3's old hashes stay in `main`'s history, as `feat/ui-shapes`' do.
  - **Worktree.** `../openac-rust-dock` is removed. To change the branch, `git worktree add ../openac-rust-dock feat/plugin-dock-v2`, and merge into `main` again.
  - **PR text.** Step 7's draft is in `docs/plugin-dock-pr.md`, with two `TODO` lines to fill in after the live test and a check at the final tip.
  - **Live test.** `sh phase6/retest.sh [theme]` in `../openac-docktest`, on `phase6/client-phase6` built at `3f62730`. Look at Classic expanders with the user, which have no frame, see step 6.
- **Rebase, for the PR text.** Upstream's UI Scale option (50% to 300%) was new to this work, and it has not been checked everywhere. The batcher now snaps quads and polygons to whole pixels when a unit is not one pixel, and coloured triangles are deliberately multiplied but not snapped, because a snap folds their one-pixel soft edge. `UiRoot`'s pixels per unit includes the player's scale, so soft edges and sharper bakes follow it. Seen in Moss at 110% (2.2 pixels a unit on Retina) and 150%. Not seen: Classic or a light theme at a scale, or pressed and hover states at one. A new density rebakes the sharper fonts, so stepping the option bakes each time, and that cost was not measured.
- **A rule phase 4b broke, fixed in the rebase.** `ac-render` had taken a dependency on `ac-plugin` for the PNG decoder, which `ac-plugin/tests/crate_graph.rs` forbids. Phase 4b ran only the `ac-ui`, `ac-render` and `ac-host` tests, so it was not caught. The decoder moved to `ac-plugin-ui` in a new commit (`0cfb7da`), ahead of the tile. `ac_plugin::icon` still names it. A window-side change should also run the `ac-plugin` tests.
- **Older plugins.** Live, a real module built by upstream's released 1.3 SDK (`phase5/uidemo-api-1.3.wasm`, its `uidemo`) and the older real 1.2 module both load and show on the dock. Tests also go through `rawbytes` built for 1.2 and 1.3.
- **Phase 5, design.** The session reads `icon_file` when the window registers, synchronously through `FileImageLoader::read_dock_icon`, which is safe at 64 KiB and does not wait on a pipe. C# reloads a changed file when the interface rebuilds. Here a changed file is seen when a window registers, so on a plugin reload. A version may exist with no named file: it draws under its theme and the other theme goes to the next icon. A path refused along with its versions for one reason logs once. `icon.png` has no light and dark versions, as in C#. Dock textures are kept by picture, not by plugin, and given back when nothing holds the pixels. They now have smaller copies, which also improves `icon.png`.
- **Live harness, phase 5:** `phase5/try.sh <theme> <run>` on `phase5/client-phase5`, output in `phase5/${BATCH:-a}/<run>`, with `ICONPLUG=1` (a second copy with an `icon.png`), `UPSTREAM13=1` and `UI_SCALE=<percent>` for `run.sh`. Theme names are case-sensitive: `moss` silently came up Classic, so use `Moss`. `phase5/a/` holds the shots. The plugin now has a window for each step of the icon order and each refusal.
- **Phase 4b, the user's changes on 2026-10-31.** They said the tile moving the dock as well as opening it was confusing, because of the move cursor over a button, and that the crescent's black square looked wrong on Parchment. They left the timing of the icon to Claude, who did it now: phase 5 is about icons a plugin supplies, and does not touch this one. The user retested by hand on 2026-10-31 and said "looks good - happy with it as is", which also accepts the slide below. To retest: `sh phase4b/retest.sh [theme]` in `../openac-docktest`, on the client at `a664b20`. Click the lock on the radar to see the grip, and quit Magnet first (memory `mac-live-launch`, tracker M4).
  - **Grip.** It comes first on the dock, 14 points long at every size, whole width across, with two rows of four dots. It shows only while windows are unlocked, the same condition as the radar's move button. Only it moves the dock or shows the move cursor. `PRESS_SLOP` is gone, along with the hold logic, because a press on the grip has no other meaning. `follow_drag` is back to its phase 4 form.
  - **Tile.** An ordinary button, the same locked or not. A press and release on two places is not a click.
  - **Hover.** The grip of a closed dock does not peek it open, so it can be moved closed. A dock already peeking stays open on the grip.
  - **Decided by Claude, accepted by the user: the dock keeps its top-left place when the lock changes, so its icons slide 14 points along it.** Keeping the icons still would mean moving the window on each change, and a saved place cannot say whether it was saved with the grip, so it would drift on restarts and against screen edges.
  - **Icon.** `assets/icons/crescent-256.png` is the same crescent and ring, cut from the 1024 px launcher art in the C# repo (`assets/icons/acdream-launcher-1024.png`). The launcher and app bundle keep `launcher-256.png`. The cut was made with throwaway scripts: background told from metal by its warm tint and by brightness, then colour unblended from the local background.
  - **Seen in the test run, unrelated:** `plugin::repeat_tests::a_150_row_repeat_builds_reconciles_and_lays_out_within_bounds` failed once on a timing bound during a heavy run, then passed three times alone and in two full runs. It was not investigated further.
- **Phase 4 retest.** The user passed phase 4's by-hand retest on 2026-10-31 and confirmed the dock stays hidden on the character list, including after logging off, then chose the closed design below. To retest: `NOCHAR=1 sh phase4/retest.sh [theme]` in `../openac-docktest`, which stops at the character list. Click the lock on the radar first, or the dock will not move.
- **Phase 4b, what changed:** three commits on top, so the series adds the chevron in commit 3 and takes it out in `b613181`. Phase 6 can fold them into the earlier commits if that reads better for Erik. The icon is `assets/icons/launcher-256.png`, the silver crescent, which the user picked over `client.ico`. It had a dark square of its own, which showed inside the tile on Parchment, so `2ef008b` swapped it for a clear cut. `ac-render` now depends on `ac-plugin` for its PNG decoder. The bundled font bakes three more sizes at mount: 10 for the count, 12 and 20 for monogram letters, each with a sharper bake. The dock no longer dims while windows are locked, because a press still works then. The saved settings are `dockSize` and `dockOpens`, beside `dock`.
- **Character list bug, fixed in `96036c2`:** the pre-game screens hide every game window on each feed, and the dock's per-frame visibility check put itself back before the frame drew. `set_covered` tells the dock. `tests/pregame_ui.rs` has a real-dat test that failed before the fix. Live runs with `--character` never pause on that screen, so a live check of anything pre-game needs `NOCHAR=1`.
- **Phase 4b, the design the user chose on 2026-10-31**, from the mockup at https://claude.ai/artifact/9JjwD66W2QaGgPxGHR7wU3:
  - Closed, the dock is the icon tile, one slot in size, with no chevron, pill or tab. The tile shows the OpenAC icon from the repo and a count of open plugin windows.
  - A setting chooses how it opens: pressing the tile opens and closes it, or hovering peeks it open.
  - Three sizes, smaller, default (today's) and bigger, measured as on the mockup. The default does not change.
  - Every dock setting lives in the Plugin appearance window: theme, position, size and how it opens.
  - The mockup itself is on the local throwaway branch `prototype/plugin-dock-options`, in `docs/prototypes/`.
- **Phase 4b, filled in by Claude where the user did not say:** the tile stays where it is when the dock opens, as the dock's first item, and there is no separate handle row. Pressing the tile works in both settings, and hover only adds a temporary peek, so a dock can still be kept open with hover on. The count only shows on a closed tile, and not at zero, since an open dock already marks each open window. Dragging the tile moved the dock, with a press counting only within 4 points, until the grip replaced that.
- **Phase 4, not seen on screen:** the wheel scrolling an overflowing dock (the probe has no wheel command, and the by-hand retest had too few windows to overflow), which only tests cover. The user's by-hand retest covered resizing with a right and bottom rail, Toggle Plugin Manager and a right click. The overflow itself, with its fades and labels, was seen on top, bottom and floating docks with 36 windows.
- **Phase 4, for the user to rule on:** keying markup icons (commit 1, as this plan says, and as C# does) means a Classic plugin window that names a dat icon draws that icon's white pixels as black. Everything else in Classic plugin windows is unchanged: the guard's hashes were not touched, and phase 3's client against phase 4's on a 1.2 plugin showed no bright pixel added or lost and no difference on shared ones. That plugin names no dat icon, so the comparison does not show this one case. Separately, Toggle Plugin Manager's hidden state is still not remembered across restarts, as today. C# does save it, but older saved layouts hold `visible: false` from runs with no plugin windows, so honouring that could hide the dock with no sign of why.
- **Phase 4, design changes from this plan:** a rail holds by the pointer, not its own place: it floats once the pointer is 32 points further from the edge than the closest it has been in the drag. A tall dock that snaps by its far end shrinks well away from the pointer, so going by the rail's place floated it again on the next move, which a slow-drag test caught. A floating dock's longest is the screen less 24 points at each end (`layout::FLOATING_MARGIN`), so a full one is not always within reach of an edge. A tab's handle is 20 long, not C#'s 14, because the outer dot fell in the rounded corner. The chevron is always visible from commit 3, so commit 5 is mainly the saved state. The gear and a right click now toggle the Appearance window, as C# does. The dock raises itself above other windows each frame unless a modal one is up, so plugin windows go under it too, not only retail ones. No root change was needed. Input reaches the host before the frame's tick (`surface.rs` `cursor_moved`, then `render`), so a snap does not lag. A whole drag between two frames, which a slow frame can cause, also snaps.
- `ImageSource::ClientArt` and `SpellIcon` were left unkeyed: C# draws client art raw, because a plugin asked for that art by name.
- **Live harness, phase 4b:** `phase4b/try.sh <theme> <run> [script] [last shot]` is `walk.sh` on `phase4b/client-phase4b`, giving up after 45 s if the window is covered. Output goes to `phase4b/${BATCH:-a}/<run>`. `phase4b/grip.txt` clicks the lock twice, so the character ends locked as found, and its coordinates fit all three sizes from the default floating place. `phase4/tile*.txt` and the other phase 4 scripts assume no grip. The first run this session sat covered for its whole length with no screenshots, and a retry worked.
- **Live harness, phase 4:** `phase4/walk.sh <theme> [run] [script] [last shot]` runs a probe script from `phase4/*.txt` and stops when the last screenshot appears. Output goes to `phase4/${BATCH:-r2}/<run>`, a fresh folder per batch, because it deletes nothing. `run.sh` takes `DOCK=`, `MANY=<n>` (more plugins, to overflow) and `KEEP=1` (a second run on the same root and settings). The themed window now names the dat icon `0x06002C41`. The final shots are in `r2`, `r4` and `walk-*`; those `walk-*` from the first pass are good only for Classic, Lavender Dusk, Bloodmoon and Parchment, and `r2` has the other four.
- **Traps for scripted runs:** the UI lock is saved per character on the server, so a script that clicks the lock toggles it every run and every other run's drags do nothing. `phase4/toggle-lock.txt` changes it once, and the character is back to locked as found. A long dock is clamped upward, so its handle is not at its saved top: with 36 windows it is at 76. The probe's `drag at` falls between two frames. An Appearance window with no saved placement came up centred, not at its markup's place; this is the older "seen, not investigated" note below, still not looked into.
- **Disk:** the main `target` reached 55 GB with 6.7 GB free, so it was cleaned, under the user's earlier permission. 30 GB is free after the full run.
- **Erik** had still not answered at the start of phase 4, and the user said to go ahead.
- **Phase 3's retest** passed on 2026-10-31 with a real mouse, in one window (`phase3/retest.sh`). Each checkbox alternated, which only happens if the plugin's stored value came back, `SetColour` arrived twice, and all eight themes were picked through Appearance. The user confirmed it on screen: "all worked".
- **The first click-through's report**, for the record: checkboxes and the colour appearing dead came from `docktest` not storing values, because bound controls show only what the plugin stores. Dock options were a wrong instruction: the gear is phase 4, and the shelf opens Appearance by right-click. The first run's odd log (five `false`, no `SetColour`) was never reproduced. The likely cause is the second, identical window, the silent 1.2 module, but that is unproven. Keep live checks to one window where a pointer is involved.
- **Scripted pointer:** `OPENAC_UI_PROBE_SCRIPT` with `click at <x> <y>`, `sleep` and `screenshot <name>`, plus `OPENAC_AUTOMATION_ARTIFACT_DIR`, clicks in the real client with the real plugin. `phase3/clicks.txt` is an example. This corrects the older note that the harness cannot move the pointer, and it will be useful for the dock's drag and snap checks. It goes through the window's input code, not the OS mouse, so a by-hand check is still the last step.
- **Before and after:** `phase3/client-phase3` is the client at the end of phase 3, and `phase4/client-phase4` at the end of phase 4, ready to be phase 5's before.
- **Erik** had still not answered at the start of phase 3, and the user said to go ahead.
- **Commits, phase 3:** themes and the raise to 1.3; light and dark colours and files in markup; plugin fonts in the session, ABI and SDK; fonts drawn on canvases; theme metrics; and a sixth commit the plan did not list, a test of typed bindings through a real module. They are local, on `feat/plugin-dock` in `../openac-rust-dock`.
- **Bugs found on the way:** `ui_theme` was missing from the host's list of UI imports, so a real plugin always read Classic. This is upstream too, fixed in the first commit. Say so in the PR.
- **Design changes from this plan:** fonts bake in `ac-plugin-ui` (`src/fonts.rs`, `src/state/fonts.rs`), not on the window side, so the client and the SDK's fake host answer alike and a frame never waits for a bake. The Noto files moved there too. The window sends `UiInput::DisplayDensity`, and the session makes the sharper bakes. Fonts have their own baker with an atlas ladder, so `bundled_font::bake` keeps its 8 to 32 px limit rather than being widened. A plugin hands over font bytes (usually `include_bytes!`); there is no path-based load, so the installer needs no new file types for fonts. An older plugin is told `ThemeKind::Moss` under any new theme, with the real name and colours. `ThemeKind` in the ABI has a hand-written `Wire`, and the local `wire_enum!` gained `; since (MAJOR, MINOR)` for a last field.
- **Facts now verified:** CFF `.otf` fonts read and kern with `ttf-parser` default features off, plus `opentype-layout`, which is now named in the workspace `Cargo.toml`. A one-off fuzz of damaged fonts, 1,176,600 runs on the CFF fixture and Noto over 180 s: no crash, no parser panic, slowest bake 0.15 s, peak 76 MB. The glyph-size check refused outlines as large as 23730 x 1014 before allocating. This is worth reporting in the PR.
- **Classic:** the Classic guard still passes. On the same 1.2 plugin, phase 2's client against phase 3's: no bright pixel appeared or vanished, shared ones differ by at most 1 level. `phase2/after-Classic.png` predates the Indeterminate checkbox, so do not compare with that file; use `baseline/client-phase2` with `ONLY12=1`.
- **Live harness, phase 3:** a window that is fully covered stops drawing (`window: surface occluded`) and takes no screenshot, so it needs no focus but must stay partly visible. Check the log for `ScreenshotSaved`. `tools/live.sh` now waits 12 s after asking for a screenshot. `run.sh` also installs `phase3/docktest-api-1.2.wasm`, a real module an API 1.2 SDK built, as `test.dock12`, and `ONLY12=1` runs it alone. The client is `phase3/client-phase3`, and the shots are in `phase3/`. The plugin has a font canvas and logs `docktest action` and `docktest theme changed`.
- **Checks:** run tests with `--no-fail-fast`. `cargo test` stops at the first failing binary, which hid two failures in the first commit until they were fixed by rewriting it. Only the tip of the second commit and the tip of the branch ran the full plugin set, so the per-commit sweep in phase 6 still matters.
- **Disk:** the Air fell to 1.3 GB free during phase 3's final test run. Deleting `target/debug/incremental` brought it back to 11 GB. Watch `df` before long runs.
- **Scope, confirmed by the user on 2026-10-31:** the plan stands as written. Themed plugin windows do get the modern look. The Command, Spell, Target, Chat and Gear Decks (tracker H1 to H13) are not part of this. Classic plugin windows must stay exactly as they are.
- **Erik:** still ask at the start of each phase.
- The stale `feat/ui-shapes` branch on the fork is the user's call to delete.
- **Worktree:** `../openac-rust-dock`, on `feat/plugin-dock-v2` (named `feat/plugin-dock` until 2026-11-01). The user had it pushed to `origin` (the fork) at `c3b81a9` on 2026-10-31, as a copy on GitHub. Later pushes still need their go-ahead, and it never goes to `upstream` until the PR.
- **Disk:** the Air has about 20 GB free. Build the worktree into the main target so dependencies are shared: `export CARGO_TARGET_DIR=/Users/davidsmith/Code/OpenAC/openac-rust/target`. Watch `df`: about 11 GB was free at the end of phase 2. `target/debug/incremental` reached 20 GB, and deleting it only costs rebuild time.
- **Classic guard:** `crates/ac-ui/src/plugin/theme_tests.rs` draws a gallery of every control through 27 states in two window shapes, plus a dialog, and compares hashes of the draw lists with `crates/ac-ui/tests/data/plugin_classic/pinned.txt`. Those hashes came from the drawing code before phase 2. Never regenerate them to make a test pass: a change there means Classic changed. Phase 4's dock is the one agreed exception, and the hashes did not need to change for it: the gallery draws a window's minimize button, which still looks the same in Classic.
- **Style:** the shapes are in `crates/ac-ui/src/plugin/style.rs`. Control tests are in `plugin/theme_control_tests.rs`, and `style::tests::{corners, has_color}` are the helpers. `Field`, `Menu`, `Meter` and `Scrollbar` in `crates/ac-ui/src/widgets` carry a `theme_palette` that only a themed plugin window sets, so retail windows never reach the style.
- **Light themes:** at rest, Parchment's slider thumb, check tick and header band look right in a screenshot. Pressed and hover states have not been seen on it, apart from the dock's label and the menu's hover row in phase 4. `style::shelf_shadow` is gone: the dock has its own shadow in `dock/style.rs`.
- **Chose differently from C#:** C# also frames expanders in Classic. Here the frame only draws under a modern theme, because Classic windows must not change. `border` is parsed either way. Also beyond C#: rounded hex field in the colour picker, and disabled fields, numbers, hotkeys, swatches and menu captions fade with their faces.
- **Not ported:** the tooltip expiry and `UiRoot` pointer changes from the C# merge. No test showed they are needed.
- **Typed bindings, not a gap:** a checkbox's `onchange` (`BoolAction`), an indeterminate checkbox (`OptionalBool`) and a `uint` colour's `onchange` (`UintAction`) already work: they go through `PanelSpec::bind_typed` and `TypedBindingKind`, not `bind` and `BindingKind`. An earlier note here called this an SDK gap, which was wrong. The test plugin now binds all three, and it loaded and drew the indeterminate box live on 2026-10-31. See phase 3's must-hold list for what is still to check.
- **Live harness:** `../openac-docktest` is a private plugin with a themed window and a window that does not opt in. Rebuild with `cargo build --release --target wasm32-unknown-unknown`. `tools/live.sh <client> <theme> <run name> <output dir>` runs a client under a theme and takes a screenshot. Run it with the sandbox off, on one line per run, because zsh does not split a variable into arguments. Allow 50 seconds to get into the world; one run at 45 produced no screenshot. The plugin now has every control a plugin can bind, in a 620x560 window, and `run.sh` places it to match. In that gallery "A scrolling group" sits inside the framed expander because the folded expander moves it up, in Classic too: that is the test's layout, not a bug. The harness cannot move the pointer, so hover, press, focus, popups and dialogs have only been checked through draw-list tests, not on screen.
- **Before and after:** build the client at the last commit before the phase, copy the binary out of the target, then compare. Classic windows are translucent and the world moves behind them, so a plain pixel diff of a Classic window is noisy. `tools/mask.py <tools dir> <before.png> <after.png>` compares only bright opaque pixels. Differences at the window's outer edge are the world. Phase 1's shots are in `baseline/`, phase 2's in `phase2/`, and `baseline/client-phase2` is the client at the tip of phase 2, ready to be phase 3's before. Phase 2's Classic comparison: no bright pixel appeared or vanished, and shared ones differed by at most 1 level in 255. The exact guarantee has to come from draw-list tests, which is why the Classic identity tests come first in phase 2.
- **Seen, not investigated:** a plugin window with no saved placement came up centred rather than at its markup's `x` and `y`, though the real-dat test's window does honour them. `run.sh` saves placements to avoid it. It was seen on this branch only, and nothing in phase 1 touches placement. If it matters for the dock, look at it there.
- `scripts/check.sh` has not run on these commits. It is due on the MBP in phase 6, along with fmt, clippy and tests on each commit. Phase 2's first commit only ran the `ac-ui` plugin tests, as it adds tests and a module nothing calls; every later commit ran the crates it touches, apart from the sixth, which ran the `ac-ui` and `ac-plugin-ui` library tests before the full run at the tip.
