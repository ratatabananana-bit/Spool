# Spool

Portable Windows YouTube downloader. A small Tauri shell wrapping `yt-dlp` and `ffmpeg`, with a queue, playlist handling, and resolution presets up to 4K.

No installer. One folder. Move it anywhere.

## Features

- Multi-URL paste + queue with configurable concurrency
- Resolutions: 2160p (4K), 1440p (2K), 1080p, 720p, 480p — any / MP4 / WebM
- Audio extraction: MP3, M4A, Opus, WAV, FLAC
- Playlist handling: off / ask / all
- yt-dlp auto-updates on launch
- Inline per-job log panel (copy / save)
- Drag-drop a `.txt` of URLs onto the window
- ffmpeg-missing and output-folder-gone banners
- Subtitles, SponsorBlock, thumbnail/metadata embedding, cookies-from-browser, rate limit, retries, passthrough args
- Light / Dark / System theme
- English + 繁體中文 UI
- All settings live in `settings.json` next to the exe

## Download

Grab the latest portable ZIP from the [Releases](https://github.com/ratatabananana-bit/Spool/releases) page. Extract anywhere, run `Spool.exe`.

## Requirements

- Windows 10 or 11
- [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) — preinstalled on Win11

## Building from source

```powershell
# 1. Install prerequisites
#    - Rust (https://rustup.rs) with the x86_64-pc-windows-msvc toolchain
#    - Node.js (https://nodejs.org)
#    - Microsoft Visual Studio Build Tools 2022 with C++ workload + Windows SDK
npm install -g @tauri-apps/cli

# 2. Clone
git clone https://github.com/ratatabananana-bit/Spool.git
cd Spool

# 3. Pull the sidecar binaries (yt-dlp + ffmpeg + ffprobe ~190 MB)
powershell -ExecutionPolicy Bypass -File scripts\fetch-binaries.ps1

# 4. Dev build
tauri build --debug --no-bundle
# → src-tauri/target/debug/Spool.exe

# 5. Release build
tauri build --no-bundle
# → src-tauri/target/release/Spool.exe
```

After the release build, copy `Spool.exe` + the three sidecars (`yt-dlp.exe`, `ffmpeg.exe`, `ffprobe.exe`) into one folder. That folder is the portable distribution.

## Repository layout

```
src-tauri/        Rust backend + Tauri config
  src/            jobs, settings, updater, debug log, lib + bin
  icons/          generated icon set (source: source.png — Tray variant)
  capabilities/   Tauri permission scopes
  binaries/       sidecar binaries — gitignored, fetched by script
ui/               vanilla HTML/CSS/JS frontend (~5 KB JS gzipped goal)
  fonts/          bundled Geist + Geist Mono (SIL OFL 1.1)
  locales.js      en + zh-Hant dictionaries
scripts/          dev helpers
```

## Licenses

- App code: MIT (see `LICENSE`)
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — Unlicense / public domain
- [ffmpeg](https://ffmpeg.org) — LGPL/GPL (bundled essentials build from [gyan.dev](https://www.gyan.dev/ffmpeg/builds/))
- [Tauri](https://tauri.app) — Apache-2.0 / MIT
- [Geist](https://vercel.com/font/geist) — SIL Open Font License 1.1

## Credits

Built with Tauri 2, Rust, and a vanilla DOM frontend. UI design based on the bundled handoff (icon + brand by the design canvas; "Tray" icon variant, name "Spool").
