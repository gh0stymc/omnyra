# Omnyra

Omnyra is a desktop app for people who post on more than one platform and are tired of keeping a browser tab open for each one. It brings YouTube, Twitch, Instagram and TikTok together in a single window — your stats, your comments, your uploads and your planning, all in one place. It runs on Windows and Linux, and everything stays on your own machine.

I built it for my own channel, so it leans towards what a solo creator actually needs from day to day rather than a big agency dashboard.

## What you can do with it

- **See all your numbers in one dashboard** — followers, views, watch time and how each post is doing, across every account, with the full history rather than just the last 28 days.
- **Know when to post.** Omnyra works out your best posting times from your own audience data.
- **Publish once, everywhere.** Upload a video and it makes the 16:9 / 9:16 / 1:1 versions automatically and sends each one to the right platform.
- **Handle comments from one inbox** — read and reply across YouTube and Instagram, with questions grouped together and spam and phishing flagged.
- **Plan your content** with a calendar and an idea board (you can turn recurring questions from your comments straight into ideas).
- **Make thumbnails** in a built-in editor, and reuse them as templates.
- **Keep an eye on the competition** — track other channels and trending topics on YouTube.
- **Manage sponsorships** — deals, deliverables, payments and a media kit you can export.
- **Lock it down.** Optional PIN lock and a security module for your accounts.

There's also an optional AI writing assistant for captions and replies — you plug in your own Anthropic API key and it's kept in your system keyring.

## Platform support

| Platform                 | Stats                                                                       | Publishing                              | Comments       |
| ------------------------ | --------------------------------------------------------------------------- | --------------------------------------- | -------------- |
| YouTube                  | Full history (Analytics API), best time to post, retention                  | Yes (private until Google's API review) | Read and reply |
| Twitch                   | Followers, VODs, clips, live viewers and chat recorded while Omnyra is open | —                                       | —              |
| Instagram (professional) | Followers, reach (last 30 days), Reels and posts                            | Reels                                   | Read and reply |
| TikTok                   | Followers, likes, videos                                                    | Yes (private until TikTok's review)     | — (no API)     |

> Screenshots are coming once there's a first signed release to grab them from.

## Installing

Grab the files from the [latest release](../../releases/latest).

**Windows 10/11** — run `Omnyra_<version>_x64-setup.exe`. It installs per-user (Spanish or English) and updates itself. There's also an `.msi` for managed setups. For the multi-format versions, clips and file checks you'll need [FFmpeg](https://ffmpeg.org/download.html) on your `PATH` (or point `OMNYRA_FFMPEG` / `OMNYRA_FFPROBE` at it).

**Debian / Ubuntu (22.04+)**

```sh
sudo apt install ./Omnyra_<version>_amd64.deb   # pulls in ffmpeg and the tray library
```

Or use the AppImage (`chmod +x Omnyra_*.AppImage`) — it updates itself and uses your system FFmpeg.

**Arch Linux** — from the AUR (`yay -S omnyra`) or from this repo: `cd packaging/arch && makepkg -si`.

To connect accounts you need a system keyring (Windows Credential Manager, or GNOME Keyring / KWallet / KeePassXC through Secret Service on Linux). On Linux the tray icon needs a StatusNotifierItem host.

## Building from source

You'll need **Node.js** (the version in `.nvmrc`), a **stable Rust toolchain** and **FFmpeg**.

System dependencies:

<details>
<summary>Debian / Ubuntu</summary>

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev libsecret-1-dev patchelf file
```

</details>

<details>
<summary>Arch Linux</summary>

```sh
sudo pacman -S webkit2gtk-4.1 base-devel openssl libayatana-appindicator librsvg libsecret
```

</details>

<details>
<summary>Windows</summary>

Microsoft C++ Build Tools (MSVC), the WebView2 runtime (already there on Windows 11) and Git for Windows (the `scripts/ci` scripts run in Git Bash).
</details>

Then:

```sh
npm install
npm run tauri dev      # run the app
npm test               # frontend tests (Vitest)
cargo test --workspace # Rust tests
```

Before connecting real accounts, copy `.env.example` to `.env` and fill in your own OAuth client IDs — `src-tauri/build.rs` reads them at build time, and any platform without an ID just shows as "not configured". The details for each platform are in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/AUTH_STRATEGY.md`](docs/AUTH_STRATEGY.md). Access tokens are only ever stored in your OS keyring, never on disk in plain text.

`scripts/ci/check.sh` runs the full set of checks (formatting, linting, typecheck and tests) — the same ones CI runs.

## How it's built

Tauri 2 with a Rust core and a React (TypeScript) front end. The heavy lifting lives in the `omnyra-core` crate; the UI talks to it through generated bindings. More detail — and the reasoning behind the bigger decisions — is in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Docs

- [User guide](docs/USER_GUIDE.md) (Spanish)
- [Architecture](docs/ARCHITECTURE.md) · [Instagram/TikTok auth](docs/AUTH_STRATEGY.md) · [Releases & packaging](docs/DISTRIBUTION.md)
- [Changelog](CHANGELOG.md)
