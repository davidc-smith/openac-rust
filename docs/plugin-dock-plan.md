# Plugin dock and modern look: plan

Local to this fork. It never goes in the upstream PR, so never add it to `feat/plugin-dock`.

Written 2026-10-31 and meant to be read by a fresh session with no other context. Work one phase per session: the user clears the context between phases.

## How to run a phase

1. Read this whole file, then the Progress table. Do the first phase that is not Done.
2. Read the "Notes for the next phase" section, and the memory notes `plugin-dock-redo`, `ui-shapes-f3`, `upstream-contributing-rules` and `mac-live-launch`.
3. Check the plan still holds: `git fetch upstream`, see whether upstream has moved from `198debb`, and whether #51 or #58 have merged. If the ABI position below has changed, tell the user before continuing.
4. Work in a worktree on `feat/plugin-dock`. Phase 1 creates that branch from `feat/antialiased-shapes` (tip `f9c403a`, which is off upstream `198debb`), so the three F3 commits stay first. Do not cut it from `main`.
5. Make the commits listed. Each must build and pass on its own, with a clean message.
6. Run the targeted checks and the live check for that phase. Long checks wait for the MBP, see Checks.
7. Update this file in the main checkout (`/Users/davidsmith/Code/OpenAC/openac-rust/docs/plugin-dock-plan.md`): the Progress row, commit hashes, and anything the next phase must know. Then report to the user and stop. Do not start the next phase in the same session.

Commit only what the phase lists. Push only when the user says. The PR stays held until the user says to open it.

## Goal

One upstream PR to `eriknihlen/openac-rust`, from `davidc-smith:feat/plugin-dock`, holding one ordered series of commits. The phases are checkpoints, not separate PRs. Erik's rule: work that builds on itself is one PR of logically ordered commits, with no stacked PRs.

Contents:

| Tracker | What |
| --- | --- |
| F3 | Anti-aliased shapes (done, three commits) |
| F4 (part) | SemiBold weight. The plugin font API part is P8 |
| F5 | Decision: use `resvg` |
| P1, P13 | Modern look for themed plugin windows, themed Confirm close X, SemiBold list headers |
| P14 | Expander framed like a group box |
| P6 | Five more themes, `is_light`, light and dark variants |
| P7 | Theme metrics and SemiBold for canvases |
| P8 | Plugin fonts |
| P2, P5 | The dock, with its gear |
| P3, P4 | SVG and per-window PNG dock icons |

Out of scope: P9 (anti-aliased shapes on canvases), P10, P11, P12, P15 to P18, P19 (samples). Live checks use a private test plugin instead of ThemeGallery, in the style of `../openac-markertest`.

## Erik's agreement

His reply about the dock, to quote in the PR:

> Yeah sounds good. Its a bit buggy how its working now. Using icons from the dats but it is missing the underlay so icons have white borders. Havent fixed it yet. That is in the c# client. I only wish it could be minimized

He agreed the dock replaces the shelf with no option, which waives the usual "non-retail features sit behind an option". That agreement covers the dock only. The modern look and the themes need no waiver, because they only apply to windows that opted in under a theme the player picked.

The user was going to message Erik about the wider scope: the modern look, themes and new icon types, the plugin font API, the `resvg` dependency, the raise to API 1.3 (which also answers tracker F1), and what else he finds buggy in the dock. Ask the user for his answers at the start of each phase and record them in Notes. A "no" to `resvg` changes only phase 5.

## Rules for this work

- **No client internals in code comments or test docs**: no `gmToolbarUI::...`, `DROPITEM_*`, `UIElement_*`. Grep the diff for `gm[A-Z]\w+UI::|DROPITEM_|UIElement_` before the PR.
- **Never name private fork classes as OpenAC.** The existing code's `(OpenAC \`X\`)` comments are fine only where upstream OpenAC has `X`. `PluginDockLayout`, `PluginUiStyle`, `CanvasGeometry` and similar are in the private C# fork only, so describe what the code does instead. Check before citing.
- **Fix known limits** in the same work rather than listing them in the PR.
- **Retail outranks C#** on behaviour questions. The decompile is `ac-client-src/symbols/acclient_2013_pseudo_c.txt` in the main checkout.
- Match the surrounding code's comment density, naming and idiom. Plain, full-sentence doc comments are the local style.
- **ABI**: follow `CONTRIBUTING.md` "The plugin ABI". New fields go at the end of a record, marked `since (1, 3)`. New imports get a test through a real plugin module. Pin the older shapes in the layout tests.

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

**Not yet verified.** Whether input is handled before the tick in a frame (if not, a snap lags by one frame). Whether `fontdue` with `ttf-parser` default features off reads CFF-based OTF fonts. C# has a fixture at `tests/Fixtures/fonts/NotoSansCffFixture.otf`.

**Versions.** `resvg` and `usvg` 0.49.0 are `Apache-2.0 OR MIT`. `tiny-skia` is `BSD-3-Clause`. All are on the accept list in `about.toml`.

## ABI position

Upstream is on API 1.2. Fork `main` shows 1.3 only because it includes #51, so do not read `main` to learn the upstream version. This branch stays off upstream and raises to 1.3 itself, in phase 3. Whichever of this PR and #51 merges second combines the additions under 1.3, as tracker F1 proposes. Expect mechanical conflicts in `version.rs`, the layout fingerprints, `crates/ac-plugin/tests/import_census.txt`, canvas commands and `plugin_ui.rs`.

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
| Minimize | The chevron is always visible, not only on hover. Floating shrinks to a small pill (48x24, or 24x48 when horizontal). A rail shrinks to a 12x44 tab, 44x12 on top and bottom. Saved per character through `capture_window_state` and `restore_window_state`. Plugin windows stay open. Toggle Plugin Manager (Shift+Ctrl+F1) still hides it fully, and showing it again also un-minimizes, as C# does. |
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

## Phase 5: Dock icons (P4, P3)

Order on the dock: the window's own PNG or SVG (`icon_file`), the plugin's `icon.svg`, its `icon.png`, the dat icon (keyed), the monogram. Failure rules are in C# `docs/plugin-ui-markup.md:28-52`. An icon that cannot be used is skipped and the next is tried.

Commits:

1. **`icon_file`, ABI and SDK**: a field at the end of `PanelRegistration` and `TypedPanelRegistration` (`crates/ac-plugin-abi/src/ui.rs`), `since (1, 3)`, plus the SDK builder in `crates/openac-plugin/src/ui.rs`.
2. **Per-window PNG (P4)**: full colour, same 64x64 px and 64 KiB rule (`crates/ac-plugin/src/icon.rs`), path stays inside the plugin folder (`set_plugin_folder` in `session.rs`), and `.light.png` / `.dark.png` variants from phase 3. Dimmer while its window is closed.
3. **SVG (P3)**: `usvg` and `resvg` with default features off, so no text, system fonts or embedded images, and never given a resources folder, so a plugin's SVG cannot read other files. Limit file size and rendered size. Render at device pixels and take only the alpha as a coverage texture (`RuntimeImages::add_coverage`), so colours in the file are ignored. Tint per theme and slot state. Check `about.toml` and `NOTICE.md`.
4. **Installer and `plugincheck` accept `.svg`**, with useful messages for a bad file. C#: `PluginContentPolicy.cs`, `PluginCheckRunner.cs`.

Live check: every step of the icon order on the dock, in a dark and a light theme, and a bad file falling through to the next icon.

## Phase 6: Finish

1. Rebase check: does the branch still merge with upstream, #51 and #58? Say what conflicts.
2. Grep the diff for client internals and private C# class names.
3. Full `scripts/check.sh` on the MBP at the tip, and fmt, clippy and tests on each commit. Needs the user's go-ahead to push.
4. Merge into fork `main` with `git merge --no-ff`, as a topic branch merge. Remove the worktree.
5. Update `docs/csharp-fork-port-tracker.md`: the rows above, plus the top and bottom edges and minimize, which are new beyond the C# dock. Memory `ui-shapes-f3` and `plugin-dock-redo` too.
6. Final live test with the user, of everything. Look together at **Classic expanders, which have no frame**. C# frames them in Classic and this port does not, because Classic windows must not change. The user will judge then whether that looks odd, and it can be adjusted. Drawing it would mean re-pinning the Classic hashes in `crates/ac-ui/tests/data/plugin_classic/pinned.txt`, on the user's say only.
7. Write the PR text for the user to review. The old F3 copy in `ui-shapes-f3` is out of date: its title, "Nothing draws with these yet", and the temporary-patch live test no longer hold. It must quote Erik's agreement, state the raise to 1.3 and that older plugins keep loading, state the `resvg` dependency, say Classic windows are unchanged, and say it is not tested on Windows or Linux.

**Do not open the PR until the user says so.**

## Estimated size

About 10,000 to 13,000 added lines including tests, of which F3 is 2,700.

## Progress

| Phase | Status | Commits | Notes |
| --- | --- | --- | --- |
| Plan | Agreed 2026-10-31 | | No project code yet |
| F3 | Done | `79da491`, `8d3c0b8`, `f9c403a` | MBP check passed at `f9c403a`, on its own. Has to run again at the new tip |
| 1. Foundations | Done 2026-10-31 | `c62fd98`, `aff4e48`, `254315d` | Tests and real-dat tests pass at the tip, 2931 in `ac-ui` and `ac-render`. Live on the Air's Retina screen. Nothing pushed |
| 2. Modern look | Done 2026-10-31 | `9e31720`, `0361d4b`, `3873f17`, `078566d`, `00b126e`, `c3b81a9` | Tests and real-dat tests pass at the tip, 2986 in `ac-ui` and `ac-render`, 264 in `ac-plugin-ui`. Live in Classic, Moss and Brass on the Air's Retina screen. Pushed to `origin` at `c3b81a9` |
| 3. Themes and plugin API | **Not done: live check open** | `896d162`, `70e548b`, `4e143a0`, `12a8b5c`, `fc0ba19`, `d5f6c52` | Tests and real-dat tests pass at the tip, 4261 in the crates touched. Screenshots good in all eight themes. The user's click-through found the checkboxes and colour picker appearing dead: diagnosed as the test plugin not storing values, see Notes. Pointer retest still to do. Do not start phase 4. Nothing pushed |
| 4. Dock | Not started | | |
| 5. Dock icons | Not started | | |
| 6. Finish | Not started | | |

## Notes for the next phase

- **Phase 3 is the next phase, not phase 4.** Its pointer check is open. The user's first click-through (2026-10-31) reported: checkboxes changed nothing when clicked, and a colour picked in the picker went back to the old blue on a click outside. The theme picker worked. They also expected dock options, but that was a wrong instruction in the check: the gear comes with the dock in phase 4, and the shelf opens Appearance by right-click.
- **Diagnosis of that report:** the first two came from the test plugin, not the client. Bound controls show only what the plugin stores (C# docs: "the box shows whatever `checked` returns next frame"), and `docktest` logged actions without storing them. Scripted clicks through the window's input path, on the real client and real module (`OPENAC_UI_PROBE_SCRIPT=phase3/clicks.txt`), reproduced both symptoms. With `docktest` storing values, the same clicks flip the boxes and keep the colour. A regression test covers it, and `docs/plugin-development.md` now says so. **Not explained:** the first run's log has five `false` and one `true` from `SetFlag`, and no `SetColour`, while scripted clicks give `false`, `true`, `true` and `SetColour`. Real and scripted input share `ui_input::button`. The likely cause, unproven, is the two identical windows: the back one is the 1.2 module, which logs nothing, and a click raises it. No synthetic OS-level mouse events have been tried.
- **To close phase 3:** the user runs `sh ../openac-docktest/phase3/retest.sh` (one plugin, one themed window, optional theme argument) and clicks by hand: Tick, Empty, Indeterminate, and the colour swatch with a click outside; then right-click the shelf for Appearance. It prints what the plugin heard. If those work, mark the row Done and report; then phase 4 is next session. If not, the difference between real and scripted input is the thing to chase.
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
- **Worktree:** `../openac-rust-dock`, on `feat/plugin-dock`. The user had it pushed to `origin` (the fork) at `c3b81a9` on 2026-10-31, as a copy on GitHub. Later pushes still need their go-ahead, and it never goes to `upstream` until the PR.
- **Disk:** the Air has about 20 GB free. Build the worktree into the main target so dependencies are shared: `export CARGO_TARGET_DIR=/Users/davidsmith/Code/OpenAC/openac-rust/target`. Watch `df`: about 11 GB was free at the end of phase 2. `target/debug/incremental` reached 20 GB, and deleting it only costs rebuild time.
- **Classic guard:** `crates/ac-ui/src/plugin/theme_tests.rs` draws a gallery of every control through 27 states in two window shapes, plus a dialog, and compares hashes of the draw lists with `crates/ac-ui/tests/data/plugin_classic/pinned.txt`. Those hashes came from the drawing code before phase 2. Never regenerate them to make a test pass: a change there means Classic changed. Phase 4's dock is the one agreed exception, and the gallery does draw the shelf's minimize button, so expect to deal with that there.
- **Style:** the shapes are in `crates/ac-ui/src/plugin/style.rs`. Control tests are in `plugin/theme_control_tests.rs`, and `style::tests::{corners, has_color}` are the helpers. `Field`, `Menu`, `Meter` and `Scrollbar` in `crates/ac-ui/src/widgets` carry a `theme_palette` that only a themed plugin window sets, so retail windows never reach the style.
- **Light themes:** at rest, Parchment's slider thumb, check tick and header band look right in a screenshot. Pressed and hover states have not been seen on it. `style::shelf_shadow` has no caller until phase 4.
- **Chose differently from C#:** C# also frames expanders in Classic. Here the frame only draws under a modern theme, because Classic windows must not change. `border` is parsed either way. Also beyond C#: rounded hex field in the colour picker, and disabled fields, numbers, hotkeys, swatches and menu captions fade with their faces.
- **Not ported:** the tooltip expiry and `UiRoot` pointer changes from the C# merge. No test showed they are needed.
- **Typed bindings, not a gap:** a checkbox's `onchange` (`BoolAction`), an indeterminate checkbox (`OptionalBool`) and a `uint` colour's `onchange` (`UintAction`) already work: they go through `PanelSpec::bind_typed` and `TypedBindingKind`, not `bind` and `BindingKind`. An earlier note here called this an SDK gap, which was wrong. The test plugin now binds all three, and it loaded and drew the indeterminate box live on 2026-10-31. See phase 3's must-hold list for what is still to check.
- **Live harness:** `../openac-docktest` is a private plugin with a themed window and a window that does not opt in. Rebuild with `cargo build --release --target wasm32-unknown-unknown`. `tools/live.sh <client> <theme> <run name> <output dir>` runs a client under a theme and takes a screenshot. Run it with the sandbox off, on one line per run, because zsh does not split a variable into arguments. Allow 50 seconds to get into the world; one run at 45 produced no screenshot. The plugin now has every control a plugin can bind, in a 620x560 window, and `run.sh` places it to match. In that gallery "A scrolling group" sits inside the framed expander because the folded expander moves it up, in Classic too: that is the test's layout, not a bug. The harness cannot move the pointer, so hover, press, focus, popups and dialogs have only been checked through draw-list tests, not on screen.
- **Before and after:** build the client at the last commit before the phase, copy the binary out of the target, then compare. Classic windows are translucent and the world moves behind them, so a plain pixel diff of a Classic window is noisy. `tools/mask.py <tools dir> <before.png> <after.png>` compares only bright opaque pixels. Differences at the window's outer edge are the world. Phase 1's shots are in `baseline/`, phase 2's in `phase2/`, and `baseline/client-phase2` is the client at the tip of phase 2, ready to be phase 3's before. Phase 2's Classic comparison: no bright pixel appeared or vanished, and shared ones differed by at most 1 level in 255. The exact guarantee has to come from draw-list tests, which is why the Classic identity tests come first in phase 2.
- **Seen, not investigated:** a plugin window with no saved placement came up centred rather than at its markup's `x` and `y`, though the real-dat test's window does honour them. `run.sh` saves placements to avoid it. It was seen on this branch only, and nothing in phase 1 touches placement. If it matters for the dock, look at it there.
- `scripts/check.sh` has not run on these commits. It is due on the MBP in phase 6, along with fmt, clippy and tests on each commit. Phase 2's first commit only ran the `ac-ui` plugin tests, as it adds tests and a module nothing calls; every later commit ran the crates it touches, apart from the sixth, which ran the `ac-ui` and `ac-plugin-ui` library tests before the full run at the tip.
