#!/bin/bash
# The full check of the P20 to P24 branches, for Davids-MBP. It changes no branch and pushes nothing.
#
# Run from the main checkout, with the branches pushed to the fork:
#   OPENAC_DAT_DIR=<dats> bash docs/mbp-check-p20-p24.sh
#
# Each branch gets scripts/check.sh at its tip, then all five merged together get it once more. It
# works in a throwaway worktree, which it removes at the end. A failed branch does not stop the rest.
# Logs and a summary: ~/code/OpenAC/check-logs/p20-p24-<date>/.
#
# Known flakes before blaming a change (memory note mac-live-launch): host_parity fails about one run
# in three, also on untouched upstream-main, so rerun it. A NotFound under a fixture path means a stale
# test binary: cargo clean that crate.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
export TMPDIR=/tmp/
: "${OPENAC_DAT_DIR:?set OPENAC_DAT_DIR to the retail dats, or the real-dat tests are skipped}"

BRANCHES=(fix/expander-arrow-v2 fix/side-by-side-expanders-v2 fix/classic-expander-frame feat/dock-overflow-cues feat/dock-animation)
REPO="$(git rev-parse --show-toplevel)"
WORK="$REPO/../openac-rust-p20-p24-check"
LOGS="$HOME/code/OpenAC/check-logs/p20-p24-$(date +%Y%m%d-%H%M)"
SUMMARY="$LOGS/summary.txt"
mkdir -p "$LOGS"

git -C "$REPO" fetch origin --prune || exit 1
git -C "$REPO" fetch upstream || exit 1
BASE="$(git -C "$REPO" rev-parse upstream/main)"
echo "upstream/main: $BASE" | tee -a "$SUMMARY"
git -C "$REPO" worktree add --detach "$WORK" "$BASE" || exit 1
trap 'git -C "$REPO" worktree remove --force "$WORK"' EXIT
cd "$WORK" || exit 1
# One target folder across the runs, so each builds only what changed.
export CARGO_TARGET_DIR="$REPO/target/p20-p24-check"

run_check() { # name of the run; the tree is already checked out
    local name="$1" log="$LOGS/${1//\//-}.log" start=$SECONDS
    if sh scripts/check.sh >"$log" 2>&1; then
        echo "PASS $name at $(git rev-parse --short HEAD) in $((SECONDS - start))s: $(grep -c '^test result: ok' "$log") binaries" | tee -a "$SUMMARY"
    else
        echo "FAIL $name at $(git rev-parse --short HEAD) in $((SECONDS - start))s: see $log" | tee -a "$SUMMARY"
    fi
}

for b in "${BRANCHES[@]}"; do
    git checkout -q --detach "origin/$b" || { echo "MISSING $b" | tee -a "$SUMMARY"; continue; }
    if ! git merge-base --is-ancestor "$BASE" HEAD; then
        echo "BEHIND $b is not on upstream/main: rebase it" | tee -a "$SUMMARY"
    fi
    # No client internals in what goes upstream.
    if git diff "$BASE" HEAD | grep -E '^\+' | grep -nE 'gm[A-Z][A-Za-z0-9_]*UI::|DROPITEM_|UIElement_' >"$LOGS/${b//\//-}.internals.txt"; then
        echo "GREP $b adds client internals: see $LOGS/${b//\//-}.internals.txt" | tee -a "$SUMMARY"
    fi
    run_check "$b"
done

# All five together. P20 and P21 conflict on one doc comment, so conflicting lines take the side
# already merged; everything else merges as usual.
git checkout -q --detach "$BASE"
for b in "${BRANCHES[@]}"; do
    if ! git merge -q --no-edit -X ours "origin/$b" >>"$LOGS/merge.log" 2>&1; then
        echo "MERGE-FAILED $b: $(git diff --name-only --diff-filter=U | tr '\n' ' ')" | tee -a "$SUMMARY"
        git merge --abort
    fi
done
run_check all-five-merged

echo "Done. Summary: $SUMMARY"
