#!/usr/bin/env sh
# Build OpenAC.app from this repository's main and install it in /Applications, to play on.
# Run it again to move to a newer main.
#
# A release's client, once installed, wins over the `openac` beside the launcher, and a release's
# launcher replaces the bundle. So the installed launcher reads no release feed: it starts through
# a wrapper that names a feed nothing answers, and it says "Update check unavailable; the launcher
# works offline." The plugin list has its own address and still works.
#
#   scripts/install-play-macos.sh
set -eu
cd "$(dirname "$0")/.."

app=/Applications/OpenAC.app
bundle_id=org.openac.rust

branch="$(git branch --show-current)"
if [ "$branch" != main ]; then
    echo "This installs from main, and the checkout is on '${branch:-a detached HEAD}'." >&2
    exit 1
fi
if pgrep -x openac-launcher > /dev/null || pgrep -x openac > /dev/null; then
    echo "OpenAC is running; quit it first." >&2
    exit 1
fi
if [ -e "$app" ]; then
    installed="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Contents/Info.plist" 2> /dev/null || true)"
    if [ "$installed" != "$bundle_id" ]; then
        echo "$app is not this client (bundle id '$installed'), so it stays; move it away first." >&2
        exit 1
    fi
fi

sh scripts/bundle-macos.sh
bundle="${CARGO_TARGET_DIR:-target}/release/macos/OpenAC.app"

cat > "$bundle/Contents/MacOS/openac-play" <<'EOF'
#!/bin/sh
exec "$(dirname "$0")/openac-launcher" --update-manifest-uri http://127.0.0.1:9/manifest.json "$@"
EOF
chmod 0755 "$bundle/Contents/MacOS/openac-play"
/usr/libexec/PlistBuddy -c 'Set :CFBundleExecutable openac-play' "$bundle/Contents/Info.plist"
# Each program's signature names Info.plist, so all of them are signed again after that edit.
for program in openac openac-launcher openac-play; do
    codesign --force --sign - --timestamp=none "$bundle/Contents/MacOS/$program"
done
codesign --force --sign - --timestamp=none "$bundle"
codesign --verify --deep --strict "$bundle"

rm -rf "$app"
ditto "$bundle" "$app"
echo "Installed $app from main at $(git rev-parse --short HEAD)"
