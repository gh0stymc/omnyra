#!/usr/bin/env bash
# Builds Omnyra in release mode and installs it for the current user only
# (~/.local), so it shows up in the application menu like any other app.
# Re-run after pulling changes to update. Uninstall with: scripts/install-local.sh --uninstall
set -euo pipefail

cd "$(dirname "$0")/.."
PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin/omnyra"
DESKTOP="$PREFIX/share/applications/omnyra.desktop"
ICONS="$PREFIX/share/icons/hicolor"

refresh_caches() {
  update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
  gtk-update-icon-cache -f -t "$ICONS" 2>/dev/null || true
}

if [[ "${1:-}" == "--uninstall" ]]; then
  rm -f "$BIN" "$DESKTOP"
  for size in 32x32 64x64 128x128 256x256 512x512; do
    rm -f "$ICONS/$size/apps/omnyra.png"
  done
  refresh_caches
  echo "Omnyra desinstalado de $PREFIX (los datos de la app se conservan)."
  exit 0
fi

if [[ "${1:-}" != "--skip-build" ]]; then
  npm run tauri -- build --no-bundle
fi

install -Dm755 target/release/omnyra "$BIN"
for size in 32x32 64x64 128x128; do
  install -Dm644 "src-tauri/icons/$size.png" "$ICONS/$size/apps/omnyra.png"
done
install -Dm644 src-tauri/icons/128x128@2x.png "$ICONS/256x256/apps/omnyra.png"
install -Dm644 src-tauri/icons/icon.png "$ICONS/512x512/apps/omnyra.png"
# Absolute Exec path: ~/.local/bin is not always in the PATH the launcher sees.
sed "s|^Exec=omnyra|Exec=$BIN|" packaging/linux/omnyra.desktop | install -Dm644 /dev/stdin "$DESKTOP"
refresh_caches

echo "Omnyra instalado en $BIN. Búscalo en el menú de aplicaciones."
