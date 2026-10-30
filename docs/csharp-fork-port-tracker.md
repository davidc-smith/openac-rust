# C# fork → Rust client: port tracker

As of 2026-10-30.

## Summary

45 features and fixes still need porting to openac-rust, on top of 6 shared
foundations and 6 live checks. The biggest are the filled world triangles, the
modern theme and dock, plugin fonts and canvas shapes, and all six Modern HUD
decks. About 30 more are already in Rust (Erik ported them from upstream),
don't apply, or are dropped.

- **Source:** davidc-smith/OpenAC `main` at `5e7d7404`, 1,092 non-merge
  commits ahead of `upstream/main` since 2026-09-30.
- **Target:** openac-rust at `5e906fe`, plugin API 1.2 (WASM plugins).
- **Rules:** one client. Every non-retail feature sits behind an option that
  is off by default.
- **Dropped:** Rynth (RynthCore host, RynthAi/Chat/Nav/Lua, its maps and
  panels) and the Gateway branding and launcher split.
- **How to use this file:** update the Status column as work moves (Not
  started, In progress, In review, Done, Blocked) and put the PR link in Notes.
  Sizes are S (under a day), M (a few days) and L (a week or more). The IDs
  are used in the Needs column and in the suggested order.

## Shared foundations

Build these six pieces first. Most features below depend on one of them, and
F1 has to be settled with Erik before any ABI change.

| ID | Foundation | What it is | Unlocks | Size | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| F1 | ABI versioning policy | Agree with Erik how fork features land: as API 1.3 upstream, or behind a capability query. New fields go at the end of records with `since (1, 3)`. | Every ABI item | S | Not started | Decision, not code |
| F2 | Translucent world pass | A second pipeline next to `world_line_pipeline`: alpha blend, depth test, no depth write, no culling, RGBA per vertex. Same world pass as the lines. | W1, W3, W4 | M | Not started | `ac-render/src/renderer/pipelines.rs`, `renderer.rs:3690` |
| F3 | Anti-aliased UI shapes | A triangle-list `DrawCmd` with a colour per vertex and a 1 px soft edge, plus rounded rects, shadows and gradients built on it. | P1, P2, P9, all decks | M | Not started | `ac-ui/src/draw.rs`, `ac-render/src/ui/gpu.rs` |
| F4 | Fonts | A bundled SemiBold weight, plus a plugin font API (bundled Noto Sans at any size and weight, or the plugin's own TTF/OTF). | P1, P7, P8, P13, decks | M-L | Not started | `ac-ui/src/font.rs`, `bundled_font.rs` |
| F5 | SVG icons | Decide between adding resvg/tiny-skia and shipping PNGs. The installer and `plugincheck` must accept `.svg` if we add it. | P3, decks | S-M | Not started | Decks use about 15 SVGs from `assets/hud` |
| F6 | Modern HUD option | One setting, off by default, in `ac-host` settings and the Options panel. Also adds the HUD text size setting. | All decks | S | Not started | `ac-host/src/settings/sections.rs` |

## World rendering and plugin world APIs

The biggest gap is the filled world triangles. Rust has world labels and opaque
lines, but no filled triangles, ground shapes, world icons or terrain reads.
None of those has an import in `ac-plugin-abi/src/table.rs`.

| ID | Feature (C# merge) | Rust today | What porting involves | Size | Needs | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W1 | Filled see-through triangles (`47b0dad8`, colour per corner from `1bdde7b1`) | Missing | `WorldTriangle` record and `ui_triangles_create/set/remove` imports. Owner layers in `ac-plugin-ui/src/state.rs` (copy what lines do), a `TriangleLayer` in the SDK, and a draw after `draw_world_lines`. Up to 65,536 per layer. | M | F1, F2 | Not started | Top priority |
| W2 | World icons over objects and positions (`a7535b82`) | Missing | Marker-layer ABI (object or position anchor), and an icon layer next to `WorldLabelLayer` in `ac-ui/src/overlay.rs`. Reuses plugin image textures and `map_point_in_frame`. | M | F1 | Not started | Ship as one marker ABI with W3 |
| W3 | Ground shapes: rings, discs, arcs (`4ea41e8e`) | Missing | Port the about 160-line tessellator. Each vertex follows the ground, as `world_lines.rs` already does. Object anchors and `FacesObject` use the object's heading. | M (S after W1) | F2, W2 | Not started | |
| W4 | Shape culling, budget, terrain cache (`9c4248b2`) | Missing | View culling with `ac-render/src/frustum.rs`, nearest first within a budget, and a sampler cache over `ac-physics/src/engine.rs:452`. | S | W1, W3 | Not started | Build into W1 and W3 |
| W5 | World labels cleared at logoff (`6a64b022`) | Missing | `PluginUi::clear_labels`, called from `SessionState::left_world`; plugins keep running and show labels again next stay. Markers (W2, W3) must clear the same way. | S | — | Done (on `main`) | Branch `fix/world-labels-logoff` (`084bb15`); PR waits for F1 answer. No ABI change, so it could go up now |
| W6 | Read any landblock's terrain (`c44d8f45`) | Missing | `world::LandblockTerrain` record and a `terrain_landblock(u32)` import built from `TerrainSurface::from_landblock` (`ac-physics/src/dat.rs:141`). Least-recently-used cache. `sample()` in the SDK. | S-M | F1 | Not started | |
| W7 | Indoor cell doorways (`f6000588`) | Missing | `doorways` field on `IndoorCell` (`ac-plugin-abi/src/world.rs:647`), filled from EnvCell portals in `ac-nav/src/indoor.rs`. | S | F1 | Not started | |
| W8 | Floorplan object floors: bridges, platforms (`a5db2384`) | Missing | `object_floors` on `DungeonCell`. In `ac-nav/src/floorplan.rs`, transform static-object physics polygons and keep the faces that point up. | M | F1 | Not started | |
| W9 | Floor heights, doorway kinds, walls that reach the floor (`0c8b3b94`) | Missing | 3D `floors` per cell, a doorway `kind` and `sill`, and a filter that keeps only walls reaching the floor in `floorplan.rs`. About 300 lines of C#. Update the parity fixtures. | M | W7 | Not started | Fixes lintels closing doorways |
| W10 | Extended level curve past 275 (`7b6641cb`) | Missing | Port `ExtendedLevelCurve.cs` (55 lines) into ac-runtime with an option that is off by default. Use it in the character panel and `to_next_level`. | S | — | Not started | ACECustom servers |
| W11 | Plugin docs for world drawing (`a0f9330b`) | Partial | Add sections for world lines, labels, triangles, shapes, icons and terrain to `docs/plugin-development.md`. | S | W1-W6 | Not started | |

## Plugin UI, dock, themes and canvases

Rust's markup is already level with the fork: flex, scroll groups, title bar,
13 controls, dialogs, `<repeat>`, tab and file icons. What's missing is the
fork-only look (modern theme, dock pill, extra themes), plugin fonts, and most
of the canvas painting work. Rust canvases are a retained `Vec<PaintCmd>`
rather than a paint callback, so the canvas items become new `PaintCmd` and
`CanvasChange` variants.

| ID | Feature (C# merge) | Rust today | What porting involves | Size | Needs | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P1 | Modern theme look (`16b0db10`) | Missing | Rounded corners, soft shadow, header band, SemiBold title, pill tabs, switch toggles, focus rings. Restyle `ac-ui/src/plugin/` widgets, controls, fold, dialog, title bar, list and menu. | L | F3, F4 | Not started | Rust themes are flat palette swaps |
| P2 | Dock as a floating pill with an edge rail (`7832a8e7`) | Partial | Rework the layout in `ac-ui/src/plugin/shelf.rs`. Add a hover label, a dock-mode setting and a menu in Plugin appearance. | M | F3 | Not started | Rust has upstream's shelf |
| P3 | Theme-tinted SVG dock icons (`e57192bb`) | Missing | Rasterise to a coverage texture and tint it per state. `icon_file` field at the end of `PanelRegistration`. Installer and `plugincheck` accept `.svg`. | M | F1, F5 | Not started | |
| P4 | Full-colour PNG dock icon per window (`7f1262fe`) | Partial | Same `icon_file` field as P3. Reuse `ac-plugin-ui/src/file_images.rs` with the 64×64 px / 64 KiB rule. | S | F1 | Not started | Rust uses one `icon.png` per plugin |
| P5 | Dock gear toggles Plugin appearance (`abfd3f9c`) | Partial | Add a gear slot to `shelf.rs` that opens and closes Appearance. | S | — | Not started | Rust opens it with a right-click only |
| P6 | Five more themes, IsLight, light/dark variants (`773f0294`) | Missing | Lavender Dusk, Ember, Bloodmoon, Glacier and Parchment in `theme.rs`. `ThemeKind` tags plus `is_light`. `color="A\|light:B"` in markup, and `.light.png`/`.dark.png` lookups. | S-M | F1 | Not started | Plugins on API 1.2 or older must be sent a kind they know |
| P7 | Theme metrics and SemiBold for canvases (`a0c1408e`) | Partial | Metric constants in the SDK. SemiBold comes with F4. | S | F4 | Not started | Theme read and ThemeChanged already exist |
| P8 | Plugin fonts (painter-v2 PR1: `7d6f7246`, `2fde2783`, `866257df`) | Missing | Imports to get a bundled font or decode the plugin's own and release it. A font handle on `PaintCmd::Text`, a measure call, and a glyph atlas per font and size with budgets. | M-L | F1, F4 | Not started | |
| P9 | Anti-aliased canvas shapes (`44a505ee`) | Missing | `PaintCmd` variants for rounded rects, ellipses, convex polygons and gradients, tessellated in `plugin/canvas.rs`. | M | F1, F3 | Not started | |
| P10 | Canvas layers and z-order (`a4ab9664`) | Missing | A second band above windows and below dialogs (`overlay.rs`, `root.rs`). `layer` and `z_order` on `CanvasRegistration`, plus `CanvasChange::ZOrder`. | M | F1 | Not started | |
| P11 | Canvas keyboard input (`84ada220`) | Missing | Keyboard flag, a key `UiEvent`, focus request and release, and key repeat through `UiRoot`. | M | F1 | Not started | |
| P12 | Image regions and nine-slice (`303c40ef`) | Missing | Three `PaintCmd` variants. The UV sub-rect already exists in `RenderContext`. | S | F1 | Not started | |
| P13 | Themed Confirm close X, SemiBold list headers (`2e0894ce`) | Partial | Ghost X in `plugin/dialog.rs:271`. SemiBold headers come with F4. | S | F4 | Not started | |
| P14 | Expander framed like a group box (`451b6f9c`) | Missing | Parse `border` and draw the frame in `plugin/fold.rs`. | S | — | Not started | |
| P15 | Screen size and presses outside a canvas (`a16e142f`) | Missing | `UiScreenSize` import and event. A `wants_outside_presses` flag and pointer kind 5. | S | F1 | Not started | |
| P16 | Canvas resize (`ebcf854c`) | Missing | `CanvasChange::Resize(w, h)`, applied in `canvas.rs::sync`. | S | F1 | Not started | |
| P17 | Canvas inside a markup window (`5279f073`, `352099df`) | Missing | `<canvas id>` element in markup, sized by flex. `in_markup` flag, paired by id in `plugin/mod.rs`. | M | F1 | Not started | |
| P18 | Remembered canvas position (`f4a5b7bf`) | Missing | `remembers_position` flag, saved per character through `layout_persistence.rs`, debounced 0.5 s. | S-M | F1 | Not started | |
| P19 | Sample plugins: ControlsDemo, FlexDemo, ThemeGallery | Missing | Port as WASM samples to test the features above against. | S each | P1, P6 | Not started | Optional |

## Modern HUD decks

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

Start with F1 and the filled triangles, then the dock and themes, then the
decks. Each step depends only on the ones before it, and each phase can go
upstream as its own set of PRs.

1. **Decide F1 with Erik:** how fork ABI additions are versioned.
2. **World drawing:** F2, then W1 filled triangles (with W4 built in), W5
   logoff fix, W6 terrain read.
3. **Small independent wins:** G1 portal margin, P5 dock gear, P14 expander
   frame, P16 canvas resize, P15 screen size, W10 level curve. Each is S and
   needs nothing new.
4. **World markers:** W2 icons and W3 shapes as one marker ABI, then W11 docs.
5. **UI foundations:** F3 shapes, F4 fonts, F5 SVG decision.
6. **Look and dock:** P6 themes, P1 modern look, P2 dock pill, P3 and P4 dock
   icons, P7, P13.
7. **Canvas painting:** P8 fonts, P9 shapes, P12 regions, P10 layers, P11
   keyboard, P17 canvas in markup, P18 remembered position.
8. **Navigation:** G2 walk corridor, then W7, W9, W8 floorplan work. Run the
   G3 to G8 live checks alongside.
9. **Modern HUD:** F6, then H5 Chat model (logic only), then H1 to H3 Command
   and Spell, H4 Target, H6 to H9 Chat, H10 to H12 Gear, with H13 text size in
   each.
