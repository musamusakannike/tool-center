# Tool Center

**A fast, lightweight, 100% offline-capable open-source desktop file preview, conversion & AI toolbox for Windows, macOS, and Linux.**

[![CI](https://github.com/musamusakannike/tool-center/actions/workflows/ci.yml/badge.svg)](https://github.com/musamusakannike/tool-center/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Tool Center combines universal instant file preview with **41 file manipulation, compression, conversion, and local AI tools** in a native desktop app — with **no Chromium/Electron/WebView**, **no accounts**, **no cloud uploads**, and **zero telemetry**.

---

## Screenshots

> _Screenshots placeholder — attach light & dark mode captures under `docs/screenshots/`._
>
> - `docs/screenshots/home-grid.png` — Home screen with Drop Zone, Category Pills (`AI`, `Images`, `PDF`, `Video`, `Audio`, `Files`, `Convert`), and Tool Cards
> - `docs/screenshots/universal-preview.png` — Split File Preview, Contextual AI Actions, and Metadata Inspector Panel
> - `docs/screenshots/ai-before-after.png` — Interactive Before / After Slider & Checkerboard Cutout Preview
> - `docs/screenshots/ocr-searchable-pdf.png` — Highlighted Click-to-Copy OCR Regions & Searchable PDF Export
> - `docs/screenshots/models-manager.png` — On-Demand AI Model Manager with Sizes, Resumable Downloads & Offline Import

---

## Highlights & Guarantees

- **Native & Lightweight:** Built with **Flutter Desktop** (GPU-rendered UI) + **Rust** (`flutter_rust_bridge` v2) for all CPU/GPU/I/O work. No Electron, CEF, or WebView2.
- **On-Demand Local AI (v2):** OCR, Background Removal, and 2×/4× Super-Resolution run 100% locally via **ONNX Runtime** (`ort`) with optional hardware acceleration (**CoreML** on macOS, **DirectML** on Windows, **CUDA** on Windows/Linux) and automatic CPU fallback.
- **Tiny Installer — Zero Bundled Model Bloat:** Neural network weights are **never bundled** in the installer; download only the models you want (with exact byte sizes shown upfront), or import `.onnx` files manually on air-gapped offline machines. Once installed, AI tools work completely offline.
- **UI Never Blocks & Idle RAM Stays Low:** Every tool runs on a bounded background worker pool (`core-jobs`) with real-time progress and instant cancellation; ONNX sessions load lazily and automatically evict when idle.
- **Originals Are Never Overwritten:** Every tool writes outputs next to the source file (or into a user-chosen folder) with a clear, collision-free suffix (`_nobg`, `_2x`, `_searchable`, `_resized`, `_merged`, etc.).

---

## Feature Matrix (v1 + v2)

### Universal File Preview + Contextual AI Quick Actions
Drop any file onto the window, press `Ctrl+O` / `Cmd+O`, or use your OS **"Open with"**:
- **Images:** JPG, PNG, WebP, GIF, BMP, TIFF, AVIF, HEIC (via bundled LGPL decoder), SVG — plus one-click **Run OCR**, **Remove Background**, and **Upscale** actions in the sidebar
- **PDF:** Multi-page crisp vector rendering (`pdfium-render`), zoom, page navigation, inline password unlock, and one-click **Run OCR** → Searchable PDF
- **Video & Audio:** MP4, MKV, MOV, WebM, AVI, MP3, WAV, FLAC, AAC/M4A, OGG (`media_kit` / `libmpv` playback + codec/bitrate metadata)
- **Text & Data:** Plain text, Markdown (rendered + source), syntax-highlighted code, CSV/TSV (`DataTable` + raw), JSON (collapsible tree + formatted)
- **Archives:** Browse `.zip`, `.tar.gz`, `.7z`, `.tar.zst`, and `.tar.xz` contents without extracting
- **Office (`DOCX`, `XLSX`, `PPTX`):** Clear _"Preview coming soon"_ card with one-click **Open in System App** fallback

### 41 Built-In Tools Across 7 Categories
| Category | Included Tools |
| :--- | :--- |
| **AI** (`core-ai`) | **OCR Text & Searchable PDF** (`PP-OCRv4` English/Latin & Multilingual packs, streaming PDF pages, `.txt` / `.md` / **Searchable PDF** with `3 Tr` invisible text layer, interactive click-to-copy region overlay, contrast/deskew preprocessing), **Remove Background** (`U²-Netp` 4.4 MB fast default, `U²-Net`, `BiRefNet-Tiny`; full-resolution joint bilateral edge refinement; transparent PNG, solid color, portrait blur, or custom backdrop + Before/After slider), **Upscale Image** (`Real-ESRGAN` 2×/4× photo & anime models with RAM-aware tiled inference and raised-cosine feathered seam blending + instant zero-download `Lanczos3` fallback) |
| **Images** (`core-image`) | Resize (`fast_image_resize` SIMD), Crop, Rotate & Flip, Convert (PNG/JPG/WebP/AVIF/TIFF/BMP/GIF + HEIC input), Compress (`oxipng` + `mozjpeg-rs`), Watermark (Text + Logo Image, 9-point grid, opacity), Strip EXIF (lossless container strip), Batch Process |
| **PDF** (`core-pdf`) | Merge, Split, Extract Pages, Reorder Pages, Rotate Pages, Compress, PDF → Images (`pdfium-render`), Images → PDF, Watermark PDF, Protect PDF (AES-256), Unlock PDF |
| **Video & Audio** (`core-media`) | Cut / Trim (lossless stream copy when possible), Merge Clips, Compress Video, Convert Video, Extract Audio, Convert Audio, Extract Video Frames, Make Animated GIF |
| **Files & Archives** (`core-archive`, `core-convert`) | Create Archive (`zip`, `tar.gz`, `7z`, `tar.zst`, `tar.xz`), Extract Archive (Zip-Slip protected), Inspect Archive Contents, Bulk Rename (with Dry-Run preview), File Hash / Checksum (SHA-256, BLAKE3, SHA-512, SHA-1, MD5), Duplicate Finder |
| **Convert** (`core-convert`) | Image ⇄ Image, Audio ⇄ Audio, Video ⇄ Video, PDF → Images, Images → PDF |

> **Permissive Model License Gate:** Every model in [`models.json`](models.json) is verified directly from its upstream repository to carry `Apache-2.0`, `MIT`, or `BSD-3-Clause` licensing. Non-commercial (`CC-BY-NC*`, `BRIA RMBG-1.4/2.0`) and ambiguous-weight (`IS-Net/DIS`) models are strictly excluded — see [`docs/LICENSES.md`](docs/LICENSES.md).

---

## Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Ctrl+K` / `Cmd+K` | Open Command Palette (search any tool, AI action, or format) |
| `Ctrl+O` / `Cmd+O` | Open file for instant Preview & Metadata inspection |
| `Ctrl+J` / `Cmd+J` | Toggle Background Jobs drawer |
| `Esc` | Close dialog or return to Home grid |

---

## Building From Source

### Prerequisites
1. **Rust** (stable toolchain, `1.88+`): <https://rustup.rs>
2. **Flutter** (stable channel, desktop support enabled)
3. **Python 3.9+** (used by `scripts/fetch_vendor.py` to fetch & verify SHA-256 for LGPL FFmpeg, PDFium, and MIT ONNX Runtime)

### Linux (Ubuntu / Debian)
```bash
sudo apt-get update
sudo apt-get install -y clang cmake ninja-build pkg-config libgtk-3-dev liblzma-dev libmpv-dev mpv

# 1. Fetch & verify pinned PDFium + LGPL FFmpeg + MIT ONNX Runtime binaries
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

# 1. Fetch & verify universal (arm64 + x86_64) PDFium + LGPL FFmpeg + MIT ONNX Runtime binaries
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
# 1. Fetch & verify pinned PDFium + LGPL FFmpeg + MIT ONNX Runtime (DirectML) binaries
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

## Automated Releases & CI Workflows

- **Pull Request & Push CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)):** Runs `cargo fmt`, `cargo clippy -D warnings`, Rust unit tests (including synthetic ~200-byte ONNX Runtime fixtures — zero network downloads in PR CI), `flutter analyze --fatal-infos`, `flutter test`, and release desktop builds across Ubuntu, macOS, and Windows.
- **Real-Model Smoke Workflow ([`.github/workflows/ai-models-nightly.yml`](.github/workflows/ai-models-nightly.yml)):** Separate manual (`workflow_dispatch`) & weekly scheduled workflow that downloads real permissive weights from `models.json`, verifies SHA-256 digests, and runs end-to-end ONNX inference.
- **Release Packaging ([`.github/workflows/release.yml`](.github/workflows/release.yml)):** Pushing any `v*` tag builds Windows (`*-setup.exe` + Portable `.zip`), macOS Universal (`.dmg`), and Linux (`AppImage` + `.deb`) artifacts with `SHA256SUMS.txt` attached to a draft GitHub Release.

---

## Documentation
- [Architecture Overview](docs/ARCHITECTURE.md)
- [Adding a New AI Model](docs/ADDING_A_MODEL.md)
- [v2 Architectural & License Audit](docs/V2_AUDIT.md)
- [Contributing & How to Add a New Tool](CONTRIBUTING.md)
- [Third-Party Licenses, ONNX Runtime & Verified AI Model Inventory](docs/LICENSES.md)

## License

Dual-licensed under either of:
- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))

at your option.
