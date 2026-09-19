#!/usr/bin/env python3
"""Check that independent tests reject deliberate semantic and ABI defects.

Mutations are built in an isolated source copy. A compiler failure or a crashed
client is not counted as a detected behavioral difference.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

MUTATIONS = [
    ("filetype_enum", "src/brands.rs", "Supported = 1,", "Supported = 7,", "brands"),
    ("error_code", "src/error.rs", 'Self::new(5, 2001, c"NULL argument passed")', 'Self::new(2, 2001, c"NULL argument passed")', "brands"),
    ("brand_box_truncation", "src/box_probe.rs", "matches!(read_box(&mut r, 0), Err((Failure::End, _)))", "matches!(read_box(&mut r, 0), Err((Failure::Other, _)))", "brand_boxes"),
    ("brand_optional_child", "src/box_probe.rs", "if let Err((error, false)) = read_box(r, level)", "if let Err((error, _)) = read_box(r, level)", "brand_boxes"),
    ("brand_parent_boundary", "src/box_probe.rs", "if end > r.input.len() as u64", "if end > r.end as u64", "brand_boxes"),
    ("item_failed_add_id", "src/items.rs", "self.items.remove(&id);", "self.items.remove(&id);\n                self.next = id;", "items"),
    ("item_reference_order", "crates/capi/src/items.rs", ".filter(|r| r.from == from)\n        .nth(index as usize)", ".filter(|r| r.from == from)\n        .rev()\n        .nth(index as usize)", "items"),
    ("item_error_compression", "crates/capi/src/items.rs", "&& method == 0\n        && !compression.is_null()", "&& false\n        && !compression.is_null()", "items"),
    ("deflate_tree_tiebreak", "src/deflate_compat.rs", "nodes[a].depth <= nodes[b].depth", "nodes[a].depth < nodes[b].depth", "items"),
    ("inflate_error_cause", "vendor/zlib-rs/src/inflate.rs", "self.error_message.get_or_insert(msg);", "self.error_message = Some(msg);", "items"),
    ("compressed_handle_metadata", "src/context.rs", "if !matches!(method, 0 | 3 | 4)", "if method != 0", "metadata_compression"),
    ("tai_copy_version", "crates/capi/src/tai.rs", "if unsafe { dst.cast::<u8>().read() } == 0", "if unsafe { dst.cast::<u8>().read() } <= 1", "tai"),
    ("tai_typed_equality", "src/properties.rs", "p.tai == property.tai", "p.data == property.data", "tai"),
    ("tai_clock_bits", "src/tai.rs", "clock_type: data[20] >> 6", "clock_type: data[20] >> 5", "tai"),
    ("tai_decoded_value", "src/decoding.rs", "image.tai_timestamp = Some(timestamp);", "let _ = timestamp;", "tai"),
    ("reload_file_tables", "crates/capi/src/decoding.rs", "if !has_iloc && document.images.contains_key(&handle.id)", "if false && !has_iloc && document.images.contains_key(&handle.id)", "tai"),
    ("metadata_exif_offset", "src/metadata.rs", "&(offset as u32).to_be_bytes()", "&(offset as u32 + 1).to_be_bytes()", "add_metadata"),
    ("metadata_deflate_wrapper", "src/metadata.rs", "let method = if matches!(compression, 3 | 4) { 4 } else { 0 };", "let method = if matches!(compression, 3 | 4) { compression } else { 0 };", "add_metadata"),
    ("metadata_reference_target", "src/metadata.rs", "to: vec![target],", "to: vec![target.wrapping_add(1)],", "add_metadata"),
    ("text_reload_registry", "src/context.rs", "self.items = crate::items::ItemStore::reading(input.clone());", "self.text_items.clear();\n        self.items = crate::items::ItemStore::reading(input.clone());", "text"),
    ("text_pending_payload", "src/text.rs", "let id = self.items.add_pending(item)?;", "let id = self.items.add(item, content.clone())?;", "text"),
    ("text_lookup_order", "crates/capi/src/text.rs", ".find(|t| t.id == id)", ".rfind(|t| t.id == id)", "text"),
    ("infe_string_terminator", "src/container.rs", ".unwrap_or(self.0.len().saturating_sub(1))", ".unwrap_or(self.0.len())", "text"),
    ('extract_chroma_origin', 'src/image_area.rs', '(x.div_ceil(sx), y.div_ceil(sy))', '(x / sx, y / sy)', 'image_area'),
    ('padding_reallocated_dimensions', 'src/image_area.rs', '*p = dest;', '*p = dest;\n                if !zero { p.width = old_w; p.height = old_h; }', 'image_area'),
    ('zero_chroma_neutral', 'src/image_area.rs', '                128\n', '                127\n', 'image_area'),
    ('extract_next_component_id', 'src/image_area.rs', 'out.component_ids = self.component_ids.clone();', 'out.component_ids = self.component_ids.clone();\n        out.component_ids.next = 1;', 'image_area'),
    ('extract_extension_budget', 'src/image_area.rs', 'out.extend_area(width, height, true, budget.as_ref())?;', 'out.extend_area(width, height, true, None)?;', 'image_area'),
    ('handle_first_property', 'src/handle_properties.rs', '.find(|p| !p.raw && p.kind == kind)', '.rfind(|p| !p.raw && p.kind == kind)', 'handle_color'),
    ('handle_property_read_only', 'src/handle_properties.rs', 'context.properties.add_to_file(self.id, property, false)?;', 'context.properties.add(self.id, property, false)?;', 'handle_color'),
    ('handle_zero_clli', 'src/handle_properties.rs', 'if v.max_content_light_level == 0 && v.max_pic_average_light_level == 0 {', 'if false {', 'handle_color'),
    ('typed_property_trailing_bytes', 'src/properties.rs', '&data[..size.min(data.len())]', 'data', 'handle_color'),
    ('derived_hdr_inheritance', 'src/decoding.rs', 'image.color.content_light = inherited_hdr.0;', 'image.color.content_light = Default::default();', 'handle_color'),
    ('ndwt_version_warning', 'src/properties.rs', 'b"ndwt" if data[0] != 0 =>', 'b"ndwt" if data[0] > 1 =>', 'handle_color'),
    ("plane_pixels", "src/image.rs", "storage.resize(allocation, 0);", "storage.resize(allocation, 0);\n        storage[..16].fill(1);", "images"),
    ("primary_coordinate", "src/color.rs", "color_primary_red_x: rx,", "color_primary_red_x: rx + 0.0001,", "color"),
    ("primary_id", "crates/capi/src/context.rs", "out.write(doc.primary)", "out.write(doc.primary.wrapping_add(1))", "context"),
    ("alpha_reload_state", "crates/capi/src/context.rs", ".is_some_and(|i| i.has_alpha)", ".is_some_and(|_| handle.image().has_alpha)", "context"),
    ("grid_worker_callbacks", "src/decoding.rs", "if options.max_decoding_threads > 0 {", "if false {", "decode_derived"),
    ("warning_text", "crates/capi/src/image.rs", "libheifer::error_text::message(error.code, error.subcode)", 'String::from("wrong warning text")', "warnings"),
    ("mask_samples", "src/mask.rs", ".copy_from_slice(&data[y * row_bytes..(y + 1) * row_bytes]);", ".copy_from_slice(&data[y * row_bytes..(y + 1) * row_bytes]);\n        plane.data_mut()[target] ^= 1;", "decode_mask"),
    ("overlay_alpha", "src/overlay.rs", "((src * a + dst * (255 - a)) / 255)", "((src * a + dst * (255 - a)) / 256)", "decode_graphs"),
    ("derived_operation_budget", "src/decoding.rs", ".saturating_mul(2)", ".saturating_mul(3)", "decode_graphs"),
    ("live_memory_budget", "src/security.rs", ".checked_add(amount)", ".checked_add(0)", "security"),
    ("auxiliary_filter", "crates/capi/src/auxiliary.rs", "filter & 2 == 0", "filter & 1 == 0", "auxiliary"),
    ("depth_value", "src/auxiliary.rs", "exponent - 31", "exponent - 30", "auxiliary"),
    ("property_ids", "crates/capi/src/properties.rs", "out.add(n as usize).write(index as u32 + 1)", "out.add(n as usize).write(index as u32 + 2)", "properties"),
    ("property_raw_class", "src/properties.rs", "None => p.raw,", "None => true,", "properties"),
    ("description_terminator", "src/properties.rs", ".unwrap_or(data.len().saturating_sub(1))", ".unwrap_or(data.len())", "properties"),
    ("property_crop_origin", "crates/capi/src/properties.rs", "l as c_int,", "(l + 1) as c_int,", "properties"),
    ("camera_focal_scale", "src/camera.rs", "focal_length_x: fx * f64::from(width as i32),", "focal_length_x: fx * f64::from(width as i32) * 2.0,", "camera"),
    ("camera_quaternion", "src/camera.rs", "quaternion[3] = (1.0 - sum).sqrt();", "quaternion[3] = (1.0 - sum).sqrt() * 0.5;", "camera"),
    ("polarization_match_order", "src/sensor.rs", ".position(|p|", ".rposition(|p|", "sensor"),
    ("polarization_nan_bits", "crates/capi/src/sensor.rs", "value.to_bits() == u32::MAX", "value.to_bits() == u32::MAX - 1", "sensor"),
    ("component_id_sequence", "src/components.rs", "self.next.wrapping_add(1)", "self.next.wrapping_add(2)", "sensor"),
    ("component_reference_count", "crates/capi/src/components.rs", "i.component_ids.descriptions.len() as u32", "i.component_ids.descriptions.iter().filter(|d| d.has_data).count() as u32", "components"),
    ("component_typed_stride", "crates/capi/src/components.rs", "stride.write(plane.map_or(0, |p| p.stride / std::mem::size_of::<$ty>()));", "stride.write(plane.map_or(0, |p| p.stride));", "components"),
    ("component_crop_datatype", "src/image.rs", "plane.datatype = source.datatype;", "plane.datatype = 0;", "components"),
    ("component_grid_parse_order", "src/context.rs", 'b"grid" => coded.map(|c| (c.colorspace, c.chroma, c.luma_bits, c.chroma_bits)),', 'b"grid" => coded.map(|c| (c.colorspace, c.chroma, c.luma_bits + 1, c.chroma_bits)),', "component_handles"),
    ("component_decode_ids", "src/decoding.rs", "image.apply_descriptions(&document.images[&id].components);", "// Deliberate defect: skip component reconciliation.", "component_handles"),
    ("component_alpha_depth", "src/components.rs", "if depth > 0 { depth as u16 } else { 8 }", "if false { depth as u16 } else { 8 }", "component_handles"),
    ("jpeg_sof_boundary", "src/jpeg_config.rs", "11 + 3 * count >= data.len()", "11 + 3 * count > data.len()", "component_handles"),
    ("coded_size_limit", "src/decoding.rs", ".max(65536)", ".max(65535)", "hevc_limits"),
    ("error_field_order", "crates/capi/src/lib.rs", "pub code: c_int,\n    pub subcode: c_int,", "pub subcode: c_int,\n    pub code: c_int,", "abi"),
]


def execute(command, cwd, env, log):
    run = subprocess.run(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180)
    log.write_bytes(run.stdout)
    return run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-build", required=True)
    parser.add_argument("--candidate", default="target/release/libheifer.so")
    parser.add_argument("--output", default=".build/mutations-report.json")
    parser.add_argument("--only", choices=[m[0] for m in MUTATIONS], action="append", help="Run selected defects; default runs the complete mutation set")
    args = parser.parse_args()
    mutations = [m for m in MUTATIONS if args.only is None or m[0] in args.only]
    root = Path.cwd()
    evidence = root / ".build/mutations"
    evidence.mkdir(parents=True, exist_ok=True)
    reference = str(Path(args.reference_build).resolve())
    candidate = str(Path(args.candidate).resolve())
    # Baselines must pass on this tree before a rejected mutant is meaningful.
    for suite in dict.fromkeys(m[4] for m in mutations if m[4] != "abi"):
        run = execute([sys.executable, f"tools/test_{suite}.py", "--reference-build", reference, "--candidate", candidate, *(["--work", str(evidence / "decode")] if suite.startswith("decode_") or suite == "hevc_limits" else []), "--output", str(evidence / f"baseline-{suite}.json")], root, os.environ, evidence / f"baseline-{suite}.log")
        if run.returncode:
            raise SystemExit(f"Baseline {suite} failed; see {evidence}")
    run = execute(["cargo", "test", "--locked", "-p", "libheifer-capi", "--test", "abi"], root, os.environ, evidence / "baseline-abi.log")
    if run.returncode:
        raise SystemExit("Baseline ABI test failed")
    results = []
    with tempfile.TemporaryDirectory(prefix="mutant-", dir=root / ".build") as temporary:
        clone = Path(temporary)
        for name in ("Cargo.toml", "Cargo.lock"):
            shutil.copy2(root / name, clone / name)
        for name in ("src", "crates", "examples", "vendor"):
            shutil.copytree(root / name, clone / name)
        (clone / "tests").mkdir()
        (clone / "tests/upstream").symlink_to(root / "tests/upstream", target_is_directory=True)
        env = dict(os.environ, CARGO_TARGET_DIR=str(clone / "target"))
        for name, file, before, after, suite in mutations:
            path = clone / file
            original = path.read_text()
            if original.count(before) != 1:
                raise SystemExit(f"Mutation anchor drift: {name}")
            path.write_text(original.replace(before, after))
            try:
                build = execute(["cargo", "build", "--locked", "--release", "-p", "libheifer-capi"], clone, env, evidence / f"{name}-build.log")
                if build.returncode:
                    raise SystemExit(f"Mutation did not compile: {name}")
                library = clone / "target/release/libheifer.so"
                record = {"mutation": name, "suite": suite, "source": file, "before": before, "after": after, "candidate_sha256": hashlib.sha256(library.read_bytes()).hexdigest()}
                if suite == "abi":
                    run = execute(["cargo", "test", "--locked", "-p", "libheifer-capi", "--test", "abi"], clone, env, evidence / f"{name}.log")
                    record["detected"] = run.returncode == 101 and b"public_structs_match_original_header_layouts ... FAILED" in run.stdout and b"assertion `left == right` failed" in run.stdout
                else:
                    report_path = evidence / f"{name}.json"
                    report_path.unlink(missing_ok=True)
                    run = execute([sys.executable, f"tools/test_{suite}.py", "--reference-build", reference, "--candidate", str(library), *(["--work", str(evidence / "decode")] if suite.startswith("decode_") or suite == "hevc_limits" else []), "--output", str(report_path)], root, os.environ, evidence / f"{name}.log")
                    report = json.loads(report_path.read_text()) if report_path.exists() else {}
                    record["mismatches"] = report.get("mismatches", 0)
                    record["detected"] = run.returncode == 1 and record["mismatches"] > 0
                results.append(record)
                print(json.dumps(record), flush=True)
            finally:
                path.write_text(original)
    report = {"scope": f"{len(mutations)} deliberate defects; not comprehensive mutation coverage", "mutations": results, "all_detected": all(r["detected"] for r in results)}
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    if not report["all_detected"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
