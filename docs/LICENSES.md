# Licenses & Third-Party Component Inventory

## Tool Center License

Tool Center's own source code is dual-licensed under **MIT OR Apache-2.0**:
- [LICENSE-MIT](../LICENSE-MIT)
- [LICENSE-APACHE](../LICENSE-APACHE)

---

## Bundled Native Runtime Components

Tool Center bundles two standalone/dynamically-loaded open-source runtime engines during release packaging via [`scripts/fetch_vendor.py`](../scripts/fetch_vendor.py) and links `libmpv` dynamically via `media_kit` for media preview playback. **No GPL or non-free components are bundled.**

### 1. FFmpeg (Bundled Sidecar Binary — LGPL)

Tool Center invokes `ffmpeg` (and `ffprobe` where present) strictly as an **unmodified external sidecar process** over standard pipes (`-progress pipe:1`), and on platforms using shared libraries (`linux-*`, `windows-*`) ships the unmodified shared `.so` / `.dll` files in a dedicated `ffmpeg/lib` directory so users may inspect or replace them freely in accordance with the GNU Lesser General Public License (LGPL).

| Platform Target | Upstream Distribution | Pinned Release / Tag | Archive SHA-256 | License |
| :--- | :--- | :--- | :--- | :--- |
| `linux-x64` | [`FFmpeg-Builds/FFmpeg-Builds`](https://github.com/FFmpeg-Builds/FFmpeg-Builds) | `autobuild-2026-11-26-12-56` (`ffmpeg-n8.1.3-41-ga8029b6528-linux64-lgpl-shared-8.1.tar.xz`) | `a8b42082934142810b98d18b3d9c36b012fc55e75dfafff61f81a139af3a399d` | **LGPL-2.1-or-later / LGPL-3.0** (`lgpl-shared`, no `--enable-gpl`, no `--enable-nonfree`) |
| `linux-arm64` | [`FFmpeg-Builds/FFmpeg-Builds`](https://github.com/FFmpeg-Builds/FFmpeg-Builds) | `autobuild-2026-11-26-12-56` (`ffmpeg-n8.1.3-41-ga8029b6528-linuxarm64-lgpl-shared-8.1.tar.xz`) | `1e6de523bc0be231284c1c5a88da1c941080a52c02960121f8ecc8025e42eb8d` | **LGPL-2.1-or-later / LGPL-3.0** (`lgpl-shared`) |
| `windows-x64` | [`FFmpeg-Builds/FFmpeg-Builds`](https://github.com/FFmpeg-Builds/FFmpeg-Builds) | `autobuild-2026-11-26-12-56` (`ffmpeg-n8.1.3-41-ga8029b6528-win64-lgpl-shared-8.1.zip`) | `372016b1248544b4ffc290ae41c9c57e6441bd0da32068ea18032a25b7b36e08` | **LGPL-2.1-or-later / LGPL-3.0** (`lgpl-shared`) |
| `windows-arm64` | [`FFmpeg-Builds/FFmpeg-Builds`](https://github.com/FFmpeg-Builds/FFmpeg-Builds) | `autobuild-2026-11-26-12-56` (`ffmpeg-n8.1.3-41-ga8029b6528-winarm64-lgpl-shared-8.1.zip`) | `e89857ba29f13d2d422d814ddc2e46b2ab89c9e3a837530a9dd16b6c242b6b02` | **LGPL-2.1-or-later / LGPL-3.0** (`lgpl-shared`) |
| `macos-arm64` | [`mt-mograph/ffmpeg-lgpl`](https://github.com/mt-mograph/ffmpeg-lgpl) | `8.1.2` (`ffmpeg-darwin-arm64.gz`) | `0c6dd6d1d0197048be12263ad34afc7f99464ea18679abca7aa111a94107b3c4` | **LGPL-2.1-or-later** (`CONFIG_GPL=0`, `CONFIG_NONFREE=0`, `CONFIG_VERSION3=0`; unmodified FFmpeg 8.1.2 source tarball archived on the upstream release) |
| `macos-x64` | [`mt-mograph/ffmpeg-lgpl`](https://github.com/mt-mograph/ffmpeg-lgpl) | `8.1.2` (`ffmpeg-darwin-x64.gz`) | `6e03487ac953e64a39d9372fbfa883cbdf756be14dd88aeb85a37b6562e25196` | **LGPL-2.1-or-later** (`CONFIG_GPL=0`, `CONFIG_NONFREE=0`, `CONFIG_VERSION3=0`) |

#### Automated LGPL Verification
Every CI and release run executes `scripts/fetch_vendor.py`, which:
1. Verifies the downloaded archive's exact **SHA-256** digest against the pinned table above.
2. Runs `ffmpeg -L` on the extracted binary and fails the build immediately if `--enable-gpl` or `--enable-nonfree` is present or if the LGPL license notice is missing.

---

### 2. PDFium (Bundled Dynamic Library — Apache-2.0 / BSD-3-Clause)

Loaded dynamically at runtime via `pdfium-render` (`libloading`) for PDF page preview and PDF-to-image rasterization.

| Platform Target | Upstream Distribution | Pinned Tag | Archive SHA-256 | License |
| :--- | :--- | :--- | :--- | :--- |
| `linux-x64` | [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) | `chromium/8179` | `b4766a6bf2856efbcc6c3cc7d6ae9022bdc6bb36ddc1acdd92d1f1873f773f4b` | **Apache-2.0 / BSD-3-Clause** |
| `linux-arm64` | [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) | `chromium/8179` | `18bb6d568257b28d14f6c3b8bfb48caf73d4615b319ca4d43b424c2a03431d9f` | **Apache-2.0 / BSD-3-Clause** |
| `macos-arm64` | [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) | `chromium/8179` | `44b7c6f55dd33454d0909728bd0adf6c86a8e141b36f07a6b6f50e228e1dfb46` | **Apache-2.0 / BSD-3-Clause** |
| `macos-x64` | [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) | `chromium/8179` | `b29bbfe37efcad5b443fa3556c0485ec96fc1992d5b19d84df49d48fbed1d0d2` | **Apache-2.0 / BSD-3-Clause** |
| `windows-x64` | [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) | `chromium/8179` | `836a77c9053f624839c66010cddea7b675628a36b8831d2330178bb169d241b6` | **Apache-2.0 / BSD-3-Clause** |
| `windows-arm64` | [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) | `chromium/8179` | `9c659bf0cd9bdefcbf7ec0e4b1675c2961e0fdfd4ce097b411efd9742832fe06` | **Apache-2.0 / BSD-3-Clause** |

---

### 3. `media_kit` / `libmpv` (Dynamic Playback Engine)

- **Flutter Wrapper (`media_kit`, `media_kit_video`, `media_kit_libs_video`):** **MIT License** (<https://github.com/media-kit/media-kit>)
- **Underlying `libmpv` Shared Library:** **LGPL-2.1-or-later** (linked dynamically as `.dll` / `.dylib` / system `libmpv.so.2` on Linux).

---

## HEIC Licensing Note

Open-source cross-platform HEIC/HEVC encoding (`x265`) is licensed under **GPL-2.0-or-later**, which is incompatible with distributing a permissive (`MIT OR Apache-2.0`) + LGPL-clean bundle. Tool Center therefore:
- **Supports HEIC/HEIF decoding** (preview + conversion to PNG, JPG, WebP, AVIF, TIFF, BMP, GIF, and PDF) using FFmpeg's built-in LGPLv2.1 HEVC/HEIF decoder.
- **Omits HEIC encoding**, providing full read+write support for royalty-free permissive **AVIF** (`ravif`, BSD-2-Clause) and **WebP** instead.

---

## Rust Crate Licenses (`core/`)

All direct Rust dependencies use permissive open-source licenses:

| Crate | Purpose | SPDX License |
| :--- | :--- | :--- |
| `flutter_rust_bridge` | Dart ⇄ Rust FFI codegen & runtime | `MIT` |
| `tokio` | Background async/blocking task scheduling | `MIT` |
| `serde`, `serde_json` | Schema & event serialization | `MIT OR Apache-2.0` |
| `thiserror` | Structured error definitions | `MIT OR Apache-2.0` |
| `uuid` | Job IDs & CSPRNG bytes | `Apache-2.0 OR MIT` |
| `image` | Raster codecs (PNG, JPEG, WebP, AVIF, GIF, BMP, TIFF) | `MIT OR Apache-2.0` |
| `fast_image_resize` | SIMD (AVX2 / NEON) image resampling | `MIT OR Apache-2.0` |
| `oxipng` | Multi-threaded lossless PNG compression | `MIT` |
| `mozjpeg-rs` | Pure-Rust MozJPEG trellis-quantized JPEG encoder | `BSD-3-Clause` |
| `img-parts` | Lossless container-level EXIF removal | `MIT OR Apache-2.0` |
| `kamadak-exif` | Pure-Rust EXIF metadata reader | `BSD-2-Clause` |
| `pdfium-render` | Safe Rust bindings for dynamic PDFium | `MIT OR Apache-2.0` |
| `lopdf` | PDF merge, split, reorder, rotate, compress, AES-256 | `MIT` |
| `csv` | Streaming CSV/TSV parser | `Unlicense OR MIT` |
| `zip` | Streaming ZIP compression & extraction | `MIT` |
| `tar` | Streaming TAR reader & builder | `MIT OR Apache-2.0` |
| `flate2` | Streaming Gzip (`tar.gz`) | `MIT OR Apache-2.0` |
| `zstd` | Streaming Zstandard (`tar.zst`, `zst`) | `BSD-3-Clause` |
| `lzma-rust2` | Pure-Rust LZMA2 / XZ (`tar.xz`, `xz`) | `Apache-2.0` |
| `sevenz-rust2` | Pure-Rust 7z creation, listing & extraction | `Apache-2.0` |
| `blake3` | High-speed cryptographic hashing & dedupe | `CC0-1.0 OR Apache-2.0` |
| `sha2`, `sha1`, `md-5` | Standard file checksum digests | `MIT OR Apache-2.0` |
| `walkdir` | Directory traversal | `Unlicense OR MIT` |
| `chrono` | Timestamp formatting | `MIT OR Apache-2.0` |

---

## Flutter / Dart Package Licenses (`app/`)

| Package | Purpose | SPDX License |
| :--- | :--- | :--- |
| `flutter`, `material_ui`, `intl` | UI framework & localization | `BSD-3-Clause` |
| `flutter_rust_bridge` | FFI bridge runtime | `MIT` |
| `file_picker` | Native OS file/folder dialogs | `MIT` |
| `desktop_drop` | Native OS drag-and-drop | `MIT` |
| `flutter_svg` | SVG vector preview | `MIT` |
| `markdown` | Markdown AST parser | `BSD-3-Clause` |
| `highlight` | Code syntax highlighter | `MIT` |
| `media_kit`, `media_kit_video`, `media_kit_libs_video` | Desktop video/audio playback (`libmpv`) | `MIT` (wrapper) / `LGPL-2.1+` (`libmpv`) |
| `url_launcher`, `path_provider`, `shared_preferences`, `path` | Flutter desktop platform utilities | `BSD-3-Clause` |

---

## v2 On-Demand AI Models Policy (`core-ai`)

- **Permitted Model Licenses:** `MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC` (enforced in code by `core_ai::validate_permissive_model_license`).
- **Curated v2 Catalog (Downloaded On Demand, Not Bundled):**
  - `PP-OCRv4` (`Apache-2.0`, PaddleOCR) — OCR & searchable PDF
  - `U-2-Netp` (`Apache-2.0`, xuebinqin/U-2-Net) — Background removal
  - `Swin2SR` (`Apache-2.0`, mv-lab/swin2sr) — 2x image super-resolution
- **Explicitly Flagged & Excluded Models (Never Used):**
  - `BRIA RMBG-1.4` — Non-commercial license; flagged and excluded.
  - Any `CC-BY-NC-4.0`, `CreativeML-OpenRAIL` non-commercial, `GPL`, or `AGPL` checkpoint — flagged and rejected by `ModelManager`.
