//! Minimal pure-Rust ONNX Protobuf (`onnx.proto3`, IR v8, Opset 13) fixture builders for fast,
//! 100% offline CI unit tests. Each generated model is ~200 bytes and runs through the real
//! ONNX Runtime (`ort`) C++ engine without downloading multi-megabyte weights in CI.

fn push_varint(buf: &mut Vec<u8>, mut val: u64) {
    loop {
        let mut byte = (val & 0x7f) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if val == 0 {
            break;
        }
    }
}

fn push_tag(buf: &mut Vec<u8>, field_number: u32, wire_type: u8) {
    push_varint(buf, ((field_number as u64) << 3) | (wire_type as u64));
}

fn push_bytes_field(buf: &mut Vec<u8>, field_number: u32, payload: &[u8]) {
    push_tag(buf, field_number, 2);
    push_varint(buf, payload.len() as u64);
    buf.extend_from_slice(payload);
}

fn push_str_field(buf: &mut Vec<u8>, field_number: u32, s: &str) {
    push_bytes_field(buf, field_number, s.as_bytes());
}

fn push_int64_field(buf: &mut Vec<u8>, field_number: u32, val: i64) {
    push_tag(buf, field_number, 0);
    push_varint(buf, val as u64);
}

#[derive(Clone, Copy)]
pub enum DimSpec<'a> {
    Fixed(i64),
    Param(&'a str),
}

fn encode_value_info_f32(name: &str, dims: &[DimSpec<'_>]) -> Vec<u8> {
    let mut shape_bytes = Vec::new();
    for d in dims {
        let mut dim_msg = Vec::new();
        match d {
            DimSpec::Fixed(v) => push_int64_field(&mut dim_msg, 1, *v),
            DimSpec::Param(p) => push_str_field(&mut dim_msg, 2, p),
        }
        push_bytes_field(&mut shape_bytes, 1, &dim_msg);
    }

    let mut tensor_type = Vec::new();
    // elem_type = 1 (FLOAT)
    push_int64_field(&mut tensor_type, 1, 1);
    push_bytes_field(&mut tensor_type, 2, &shape_bytes);

    let mut type_proto = Vec::new();
    push_bytes_field(&mut type_proto, 1, &tensor_type);

    let mut vi = Vec::new();
    push_str_field(&mut vi, 1, name);
    push_bytes_field(&mut vi, 2, &type_proto);
    vi
}

fn encode_tensor_f32(name: &str, dims: &[i64], values: &[f32]) -> Vec<u8> {
    let mut tp = Vec::new();
    for &d in dims {
        push_int64_field(&mut tp, 1, d);
    }
    // data_type = 1 (FLOAT)
    push_int64_field(&mut tp, 2, 1);
    // raw_data (field 9) is little-endian IEEE754 bytes
    let mut raw = Vec::with_capacity(values.len() * 4);
    for &v in values {
        raw.extend_from_slice(&v.to_le_bytes());
    }
    push_str_field(&mut tp, 8, name);
    push_bytes_field(&mut tp, 9, &raw);
    tp
}

fn encode_attr_ints(name: &str, ints: &[i64]) -> Vec<u8> {
    let mut attr = Vec::new();
    push_str_field(&mut attr, 1, name);
    for &v in ints {
        push_int64_field(&mut attr, 8, v); // field 8 = repeated int64 ints
    }
    // field 20 = type (7 = INTS)
    push_int64_field(&mut attr, 20, 7);
    attr
}

fn encode_attr_int(name: &str, val: i64) -> Vec<u8> {
    let mut attr = Vec::new();
    push_str_field(&mut attr, 1, name);
    push_int64_field(&mut attr, 3, val); // field 3 = int64 i
    push_int64_field(&mut attr, 20, 2); // field 20 = type (2 = INT)
    attr
}

fn encode_attr_str(name: &str, val: &str) -> Vec<u8> {
    let mut attr = Vec::new();
    push_str_field(&mut attr, 1, name);
    push_str_field(&mut attr, 4, val); // field 4 = bytes s
    push_int64_field(&mut attr, 20, 3); // field 20 = type (3 = STRING)
    attr
}

fn encode_node(
    op_type: &str,
    name: &str,
    inputs: &[&str],
    outputs: &[&str],
    attrs: &[Vec<u8>],
) -> Vec<u8> {
    let mut node = Vec::new();
    for inp in inputs {
        push_str_field(&mut node, 1, inp);
    }
    for out in outputs {
        push_str_field(&mut node, 2, out);
    }
    push_str_field(&mut node, 3, name);
    push_str_field(&mut node, 4, op_type);
    for a in attrs {
        push_bytes_field(&mut node, 5, a);
    }
    node
}

fn wrap_model_proto(
    graph_name: &str,
    nodes: &[Vec<u8>],
    initializers: &[Vec<u8>],
    inputs: &[Vec<u8>],
    outputs: &[Vec<u8>],
    opset_version: i64,
) -> Vec<u8> {
    let mut graph = Vec::new();
    for n in nodes {
        push_bytes_field(&mut graph, 1, n);
    }
    push_str_field(&mut graph, 2, graph_name);
    for init in initializers {
        push_bytes_field(&mut graph, 5, init);
    }
    for inp in inputs {
        push_bytes_field(&mut graph, 11, inp);
    }
    for out in outputs {
        push_bytes_field(&mut graph, 12, out);
    }

    let mut opset = Vec::new();
    push_str_field(&mut opset, 1, "");
    push_int64_field(&mut opset, 2, opset_version);

    let mut model = Vec::new();
    // ir_version = 8 (compatible with ONNX Runtime 1.12+)
    push_int64_field(&mut model, 1, 8);
    push_str_field(&mut model, 2, "tool-center-fixture");
    push_bytes_field(&mut model, 7, &graph);
    push_bytes_field(&mut model, 8, &opset);
    model
}

/// Builds a tiny valid `.onnx` model computing `Y = X * scale` on dynamic `[N, C, H, W]` `f32` tensors.
pub fn build_tiny_mul_onnx(scale: f32) -> Vec<u8> {
    let dims = [
        DimSpec::Param("N"),
        DimSpec::Param("C"),
        DimSpec::Param("H"),
        DimSpec::Param("W"),
    ];
    let inp = encode_value_info_f32("input", &dims);
    let out = encode_value_info_f32("output", &dims);
    let scale_init = encode_tensor_f32("scale", &[1], &[scale]);
    let node = encode_node("Mul", "mul_node", &["input", "scale"], &["output"], &[]);
    wrap_model_proto("tiny_mul", &[node], &[scale_init], &[inp], &[out], 13)
}

/// Builds a tiny valid `.onnx` salient-matting fixture (`[N, 3, H, W]` -> `[N, 1, H, W]` in `[0, 1]`)
/// via `ReduceMean(axes=[1], keepdims=1)` followed by `Sigmoid`.
pub fn build_tiny_matting_onnx() -> Vec<u8> {
    let in_dims = [
        DimSpec::Param("N"),
        DimSpec::Fixed(3),
        DimSpec::Param("H"),
        DimSpec::Param("W"),
    ];
    let out_dims = [
        DimSpec::Param("N"),
        DimSpec::Fixed(1),
        DimSpec::Param("H"),
        DimSpec::Param("W"),
    ];
    let inp = encode_value_info_f32("input", &in_dims);
    let out = encode_value_info_f32("mask", &out_dims);
    let mean_node = encode_node(
        "ReduceMean",
        "mean_rgb",
        &["input"],
        &["logit"],
        &[
            encode_attr_ints("axes", &[1]),
            encode_attr_int("keepdims", 1),
        ],
    );
    let sig_node = encode_node("Sigmoid", "sigmoid_mask", &["logit"], &["mask"], &[]);
    wrap_model_proto(
        "tiny_matting",
        &[mean_node, sig_node],
        &[],
        &[inp],
        &[out],
        13,
    )
}

/// Builds a tiny valid `.onnx` super-resolution fixture (`[N, 3, H, W]` -> `[N, 3, H*scale, W*scale]`)
/// via ONNX `Resize` (nearest-neighbor spatial upsampling).
pub fn build_tiny_upscale_onnx(scale: u32) -> Vec<u8> {
    let in_dims = [
        DimSpec::Param("N"),
        DimSpec::Fixed(3),
        DimSpec::Param("H"),
        DimSpec::Param("W"),
    ];
    let out_dims = [
        DimSpec::Param("N"),
        DimSpec::Fixed(3),
        DimSpec::Param("H_out"),
        DimSpec::Param("W_out"),
    ];
    let inp = encode_value_info_f32("input", &in_dims);
    let out = encode_value_info_f32("output", &out_dims);
    let roi_init = encode_tensor_f32("roi", &[0], &[]);
    let s = scale.max(1) as f32;
    let scales_init = encode_tensor_f32("scales", &[4], &[1.0, 1.0, s, s]);
    let resize_node = encode_node(
        "Resize",
        "upscale_resize",
        &["input", "roi", "scales"],
        &["output"],
        &[encode_attr_str("mode", "nearest")],
    );
    wrap_model_proto(
        "tiny_upscale",
        &[resize_node],
        &[roi_init, scales_init],
        &[inp],
        &[out],
        13,
    )
}

/// Builds a tiny valid `.onnx` DBNet text-detector fixture (`[N, 3, H, W]` -> `[N, 1, H, W]`)
/// where dark ink pixels (`normalized < 0`) map to high probability (`> 0.85`) and white background
/// pixels (`normalized > 0`) map to low probability (`< 0.15`).
pub fn build_tiny_ocr_det_onnx() -> Vec<u8> {
    let in_dims = [
        DimSpec::Param("N"),
        DimSpec::Fixed(3),
        DimSpec::Param("H"),
        DimSpec::Param("W"),
    ];
    let out_dims = [
        DimSpec::Param("N"),
        DimSpec::Fixed(1),
        DimSpec::Param("H"),
        DimSpec::Param("W"),
    ];
    let inp = encode_value_info_f32("x", &in_dims);
    let out = encode_value_info_f32("prob", &out_dims);
    let neg_init = encode_tensor_f32("neg_scale", &[1], &[-1.5]);
    let mean_node = encode_node(
        "ReduceMean",
        "det_mean",
        &["x"],
        &["mean_c"],
        &[
            encode_attr_ints("axes", &[1]),
            encode_attr_int("keepdims", 1),
        ],
    );
    let mul_node = encode_node(
        "Mul",
        "det_invert",
        &["mean_c", "neg_scale"],
        &["logit"],
        &[],
    );
    let sig_node = encode_node("Sigmoid", "det_sig", &["logit"], &["prob"], &[]);
    wrap_model_proto(
        "tiny_ocr_det",
        &[mean_node, mul_node, sig_node],
        &[neg_init],
        &[inp],
        &[out],
        13,
    )
}

/// Builds a tiny valid `.onnx` text angle classifier (`[N, 3, H, W]` -> `[1, 2]`) predicting `0°`.
pub fn build_tiny_ocr_cls_onnx() -> Vec<u8> {
    let in_dims = [
        DimSpec::Param("N"),
        DimSpec::Fixed(3),
        DimSpec::Param("H"),
        DimSpec::Param("W"),
    ];
    let out_dims = [DimSpec::Fixed(1), DimSpec::Fixed(2)];
    let inp = encode_value_info_f32("x", &in_dims);
    let out = encode_value_info_f32("cls_out", &out_dims);
    let zero_init = encode_tensor_f32("zero", &[1], &[0.0]);
    let bias_init = encode_tensor_f32("cls_probs", &[1, 2], &[0.98, 0.02]);
    let mean_node = encode_node(
        "ReduceMean",
        "cls_global_mean",
        &["x"],
        &["scalar"],
        &[encode_attr_int("keepdims", 0)],
    );
    let zero_node = encode_node("Mul", "cls_zero", &["scalar", "zero"], &["z"], &[]);
    let add_node = encode_node("Add", "cls_add", &["z", "cls_probs"], &["cls_out"], &[]);
    wrap_model_proto(
        "tiny_ocr_cls",
        &[mean_node, zero_node, add_node],
        &[zero_init, bias_init],
        &[inp],
        &[out],
        13,
    )
}

/// Builds a tiny valid `.onnx` CTC recognizer (`[N, 3, H, W]` -> `[1, 6, 5]`) emitting token
/// sequence `[1, 1, 0, 2, 4, 3]` -> `"OK 1"` when paired with `dict = ["O", "K", "1"]`
/// (testing repeat-collapsing `1, 1 -> O`, blank `0`, and trailing space index `4 == dict.len() + 1`).
pub fn build_tiny_ocr_rec_onnx() -> Vec<u8> {
    let in_dims = [
        DimSpec::Param("N"),
        DimSpec::Fixed(3),
        DimSpec::Param("H"),
        DimSpec::Param("W"),
    ];
    let out_dims = [DimSpec::Fixed(1), DimSpec::Fixed(6), DimSpec::Fixed(5)];
    let inp = encode_value_info_f32("x", &in_dims);
    let out = encode_value_info_f32("ctc_probs", &out_dims);
    let zero_init = encode_tensor_f32("zero", &[1], &[0.0]);
    // 6 timesteps × 5 classes (0=blank, 1='O', 2='K', 3='1', 4=' ')
    #[rustfmt::skip]
    let ctc_table: [f32; 30] = [
        0.01, 0.96, 0.01, 0.01, 0.01, // t=0 -> 1 ('O')
        0.01, 0.95, 0.02, 0.01, 0.01, // t=1 -> 1 ('O', collapsed!)
        0.96, 0.01, 0.01, 0.01, 0.01, // t=2 -> 0 (blank)
        0.01, 0.01, 0.96, 0.01, 0.01, // t=3 -> 2 ('K')
        0.01, 0.01, 0.01, 0.01, 0.96, // t=4 -> 4 (' ' space)
        0.01, 0.01, 0.01, 0.96, 0.01, // t=5 -> 3 ('1')
    ];
    let probs_init = encode_tensor_f32("probs_const", &[1, 6, 5], &ctc_table);
    let mean_node = encode_node(
        "ReduceMean",
        "rec_global_mean",
        &["x"],
        &["scalar"],
        &[encode_attr_int("keepdims", 0)],
    );
    let zero_node = encode_node("Mul", "rec_zero", &["scalar", "zero"], &["z"], &[]);
    let add_node = encode_node("Add", "rec_add", &["z", "probs_const"], &["ctc_probs"], &[]);
    wrap_model_proto(
        "tiny_ocr_rec",
        &[mean_node, zero_node, add_node],
        &[zero_init, probs_init],
        &[inp],
        &[out],
        13,
    )
}
