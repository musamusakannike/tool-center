# Adding a New AI Model to Tool Center

Tool Center's AI engine (`core-ai`) is driven by a versioned [`models.json`](../models.json) manifest at the repository root. Adding a new language pack for OCR, a new matting model, or a new upscaler requires **zero Flutter UI layout changes** as long as it follows the steps below.

---

## 1. Verify the License First (Hard Gate)

Tool Center is open source (`MIT OR Apache-2.0`). Every model in `models.json` **must** carry an unambiguous, commercial-friendly permissive license verified directly from its official upstream repository:

- **Allowed SPDX identifiers:** `MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`
- **Strictly Prohibited:** `CC-BY-NC-4.0` (or any `-NC` variant), `Research-Only` / academic-only weights, `CreativeML-OpenRAIL` non-commercial checkpoints, `GPL*`, `AGPL*`, or models whose code is Apache/MIT while pre-trained weights are subject to non-commercial dataset terms (e.g. `BRIA RMBG-1.4/2.0`, `IS-Net/DIS`).

> `core_ai::validate_permissive_model_license` enforces this check programmatically at manifest load time, download time, and manual offline import time. Any non-permissive license will fail `cargo test -p core-ai`.

---

## 2. Inspect Tensor Shapes & Compute Exact Size + SHA-256

Download the `.onnx` file(s) locally and compute exact byte sizes and lowercase 64-character SHA-256 digests:

```bash
stat -c "%s %n" model.onnx          # macOS: stat -f "%z %N" model.onnx
sha256sum model.onnx                # macOS: shasum -a 256 model.onnx
```

### Expected Tensor Conventions by Task

| Task (`task`) | Component Roles (`files[].role`) | Input Tensor (`f32`) | Output Tensor (`f32`) |
| :--- | :--- | :--- | :--- |
| `"background_removal"` | `"primary"` (`.onnx`) | `[1, 3, H, W]` (`320×320`, or `1024×1024` if `id` contains `birefnet`), ImageNet mean/std | `[1, 1, H, W]` probabilities in `[0, 1]` or raw logits (auto-sigmoid + edge-aware upsampling to full original resolution) |
| `"upscale"` | `"primary"` (`.onnx`), plus `"scale": 2` or `4` | `[1, 3, tile_h, tile_w]` RGB in `[0.0, 1.0]` (automatically padded to multiples of 4) | `[1, 3, tile_h * scale, tile_w * scale]` RGB in `[0.0, 1.0]` |
| `"ocr"` | `"det"` (`.onnx`), `"cls"` (`.onnx`, optional), `"rec"` (`.onnx`), `"dict"` (`.txt` UTF-8 one token per line) | PaddleOCR `PP-OCRv3/v4` conventions (`det` `[1, 3, H, W]`, `cls` `[1, 3, 48, 192]`, `rec` `[1, 3, 48, W]`) | `det` `[1, 1, H, W]`, `cls` `[1, 2]`, `rec` `[1, T, V]` CTC probabilities/logits |

---

## 3. Add the Entry to `models.json`

Add a new object to the `"models"` array in [`models.json`](../models.json):

```json
{
  "id": "ocr.ppocr_v4_latin_ext",
  "display_name": "PP-OCRv4 Extended Latin Pack",
  "task": "ocr",
  "language": "latin",
  "default_for_task": false,
  "url": "https://example.org/latin_rec.onnx",
  "size_bytes": 10485760,
  "sha256": "<64-char-lowercase-hex-sha256-of-primary-or-rec-file>",
  "license": "Apache-2.0",
  "source_url": "https://github.com/PaddlePaddle/PaddleOCR",
  "attribution": "PaddleOCR Contributors (Apache-2.0 License)",
  "description": "Extended European Latin character recognition pack.",
  "files": [
    {
      "role": "det",
      "filename": "en_PP-OCRv3_det_infer.onnx",
      "url": "https://...",
      "size_bytes": 2423224,
      "sha256": "f139598bc2af4e4b6fe98dec11574e30edfdd91fc94ac1425c18ace3bd5a866b"
    },
    {
      "role": "rec",
      "filename": "latin_PP-OCRv4_rec_infer.onnx",
      "url": "https://...",
      "size_bytes": 8062336,
      "sha256": "<64-char-lowercase-hex-sha256>"
    },
    {
      "role": "dict",
      "filename": "latin_dict.txt",
      "url": "https://...",
      "size_bytes": 200,
      "sha256": "<64-char-lowercase-hex-sha256>"
    }
  ]
}
```

> **Note:** For multi-file models (like OCR packs), set top-level `"size_bytes"` to the exact sum of `files[].size_bytes` so the **Models** screen and the inline `"Download Model (X MB)"` prompt display the exact total download size upfront.

---

## 4. Document in `docs/LICENSES.md` & Run Verification

1. Add a row to the **Verified Permissive Models Included in `models.json`** table in [`docs/LICENSES.md`](LICENSES.md) linking the upstream repository and its verified license.
2. Run the automated manifest & license audit tests:
   ```bash
   cargo test --manifest-path core/Cargo.toml -p core-ai
   ```
3. Launch Tool Center (or run the manual **AI Models Real-Weight Smoke Test** workflow) to verify downloading, SHA-256 verification, offline manual import, and inference.
