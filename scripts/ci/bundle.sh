#!/usr/bin/env bash
# Build release bundles and copy them to ./artifacts/.
#   scripts/ci/bundle.sh deb,appimage   # Linux
#   scripts/ci/bundle.sh msi,nsis       # Windows (Git Bash)
set -euo pipefail
cd "$(dirname "$0")/../.."

bundles="${1:?usage: bundle.sh <bundle[,bundle...]>}"

[[ "${CI:-}" == "true" || ! -d node_modules ]] && npm ci --no-audit --no-fund

# Release CI sets OMNYRA_UPDATER_ARTIFACTS=1 (with the signing key) to produce .sig files.
extra=()
if [[ "${OMNYRA_UPDATER_ARTIFACTS:-}" == "1" ]]; then
  extra=(--config '{"bundle":{"createUpdaterArtifacts":true}}')
fi
npm run tauri -- build --ci --bundles "$bundles" "${extra[@]}"

out=artifacts
rm -rf "$out" && mkdir -p "$out"
dir=target/release/bundle
IFS=, read -ra list <<<"$bundles"
for b in "${list[@]}"; do
  case "$b" in
    deb) cp -v "$dir"/deb/*.deb "$out"/ ;;
    appimage) cp -v "$dir"/appimage/*.AppImage* "$out"/ ;;
    msi) cp -v "$dir"/msi/*.msi* "$out"/ ;;
    nsis) cp -v "$dir"/nsis/*-setup.exe* "$out"/ ;;
    *) echo "unknown bundle: $b" >&2; exit 1 ;;
  esac
done

( cd "$out" && sha256sum -- $(ls | grep -v '\.sig$') > SHA256SUMS )
ls -la "$out"
