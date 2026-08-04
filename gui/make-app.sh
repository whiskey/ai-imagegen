#!/usr/bin/env bash
set -euo pipefail

# Build ai-imagegen-gui and wrap it in a double-clickable macOS .app bundle.
# Unsigned + ad-hoc signed, arm64-only — fine for local use on this machine.
#
#   ./make-app.sh          # build + bundle into ./dist/AI ImageGen.app
#   open "dist/AI ImageGen.app"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

APP_NAME="AI ImageGen"                 # Finder / Dock display name
BIN_NAME="ai-imagegen-gui"             # binary name from Cargo.toml
BUNDLE_ID="de.staticline.ai-imagegen"
VERSION="0.1.0"

APP="dist/${APP_NAME}.app"

echo ">>> building release binary…"
source "$HOME/.cargo/env" 2>/dev/null || true
cargo build --release

echo ">>> assembling ${APP}…"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/release/$BIN_NAME" "$APP/Contents/MacOS/$BIN_NAME"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>               <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>        <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>         <string>${BUNDLE_ID}</string>
    <key>CFBundleExecutable</key>         <string>${BIN_NAME}</string>
    <key>CFBundleVersion</key>            <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key> <string>${VERSION}</string>
    <key>CFBundlePackageType</key>        <string>APPL</string>
    <key>LSMinimumSystemVersion</key>     <string>11.0</string>
    <key>NSHighResolutionCapable</key>    <true/>
    <key>LSApplicationCategoryType</key>  <string>public.app-category.graphics-design</string>
</dict>
</plist>
PLIST

# Ad-hoc signature: lets macOS treat it as a stable app identity (no cert needed).
# Locally-built apps aren't quarantined, so this runs on double-click without a
# Gatekeeper prompt.
if codesign --force --sign - "$APP" 2>/dev/null; then
  echo ">>> ad-hoc signed"
else
  echo ">>> codesign unavailable — left unsigned (still runnable locally)"
fi

echo ">>> done: $SCRIPT_DIR/$APP"
echo "    launch:  open \"$SCRIPT_DIR/$APP\"   (or double-click in Finder)"
echo
echo "    Note: the app finds generate.sh at the repo path baked in at build time."
echo "    If you move the repo, set AI_IMAGEGEN_ROOT=/path/to/ai-imagegen."
