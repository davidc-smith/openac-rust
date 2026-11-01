# C# fork → Rust client: port tracker

As of 2026-11-01.

## Summary

39 features and fixes still need porting to openac-rust, on top of 5 shared
foundations and 6 live checks. The biggest are the modern theme and dock, plugin
fonts and canvas shapes, and all six Modern HUD decks. About 30 more are already in Rust (Erik ported them from upstream),
don't apply, or are dropped.

- **Source:** davidc-smith/OpenAC `main` at `5e7d7404`, 1,092 non-merge
  commits ahead of `upstream/main` since 2026-09-30.
- **Target:** openac-rust at `198debb` (release 0.1.8), plugin API 1.2 (WASM plugins).
- **Rules:** one client. Every non-retail feature sits behind an option that
  is off by default.
- **Dropped:** Rynth (RynthCore host, RynthAi/Chat/Nav/Lua, its maps and
  panels) and the Gateway branding and launcher split.
- **Upstream PRs:** Erik asked on 2026-10-30 for work that builds on itself to
  go up as one PR of logically ordered commits, not as stacked PRs. Each commit
  builds and passes on its own.
- **How to use this file:** update the Status column as work moves (Not
  started, In progress, In review, Done, Blocked) and put the PR link in Notes.
  Sizes are S (under a day), M (a few days) and L (a week or more). The IDs
  are used in the Needs column and in the suggested order.

## Current focus

Decided 2026-10-31. The work now is what plugins need to draw world art, and
plugin HUDs that take clicks and move with Ctrl+drag. The Modern HUD decks are
a separate project from these ports and are parked: no code that replaces
retail UI goes in yet.

Evidence for what plugins need comes from the Aethrix plugin: its world drawing
and its five HUDs (status, XP, map, Vision, Wayfinder). Every item is in one of
these groups:

| Group | Items | Why |
| --- | --- | --- |
| **Focus: world art** | Done: F1 (awaiting Erik), F2, W1-W5, W11. To do: W6 | Vision's terrain drawing reads landblock terrain with W6, then draws it with triangles and lines |
| **Focus: plugin HUDs** | Done: F3, F4, P8 (merged upstream as PR [#64](https://github.com/eriknihlen/openac-rust/pull/64)). To do: P9, P10, P15, P16, P18 | The five HUDs use all of these: remembered position (P18), screen size and outside presses for menus (P15), resize (P16), layers (P10), polygons, rounded rects and ellipses (F3, P9), fonts and SemiBold (F4, P8) |
| **Parked: replaces retail UI** | F6, H1-H13 | Includes H5, even though it is logic only, because nothing uses it until H6 |
| **Other C# ports, not now** | Plugin windows and dock: F5, P3, P19 (P1, P2, P4-P7, P13 and P14 are done on `main`). Canvas extras with no use found in the five HUDs: P11, P12, P17. Data for navigation: W7-W9. Levels: W10. Walks and live checks: G1-G8. Mac: M1 | Still to port, but no focus item needs them. P1 and P2 restyle plugin windows and the dock, not retail windows |

**Clicks and Ctrl+drag work, with fix M3.** A canvas gets press, release, move,
wheel and cancel events with the Shift, Ctrl and Alt bits, keeps the pointer
until the button comes up, and can change its anchor and offset. Moving a HUD is
plugin code, as `HudDrag.cs` does it in Aethrix. Live-checked 2026-10-31 on the
Air's trackpad against ACECustom with a test plugin: clicks, wheel and a smooth
Ctrl+drag or Option+drag. The middle button was not tried. On a Mac the bit
means the physical Ctrl key; Command is not sent to plugins. A delay of about a
second on every press is Magnet, not the HUD (M4).

Not in the C# fork, so not a port: a canvas setting the mouse cursor, so that
holding Ctrl shows it can move (Aethrix `docs/api-gaps.md` #56, still open).

Order within the focus:

1. **M3 upstream:** the two fixes Ctrl+drag depends on. Done on `main`, in
   review as PR [#58](https://github.com/eriknihlen/openac-rust/pull/58).
2. **F3, then P9:** F3 is done on `main`. It went up with the plugin dock in one
   PR, [#64](https://github.com/eriknihlen/openac-rust/pull/64), merged 2026-11-01.
   `feat/antialiased-shapes` is replaced. P9 is next, now #51 has merged.
3. **P18, P15, P16 and W6:** each adds to the ABI, on top of API 1.4, which #64 brought. P10 follows.
4. **F4, then P8:** merged upstream, in the same PR as F3.

## Shared foundations

Build these six pieces first. Most features below depend on one of them, and
F1 has to be settled with Erik before any ABI change.

| ID | Foundation | What it is | Unlocks | Size | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| F1 | ABI versioning policy | Asked Erik on 2026-10-30, follow-up sent the same day. The ABI cannot hold an import newer than `API_CURRENT` (the SDK writes every import, which must equal `section_at(API_CURRENT)`), so a PR that adds to the API raises the version itself. Proposal: the first API-adding PR after a release raises it, later ones before the next release reuse it. | Every ABI item | S | In progress | Waiting on Erik for the policy. Upstream released 1.3 in v0.1.9 with the world drawing, which is what the proposal says. `feat/plugin-dock-v2` raises to 1.4 itself |
| F2 | Translucent world pass | A second pipeline next to `world_line_pipeline`: alpha blend, depth test, no depth write, no culling, RGBA per vertex. Same world pass as the lines. | W1, W3, W4 | M | Done (on `main`) | Built with W1 on `feat/world-triangles`; GPU test in `ac-render/src/renderer/world_triangles.rs` |
| F3 | Anti-aliased UI shapes | `DrawCmd::Triangles` with a colour per vertex, drawn by the existing pipeline. `ac-ui/src/shapes.rs` turns rounded rects, ellipses, convex polygons, strokes and shadows into triangles with a soft edge one framebuffer pixel wide, and `RenderContext` draws them, smooth lines and vertical gradients too. Numbers that are not finite draw nothing, shadows fade with no step, and ellipse strokes are even all the way round. | P1, P2, P9, all decks | M | Done (on `main`) | The first 3 commits of `feat/plugin-dock-v2`, no ABI change of its own, replacing `feat/antialiased-shapes` and `feat/ui-shapes`. Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64). The dock and themed windows draw with it. Not tested on Windows or Linux |
| F4 | Fonts | A bundled SemiBold weight, plus a plugin font API (bundled Noto Sans at any size and weight, or the plugin's own TTF/OTF). | P1, P7, P8, P13, decks | M-L | Done (on `main`) | `ac-ui/src/font.rs`, `bundled_font.rs`. SemiBold and the plugin font API, with P8. Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64) |
| F5 | SVG icons | Decide between adding resvg/tiny-skia and shipping PNGs. The installer and `plugincheck` must accept `.svg` if we add it. | P3, decks | S-M | Not started | Decks use about 15 SVGs from `assets/hud`. Left out for now by the user, 2026-11-01, so no `resvg` dependency |
| F6 | Modern HUD option | One setting, off by default, in `ac-host` settings and the Options panel. Also adds the HUD text size setting. | All decks | S | Not started | `ac-host/src/settings/sections.rs` |

## World rendering and plugin world APIs

World drawing is done on `main`: labels, lines, filled triangles, icons and
ground shapes. All of it is up as one PR, [#51](https://github.com/eriknihlen/openac-rust/pull/51),
from `feat/world-markers`. Still missing are terrain reads (W6) and the indoor
and floorplan data (W7 to W9).

| ID | Feature (C# merge) | Rust today | What porting involves | Size | Needs | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W1 | Filled see-through triangles (`47b0dad8`, colour per corner from `1bdde7b1`) | Done | `ui_triangles_create/set/remove` (API 1.3), `TriangleLayer` in the SDK, per-plugin layers in `PluginUi`, range/view/budget planning in `ac-render/src/world_triangles.rs`. Live-checked 2026-10-30 with a test plugin on the Mac. | M | F1, F2 | Done (on `main`) | PR [#51](https://github.com/eriknihlen/openac-rust/pull/51), branch `feat/world-markers` |
| W2 | World icons over objects and positions (`a7535b82`) | Done | `ui_markers_create/set_icons/remove` (API 1.3), `MarkerLayer` in the SDK, per-plugin layers in `PluginUi`, `WorldIconOverlay` in `ac-ui/src/overlay.rs`. Beyond the C#: a `hidden_by_world` option (occlusion queries, `renderer/icon_probes.rs`) and labels hide in the portal view. Live-checked 2026-10-30 by the user on the Air. | M | F1 | Done (on `main`) | PR [#51](https://github.com/eriknihlen/openac-rust/pull/51), branch `feat/world-markers` |
| W3 | Ground shapes: rings, discs, arcs (`4ea41e8e`) | Done | `ui_markers_set_shapes` (API 1.3), tessellator in `ac-render/src/world_shapes.rs`, drawn with the triangles' pipeline and buffer. Split along the land's creases, unlike the C# tessellator, whose rings let ridges show through. Live-checked 2026-10-30 by the user on the Air (Retina, Metal): slope, turning, walls, indoors, portal, logout, creature and wielded item. | M (S after W1) | F2, W2 | Done (on `main`) | PR [#51](https://github.com/eriknihlen/openac-rust/pull/51), branch `feat/world-markers` |
| W4 | Shape culling, budget, terrain cache (`9c4248b2`) | Done | Range, view and a nearest-first budget in `world_shapes::build`. No terrain cache: the C# one avoids scanning every landblock, and `sample_terrain_z` already looks up the point's landblock directly. | S | W3 | Done (on `main`) | PR [#51](https://github.com/eriknihlen/openac-rust/pull/51), branch `feat/world-markers` |
| W5 | World labels cleared at logoff (`6a64b022`) | Missing | `PluginUi::clear_labels`, called from `SessionState::left_world`; plugins keep running and show labels again next stay. Markers (W2, W3) must clear the same way. | S | — | Done (on `main`) | PR [#51](https://github.com/eriknihlen/openac-rust/pull/51), branch `feat/world-markers` |
| W6 | Read any landblock's terrain (`c44d8f45`) | Missing | `world::LandblockTerrain` record and a `terrain_landblock(u32)` import built from `TerrainSurface::from_landblock` (`ac-physics/src/dat.rs:141`). Least-recently-used cache. `sample()` in the SDK. | S-M | F1 | Not started | |
| W7 | Indoor cell doorways (`f6000588`) | Missing | `doorways` field on `IndoorCell` (`ac-plugin-abi/src/world.rs:647`), filled from EnvCell portals in `ac-nav/src/indoor.rs`. | S | F1 | Not started | |
| W8 | Floorplan object floors: bridges, platforms (`a5db2384`) | Missing | `object_floors` on `DungeonCell`. In `ac-nav/src/floorplan.rs`, transform static-object physics polygons and keep the faces that point up. | M | F1 | Not started | |
| W9 | Floor heights, doorway kinds, walls that reach the floor (`0c8b3b94`) | Missing | 3D `floors` per cell, a doorway `kind` and `sill`, and a filter that keeps only walls reaching the floor in `floorplan.rs`. About 300 lines of C#. Update the parity fixtures. | M | W7 | Not started | Fixes lintels closing doorways |
| W10 | Extended level curve past 275 (`7b6641cb`) | Missing | Port `ExtendedLevelCurve.cs` (55 lines) into ac-runtime with an option that is off by default. Use it in the character panel and `to_next_level`. | S | — | Not started | ACECustom servers |
| W11 | Plugin docs for world drawing (`a0f9330b`) | Done | Sections for world lines, labels, triangles, shapes and icons in `docs/plugin-development.md`. W6 adds its own when it comes. | S | W1-W5 | Done (on `main`) | PR [#51](https://github.com/eriknihlen/openac-rust/pull/51), branch `feat/world-markers` |

## Plugin UI, dock, themes and canvases

Rust's markup is already level with the fork: flex, scroll groups, title bar,
13 controls, dialogs, `<repeat>`, tab and file icons. What's missing is the
fork-only look (modern theme, dock pill, extra themes), plugin fonts, and most
of the canvas painting work. Rust canvases are a retained `Vec<PaintCmd>`
rather than a paint callback, so the canvas items become new `PaintCmd` and
`CanvasChange` variants.

| ID | Feature (C# merge) | Rust today | What porting involves | Size | Needs | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1 | Modern theme look (`16b0db10`) | Missing | Rounded corners, soft shadow, header band, SemiBold title, pill tabs, switch toggles, focus rings. Restyle `ac-ui/src/plugin/` widgets, controls, fold, dialog, title bar, list and menu. | L | F3, F4 | Done (on `main`) | Rust themes are flat palette swaps. Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64). Classic plugin windows draw as before, pinned by `tests/data/plugin_classic/pinned.txt` |
| P2 | Dock as a floating pill with an edge rail (`7832a8e7`) | Partial | Rework the layout in `ac-ui/src/plugin/shelf.rs`. Add a hover label, a dock-mode setting and a menu in Plugin appearance. | M | F3 | Done (on `main`) | Rust has upstream's shelf. Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64). Replaces the shelf with no option, as Erik agreed. Beyond the C# dock: top and bottom edges, a horizontal layout, a remembered closed state as an icon tile, three sizes, and hover or click to open |
| P3 | Theme-tinted SVG dock icons (`e57192bb`) | Missing | Rasterise to a coverage texture and tint it per state. `icon_file` field at the end of `PanelRegistration`. Installer and `plugincheck` accept `.svg`. | M | F1, F5 | Not started | Left out for now by the user, 2026-11-01. `icon_file` went in with P4 |
| P4 | Full-colour PNG dock icon per window (`7f1262fe`) | Partial | Same `icon_file` field as P3. Reuse `ac-plugin-ui/src/file_images.rs` with the 64×64 px / 64 KiB rule. | S | F1 | Done (on `main`) | Rust uses one `icon.png` per plugin. Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64). A `.svg` is refused and logged, and the next icon is drawn |
| P5 | Dock gear toggles Plugin appearance (`abfd3f9c`) | Partial | Add a gear slot to `shelf.rs` that opens and closes Appearance. | S | — | Done (on `main`) | Rust opens it with a right-click only. Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64) |
| P6 | Five more themes, IsLight, light/dark variants (`773f0294`) | Missing | Lavender Dusk, Ember, Bloodmoon, Glacier and Parchment in `theme.rs`. `ThemeKind` tags plus `is_light`. `color="A\|light:B"` in markup, and `.light.png`/`.dark.png` lookups. | S-M | F1 | Done (on `main`) | Plugins on API 1.2 or older must be sent a kind they know. Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64). A plugin on API 1.3 or older is told Moss under a new theme, with the real name and colours |
| P7 | Theme metrics and SemiBold for canvases (`a0c1408e`) | Partial | Metric constants in the SDK. SemiBold comes with F4. | S | F4 | Done (on `main`) | Theme read and ThemeChanged already exist. Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64) |
| P8 | Plugin fonts (painter-v2 PR1: `7d6f7246`, `2fde2783`, `866257df`) | Missing | Imports to get a bundled font or decode the plugin's own and release it. A font handle on `PaintCmd::Text`, a measure call, and a glyph atlas per font and size with budgets. | M-L | F1, F4 | Done (on `main`) | Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64). Bytes from the plugin, with no path-based load. Fuzzed with damaged fonts, no crash |
| P9 | Anti-aliased canvas shapes (`44a505ee`) | Missing | `PaintCmd` variants for rounded rects, ellipses, convex polygons and gradients, drawn through the `RenderContext` methods from F3, which already refuse numbers that are not finite. | M | F1, F3 | Not started | |
| P10 | Canvas layers and z-order (`a4ab9664`) | Missing | A second band above windows and below dialogs (`overlay.rs`, `root.rs`). `layer` and `z_order` on `CanvasRegistration`, plus `CanvasChange::ZOrder`. | M | F1 | Not started | |
| P11 | Canvas keyboard input (`84ada220`) | Missing | Keyboard flag, a key `UiEvent`, focus request and release, and key repeat through `UiRoot`. | M | F1 | Not started | |
| P12 | Image regions and nine-slice (`303c40ef`) | Missing | Three `PaintCmd` variants. The UV sub-rect already exists in `RenderContext`. | S | F1 | Not started | |
| P13 | Themed Confirm close X, SemiBold list headers (`2e0894ce`) | Partial | Ghost X in `plugin/dialog.rs:271`. SemiBold headers come with F4. | S | F4 | Done (on `main`) | Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64) |
| P14 | Expander framed like a group box (`451b6f9c`) | Missing | Parse `border` and draw the frame in `plugin/fold.rs`. | S | — | Done (on `main`) | Merged upstream, PR [#64](https://github.com/eriknihlen/openac-rust/pull/64). Framed under a modern theme only, unlike C#, because Classic windows must not change |
| P15 | Screen size and presses outside a canvas (`a16e142f`) | Missing | `UiScreenSize` import and event. A `wants_outside_presses` flag and pointer kind 5. | S | F1 | Not started | |
| P16 | Canvas resize (`ebcf854c`) | Missing | `CanvasChange::Resize(w, h)`, applied in `canvas.rs::sync`. | S | F1 | Not started | |
| P17 | Canvas inside a markup window (`5279f073`, `352099df`) | Missing | `<canvas id>` element in markup, sized by flex. `in_markup` flag, paired by id in `plugin/mod.rs`. | M | F1 | Not started | |
| P18 | Remembered canvas position (`f4a5b7bf`) | Missing | `remembers_position` flag, saved per character through `layout_persistence.rs`, debounced 0.5 s. | S-M | F1 | Not started | |
| P19 | Sample plugins: ControlsDemo, FlexDemo, ThemeGallery | Missing | Port as WASM samples to test the features above against. | S each | P1, P6 | Not started | Optional |
| P20 | Side-by-side expanders (not a port) | Bug | Folding moves what is below by each expander's height, so side-by-side expanders add up and can put a group over a header, which then cannot reopen. Siblings are now placed afresh on every fold and open: each keeps its authored gap to the lowest thing above it, and only what overlaps it across the window counts, so each column follows its own expander. | S-M | — | Done (on `main`) | Found in the dock's live test, 2026-11-01. Also upstream at `57a8e81` and in C#. Separate PR, by the user's choice. Fix on `fix/side-by-side-expanders` (`8eb0e3d`, off `upstream-main`), PR not open, full check not run. Live-checked 2026-11-01 for the folded-at-start gallery under Moss; clicking folds live not tried, those are covered by tests. An element below an expander but outside its width no longer moves, a change from C#. Conflicts with PR #64 only on `MarkupExpander`'s doc comment; no other dependency, it builds and passes on `upstream-main`. #64 has merged, so the hold is over: rebase on `upstream-main`, run the full check, open |
| P21 | Classic expander frame | Missing | Frame Classic expanders as modern themes do. Re-pins `crates/ac-ui/tests/data/plugin_classic/pinned.txt`. | S | P14 | Not started | The user, 2026-11-01: it looks odd unframed, and the frame goes in a different PR from the dock |
| P22 | Dock with many windows | Partial | When the icons overflow and scroll: (1) it is not obvious you need to scroll, and (2) while scrolling, it is not obvious how far through the list you are. The fades alone do not say enough. | S-M | P2 | Not started | The user, 2026-11-01. The wheel, gear and labels work. Separate PR from the dock |
| P23 | Dock open and close animation (nice to have) | Missing | Explore a nicer animation of the dock expanding and closing. | ? | P2 | Not started | The user wants to explore this, 2026-11-01. Not a commitment yet |
| P24 | Expander arrow ignores a click (not a port) | Bug | A click or key press runs in the header widget, which the expander cannot reach while it runs, so the arrow kept its old direction on an unbound expander. The header now reads the state back after toggling. | S | — | Done (on `main`) | Noticed by the user, 2026-11-01. Upstream at `57a8e81`, not in C#, whose header reads the state as it draws. Fix on `fix/expander-arrow` (`078aed6`, off `upstream-main`), PR not open, not tried live. Merges cleanly with P20's branch, `main` and PR #64, and does not need any of them. #64 has merged, so the hold is over: rebase, run the full check, open |

## Modern HUD decks

Parked as of 2026-10-31, along with F6 (see Current focus).

None of the decks are in Rust. The C# code is about 15,500 lines
(`src/AcDream.App/UI/Hud`, 77 files, plus `AcDream.Core/Chat/Deck`). Each deck
replaces retail windows only when the Modern HUD option (F6) is on. Port them
in the fork's order, and add HUD text size with each one. The specs and plans
in `docs/superpowers/` of the C# repo are the reference.

| ID | Deck (C# merge) | Replaces in Rust | What porting involves | Size | Needs | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| H1 | Command Deck (`43087515`) | `panels/vitals.rs`, `combat_bar.rs`, `jump_powerbar.rs` | Deck model, geometry and drawing. Stance, power, auto-target, keep-in-view and recast. Hide the retail windows when on. | L | F3, F4, F5, F6 | Not started | |
| H2 | Resizable Command Deck and power snap (`d996784a`) | — | Reuse `widgets/resize_grip.rs` and `layout_persistence.rs`. The power slider snaps to steps. | M | H1 | Not started | |
| H3 | Spell Deck, v1 to v3 (`d4f05350`) | Favourites in `panels/magic.rs` | 1 to 10 rows of 12 slots docked to the Command Deck. Click to cast, an item-spell slot, and `UseSpellRow{n}Slot{m}` hotkeys. About 2,800 lines of C#. | L | H1 | Not started | |
| H4 | Target Deck (`f0015350`) | `panels/selected_object.rs` | A plate per kind of target. Auto-ID (2 s throttle, 10 s stale, "Assess failed"). The plugin target-plate hook becomes a WASM ABI addition. | L | F1, F6 | Not started | Rust has manual Appraise only |
| H5 | Chat Deck model and rules engine (`9d4b65c0`) | Retail chat filters | Pure logic: rules, matcher, presets and settings JSON. Ports easily and can be tested alone. | M | — | Not started | Good first deck task |
| H6 | Chat Deck window (`ff48c31a`) | `panels/chat.rs`, `floating_chat.rs` | Tabs, transcript, input, and the Modern HUD switch. | L | H5, F6 | Not started | |
| H7 | Chat Deck strip, peek, fly-out, pin back (`8d9c6f5d`) | — | Edge strip and peek feed. Needs about 15 SVG icons. | M | H6, F5 | Not started | |
| H8 | Chat Deck appearance (`b729ca05`) | — | Channel colours, idle fade, alert beep, prefixes. | M | H6 | Not started | |
| H9 | Chat Deck filters and tabs editor (`1f7dc535`, `f3a19364`) | — | Rule form, channel picker, make a rule from a chat line. | M | H6 | Not started | |
| H10 | Gear Deck core (`fb7d3378`) | `panels/inventory.rs`, `paperdoll.rs` | Modern inventory and paper doll, plus the drop dispatcher. | L | F3, F6 | Not started | |
| H11 | Gear Deck query (`67d1e47b`) | — | Search, sort, saved filters, multi-select, "Add N to salvage" through `panels/salvage.rs`. | M | H10 | Not started | |
| H12 | Gear Deck figure view (`5e7d7404`) | — | 3D figure with a pose and padlock. Rust already renders paper-doll viewports (`ac-render/src/ui/viewports.rs`). | M | H10 | Not started | |
| H13 | HUD text size (`170de1b7`) | — | Global and per-deck text scale, within the bundled font's 8–32 px range. | S | F6 | Not started | Add with each deck |

## Gameplay and navigation fixes

Two navigation fixes are real gaps. Most other fixes are already in Rust, or
the bug looks impossible because Rust is built differently. Those still need a
quick live check to confirm.

| ID | Fix (C# merge) | Rust today | What to do | Size | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| G1 | Walks keep out of portals (`31b9f4e9`) | Bug present | Add a 0.75 m margin over the body radius (`ac-nav/src/walk.rs:2827`, `2850`). Re-check the next 30 m every 15 frames, up to 4 replans. On NoRoute, retry and name the portal that blocks. | S-M | Not started | Rust avoids portals only when it plans the route |
| G2 | Walk corridor for far or blocked walks (`c03dfa36`) | Partial | Port `NavCorridor` (about 370 lines) and its controller (about 300 lines): a coarse grid, walked in fine stages, replanned with leaps. | M | Not started | Rust only walks in greedy stages to the grid edge |
| G3 | Live check: stuck in portal space (`8e97e941`, upstream #274) | Probably fine | Take the DreamWeave Drift portal and confirm the cell arrives. | S | Not started | Rust reloads the 3×3 area on teleport |
| G4 | Live check: held items after a teleport (`68610074`) | Probably fine | Teleport to someone who is fishing and check their rod shows. | S | Not started | Covered by `world/attachment_tests.rs:142` |
| G5 | Live check: enchantment clock through portal space (`52532152`, `c14dad9b`) | Probably fine | Check time left on buffs after a portal. | S | Not started | One session clock both stamps and reads |
| G6 | Live check: hook messages and house purchase wording | Present | Compare the wrong-hook message and the slumlord text with retail. | S | Not started | Rust and C# word the hook refusal differently |
| G7 | Live check: late create on a hidden player (`b035cb2c`) | Present | Confirm an object created on the player uses Placement \| Slide. | S | Not started | |
| G8 | Live check: Retina screenshots (`834b8eb4`) | Probably fine | Take a screenshot on a Retina Mac. | S | Not started | Rust captures at physical pixel size |

## Mac fixes found along the way

Bugs found while porting that are not C# fork work.

| ID | Fix | What to do | Size | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| M1 | Plugin peer socket path too long on macOS | macOS caps Unix socket paths at 104 bytes. `endpoint_for` (`ac-runtime/src/plugins/peer_transport.rs:123`) puts the socket at `<cache>/plugin-peers/openac-peer-<24 hex>`, 113 bytes for the live client on this Mac and 131 in the tests. Use a short socket folder when the path would be too long; the hash already names the directory and user. | S | Not started | Live peers probably fail on Mac; the 10 `peers` host-parity tests fail on Mac unless `TMPDIR=/tmp/`. Linux CI never sees it (108-byte limit, short `/tmp`) |
| M3 | Ctrl+drag on a canvas: no modifier keys, and a bouncing drag (all platforms) | Two fixes, in order. Only key events set `UiEvent::modifiers`, so a canvas was always told none were held, and a Ctrl- or Shift-click on a multi-select list did a plain pick; the window now passes each change to `UiRoot::set_modifiers`, and `event_for` puts them on every event. The window follows a plugin's moves a moment late, so a held pointer was measured from where the canvas had been and a drag kept overshooting; `PluginUi::pointer_as_placed_now` now measures it from where the plugin last put the canvas. No ABI change. | S | In review | PR [#58](https://github.com/eriknihlen/openac-rust/pull/58), branch `fix/canvas-drag-bounce` off `upstream-main` (both commits), on `main`. Full check, tests and live check done; the list click is from reading the code, not tried. Conflicts with PR #51 only on the import lines in `ac-plugin-ui` `lib.rs` and `state.rs`; rebase whichever merges second |
| M4 | Every press in the client about a second late while Magnet runs (macOS) | Clicks and drags in all the client's UI, retail windows too. Quitting Magnet makes them instant. A plain AppKit test window stayed instant with Magnet running, so it is something about the client. Once the client hears a press it draws the change in about 20 ms. Not known yet: why, or whether other window-snapping tools do it. A guess to test: Magnet asks the app about its windows before passing the press on, and the client answers slowly. | ? | Not started | Found 2026-10-31 on the Air. With Magnet a click's press and release reached the client a median 1 ms apart; without it, 165 ms |
| M2 | Load-sensitive host-parity tests | The walk scenarios re-planned on one arm (`Planning, Walking, Planning, Walking, Arrived`): the staged hidden monster stood a metre out on the walking line, and on that slope a walk that brushed it was pinned (the step down lands the body in its sphere and the placement check refuses the move). Retail does the same: a hidden creature still collides (`FindObjCollisions` skips only ethereal ones) and `CTransition::step_down` ignores creatures then refuses an overlapping placement. Whether a walk brushed it was frame timing. Harness fix: the hidden monster stands 3 m north, and the two corpse-walk scenarios leave the walking body's position updates (`0xF753`) out of the transcript. | S-M | Done | Merged upstream in #46 (PR #42). Not a client bug; the client matches retail |

## Nothing to port

These are already in Rust, don't apply to Rust's design, or have been dropped.

| Fork work (C# merge) | Verdict | Why |
| --- | --- | --- |
| Flex layout, title bar, scroll groups, field onblur, re-fit (`0991b97c`, `c4767d26`, `4f16abe3`, `4aa1492d`, `053cedf7`, `7e382226`) | Already in Rust | Went upstream; `ac-plugin-ui/src/flex.rs`, `chrome.rs` |
| 13 markup controls, dialogs, Confirm withdraw (`54d19bb7`, `58af7f30`) | Already in Rust | `ac-ui/src/plugin/controls.rs`, `dialog.rs` |
| `<repeat>`, long menu items, markup file icons, tab icons, list button column (`7a028084`, `a0df9c14`, `3bece030`, `676cacb5`, `c4090bd0`) | Already in Rust | Went upstream |
| Hotkeys respect focus, overlay follows root size (`301852fc`, `30208991`) | Already in Rust | `ac-plugin-ui/src/hotkeys.rs:510`, `ac-ui/src/overlay.rs:116` |
| Chat coordinate links, experience and luminance, cast without selecting, VTank hook mask, dungeon place on floor (`fa682513`, `ebcf854c`, `85ecc1a8`, `035a7867`, `ba0421b8`) | Already in Rust | Plugin API 1.2 has them |
| House fixes: hook single slot, hook type check, redraw on re-sent description, hook pose, string property updates, purchase window, guest list, owner entry (`9e5a2047`, `b71f0210`, `91e6e738`, `19761ae0`, `f995df90`, `df5ca532`, `01da62be`, `da13fa7b`) | Already in Rust | Wording checked in G6 |
| Lifestone hidden collision, focus inventory completeness, inventory pack frame (`cafff203`, `a5b06750`, `23533d10`) | Already in Rust | |
| Quick press-and-drag (`c68c2f8d`) | Not needed | Rust input is event-driven through winit |
| HiDPI canvases and world shapes (`450dd2b9`, `d317a853`) | Not needed | Rust draws 1:1 in physical pixels in one world pass. Revisit if an interface scale is added. |
| Contract version packaging (`759aeebc`) | Not needed | Replaced by F1 |
| Rynth: RynthCore host and about 35 `feat/rynth-*` merges, Nav3D markers, outdoor detours, ImGui fonts and clipboard | Dropped | A .NET host that can't load in Rust. W1 gives WASM plugins the same route-marker drawing. |
| Gateway versions, per-product launchers, fork feed and navdata install (`eca0047c`, `3a2cf0db`, `5f774494`) | Dropped | One client. Use `--update-manifest-uri` to test fork builds. |

## Suggested order

This is the order for the whole list, from before the focus was set. Current
focus comes first and its order wins, and step 8 is parked.

World drawing is done, so next come the dock and themes, then the decks. Each
step depends only on the ones before it. A chain of work that builds on itself
goes up as one PR; independent items go up on their own. Two PRs that both add
to the plugin ABI touch the same version and import table, so they conflict
until one is merged.

1. **Decide F1 with Erik:** how fork ABI additions are versioned.
2. **World drawing (done on `main`):** F2, W1, W5, W2, W3, W4 and W11, one PR
   from `feat/world-markers`. W6 terrain read follows on its own.
3. **Small independent wins:** G1 portal margin, P5 dock gear, P14 expander
   frame and W10 level curve need no ABI change. P16 canvas resize and P15
   screen size add to the ABI, so they wait for the world drawing PR or share
   its version.
4. **UI foundations:** F3 shapes, F4 fonts, F5 SVG decision.
5. **Look and dock:** P6 themes, P1 modern look, P2 dock pill, P3 and P4 dock
   icons, P7, P13.
6. **Canvas painting:** P8 fonts, P9 shapes, P12 regions, P10 layers, P11
   keyboard, P17 canvas in markup, P18 remembered position.
7. **Navigation:** G2 walk corridor, then W7, W9, W8 floorplan work. Run the
   G3 to G8 live checks alongside.
8. **Modern HUD:** F6, then H5 Chat model (logic only), then H1 to H3 Command
   and Spell, H4 Target, H6 to H9 Chat, H10 to H12 Gear, with H13 text size in
   each.
