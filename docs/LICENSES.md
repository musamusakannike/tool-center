# Licenses & Third-Party Component Inventory

## Tool Center License

Tool Center's own source code is dual-licensed under **MIT OR Apache-2.0**:
- [LICENSE-MIT](../LICENSE-MIT)
- [LICENSE-APACHE](../LICENSE-APACHE)

---

## Bundled Native Runtime Components

Tool Center bundles three standalone/dynamically-loaded open-source runtime engines during release packaging via [`scripts/fetch_vendor.py`](../scripts/fetch_vendor.py) and links `libmpv` dynamically via `media_kit` for media preview playback. **No GPL or non-free components are bundled, and AI model weights are never bundled in the installer.**

### 1. FFmpeg (Bundled Sidecar Binary — LGPL)

Tool Center invokes `ffmpeg` (and `ffprobe` where present) strictly as an **unmodified external sidecar process** over standard pipes (`-progress pipe:1`), and on platforms using shared libraries (`linux-*`, `windows-*`) ships the unmodified shared `.so` / `.dll` files in a dedicated `ffmpeg/lib` directory so users may inspect or replace them freely in accordance with the GNU Lesser General Public License (LGPL).

| Platform Target | Upstream Distribution | Pinned Release / Tag | Archive SHA-256 | License |
| :--- | :--- | :--- | :--- | :--- |
| `linux-x64` | [`FFmpeg-Builds/FFmpeg-Builds`](https://github.com/FFmpeg-Builds/FFmpeg-Builds) | `autobuild-2026-11-26-12-56` (`ffmpeg-n8.1.3-41-ga8029b6528-linux64-lgpl-shared-8.1.tar.xz`) | `a8b42082934142810b98d18b3d9c36b012fc55e75dfafff61f81a139af3a399d` | **LGPL-2.1-or-later / LGPL-3.0** (`lgpl-shared`, no `--enable-gpl`, no `--enable-nonfree`) |
| `linux-arm64` | [`FFmpeg-Builds/FFmpeg-Builds`](https://github.com/FFmpeg-Builds/FFmpeg-Builds) | `autobuild-2026-11-26-12-56` (`ffmpeg-n8.1.3-41-ga8029b6528-linuxarm64-lgpl-shared-8.1.tar.xz`) | `1e6de523bc0be231284c1c5a88da1c941080a52c02960121f8ecc8025e42eb8d` | **LGPL-2.1-or-later / LGPL-3.0** (`lgpl-shared`) |
| `windows-x64` | [`FFmpeg-Builds/FFmpeg-Builds`](https://github.com/FFmpeg-Builds/FFmpeg-Builds) | `autobuild-2026-11-26-12-56` (`ffmpeg-n8.1.3-41-ga8029b6528-win64-lgpl-shared-8.1.zip`) | `372016b1248544b4ffc290ae41c9c57e6441bd0da32068ea18032a25b7b36e08` | **LGPL-2.1-or-later / LGPL-3.0** (`lgpl-shared`) |
| `windows-arm64` | [`FFmpeg-Builds/FFmpeg-Builds`](https://github.com/FFmpeg-Builds/FFmpeg-Builds) | `autobuild-2026-11-26-12-56` (`ffmpeg-n8.1.3-41-ga8029b6528-winarm64-lgpl-shared-8.1.zip`) | `e89857ba29f13d2d422d814ddc2e46b2ab89c9e3a837530a9dd16b6c242b6b02` | **LGPL-2.1-or-later / LGPL-3.0** (`lgpl-shared`) |
| `macos-arm64` | [`genesis-webagentur-org/ffmpeg-lgpl-macos`](https://github.com/genesis-webagentur-org/ffmpeg-lgpl-macos) | `9.0.2-1` (`ffmpeg-darwin-arm64.gz`) | `c9ff6614a678ef0540d63c9d0ff252c44736c90ce9473d45df232c88ebb3927d` | **LGPL-2.1-or-later** (`--enable-libmp3lame --enable-videotoolbox --enable-audiotoolbox`, no `--enable-gpl`/`--enable-nonfree`) |
| `macos-x64` | [`genesis-webagentur-org/ffmpeg-lgpl-macos`](https://github.com/genesis-webagentur-org/ffmpeg-lgpl-macos) | `9.0.2-1` (`ffmpeg-darwin-x86_64.gz`) | `f72ebc3b2488fcb8d77a9dad3f0eac824b80fb84b830b4e07f57c701ab7114dc` | **LGPL-2.1-or-later** (`--enable-libmp3lame --enable-videotoolbox --enable-audiotoolbox`, no `--enable-gpl`/`--enable-nonfree`) |

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

### 3. ONNX Runtime (Bundled Dynamic Library — MIT)

Loaded lazily at runtime via `ort` (`load-dynamic`, `with_telemetry(false)`) only when an AI job executes. Upstream license (`LICENSE`) and `ThirdPartyNotices.txt` are extracted alongside the C shared library in `vendor/<target>/onnxruntime/`.

| Platform Target | Upstream Distribution | Pinned Version | Wheel SHA-256 | License |
| :--- | :--- | :--- | :--- | :--- |
| `linux-x64` | [`microsoft/onnxruntime`](https://github.com/microsoft/onnxruntime) | `1.23.2` (`manylinux_2_27_x86_64`) | `0f9b4ae77f8e3c9bee50c27bc1beede83f786fe1d52e99ac85aa8d65a01e9b77` | **MIT** |
| `linux-arm64` | [`microsoft/onnxruntime`](https://github.com/microsoft/onnxruntime) | `1.23.2` (`manylinux_2_27_aarch64`) | `76ff670550dc23e58ea9bc53b5149b99a44e63b34b524f7b8547469aaa0dcb8c` | **MIT** |
| `macos-arm64` | [`microsoft/onnxruntime`](https://github.com/microsoft/onnxruntime) | `1.23.2` (`macosx_13_0_arm64`, CoreML EP enabled) | `b8f029a6b98d3cf5be564d52802bb50a8489ab73409fa9db0bf583eabb7c2321` | **MIT** |
| `macos-x64` | [`microsoft/onnxruntime`](https://github.com/microsoft/onnxruntime) | `1.23.2` (`macosx_13_0_x86_64`) | `218295a8acae83905f6f1aed8cacb8e3eb3bd7513a13fe4ba3b2664a19fc4a6b` | **MIT** |
| `windows-x64` | [`microsoft/onnxruntime`](https://github.com/microsoft/onnxruntime) | `1.23.0` (`onnxruntime-directml`, DirectML + CPU EPs) | `e621e3c0a251f83da338652508d9d68bfb8a1ad6123fc50fed4da9a05b4950bf` | **MIT** |
| `windows-arm64` | [`microsoft/onnxruntime`](https://github.com/microsoft/onnxruntime) | `1.24.2` (`win_arm64`) | `8d770a934513f6e17937baf3438eaaec5983a23cdaedb81c9fc0dfcf26831c24` | **MIT** |

---

### 4. `media_kit` / `libmpv` (Dynamic Playback Engine)

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
| `ort` | Safe Rust bindings for dynamic ONNX Runtime (`load-dynamic`) | `MIT OR Apache-2.0` |
| `ureq` | Pure-Rust TLS (`rustls`) HTTPS Range downloader for on-demand AI models | `MIT OR Apache-2.0` |
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
| `lopdf` | PDF merge, split, reorder, rotate, compress, AES-256, Searchable PDF | `MIT` |
| `csv` | Streaming CSV/TSV parser | `Unlicense OR MIT` |
| `zip` | Streaming ZIP compression & extraction | `MIT` |
| `tar` | Streaming TAR reader & builder | `MIT OR Apache-2.0` |
| `flate2` | Streaming Gzip (`tar.gz`) | `MIT OR Apache-2.0` |
| `zstd` | Streaming Zstandard (`tar.zst`, `zst`) | `BSD-3-Clause` |
| `lzma-rust2` | Pure-Rust LZMA2 / XZ (`tar.xz`, `xz`) | `Apache-2.0` |
| `sevenz-rust2` | Pure-Rust 7z creation, listing & extraction | `Apache-2.0` |
| `blake3` | High-speed cryptographic hashing & dedupe | `CC0-1.0 OR Apache-2.0` |
| `sha2`, `sha1`, `md-5` | Standard file & model checksum digests | `MIT OR Apache-2.0` |
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

## v2 On-Demand AI Models (`models.json` & `core-ai`)

AI model weights are **never bundled** in Tool Center installers; they are downloaded on demand by the user (or imported offline) and verified against pinned SHA-256 digests before atomic installation. Every model in [`models.json`](../models.json) has been verified directly from its official upstream repository to carry a permissive commercial-friendly license (`MIT`, `Apache-2.0`, or `BSD-3-Clause`), enforced in code by `core_ai::validate_permissive_model_license`.

### Verified Permissive Models Included in `models.json`

| Model ID | Display Name | Task | Size | Verified License | Official Upstream Repository & Attribution |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `bg.u2netp` | U²-Netp (Fast General & Portrait) | Background Removal | 4.36 MiB (`4,574,861 B`) | **Apache-2.0** | [`xuebinqin/U-2-Net`](https://github.com/xuebinqin/U-2-Net) (`LICENSE`: Apache-2.0); ONNX build hosted by [`danielgatis/rembg`](https://github.com/danielgatis/rembg) (MIT) |
| `bg.u2net` | U²-Net Full (High-Detail Matting) | Background Removal | 167.84 MiB (`175,997,641 B`) | **Apache-2.0** | [`xuebinqin/U-2-Net`](https://github.com/xuebinqin/U-2-Net) (`LICENSE`: Apache-2.0); ONNX build hosted by [`danielgatis/rembg`](https://github.com/danielgatis/rembg) (MIT) |
| `bg.birefnet_tiny` | BiRefNet Swin-Tiny (High-Precision Edges) | Background Removal | 213.63 MiB (`224,005,088 B`) | **MIT** | [`ZhengPeng7/BiRefNet`](https://github.com/ZhengPeng7/BiRefNet) (`LICENSE`: MIT, Peng Zheng et al.) |
| `upscale.realesrgan_x2plus` | Real-ESRGAN x2plus (2x General Photo) | Upscale (2x) | 63.97 MiB (`67,073,434 B`) | **BSD-3-Clause** | [`xinntao/Real-ESRGAN`](https://github.com/xinntao/Real-ESRGAN) (`LICENSE`: BSD-3-Clause, Xintao Wang et al.) |
| `upscale.realesrgan_x4plus` | Real-ESRGAN x4plus (4x General Photo) | Upscale (4x) | 63.95 MiB (`67,051,640 B`) | **BSD-3-Clause** | [`xinntao/Real-ESRGAN`](https://github.com/xinntao/Real-ESRGAN) (`LICENSE`: BSD-3-Clause, Xintao Wang et al.) |
| `upscale.realesrgan_x4plus_anime` | Real-ESRGAN x4plus Anime 6B (4x Anime & Line Art) | Upscale (4x) | 17.11 MiB (`17,939,965 B`) | **BSD-3-Clause** | [`xinntao/Real-ESRGAN`](https://github.com/xinntao/Real-ESRGAN) (`LICENSE`: BSD-3-Clause, Xintao Wang et al.) |
| `ocr.ppocr_v4_en` | PP-OCRv4 English & Latin Pack (`det` + `cls` + `rec` + `en_dict.txt`) | OCR | 10.17 MiB (`10,665,748 B`) | **Apache-2.0** | [`PaddlePaddle/PaddleOCR`](https://github.com/PaddlePaddle/PaddleOCR) & [`RapidAI/RapidOCR`](https://github.com/RapidAI/RapidOCR) (`LICENSE`: Apache-2.0) |
| `ocr.ppocr_v4_ch_en` | PP-OCRv4 Multilingual (Chinese + English) Pack | OCR | 15.46 MiB (`16,215,257 B`) | **Apache-2.0** | [`PaddlePaddle/PaddleOCR`](https://github.com/PaddlePaddle/PaddleOCR) & [`RapidAI/RapidOCR`](https://github.com/RapidAI/RapidOCR) (`LICENSE`: Apache-2.0) |

### Explicitly Flagged & Excluded Models (Never Used)

- **`BRIA RMBG-1.4` & `BRIA RMBG-2.0`** (`briaai/RMBG-1.4`, `briaai/RMBG-2.0`): Licensed under BRIA AI Non-Commercial / Creative Commons Attribution-NonCommercial (`CC-BY-NC-4.0`) — **excluded**.
- **`IS-Net` (`xuebinqin/DIS`)**: While the `DIS` repository carries an `Apache-2.0` `LICENSE` file, Section 7 of its upstream `README.md` specifies Apache-2.0 for *"code and evaluation metric"* while linking `DIS5K-Dataset-Terms-of-Use.pdf` (academic/research dataset terms). Because commercial-use rights on pre-trained `isnet` weights are ambiguous, `IS-Net` is **excluded** and replaced by **BiRefNet Swin-Tiny** (`MIT`) and **U²-Net** (`Apache-2.0`).
- **Any `CC-BY-NC*`, `Research-Only`, `GPL`, or `AGPL` checkpoint**: Rejected automatically at load and import time by `core_ai::validate_permissive_model_license`.
