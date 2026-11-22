# Changelog

All notable changes to Omnyra. The format follows [Keep a Changelog](https://keepachangelog.com/) and the project uses [Semantic Versioning](https://semver.org/).

## [1.0.0] - 2026-11-22

First release.

### Networks

- **YouTube**: OAuth with PKCE and loopback, tokens only in the OS keyring; daily snapshots with full history from the Analytics API; Shorts detection; quota ledger with per-feature budgets; resumable uploads with thumbnail and playlist (private until Google's API audit).
- **Twitch**: device code login (public client, no secret); followers history from follow dates, VODs and clips; average and peak viewers sampled while Omnyra is open; public chat activity recorded during streams.
- **Instagram** (professional accounts) and **TikTok**: login through a stateless token broker that keeps their client secrets out of the app (`broker/`, Cloudflare Worker); profile, contents and stats; Reels publishing (resumable `rupload`) and TikTok Direct Post (private while the app is unaudited).

### Features

- Global dashboard and per-network views; best time to publish heatmap; period selector.
- Calendar (month/week, drag to reschedule) and Kanban idea board with attachments and reminders.
- Multi-network publishing: one video, several destinations, FFmpeg versions in 16:9, 9:16 or 1:1 (crop, black bars or blurred background), trimming and size limits, local scheduling, and "prepare and download".
- Unified comment inbox (YouTube, Instagram) with filters, replies, rule-based classification (questions, spam, negative, positive) and repeated-question groups that can be sent to the idea board.
- Thumbnail editor with layers (images, outlined text, shapes, stickers), asset library (skins, avatars...), templates and exports sized for each network.
- Text assistant with per-network templates and hashtags, plus optional suggestions from Claude with the user's own API key.
- Moments for shorts from YouTube retention and Twitch chat peaks, cut vertically with FFmpeg.
- Sponsorship manager (deliverables, payments, overdue amounts, files) and a printable media kit.
- Competitor tracking and keyword trends on YouTube.
- Security: account checklist with links to each network's settings, phishing and scam detection in comments, impersonation suspects, PIN lock (interface only) and a tamper-evident audit log.
- System tray, background sync, start at login, native notifications (reminders, milestones, performance), Spanish and English.
- Signed auto-updates for the Windows installer and the AppImage; `.deb`, `.msi` and Arch PKGBUILD.

### Known limitations

- No X connector (paid API, H1).
- Instagram history only covers the last 30 days of insights; TikTok has no history, so both fill in over time.
- The PIN lock does not encrypt the database (decision D7).
- FFmpeg is not bundled on Windows.
- Real-world tests pending for Instagram, TikTok, Windows and several Linux desktops (see `PROGRESS.md`).
