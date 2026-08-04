#!/usr/bin/env bash
set -euo pipefail

# Regenerate the app icon: draw the master PNG (Pillow, in the repo venv), then
# build the multi-resolution AppIcon.icns via sips + iconutil. Commit the
# resulting icon/AppIcon.icns so make-app.sh does not need Python/Pillow.
#
#   ./make-icon.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

MASTER="icon/icon_1024.png"
ICNS="icon/AppIcon.icns"

echo ">>> drawing ${MASTER}"
# shellcheck disable=SC1091
source "${SCRIPT_DIR}/../.venv/bin/activate"
python icon/gen-icon.py "${MASTER}"

echo ">>> building iconset -> ${ICNS}"
ICONSET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "${ICONSET}"
# slot-name -> pixel size for each required entry
for spec in \
  "16x16:16" "16x16@2x:32" "32x32:32" "32x32@2x:64" \
  "128x128:128" "128x128@2x:256" "256x256:256" "256x256@2x:512" \
  "512x512:512" "512x512@2x:1024"; do
  name="${spec%%:*}"; px="${spec##*:}"
  sips -z "${px}" "${px}" "${MASTER}" --out "${ICONSET}/icon_${name}.png" >/dev/null
done
iconutil -c icns "${ICONSET}" -o "${ICNS}"
rm -rf "$(dirname "${ICONSET}")"

echo ">>> done: ${SCRIPT_DIR}/${ICNS}"
