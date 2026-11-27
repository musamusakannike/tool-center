# Tool Center

**A fast, lightweight, 100% offline open-source desktop file preview & conversion toolbox for Windows, macOS, and Linux.**

[![CI](https://github.com/musamusakannike/tool-center/actions/workflows/ci.yml/badge.svg)](https://github.com/musamusakannike/tool-center/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Tool Center combines universal instant file preview with **35+ file manipulation, compression, and conversion tools** in a native desktop app — with **no Chromium/Electron/WebView**, **no accounts**, **no cloud uploads**, and **zero telemetry**.

---

## Screenshots

> _Screenshots placeholder — attach light & dark mode captures under `docs/screenshots/`._
>
> - `docs/screenshots/home-grid.png` — Home screen with Drop Zone, Category Pills, and Tool Cards
> - `docs/screenshots/universal-preview.png` — Split File Preview & Metadata Inspector Panel
> - `docs/screenshots/tool-workspace.png` — 3-Step Schema-Driven Tool Flow with Live Progress

---

## Highlights & Guarantees

- **Native & Lightweight:** Built with **Flutter Desktop** (GPU-rendered UI) + **Rust** (`flutter_rust_bridge` v2) for all CPU/I/O work. No Electron, CEF, or WebView2.
- **UI Never Blocks:** Every tool runs on a bounded background worker pool (`core-jobs`) with real-time progress streaming and instant cancellation (including spawned sidecar processes).
- **Streaming I/O:** Large multi-gigabyte archives, checksums, duplicate scans, and text/CSV/JSON previews stream in bounded chunks instead of loading entire files into RAM.
- **Originals Are Never Overwritten:** Every tool writes outputs next to the source file (or into a user-chosen folder) with a clear, collision-free suffix (`_resized`, `_merged`, `_compressed`, etc.).
- **Works 100% Offline:** Zero network calls in v1; no telemetry, analytics, or sign-in.

---

## v1 Feature Matrix

### Universal File Preview + Metadata Inspector
Drop any file onto the window, press `Ctrl+O` / `Cmd+O`, or use your OS **"Open with"**:
- **Images:** JPG, PNG, WebP, GIF, BMP, TIFF, AVIF, HEIC (via bundled LGPL decoder), SVG (interactive pan & zoom + EXIF inspector)
- **Video & Audio:** MP4, MKV, MOV, WebM, AVI, MP3, WAV, FLAC, AAC/M4A, OGG (`media_kit` / `libmpv` playback + codec/bitrate metadata)
- **PDF:** Multi-page crisp vector rendering (`pdfium-render`), zoom, page navigation, and inline password unlock
- **Text & Data:** Plain text, Markdown (rendered + source), syntax-highlighted code, CSV/TSV (`DataTable` + raw), JSON (collapsible tree + formatted)
- **Archives:** Browse `.zip`, `.tar.gz`, `.7z`, `.tar.zst`, and `.tar.xz` contents without extracting
- **Office (`DOCX`, `XLSX`, `PPTX`):** Clear _"Preview coming soon"_ card with one-click **Open in System App** fallback

### 38 Built-In Tools Across 6 Categories
| Category | Included Tools |
| :--- | :--- |
| **Images** (`core-image`) | Resize (`fast_image_resize` SIMD), Crop, Rotate & Flip, Convert (PNG/JPG/WebP/AVIF/TIFF/BMP/GIF + HEIC input), Compress (`oxipng` + `mozjpeg-rs`), Watermark (Text + Logo Image, 9-point grid, opacity), Strip EXIF (lossless container strip), Batch Process |
| **PDF** (`core-pdf`) | Merge, Split, Extract Pages, Reorder Pages, Rotate Pages, Compress, PDF → Images (`pdfium-render`), Images → PDF, Watermark PDF, Protect PDF (AES-256), Unlock PDF |
| **Video & Audio** (`core-media`) | Cut / Trim (lossless stream copy when possible), Merge Clips, Compress Video, Convert Video, Extract Audio, Convert Audio, Extract Video Frames, Make Animated GIF |
| **Files & Archives** (`core-archive`, `core-convert`) | Create Archive (`zip`, `tar.gz`, `7z`, `tar.zst`, `tar.xz`), Extract Archive (Zip-Slip protected), Inspect Archive Contents, Bulk Rename (with Dry-Run preview), File Hash / Checksum (SHA-256, BLAKE3, SHA-512, SHA-1, MD5), Duplicate Finder |
| **Convert** (`core-convert`) | Image ⇄ Image, Audio ⇄ Audio, Video ⇄ Video, PDF → Images, Images → PDF |

> **Note on HEIC:** Tool Center decodes `.heic`/`.heif` files on all platforms for preview and conversion into PNG, JPG, WebP, AVIF, TIFF, or PDF. Cross-platform `.heic` encoding is intentionally omitted because open-source HEVC encoding (`x265`) is GPL-licensed; AVIF and WebP encoding are provided under permissive licenses.

---

## Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Ctrl+K` / `Cmd+K` | Open Command Palette (search any tool or format) |
| `Ctrl+O` / `Cmd+O` | Open file for instant Preview & Metadata inspection |
| `Ctrl+J` / `Cmd+J` | Toggle Background Jobs drawer |
| `Esc` | Close dialog or return to Home grid |

---

## Building From Source

### Prerequisites
1. **Rust** (stable toolchain, `1.88+`): <https://rustup.rs>
2. **Flutter** (stable channel, desktop support enabled)
3. **Python 3.9+** (used by `scripts/fetch_vendor.py` to fetch & verify SHA-256 + LGPL flags for FFmpeg and PDFium)

### Linux (Ubuntu / Debian)
```bash
sudo apt-get update
sudo apt-get install -y clang cmake ninja-build pkg-config libgtk-3-dev liblzma-dev libmpv-dev mpv

# 1. Fetch & verify pinned PDFium + LGPL FFmpeg binaries
python3 scripts/fetch_vendor.py --target linux-x64

# 2. Build & test Rust core
cargo test --manifest-path core/Cargo.toml --workspace
cargo build --manifest-path core/Cargo.toml -p bridge --release

# 3. Build Flutter desktop app
cd app
flutter pub get
flutter gen-l10n
flutter build linux --release
```

### macOS (Apple Silicon & Intel)
```bash
xcode-select --install

# 1. Fetch & verify universal (arm64 + x86_64) PDFium + LGPL FFmpeg binaries
python3 scripts/fetch_vendor.py --target macos-universal

# 2. Build Rust core
cargo build --manifest-path core/Cargo.toml -p bridge --release

# 3. Build Flutter macOS app
cd app
flutter pub get
flutter gen-l10n
flutter build macos --release
```

### Windows (10 / 11 x64)
Install Visual Studio 2022 ("Desktop development with C++" workload):
```powershell
# 1. Fetch & verify pinned PDFium + LGPL FFmpeg binaries
python scripts\fetch_vendor.py --target windows-x64

# 2. Build Rust core
cargo build --manifest-path core\Cargo.toml -p bridge --release

# 3. Build Flutter Windows app
cd app
flutter pub get
flutter gen-l10n
flutter build windows --release
```

---

## Automated Releases & Optional Code Signing

Pushing any `v*` tag (e.g. `git tag v0.1.0 && git push origin v0.1.0`) triggers [`.github/workflows/release.yml`](.github/workflows/release.yml), building:
- **Windows:** Inno Setup installer (`*-setup.exe`) + Portable `.zip`
- **macOS:** Universal (`arm64` + `x86_64`) `.dmg`
- **Linux:** `AppImage` + `.deb`
- **`SHA256SUMS.txt`** attached to a **draft GitHub Release**

Unsigned builds work out of the box. To enable automatic code signing & Apple notarization, add these optional repository secrets in GitHub Settings → Secrets and variables → Actions:
- **Windows Authenticode:** `WINDOWS_CERT_PFX_BASE64`, `WINDOWS_CERT_PASSWORD`
- **macOS Developer ID & Notarization:** `APPLE_CERTIFICATE_BASE64`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_SPECIFIC_PASSWORD`

---

## Documentation
- [Architecture Overview](docs/ARCHITECTURE.md)
- [Contributing & How to Add a New Tool](CONTRIBUTING.md)
- [Third-Party Licenses & LGPL Verification](docs/LICENSES.md)

## License

Dual-licensed under either of:
- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))

at your option.
