#!/usr/bin/env bash
# Builds "CuantoQuedapp.app" (Apple Silicon) into dist/ and signs it ad hoc.
#
#   scripts/package-macos.sh             build only
#   scripts/package-macos.sh --install   also copy it to /Applications (replaces a previous copy)
#   scripts/package-macos.sh --zip       also write dist/CuantoQuedapp-<version>-macos-arm64.zip
#                                        (+ .sha256) to share; flags can be combined
#
# Ad-hoc signing is enough to run the app on the Mac that built it. Sharing it with other
# Macs needs a Developer ID signature and notarization, which this script does not do.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

APP_NAME="CuantoQuedapp"
BUNDLE_ID="app.cuantoquedapp"
EXECUTABLE="cuantoquedapp"
# Claude Code status line helper; see scripts/claude-statusline.sh.
STATUSLINE_HELPER="cuantoquedapp-statusline"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="dist/$APP_NAME.app"

# Keep the machine's user name and folders out of the binary (panic messages embed paths).
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$PWD=. --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup --remap-path-prefix=$HOME=/home"

echo "==> Building (release)"
cargo build --release --locked --bin "$EXECUTABLE" --bin "$STATUSLINE_HELPER"

echo "==> Creating $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/release/$EXECUTABLE" "$APP/Contents/MacOS/$EXECUTABLE"
cp "target/release/$STATUSLINE_HELPER" "$APP/Contents/MacOS/$STATUSLINE_HELPER"

echo "==> Generating the icon"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
ICONSET="$WORK/AppIcon.iconset"
cargo run --quiet --release --example generate_icon -- "$ICONSET"
# The generator writes uncompressed PNGs; sips recompresses them.
for png in "$ICONSET"/*.png; do
  sips -s format png "$png" --out "$png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>$EXECUTABLE</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" >/dev/null

echo "==> Writing the third-party licenses"
# Most dependencies ask for their license notice to travel with the binary: it goes inside the
# app, so every copy (installed or zipped) carries it. Never package without it.
if ! cargo about --version >/dev/null 2>&1; then
  echo "cargo-about is missing. Install it with: cargo install cargo-about --locked --features cli" >&2
  exit 1
fi
cargo about generate --fail --locked about.hbs -o "$APP/Contents/Resources/THIRD-PARTY-LICENSES.html"

echo "==> Signing (ad hoc)"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

echo "Done: $APP ($(du -sh "$APP" | cut -f1))"

INSTALL=0
ZIP=0
for flag in "$@"; do
  case "$flag" in
    --install) INSTALL=1 ;;
    --zip) ZIP=1 ;;
    *) echo "Unknown option: $flag" >&2; exit 2 ;;
  esac
done

if [ "$ZIP" = 1 ]; then
  ZIP_NAME="CuantoQuedapp-$VERSION-macos-arm64.zip"
  echo "==> Creating dist/$ZIP_NAME"
  rm -f "dist/$ZIP_NAME" "dist/$ZIP_NAME.sha256"
  # ditto keeps the signature and the bundle structure intact (plain zip can break them).
  # --norsrc leaves out extended attributes, which other unzip tools turn into ._ files.
  ditto -c -k --norsrc --keepParent "$APP" "dist/$ZIP_NAME"
  (cd dist && shasum -a 256 "$ZIP_NAME" > "$ZIP_NAME.sha256")
  echo "Done: dist/$ZIP_NAME ($(du -h "dist/$ZIP_NAME" | cut -f1))"
fi

if [ "$INSTALL" = 1 ]; then
  echo "==> Installing in /Applications"
  rm -rf "/Applications/$APP_NAME.app"
  cp -R "$APP" "/Applications/$APP_NAME.app"
  echo "Installed: /Applications/$APP_NAME.app"
fi
