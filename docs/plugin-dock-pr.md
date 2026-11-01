# Plugin dock PR: draft for review

Local to this fork. Not opened: the user reviews this and decides when.
Fill in the `TODO` line first, and recheck that upstream has not moved from `57a8e81`.

Base `eriknihlen/openac-rust` `main`, head `davidc-smith:feat/plugin-dock-v2`.

Title: `Plugin dock and modern plugin windows: smooth shapes, themes, fonts, API 1.4`

```markdown
## What this changes

This replaces the plugin shelf with a dock, and gives plugin windows that opt in to a theme a modern look. It is one series of 32 ordered commits, each of which builds and passes by itself, because each part builds on the one before. It is large, about 20,000 added lines, much of it tests.

On the dock you agreed to this:

> Yeah sounds good. Its a bit buggy how its working now. Using icons from the dats but it is missing the underlay so icons have white borders. Havent fixed it yet. That is in the c# client. I only wish it could be minimized

So the dock replaces the shelf with no option. The rest was not part of that message, and it is new to you here: the modern look, the extra themes, plugin fonts and the raise to API 1.4. These only reach windows that opted in with `theme="plugin"`, under a theme the player picked.

In order:

1. **Smooth shapes.** `DrawCmd::Triangles`, with a colour at every corner, and `ac-ui/src/shapes.rs`, which turns rounded rectangles, ellipses, polygons, strokes and soft shadows into triangles. A band one framebuffer pixel wide fades each edge. No new shader or vertex format.
2. **Fonts.** A bundled Noto Sans SemiBold, a sharper bake at the display's density with the same layout, and horizontal gradients.
3. **Modern look.** Rounded, shadowed windows with a header band, pill tabs, switches, rounded fields and menus, a themed Confirm box, and expanders framed like a group box.
4. **Themes and plugin API.** Lavender Dusk, Ember, Bloodmoon, Glacier and Parchment, which is the first light theme. `color="A|light:B"` and `.light.png` / `.dark.png` in markup. Plugin fonts: the bundled font in either weight or the plugin's own TTF or OTF, handed over as bytes. Theme metrics for canvases.
5. **The dock.** Floating, floating horizontally, or snapped to any of the four screen edges. Closed, it is one tile with the OpenAC icon and a count of open windows, and the closed state is remembered. Three sizes, and it opens on a click or on hover. A grip shows while windows are unlocked, and only the grip moves it. Hover labels, and a menu for all of this in Plugin appearance. The saved window name stays `plugin-shelf`, so placements survive.
6. **Icons.** Dat icons a plugin names are keyed, which removes the white borders you mentioned. On `0x06002C41`, 102 opaque white pixels become none. A window can name its own PNG, with light and dark versions, ahead of the plugin's `icon.png`.

**Plugin API.** Raises `API_CURRENT` to 1.4, and `API_MINIMUM` stays 1.0. The additions are the five theme kinds, the font imports, a font at the end of `PaintCmd::Text` and `icon_file`, all `since (1, 4)`. **Plugins do not need rebuilding.** A plugin built for 1.3 or older cannot read the new theme tags, so it is told Moss under any of them, with the theme's real name and colours.

**A bug this fixes.** `ui_theme` was missing from the host's list of UI imports, so a real plugin always read Classic.

**At a UI Scale.** The batcher's whole-pixel snap skips coloured triangles, which are multiplied but not snapped, because a snap folds their soft edge. `UiRoot`'s pixels per unit includes the player's scale, so soft edges and sharper bakes follow it. Each new scale bakes the sharper fonts again, and I did not measure what that costs when stepping through the option.

**Dependencies.** No new crates. `ttf-parser` gains its `opentype-layout` feature for GPOS kerning, and `ac-plugin-ui` now uses `fontdue` and `ttf-parser`. Canvas fonts are baked there, so the client and the SDK's fake host answer alike. The PNG decoder moved from `ac-plugin` to `ac-plugin-ui`, so that `ac-render` can use it within the crate graph test.

**Where it comes from.** It is ported from my own fork of OpenAC. The top and bottom edges, the horizontal layout, the closed tile, the sizes and the grip are new here.

**Found while testing, not changed here.** Expanders placed side by side can make a header impossible to reopen. Each one that folds moves what is below it up by its own height, so together they can put a scrolling group over a header, and the group takes the clicks. It happens on `main` today, and in OpenAC. I'll send a fix separately, along with a frame for Classic expanders, which stay unframed here.

## Original behavior

The original client has none of this, and no retail window changes. Plugin windows in Classic draw as they did: a test draws every control through 27 states and compares the draw lists with hashes taken from the code before this work, and it passes through the series unchanged. The dock has its own Classic colours, the one agreed change there. One small exception: a Classic plugin window that names a dat icon now shows that icon keyed, so its white pixels draw black.

## Testing

- TODO: `scripts/check.sh` on macOS (Apple Silicon) at the final tip, with real dats. It last ran at the commit before the comment rewording, green with 10849 tests. fmt, clippy with `-D warnings` and the tests with `--include-ignored` also passed at each of the 32 commits there.
- New tests port the fork's dock, theme and font tests, with cases added for horizontal layouts and the top and bottom edges. GPU tests on Metal read back shape pixels.
- Older plugins: real modules built by the 1.2 and 1.3 SDKs load, show on the dock and are told a theme kind they know. New imports each have a test through a real plugin module.
- Hostile numbers: 300,000 shape calls with NaN, infinities and huge values gave no panics and nothing non-finite in the draw list.
- Damaged fonts: 1,176,600 runs on a CFF fixture and Noto over 180 s gave no crash or parser panic, with the slowest bake 0.15 s and a peak of 76 MB.
- Live on a local ACE server on a Retina Mac (Metal), with a real mouse: all six positions in all eight themes, the three sizes and the closed tile in several, Moss at UI Scale 110% and 150%, Parchment and Classic at 150%, and an overflowing dock with 36 more plugins, scrolled with the wheel.
- Not tested on Windows or Linux.

## Checklist
- [x] `scripts/check.sh` (or `scripts/check.ps1`) passes: fmt, clippy with `-D warnings`, the WebAssembly build, tests
- [x] Ran the real-data tests (`--include-ignored`) if this touches data files, rendering or the interface
- [ ] One topic per PR: see the top. One series because each part builds on the last, as you asked for dependent work
- [x] If `ac-plugin-abi` changed: the layout fingerprint is updated and the PR says plugins must be rebuilt: updated, and they need not be
- [x] New options are documented in `docs/launch-options.md`: no new launch options. The dock's settings are in Plugin appearance, and the plugin API is in `docs/plugin-development.md`
- [x] No code copied from ACEmulator, ACViewer, or holtburger (AGPL/GPL)
- [x] No comments describing the original client's internals

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_01SvfgcdhJai8fyEmREnNEJB
```
