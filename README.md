# Omnyra

Desktop app for content creators (Windows and Linux): every network in one place — analytics, best time to post, calendar and idea board, multi-network publishing with automatic 16:9 / 9:16 / 1:1 versions, a unified comment inbox, a thumbnail editor, a text assistant, moments for shorts, sponsorships and media kit, competitor tracking and a security module.

| Network                  | Analytics                                                                   | Publish                                | Comments       |
| ------------------------ | --------------------------------------------------------------------------- | -------------------------------------- | -------------- |
| YouTube                  | Full history (Analytics API), best time, retention                          | Yes (private until Google's API audit) | Read and reply |
| Twitch                   | Followers, VODs, clips, live viewers and chat recorded while Omnyra is open | —                                      | —              |
| Instagram (professional) | Followers, reach (last 30 days), Reels and posts                            | Reels                                  | Read and reply |
| TikTok                   | Followers, likes, videos                                                    | Yes (private until TikTok's audit)     | — (no API)     |

- User guide: [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md) (Spanish)
- Architecture: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) · Instagram/TikTok auth: [`docs/AUTH_STRATEGY.md`](docs/AUTH_STRATEGY.md) · Releases: [`docs/DISTRIBUTION.md`](docs/DISTRIBUTION.md) · Review: [`docs/REVIEW.md`](docs/REVIEW.md)
- Changelog: [`CHANGELOG.md`](CHANGELOG.md)

> Screenshots: pending (to be taken from the first signed release).

## Install

Download the files from the latest release.

**Windows 10/11** — run `Omnyra_<version>_x64-setup.exe` (per-user install, Spanish or English; updates itself). An `.msi` is also provided for managed installs. Install [FFmpeg](https://ffmpeg.org/download.html) and put it on the `PATH` (or set `OMNYRA_FFMPEG` / `OMNYRA_FFPROBE`) to use multi-network versions, clips and file checks.

**Debian / Ubuntu (22.04+)**

```sh
sudo apt install ./Omnyra_<version>_amd64.deb   # pulls ffmpeg and the tray library
```

Or use the AppImage (`chmod +x Omnyra_*.AppImage`; it updates itself and uses the system FFmpeg).

**Arch Linux** — from AUR (`yay -S omnyra`) or from this repository: `cd packaging/arch && makepkg -si`.

A system keyring is required to connect accounts (Windows Credential Manager; on Linux GNOME Keyring, KWallet or KeePassXC through Secret Service). For the tray icon on Linux the desktop needs a StatusNotifierItem host.

## Development

### Requirements

- Node.js (version in `.nvmrc`)
- Rust stable toolchain
- FFmpeg (see below)

Platform dependencies:

**Windows**

- Microsoft C++ Build Tools (MSVC)
- WebView2 runtime (preinstalled on Windows 11)
- Git for Windows, for Git Bash (used to run the `scripts/ci` scripts)

**Debian / Ubuntu**

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev libsecret-1-dev patchelf file
```

**Arch Linux**

```sh
sudo pacman -S webkit2gtk-4.1 base-devel openssl libayatana-appindicator librsvg libsecret
```

FFmpeg:

- Linux: install `ffmpeg` from your package manager. The `.deb` and the Arch package both depend on it.
- Windows: FFmpeg is not bundled (licence, H9): install it and put it on the `PATH`, or point `OMNYRA_FFMPEG` / `OMNYRA_FFPROBE` at the binaries.

### OAuth client IDs

Copy `.env.example` to `.env` and fill in the IDs. `src-tauri/build.rs` reads `.env` at build time. If a variable is already set in the environment (for example, a CI secret), that value is used instead. Without a client ID, the network shows as "not configured".

- Google (YouTube): in Google Cloud Console, enable the YouTube Data API v3 and the YouTube Analytics API. Then create an OAuth client of type **Desktop app**. Google documents the client secret of a Desktop app as non-confidential, so it is embedded in the build (`docs/ARCHITECTURE.md`, R3).
- Twitch: register the app as a **Public** client (device code flow, no secret).
- Instagram and TikTok: their secrets are confidential, so they live in the token broker (`broker/`, a Cloudflare Worker); only the public IDs and the broker URL go in `.env`. Setup in `docs/AUTH_STRATEGY.md`.
- Optional AI text assistant: each user enters their own Anthropic API key in the app; it is kept in the keyring.

Tokens are stored only in the OS keyring (Windows Credential Manager, or Secret Service on Linux via GNOME Keyring, KWallet or KeePassXC). If no keyring is available, accounts cannot be connected.

### Running

```sh
npm install
npm run tauri dev      # run the app
npm test               # frontend tests (Vitest)
cargo test --workspace # Rust tests (also regenerates src/lib/bindings.ts)
cargo test -p omnyra-core -- --ignored keyring  # real OS keyring round-trip (opt-in)
npm run lint && npm run typecheck
```

`src/lib/bindings.ts` is generated by `tauri-specta`; do not edit it by hand.

### Checks

`scripts/ci/check.sh` runs the same checks as CI:

- Prettier, ESLint and the TypeScript typecheck
- rustfmt and Clippy (with `-D warnings`)
- Vitest and `cargo test`
- verifies that `src/lib/bindings.ts` is committed and up to date

```sh
scripts/ci/check.sh          # all checks
scripts/ci/check.sh --tests  # tests only (Vitest and cargo test)
```

### Building installers

```sh
scripts/ci/bundle.sh deb,appimage   # Linux
scripts/ci/bundle.sh msi,nsis       # Windows (run in Git Bash)
```

The bundles are copied to `artifacts/` together with a `SHA256SUMS` file. Each run replaces the previous `artifacts/` directory. The `.deb` depends on `ffmpeg`.

### Arch Linux (PKGBUILD)

`packaging/arch/PKGBUILD` builds from a release tarball. Before a release exists, build from the current commit:

```sh
git archive --format=tar.gz --prefix=omnyra-1.0.0/ -o packaging/arch/omnyra-1.0.0.tar.gz HEAD
cd packaging/arch && makepkg -si
```

When the tarball sits next to the PKGBUILD, makepkg uses it instead of downloading.

After editing the PKGBUILD, regenerate `.SRCINFO`. CI fails if it is stale:

```sh
cd packaging/arch && makepkg --printsrcinfo > .SRCINFO
```

The `_owner` placeholder and the `sha256sums` entry (currently `SKIP`) are filled in at release time (`docs/DISTRIBUTION.md`).

### CI

`.github/workflows/ci.yml` runs on pushes to `main`, on pull requests and on manual dispatch:

- `check`: runs `scripts/ci/check.sh` on Linux (full checks) and on Windows (`--tests`).
- `bundle`: builds the `.deb` and AppImage on Ubuntu 22.04, so they run on more distributions, and the `.msi` and NSIS `.exe` on Windows.
- `arch`: builds the package with `makepkg` in an `archlinux` container, then lints it with namcap and checks its shared libraries.
- `install-debian`: installs the `.deb` on Debian stable.
- `install-windows`: silently installs the MSI, then the NSIS installer.

The `omnyra-linux`, `omnyra-windows` and `omnyra-arch` artifacts are kept for 14 days.

`.github/workflows/release.yml` runs on `v*` tags: signed bundles for Linux and Windows, the updater manifest `latest.json` and a GitHub release with the CHANGELOG notes (`docs/DISTRIBUTION.md`).

The logic lives in `scripts/ci/`, so the pipeline can be ported to another CI provider by calling the same scripts.
