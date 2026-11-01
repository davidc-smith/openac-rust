# PR drafts: tracker P20 to P24

Local to this fork. Five separate PRs, each off `upstream-main`, one commit each. None is open yet.

Before sending any of them: run the MBP check (`docs/mbp-check-p20-p24.sh`), replace each `<hash>` with the checked commit, tick the checklist only for what really passed, and let the user read the copy. Each PR ends with the attribution lines the session gives.

The suggested order is P24, P20, P21, P22, P23. P20 and P21 conflict on `MarkupExpander`'s doc comment only; P22 and P23 merge cleanly. Every other pair merges cleanly too.

---

## P24: `fix/expander-arrow`

**Title:** An expander's arrow follows a header click

### What this changes

Clicking an unbound expander's header folded and opened it, but the arrow kept pointing the old way. A click or key press runs in the header widget, and the expander cannot reach a widget while it runs, so its update to the arrow was lost. An expander bound to a value was fine, since that changes on a tick. The header now reads the state back after toggling.

**Plugin API.** No change.

### Original behavior

The original client has no plugin expanders, so there is nothing in it to compare against. OpenAC's header reads the state as it draws, so it does not have this bug.

### Testing

- `scripts/check.sh` green on macOS (Apple Silicon) at `<hash>`, with the real-dat tests.
- A test clicks an unbound expander and checks the arrow each way.
- By hand on a local ACE server on macOS (Metal), under Classic: clicking each header flips its arrow both ways. Not tested on Windows or Linux.

### Checklist
- [ ] `scripts/check.sh` (or `scripts/check.ps1`) passes: fmt, clippy with `-D warnings`, the WebAssembly build, tests
- [ ] Ran the real-data tests (`--include-ignored`) if this touches data files, rendering or the interface
- [x] One topic per PR
- [x] No code copied from ACEmulator, ACViewer, or holtburger (AGPL/GPL)
- [x] No comments describing the original client's internals

---

## P20: `fix/side-by-side-expanders`

**Title:** Expanders place their siblings afresh when one folds

### What this changes

This is the bug I mentioned on #64. Folding moved everything below an expander by that expander's height, so expanders side by side added up: what was below them rose by the sum and could end up over a header, which then could not be clicked open again. Limiting each move would break when they reopened in another order.

Each sibling is now placed from its authored box whenever an expander folds or opens: it keeps its authored gap to the lowest thing above it. Where it lands depends only on which expanders are folded now. Only what overlaps it across the window counts as above it, so each column follows its own expander. Expanders stacked in one column add up as before.

**Behaviour change.** An element below an expander but outside its width no longer moves when the expander folds. OpenAC moves it. A plugin that relied on that would see a gap, which is why this is stated here.

**Plugin API.** No change.

### Original behavior

The original client has no plugin expanders, so there is nothing in it to compare against. OpenAC has the same adding-up.

### Testing

- `scripts/check.sh` green on macOS (Apple Silicon) at `<hash>`, with the real-dat tests.
- New tests cover expanders side by side, folding and reopening in any order, stacked expanders, stretched siblings and separate columns.
- By hand on a local ACE server on macOS (Metal): a window with expanders folded at the start, under Moss; and under Classic, three side by side folded and reopened in different orders, with what is below staying clear of the headers and coming back to its place. Not tested on Windows or Linux.

### Merging with the Classic frame PR

This conflicts with the Classic expander frame PR only on `MarkupExpander`'s doc comment. I'll rebase whichever merges second.

### Checklist
- [ ] `scripts/check.sh` (or `scripts/check.ps1`) passes: fmt, clippy with `-D warnings`, the WebAssembly build, tests
- [ ] Ran the real-data tests (`--include-ignored`) if this touches data files, rendering or the interface
- [x] One topic per PR
- [x] No code copied from ACEmulator, ACViewer, or holtburger (AGPL/GPL)
- [x] No comments describing the original client's internals

---

## P21: `fix/classic-expander-frame`

**Title:** Classic expanders are framed like a group box

### What this changes

This is the frame I mentioned on #64. A modern theme frames an expander with a border round it, a band behind the header and a divider under that. Classic drew only the arrow and caption, which looked odd beside the framed themes. It now draws the same three parts, square and in Classic colours. `border="false"` leaves them out, as it does under a theme.

**This changes how Classic windows look,** for any plugin with an expander. Plugins keep the old look with `border="false"`.

**Pinned draws.** `crates/ac-ui/tests/data/plugin_classic/pinned.txt` moves, because the gallery behind it has expanders. With `border="false"` on those expanders, the new code matches the old pins exactly, so the frame is the only change. The dialog line does not move.

**Plugin API.** No change. `border` already exists.

### Original behavior

The original client has no plugin expanders, so there is nothing in it to compare against.

### Testing

- `scripts/check.sh` green on macOS (Apple Silicon) at `<hash>`, with the real-dat tests.
- New tests: the frame reaches round the content with its band and divider, is just the header when folded, and is absent with `border="false"`.
- By hand on a local ACE server on macOS (Metal), under Classic: framed, folded, and `border="false"`. Not tested on Windows or Linux.

### Merging with the side-by-side expanders PR

This conflicts with that PR only on `MarkupExpander`'s doc comment. I'll rebase whichever merges second.

### Checklist
- [ ] `scripts/check.sh` (or `scripts/check.ps1`) passes: fmt, clippy with `-D warnings`, the WebAssembly build, tests
- [ ] Ran the real-data tests (`--include-ignored`) if this touches data files, rendering or the interface
- [x] One topic per PR
- [x] No code copied from ACEmulator, ACViewer, or holtburger (AGPL/GPL)
- [x] No comments describing the original client's internals

---

## P22: `feat/dock-overflow-cues`

**Title:** An overflowing plugin dock shows there is more and how far through it is

### What this changes

When the plugin windows did not fit, the dock showed as many as it could, scrolled with the wheel, and faded the ends of the list. Nothing said the list could scroll, or where in it you were.

An overflowing list now has an arrow at each end and a thin bar beside it.

- An arrow moves a page, keeping one slot on screen to hold your place, dims when there is no more that way, and its hover label says how many more plugins.
- The bar's thumb shows how much of the list is on screen and where, on the side away from the screen edge, clear of the open dots and edge pills.
- The wheel still moves one slot.

The list is now as long as its longest page, because one plugin's windows sit closer together than two plugins', and pages used to differ in length: the gear and the dock's end moved as it scrolled, which would have moved the arrow out from under the pointer. Closing the dock keeps the place in the list.

The fades are gone. They dimmed whole icons that could still be clicked, and with the longest-page rule could sit over empty space.

**Plugin API.** No change.

### Original behavior

The original client has no plugin dock, so there is nothing in it to compare against.

### Testing

- `scripts/check.sh` green on macOS (Apple Silicon) at `<hash>`, with the real-dat tests.
- New tests cover paging and stopping at the ends, the wheel on an arrow, the arrows' labels and how long they stay, dimming, the bar's place, and a list that fits having none of it.
- By hand on a local ACE server on macOS (Metal), under Classic with 36 extra plugins: the arrows and their labels, paging to the end, the wheel, the bar, and closing and reopening at the same place. Not tested on Windows or Linux.

### Checklist
- [ ] `scripts/check.sh` (or `scripts/check.ps1`) passes: fmt, clippy with `-D warnings`, the WebAssembly build, tests
- [ ] Ran the real-data tests (`--include-ignored`) if this touches data files, rendering or the interface
- [x] One topic per PR
- [x] No code copied from ACEmulator, ACViewer, or holtburger (AGPL/GPL)
- [x] No comments describing the original client's internals

---

## P23: `feat/dock-animation`

**Title:** The plugin dock opens and closes with a short slide

### What this changes

Opening and closing snapped from the tile to the whole dock in one frame, and a dock that had to move to fit jumped there. The dock now runs from where it was to where it ends up in 0.14 seconds, quick at first and settling at the end, so its slots show as the frame reaches them.

Only the look takes time:

- It counts as opened or closed at once, and a press on the way turns it round from where it is.
- What is saved is where it is going, not where it has got to.
- On the way closed it keeps its slots, so they go with it.
- Taking hold of it, a new screen size or a new mode end the slide.
- Putting it back from saved state does not slide.

There is no option for this, as with the rest of the dock's look.

**Plugin API.** No change.

### Original behavior

The original client has no plugin dock, so there is nothing in it to compare against.

### Testing

- `scripts/check.sh` green on macOS (Apple Silicon) at `<hash>`, with the real-dat tests.
- New tests cover opening, closing, the easing, turning round on the way, sliding to fit and back, what is saved on the way, a new screen size or mode on the way, and saved state not sliding.
- By hand on a local ACE server on macOS (Metal), floating under Moss, and under Classic with the dock overflowing. Not tested on Windows or Linux.

### Checklist
- [ ] `scripts/check.sh` (or `scripts/check.ps1`) passes: fmt, clippy with `-D warnings`, the WebAssembly build, tests
- [ ] Ran the real-data tests (`--include-ignored`) if this touches data files, rendering or the interface
- [x] One topic per PR
- [x] No code copied from ACEmulator, ACViewer, or holtburger (AGPL/GPL)
- [x] No comments describing the original client's internals
