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
    ('jpeg2000_packet_extent', 'vendor/hayro-jpeg2000/src/j2c/segment.rs', 'if oversized_segment || (!complete && header.strict)', 'if !complete && header.strict', 'jpeg2000_sampling'),
    ('jpeg2000_tile_transform', 'vendor/hayro-jpeg2000/src/j2c/decode.rs', 'if tile.mct && !tile.tile_parts.is_empty()', 'if tiles[0].mct && !tile.tile_parts.is_empty()', 'jpeg2000_tiles'),
    ('jpeg2000_tile_wavelet', 'vendor/hayro-jpeg2000/src/j2c/decode.rs', 'for (idx, component_info) in tile.component_infos.iter().enumerate()', 'for (idx, component_info) in header.component_infos.iter().enumerate()', 'jpeg2000_tiles'),
    ('jpeg2000_transform_tail', 'vendor/hayro-jpeg2000/src/j2c/mct.rs', 'let tail = s0.len() / 8 * 8;', 'let tail = s0.len();', 'jpeg2000_tiles'),
    ('jpeg2000_raw_component_header', 'src/jpeg2000.rs', 'hayro_jpeg2000::RawCodestream::new', 'hayro_jpeg2000::Image::new', 'jpeg2000_tiles'),
    ('jpeg2000_property_components', 'src/jpeg2000_properties.rs', 'if kind == *b"cdef" && max_components != 0', 'if false && kind == *b"cdef" && max_components != 0', 'jpeg2000_properties'),
    ('jpeg2000_palette_depth', 'src/jpeg2000_properties.rs', 'depth > 16', 'depth > 17', 'jpeg2000_properties'),
    ('jpeg2000_palette_width', 'src/jpeg2000_properties.rs', 'depth <= 8', 'depth <= 7', 'jpeg2000_properties'),
    ('jpeg2000_child_extent', 'src/context.rs', 'ContextError::invalid(101, "Invalid box size")', 'ContextError::invalid(100, "Unexpected end of file")', 'jpeg2000_properties'),
    ('jpeg2000_palette_dump', 'src/debug.rs', 'NE: {entries}, NPC: {columns}', 'NE: {columns}, NPC: {columns}', 'jpeg2000_debug'),
    ('jpeg2000_absent_tile', 'src/jpeg2000.rs', 'if !component.is_present(x as u32 * dx, y as u32 * dy) {', 'if false && !component.is_present(x as u32 * dx, y as u32 * dy) {', 'jpeg2000_handles'),
('jpeg2000_tile_order', 'src/jpeg2000.rs', 'if *next != u16::from(record[6]) {', 'if false && *next != u16::from(record[6]) {', 'jpeg2000_errors'),
    ('jpeg2000_wavelet_normalization', 'vendor/hayro-jpeg2000/src/j2c/decode.rs', 'if irreversible { 0 } else { log_gain }', 'log_gain', 'jpeg2000_pixels'),
    ('jpeg2000_round_before_shift', 'src/jpeg2000.rs', '(sample.round_ties_even() as i64 + offset)', '((sample + offset as f32).round_ties_even() as i64)', 'jpeg2000_pixels'),
    ('jpeg2000_signed_level', 'src/jpeg2000.rs', 'let offset = if c.signed { 0 } else { 1i64 << (c.depth - 1) };', 'let offset = 1i64 << (c.depth - 1);', 'jpeg2000_pixels'),
    ('jpeg2000_chroma_sample', 'src/jpeg2000.rs', 'x * dx as usize', 'x', 'jpeg2000_pixels'),
    ('jpeg2000_siz_length', 'src/jpeg2000.rs', '!= 38 + 3 * h.components.len()', '< 38 + 3 * h.components.len()', 'jpeg2000_errors'),
    ('jpeg2000_end_marker', 'src/jpeg2000.rs', 'if end + 2 > data.len() {\n                    return Err(error("opj_decode()"));\n                }', 'if end + 2 > data.len() {\n                    return Ok(());\n                }', 'jpeg2000_errors'),
    ('jpeg2000_decode_memory', 'src/jpeg2000.rs', 'estimated = estimated.saturating_mul(3);', 'estimated = estimated.saturating_mul(2);', 'jpeg2000_limits'),
    ('jpeg2000_handle_precision', 'src/jpeg2000_config.rs', 'depth: (c[0] & 127) + 1,', 'depth: (c[0] & 127) + 2,', 'jpeg2000_handles'),

    ('vvc_profile', 'src/vvc_config.rs', 'self.profile = b.get(7) as u8;', 'self.profile = b.get(7) as u8 ^ 1;', 'vvc_encoding'),
    ('vvc_configuration_reset', 'src/vvc_config.rs', 'self.config = Configuration::default();', '// omit SPS reset', 'vvc_encoding'),
    ('vvc_sublayer_order', 'src/vvc_config.rs', 'out.push(c.levels[i]);', 'out.push(c.levels[i] ^ 1);', 'vvc_encoding'),
    ('vvc_dimensions', 'src/vvc_config.rs', 'out.extend_from_slice(&c.width.to_be_bytes());', 'out.extend_from_slice(&c.height.to_be_bytes());', 'vvc_encoding'),
    ('vvc_parameter_count', 'src/vvc_config.rs', 'u16::try_from(units.len())', 'u16::try_from(units.len() + 1)', 'vvc_encoding'),
    ('vvc_array_completeness', 'src/vvc_config.rs', 'out.push(128 | kind);', 'out.push(*kind);', 'vvc_encoding'),
    ('vvc_brand', 'src/writing.rs', 'b"vvc1" => Some(*b"vvic"),', 'b"vvc1" => Some(*b"vvis"),', 'vvc_encoding'),

    ('jpeg_idct_rounding', 'vendor/jpeg-decoder/src/idct.rs', 'const X_SCALE: i32 = 131072 + (128 << 18);', 'const X_SCALE: i32 = 0 + (128 << 18);', 'jpeg_pixels'),
    ('jpeg_horizontal_rounding', 'vendor/jpeg-decoder/src/upsampler.rs', '((sample + input[i - 1] as u32 + 1) >> 2)', '((sample + input[i - 1] as u32 + 2) >> 2)', 'jpeg_pixels'),
    ('jpeg_vertical_rounding', 'vendor/jpeg-decoder/src/upsampler.rs', '((3 * t1 + t0 + 8) >> 4)', '((3 * t1 + t0 + 7) >> 4)', 'jpeg_pixels'),
    ('jpeg_narrow_upsampling', 'vendor/jpeg-decoder/src/upsampler.rs', 'h2 && input_width <= 2', 'h2 && input_width <= 1', 'jpeg_pixels'),
    ('jpeg_chroma_scanline', 'src/jpeg.rs', 'let sy = if channel == 0 { y } else { y * 2 };', 'let sy = if channel == 0 { y } else { (y * 2 + 1).min(height as usize - 1) };', 'jpeg_pixels'),
    ('jpeg_progressive_smoothing', 'vendor/jpeg-decoder/src/smoothing.rs', 'bits[0]>=0 && POS.iter().all(|&p|q[p]!=0)', 'false && bits[0]>=0 && POS.iter().all(|&p|q[p]!=0)', 'jpeg_recovery'),
    ('jpeg_entropy_recovery', 'vendor/jpeg-decoder/src/decoder.rs', 'if skip_mcu { continue; }', 'if false && skip_mcu { continue; }', 'jpeg_recovery'),
    ('jpeg_eof_marker', 'src/jpeg.rs', 'let mut decoder = Decoder::new(PaddedInput { data: &data, at: 0 });', 'let mut decoder = Decoder::new(data.as_slice());', 'jpeg_recovery'),
    ('jpeg_decoder_input_replacement', 'src/decoding.rs', '    *cached = None;', '    // omit prior decoder-input release', 'jpeg_limits'),
    ('jpeg_decode_memory_estimate', 'src/jpeg.rs', 'let estimated = pixels * info.pixel_format.pixel_bytes() as u64 * 3;', 'let estimated = pixels * info.pixel_format.pixel_bytes() as u64 * 2;', 'jpeg_limits'),

    ('avc_interlaced_height', 'src/avc_config.rs', 'h *= u64::from(2 - frame);', 'h *= 1;', 'avc_encoding'),
    ('avc_crop_units', 'src/avc_config.rs', 'let sx = if matches!(self.chroma, 1 | 2) { 2 } else { 1 };', 'let sx = 1;', 'avc_encoding'),
    ('avc_scaling_break', 'src/avc_config.rs', 'if next == 0 {', 'if next != 0 {', 'avc_encoding'),
    ('avc_sps_partial', 'src/avc_config.rs', 'let _ = self.parse(nal);', 'self.parse(nal)?;', 'avc_encoding'),
    ('avc_extended_configuration', 'src/avc_config.rs', 'if !matches!(self.profile, 66 | 77 | 88) {', 'if matches!(self.profile, 66 | 77 | 88) {', 'avc_encoding'),
    ('avc_parameter_count', 'src/avc_config.rs', 'self.sps.len() > 31', 'self.sps.len() >= 31', 'avc_encoding'),
    ('configuration_write_prefix', 'src/writing.rs', 'ipco.extend([0; 8]);', 'ipco.extend([1; 8]);', 'avc_encoding'),
    ('hevc_partial_depth', 'src/hevc_config.rs', 'self.header[17] = 0xf8 | luma as u8;', '// omit partial luma configuration update', 'hevc_encoding'),

    ('jpeg_encoder_forced_profile', 'crates/capi/src/plugin_encoding.rs', 'primaries: 6,', 'primaries: 7,', 'other_encoding'),
    ('j2k_encoder_channel_count', 'crates/capi/src/plugin_encoding.rs', '0 | 1 => 3,', '0 | 1 => 2,', 'other_encoding'),
    ('j2k_encoder_channel_association', 'crates/capi/src/plugin_encoding.rs', 'data.extend_from_slice(&(i + 1).to_be_bytes());', 'data.extend_from_slice(&i.to_be_bytes());', 'other_encoding'),
    ('j2k_encoder_container_dedup', 'src/properties.rs', 'p.kind != *b"j2kH"', 'true', 'other_encoding'),
    ('j2k_encoder_error_prefix', 'crates/capi/src/plugin_encoding.rs', '                matches!(encoder.source.format(), 7 | 10),\n            ));\n        }\n        if packet.is_null()', '                false,\n            ));\n        }\n        if packet.is_null()', 'other_encoding'),
    ('jpeg_encoder_brand', 'src/writing.rs', 'b"jpeg" => Some(*b"jpeg"),', 'b"jpeg" => Some(*b"jpgx"),', 'other_encoding'),

    ('hevc_encoder_crop', 'src/hevc_config.rs', 'width -= crop_x as u32;', 'width -= 0;', 'hevc_encoding'),
    ('hevc_encoder_profile', 'src/hevc_config.rs', 'self.header[1] = bits.get(8) as u8;', 'self.header[1] = bits.get(8) as u8 ^ 1;', 'hevc_encoding'),
    ('hevc_encoder_dedup', 'src/hevc_config.rs', 'if existing[..common] == nal[..common] {', 'if false && existing[..common] == nal[..common] {', 'hevc_encoding'),
    ('hevc_encoder_length', 'crates/capi/src/plugin_encoding.rs', 'data.extend_from_slice(&(size as u32).to_be_bytes());', 'data.extend_from_slice(&((size + 1) as u32).to_be_bytes());', 'hevc_encoding'),
    ('hevc_encoder_aux_type', 'src/encoding.rs', 'b"urn:mpeg:hevc:2015:auxid:1\\0".as_slice()', 'b"urn:mpeg:hevc:2015:auxid:2\\0".as_slice()', 'hevc_encoding'),
    ('hevc_encoder_brand', 'src/writing.rs', 'flags & 0x50 != 0', 'flags & 0x50 == 0', 'hevc_encoding'),
    ('hevc_encoder_array', 'src/hevc_config.rs', 'bytes.push(64 | kind);', 'bytes.push(128 | kind);', 'hevc_mini_encoding'),

    ('encoder_empty_pixi', 'src/encoding.rs', 'image.plane(ch).map_or(0, |p| p.bit_depth)', 'image.plane(ch).map_or(1, |p| p.bit_depth)', 'plugin_encoding'),
    ('encoder_versioned_query', 'crates/capi/src/plugin_encoding.rs', 'if version >= 2 {', 'if version >= 3 {', 'plugin_encoding'),
    ('encoder_input_class', 'crates/capi/src/plugin_encoding.rs', 'encode(encoder.state, &image, input_class)', 'encode(encoder.state, &image, input_class + 1)', 'plugin_encoding'),
    ('encoder_packet_configuration', 'crates/capi/src/plugin_encoding.rs', 'config.update(packet);', '// omit sequence configuration', 'plugin_encoding'),
    ('encoder_alpha_class', 'crates/capi/src/plugin_encoding.rs', 'encode_coded(&alpha, &alpha_encoder, copied, options, 2)?', 'encode_coded(&alpha, &alpha_encoder, copied, options, 1)?', 'plugin_encoding'),
    ('encoder_parameter_copy', 'crates/capi/src/plugin_encoding.rs', 'set(out.state, value);', 'set(out.state, value + 1);', 'plugin_encoding'),
    ('encoder_packet_bytes', 'crates/capi/src/plugin_encoding.rs', 'data.extend_from_slice(packet);', 'data.extend_from_slice(packet); data.push(0);', 'plugin_encoding'),
    ('encoder_clap_range', 'src/encoding.rs', 'encoded_size.0 - image.width > (1u32 << 31)', 'encoded_size.0 - image.width >= (1u32 << 31)', 'plugin_encoding'),
    ('mini_writer_orientation', 'src/mini_write.rs', 'orientation = crate::geometry::orientation_concat(orientation, transform);', 'orientation = 1; let _ = transform;', 'mini_encoding'),
    ('mini_writer_alpha_inheritance', 'src/mini_write.rs', 'alpha_data.is_empty() || alpha_config == config', 'alpha_data.is_empty()', 'mini_encoding'),
    ('mini_writer_metadata_width', 'src/mini_write.rs', '> 1024', '>= 1024', 'mini_encoding'),
    ('mini_writer_item_width', 'src/mini_write.rs', 'main_data.len() > 32768', 'main_data.len() >= 32768', 'mini_encoding'),
    ('mini_writer_brand', 'src/mini_write.rs', 'b"mif3".as_slice()', 'b"mif1".as_slice()', 'mini_encoding'),
    ('mini_writer_diffuse_white', 'src/mini_write.rs', '4 => bits.bytes(p.get(4..).unwrap_or_default()),', '4 => bits.bytes(p.get(..4).unwrap_or_default()),', 'mini_encoding'),
    ('mini_debug_gain_depth', 'src/mini.rs', 'value!("gainmap_bit_depth", bits.depth(gain_float, true)?);', 'value!("gainmap_bit_depth", bits.depth(gain_float, true)? + 1);', 'mini_debug'),
    ('mini_debug_hdr_primaries', 'src/mini.rs', 'bits.get(32) as i32', 'bits.get(32) as i64', 'mini_debug'),
    ('mini_debug_failed_expansion', 'src/context.rs', 'if let Some(error) = expansion_error {', 'if let Some(error) = expansion_error { self.debug_loaded = 0;', 'mini_debug'),
    ('mini_debug_config_inheritance', 'src/mini.rs', '"gainmap_item_codec_config size",', '"gainmap_item_code_config size",', 'mini_debug'),
    ('mini_partial_items', 'src/context.rs', 'if input.is_minimized() {', 'if false && input.is_minimized() {', 'mini_reader'),
    ('reader_empty_timeout', 'crates/capi/src/input.rs', 'None => Ok(Cow::Borrowed(&[])),', 'None => Err(eof()),', 'mini_reader'),
    ('mini_reader_payload_budget', 'src/input.rs', 'crate::mini::payload_budget(amount)?;', 'crate::mini::payload_budget(amount + 1)?;', 'mini_reader'),
    ('av1_pixel_copy', 'src/av1.rs', '.copy_from_slice(&frame.planes[channel][y * row..y * row + row]);', '.copy_from_slice(&frame.planes[channel][y * row..y * row + row]);\n            plane.data_mut()[y * stride] ^= 1;', 'av1'),
    ('av1_matrix', 'src/av1.rs', 'matrix: u16::from(frame.matrix_coefficients),', 'matrix: 2,', 'av1'),
    ('color_mismatch_warning', 'src/decoding.rs', 'let mismatch = |a, b| a != 2 && b != 2 && a != b;', 'let mismatch = |a, b| a != 2 && b != 2 && a == b;', 'av1'),
    ('color_warning_persistence', 'src/decoding.rs', 'let range = |full| if full { "full" } else { "limited" };', 'warnings.clear();\n            let range = |full| if full { "full" } else { "limited" };', 'av1'),
    ('color_full_range_correction', 'src/decoding.rs', 'options.autocorrect_broken_input && bitstream.full_range && !profile.full_range', 'false && options.autocorrect_broken_input && bitstream.full_range && !profile.full_range', 'av1'),
    ('mini_orientation', 'src/mini.rs', 'match self.orientation {', 'match 1 {', 'mini'),
    ('mini_payload_offset', 'src/mini.rs', 'extents.push((id, offset + at as u64, size));', 'extents.push((id, offset + at as u64 + 1, size));', 'mini'),
    ('mini_alpha_configuration', 'src/mini.rs', '} else if alpha_config_size == 0 {\n            config', '} else if alpha_config_size == 0 {\n            &[]', 'mini_properties'),
    ('mini_diffuse_white', 'src/mini.rs', 'data.extend([0; 4]); // ndwt is a version-zero FullBox.', '// deliberately omit the FullBox header', 'mini_properties'),
    ('mini_context_table_limit', 'src/mini.rs', 'max_items: 0,', 'max_items: limits.max_items,', 'av1_limits'),
    ('mini_nclx_range', 'src/mini.rs', 'nclx.push(u8::from(full_range) << 7);', 'nclx.push(u8::from(!full_range) << 7);', 'mini'),
    ('plugin_decode_threads', 'crates/capi/src/plugin_decoding.rs', 'num_threads: options.num_codec_threads,', 'num_threads: options.num_codec_threads + 1,', 'plugin_decoding'),
    ('plugin_decode_strict', 'crates/capi/src/plugin_decoding.rs', 'strict_decoding: options.plugin_strict,', 'strict_decoding: i32::from(options.strict),', 'plugin_decoding'),
    ('plugin_decode_poll_limit', 'crates/capi/src/plugin_decoding.rs', 'for _ in 0..50 {', 'for _ in 0..49 {', 'plugin_decoding'),
    ('plugin_decode_release', 'crates/capi/src/plugin_decoding.rs', 'unsafe { f(self.state) }', 'let _ = f;', 'plugin_decoding'),
    ('plugin_decode_error_unpack', 'crates/capi/src/plugin_decoding.rs', 'if unpack && let Some(rest) = detail.strip_prefix(code) {', 'if !unpack && let Some(rest) = detail.strip_prefix(code) {', 'plugin_decoding'),
    ('plugin_decode_cached_selection', 'src/decoding.rs', 'if let Some(cached) = cached {', 'if let Some(cached) = cached.filter(|_| false) {', 'plugin_decoding'),
    ('plugin_decode_empty_nal', 'src/decoding.rs', 'if !nal.is_empty() {', 'if true {', 'plugin_decoding'),
    ('av1_handle_depth', 'src/context.rs', '} else if flags & 32 != 0 {', '} else if flags & 32 == 0 {', 'plugin_decoding'),

    ('debug_writer_base', 'src/writing.rs', 'loc.base = debug_base;', 'loc.base = debug_base + 1;', 'debug_writing'),
    ('debug_writer_duplicates', 'src/debug.rs', 'self.write_meta(&layout, 0, true)', 'self.write_meta(&layout, 0, false)', 'debug_writing'),
    ('debug_grid_rows', 'src/debug.rs', 'grid.rows, grid.columns, grid.width, grid.height', 'grid.rows + 1, grid.columns, grid.width, grid.height', 'debug_dump'),
    ('debug_overlay_offsets', 'src/debug.rs', 'write!(details, "{x};{y} ")', 'write!(details, "{y};{x} ")', 'debug_dump'),

    ('debug_size_label', 'src/debug.rs', '(header size: {header_size})', '(header size: 0)', 'debug_dump'),
    ('debug_hidden_flag', 'src/debug.rs', '"{indent}hidden item: {}", flags & 1 != 0', '"{indent}hidden item: {}", flags & 1 == 0', 'debug_dump'),
    ('debug_nclc_range', 'src/debug.rs', 'u64::from(matrix == 0)', 'u64::from(matrix != 0)', 'debug_dump'),
    ('debug_short_write', 'crates/capi/src/debug.rs', 'write(fd, dump.as_ptr().cast(), dump.len());', 'write(fd, dump.as_ptr().cast(), dump.len().saturating_sub(1));', 'debug_dump'),
    ('debug_property_index', 'src/debug.rs', 'let _ = writeln!(out, "{indent}index: {index}");', 'let _ = writeln!(out, "{indent}index: {}", index + 1);', 'debug_dump'),

    ('reader_initial_range', 'src/input.rs', 'let mut available = source.request_range(0, 1024);', 'let mut available = source.request_range(0, 31);', 'reader_input'),
    ('reader_message_release', 'crates/capi/src/input.rs', 'unsafe { release(result.reader_error_msg) };', 'let _ = release;', 'reader_input'),
    ('reader_payload_origin', 'crates/capi/src/input.rs', '        Ok(Cow::Owned(data))\n    }\n    fn read_range', '        if let Some(first) = data.first_mut() { *first ^= 1; }\n        Ok(Cow::Owned(data))\n    }\n    fn read_range', 'reader_input'),
    ('reader_idat_wait', 'crates/capi/src/input.rs', 'if status == 1 || status == 2 {', 'if status == 0 || status == 2 {', 'reader_input'),
    ('reader_initial_timeout', 'crates/capi/src/input.rs', '1 => 0,', '1 => end,', 'reader_input'),
    ('sequence_short_movie', 'src/context.rs', '151,\n                    "No \'moov\' box: Cannot read full moov box"', '100,\n                    "No \'moov\' box: Cannot read full moov box"', 'sequence_reading'),
    ('sequence_failed_frame_advance', 'src/sequences.rs', 'self.decode_failed = u64::from(self.next) >= self.output_count;', 'self.next = self.next.wrapping_sub(1);\n                    self.decode_failed = u64::from(self.next) >= self.output_count;', 'sequence_reading'),

    ("file_snapshot_origin", "src/input.rs", "|(start, offset, _)| offset + (at - start) as u64", "|(start, offset, _)| offset + (at - start) as u64 + 1", "file_input"),
    ("file_source_boundary", "src/input.rs", "is_none_or(|n| n > self.length())", "is_none_or(|n| n >= self.length())", "file_sequence_reading"),
    ("file_failed_open_state", "crates/capi/src/input.rs", "    let _ = state.read(Arc::new(Vec::<u8>::new()));", "    // deliberately retain the old file tables", "file_input"),
    ("file_header_origin", "src/input.rs", "let prefix = take(source, 0, length.min(32))?;", "let prefix = take(source, 1, length.min(32))?;", "file_input"),
    ("file_open_error_class", "crates/capi/src/input.rs", "                    1,\n                    0,\n                    format!(", "                    2,\n                    0,\n                    format!(", "file_input"),
    ('dynamic_trailing_directory', 'crates/capi/src/dynamic_plugins.rs', '            v.pop();', '            // keep the trailing segment', 'dynamic_plugins'),
    ('dynamic_encoder_version', 'crates/capi/src/dynamic_plugins.rs', 'if field!(p, plugin_api_version) < 4 {', 'if field!(p, plugin_api_version) < 3 {', 'dynamic_plugins'),
    ('dynamic_repeated_load', 'crates/capi/src/dynamic_plugins.rs', '            p.count += 1;', '            // forget the duplicate reference', 'dynamic_plugins'),
    ('dynamic_reload_identity', 'crates/capi/src/dynamic_plugins.rs', 'p.matchable && p.handle == handle as usize', 'p.handle == handle as usize', 'dynamic_plugins'),
    ('dynamic_bulk_capacity', 'crates/capi/src/dynamic_plugins.rs', 'if n == capacity {', 'if n >= capacity {', 'dynamic_plugins'),
    ('dynamic_bulk_count', 'crates/capi/src/dynamic_plugins.rs', 'count.write(n)', 'count.write(n+1)', 'dynamic_plugins'),
    ('dynamic_bulk_terminator', 'crates/capi/src/dynamic_plugins.rs', 'out.offset(n as isize).write(ptr::null())', 'out.offset(n as isize).write(0x1234usize as *const PluginInfo)', 'dynamic_plugins'),
    ('dynamic_cleanup_order', 'crates/capi/src/plugin_registry.rs', '    REGISTRY.lock().unwrap().encoders.clear();\n    crate::dynamic_plugins::unload_all();', '    crate::dynamic_plugins::unload_all();', 'dynamic_plugins'),

    ('sequence_default_timescale', 'src/sequences.rs', 'timescale: 90000,', 'timescale: 90001,', 'sequences'),
    ('sequence_fresh_handler', 'src/sequences.rs', 'reported_handler: 0,', 'reported_handler: handler,', 'sequences'),
    ('sequence_reference_order', 'src/sequences.rs', '            ids.push(id);', '            ids.insert(0,id);', 'sequences'),
    ('sequence_sample_duration', 'src/sequences.rs', 'self.durations.push(sample.metadata.duration);', 'self.durations.push(sample.metadata.duration.wrapping_add(1));', 'sequences'),
    ('sequence_optional_tai', 'src/sequences.rs', '} else if self.options.tai_presence == 1 {', '} else if self.options.tai_presence == 2 {', 'sequences'),
    ('sequence_infinite_duration', 'src/sequences.rs', 't.movie_duration = if self.repetitions == 0 {', 't.movie_duration = if self.repetitions == u32::MAX {', 'sequences'),
    ('sequence_sample_offset', 'src/sequences.rs', 'let (offset, size) = self.ranges[idx];', 'let (offset, size) = self.ranges[idx];\n        let offset=offset+1;', 'sequences'),
    ('sequence_clock_copy', 'src/sequences.rs', 't.first_clock = Some(Box::new(c));', 'let mut c=c; c.clock_resolution=c.clock_resolution.wrapping_add(1); t.first_clock = Some(Box::new(c));', 'sequences'),
    ('sequence_decoder_duration', 'src/sequences.rs', 'self.durations[self.next as usize % self.durations.len()]', 'self.durations[self.next.wrapping_sub(1) as usize % self.durations.len()]', 'sequences'),
    ('sequence_raw_output_sentinel', 'crates/capi/src/sequences.rs', 'let result = track.track.lock().unwrap().next_raw();', 'unsafe{out.write(ptr::null_mut())};\n    let result = track.track.lock().unwrap().next_raw();', 'sequences'),
    ('sequence_coding_constraints', 'src/sequences.rs', 'full(*b"ccst", 0, 0, &[0x80, 0, 0, 0])', 'full(*b"ccst", 0, 0, &[0, 0, 0, 0])', 'sequences'),
    ('sequence_movie_validation', 'src/sequences.rs', 'validate_sequence_boxes(movie, &self.limits.read().unwrap())?;', '// deliberately skip sequence box validation', 'sequence_reading'),

    ('grid_copy_error_propagation', 'src/decoding.rs', 'let _ = out.paste(&image, x, y);', 'out.paste(&image, x, y)?;', 'tile_encoding'),
    ('tiling_coded_width', 'src/tiling.rs', 't.tile_width = w;', 't.tile_width = w.wrapping_add(1);', 'tiling'),
    ('tiling_inverse_rotation', 'src/tiling.rs', '1 => (self.num_rows - 1 - y, x),', '1 => (y, x),', 'tiling'),
    ('tiling_crop_offset', 'src/tiling.rs', 'left = left.wrapping_add(l);', 'left = left.wrapping_add(l+1);', 'tiling'),
    ('tile_decode_output_sentinel', 'crates/capi/src/decoding.rs', 'if tile.is_none() {\n        unsafe {', 'if true {\n        unsafe {', 'tiling'),
    ('tile_decode_payload_index', 'src/uncompressed_decode.rs', 'u64::from((ty as u32).wrapping_mul(c.columns).wrapping_add(tx as u32))', 'u64::from((ty as u32).wrapping_mul(c.columns))', 'tiling'),
    ('grid_tile_hidden', 'src/tile_encoding.rs', 'self.items.items.get_mut(&tile.id).unwrap().hidden = true;', 'self.items.items.get_mut(&tile.id).unwrap().hidden = false;', 'tile_encoding'),
    ('grid_orientation', 'src/tile_encoding.rs', 'self.add_orientation(grid.id, orientation)?;', 'self.add_orientation(grid.id, 1)?;', 'tile_encoding'),
    ('unci_tile_compression_essential', 'src/tile_encoding.rs', 'self.retained_property(&info, property(*b"cmpC", d), true)?;', 'self.retained_property(&info, property(*b"cmpC", d), false)?;', 'tile_encoding'),
    ('unci_tile_unit_offset', 'src/tile_encoding.rs', 'units[index] = (*next_offset, size);', 'units[index] = (0, size);', 'tile_encoding'),
    ('unci_tile_replace_offset', 'src/tile_encoding.rs', 'let start = index * *tile_size;', 'let start = 0 * *tile_size;', 'tile_encoding'),
    ('unci_tile_incomplete_properties', 'src/writing.rs', 'incomplete = true;', 'incomplete = false;', 'tile_encoding'),

    ('encode_mask_stride', 'src/encoding.rs', '.get(y * plane.stride..y * plane.stride + image.width as usize)', '.get(0..image.width as usize)', 'encoding'),
    ('encode_orientation', 'src/encoding.rs', '6 => (3, None)', '6 => (1, None)', 'encoding'),
    ('encode_primary_flag', 'src/encoding.rs', 'old.primary.store(false, Ordering::Relaxed);', 'old.primary.store(true, Ordering::Relaxed);', 'encoding'),
    ('encode_profile_fallback', 'crates/capi/src/encoding.rs', '        image.color.nclx\n', '        None\n', 'encoding'),
    ('encode_unc_component_endian', 'src/uncompressed_encode.rs', 'if !dense && cfg!(target_endian = "little")', 'if false', 'encoding'),
    ('encode_unc_compression_flag', 'src/uncompressed_encode.rs', 'props.push((property(*b"cmpC", cmp), false));', 'props.push((property(*b"cmpC", cmp), true));', 'encoding'),
    ('encode_thumbnail_noop', 'crates/capi/src/encoding.rs', 'ContextError::new(0, 0, "Success")', 'ContextError::new(5, 2006, "Invalid thumbnail")', 'encoding'),
    ('encode_thumbnail_direction', 'crates/capi/src/encoding.rs', 'from: image.id,\n                kind: u32::from_be_bytes(*b"thmb"),\n                to: vec![master.id],', 'from: master.id,\n                kind: u32::from_be_bytes(*b"thmb"),\n                to: vec![image.id],', 'encoding'),
    ('encode_overlay_background', 'src/encoding.rs', 'for n in background {', 'for n in [0u16;4] {', 'encoding'),
    ('encode_repeated_extent', 'src/items.rs', '*len = bytes.len() as u64 + data.len() as u64;', '*len = data.len() as u64;', 'encoding'),

    ('writer_mdat_base', 'src/writing.rs', 'out.len() as u64 + meta_size as u64 + moov_size as u64 + 8', 'out.len() as u64 + meta_size as u64 + moov_size as u64 + 9', 'writing'),
    ('writer_brand_dedup', 'src/writing.rs', 'if !self.brands.contains(&brand) {', 'if true {', 'writing'),
    ('writer_uuid_bytes', 'src/writing.rs', 'p.extend(prop.uuid.unwrap_or([0; 16]));', 'p.extend([0; 16]);', 'writing'),
    ('writer_large_property_index', 'src/writing.rs', 'any(|i| *i >= 127)', 'any(|i| *i >= 128)', 'writing'),
    ('writer_userdata', 'crates/capi/src/writing.rs', 'data.len(), userdata)', 'data.len(), ptr::null_mut())', 'writing'),
    ('writer_success_message', 'crates/capi/src/writing.rs', 'error.message = SUCCESS.message;', 'error.message = c"Wrong success".as_ptr();', 'writing'),

    ('plugin_encoder_priority', 'crates/capi/src/plugin_registry.rs', 'Self::External(p) => field!(p, priority)', 'Self::External(p) => -field!(p, priority)', 'plugins'),
    ('plugin_decoder_negative_priority', 'crates/capi/src/plugin_registry.rs', 'if priority != 0 {', 'if priority > 0 {', 'plugins'),
    ('plugin_decoder_duplicate', 'crates/capi/src/plugin_registry.rs', 'if !r\n        .decoders', 'if true || !r\n        .decoders', 'plugins'),
    ('plugin_cleanup_order', 'crates/capi/src/plugin_registry.rs', '    REGISTRY.lock().unwrap().decoders.clear();', '    // deliberate: leave decoders until after encoder cleanup', 'plugins'),
    ('plugin_decoder_count', 'crates/capi/src/plugin_registry.rs', 'let n = count.min(found.len() as c_int);', 'let n = count.max(0).min(found.len() as c_int);', 'plugins'),
    ('encoder_duplicate_constraints', 'crates/capi/src/encoder.rs', 'for p in unsafe { matching(e, name) } {', 'for p in unsafe { matching(e, name) }.take(1) {', 'plugins'),
    ('encoder_range_boundary', 'crates/capi/src/encoder.rs', 'value < min', 'value <= min', 'plugins'),
    ('encoder_allowed_values', 'crates/capi/src/encoder.rs', 'if count > 0', 'if count > 100', 'plugins'),
    ('encoder_boolean_spelling', 'crates/capi/src/encoder.rs', 'b"true" | b"1"', 'b"true" | b"1" | b"TRUE"', 'plugins'),
    ('encoder_old_default', 'crates/capi/src/encoder.rs', 'if field!(p, version) < 2 {\n            1', 'if field!(p, version) < 2 {\n            0', 'plugins'),
    ('encoder_output_terminator', 'crates/capi/src/encoder.rs', 'out.add(n).write(0)', 'out.add(n).write(32)', 'plugins'),
    ('encoder_unknown_fallback', 'crates/capi/src/encoder.rs', 'unsafe { heif_encoder_set_parameter_string(e, name, value) }\n    }\n}', 'unsupported()\n    }\n}', 'plugins'),

    ('parameter_raw_range_flag', 'crates/capi/src/encoder_parameters.rs', 'have.write(have_range.into())', 'have.write(i32::from(have_range != 0))', 'encoder_parameters'),
    ('parameter_empty_array', 'crates/capi/src/encoder_parameters.rs', 'if num_values > 0 && !array.is_null()', 'if num_values >= 0 && !array.is_null()', 'encoder_parameters'),
    ('parameter_signed_count', 'crates/capi/src/encoder_parameters.rs', 'count.write(num_values)', 'count.write(num_values.max(0))', 'encoder_parameters'),
    ('parameter_string_values', 'crates/capi/src/encoder_parameters.rs', 'array.write(ptr::addr_of!((*p).value.string.valid_values).read())', 'array.write(ptr::null())', 'encoder_parameters'),
    ('parameter_alias_order', 'crates/capi/src/encoder_parameters.rs', 'have_max.write(have_range.into())', 'have_max.write(have_range.into()); if !minimum.is_null() { minimum.write(ptr::addr_of!((*p).value.integer.minimum).read()); }', 'encoder_parameters'),

    ('gimi_first_property', 'src/gimi.rs', '.find(|p| !p.raw && p.kind == *b"uuid" && p.uuid == Some(CONTENT_UUID))', '.rfind(|p| !p.raw && p.kind == *b"uuid" && p.uuid == Some(CONTENT_UUID))', 'gimi'),
    ('gimi_shared_property', 'src/gimi.rs', '.find_map(|p| p.gimi_components.clone())', '.find_map(|p| p.gimi_components.as_ref().map(|ids| Arc::new(Mutex::new(ids.lock().unwrap().clone()))))', 'gimi'),
    ('gimi_read_only_property', 'src/gimi.rs', 'ctx.properties.add(self.id, property.clone(), false)?;', 'ctx.properties.add_to_file(self.id, property.clone(), false)?;', 'gimi'),
    ('gimi_decoded_id', 'src/decoding.rs', 'image.sample.content_id = content_id;', 'image.sample.content_id = Vec::new(); let _ = content_id;', 'gimi'),
    ('gimi_embedded_nul_presence', 'crates/capi/src/gimi.rs', 'if v.is_empty() {', 'if v.is_empty() || v.first() == Some(&0) {', 'gimi'),
    ('gimi_component_terminator', 'src/gimi.rs', '.unwrap_or(data.len() - 1);', '.unwrap_or(data.len().saturating_sub(2));', 'gimi'),

    ('entity_group_id', 'crates/capi/src/entity_groups.rs', 'entity_group_id: g.id,', 'entity_group_id: g.id.wrapping_add(1),', 'entity_groups'),
    ('entity_group_filter', 'crates/capi/src/entity_groups.rs', 'g.entities.contains(&item_filter)', 'g.entities.first() == Some(&item_filter)', 'entity_groups'),
    ('entity_group_empty_result', 'crates/capi/src/entity_groups.rs', 'if groups.children == 0 {', 'if groups.groups.is_empty() {', 'entity_groups'),
    ('entity_group_pyramid_error', 'src/entity_groups.rs', 'Err(_) if kind == *b"pymd" => Ok(None),', 'Err(_) if kind == *b"altr" => Ok(None),', 'entity_groups'),
    ('entity_group_member_order', 'src/entity_groups.rs', '(0..count as usize).map(|i| read(12 + 4 * i))', '(0..count as usize).rev().map(|i| read(12 + 4 * i))', 'entity_groups'),

    ('region_signed_coordinates', 'src/regions.rs', 'value as i16 as i32', 'value as i32', 'regions'),
    ('region_polygon_minimum', 'src/regions.rs', 'if kind == 3 { 3 } else { 2 }', 'if kind == 3 { 4 } else { 2 }', 'regions'),
    ('region_partial_parse', 'src/regions.rs', 'let Some(g) = r.geometry(kind, limits, budget) else {\n                break;', 'let Some(g) = r.geometry(kind, limits, budget) else {\n                item.regions.clear(); break;', 'regions'),
    ('region_transform_x', 'src/regions.rs', 'f64::from(x) * self.a + f64::from(x) * self.b + self.tx', 'f64::from(x) * self.a + f64::from(y) * self.b + self.tx', 'regions'),
    ('region_mask_default_size', 'src/context.rs', 'geometry.width = image.ispe.0;', 'geometry.width = image.ispe.1;', 'regions'),
    ('region_reload_registry', 'src/context.rs', 'self.items = crate::items::ItemStore::reading(input.clone());', 'self.region_items.clear();\n        self.items = crate::items::ItemStore::reading(input.clone());', 'regions'),
    ('region_mask_pixels', 'crates/capi/src/regions.rs', '255\n            } else {', '254\n            } else {', 'regions'),
    ('region_mask_high_bit', 'crates/capi/src/regions.rs', '& 0x80)', '& 0x40)', 'regions'),

    ('encoding_option_default', 'crates/capi/src/encoding_options.rs', 'version: 8,\n            save_alpha_channel: 1,', 'version: 8,\n            save_alpha_channel: 0,', 'encoding_options'),
    ('encoding_option_future_version', 'crates/capi/src/encoding_options.rs', '!(1..=$max).contains(&version)', '!(1..=255).contains(&version)', 'encoding_options'),
    ('encoding_option_copy_boundary', 'crates/capi/src/encoding_options.rs', 'version >= $version', 'version > $version', 'encoding_options'),
    ('orientation_composition_order', 'src/geometry.rs', 'TABLE[first as usize - 1][second as usize - 1]', 'TABLE[second as usize - 1][first as usize - 1]', 'encoding_options'),

    ('omaf_projection_width', 'src/omaf.rs', '.map(|v| i32::from(v & 31))', '.map(|v| i32::from(v & 15))', 'omaf'),
    ('omaf_latest_property', 'src/omaf.rs', '.find(|p| !p.raw && p.kind == *b"prfr")', '.rfind(|p| !p.raw && p.kind == *b"prfr")', 'omaf'),
    ('omaf_description_value', 'src/omaf.rs', 'self.projection.store(value, Ordering::Relaxed);', 'self.projection.store(value & 31, Ordering::Relaxed);', 'omaf'),
    ('omaf_setter_property_range', 'src/omaf.rs', 'if !(0..32).contains(&value)', 'if !(0..31).contains(&value)', 'omaf'),
    ('omaf_decoded_property', 'src/decoding.rs', 'image.projection = projection;', 'image.projection = crate::omaf::FLAT; let _ = projection;', 'omaf'),

    ('sample_payload_copy', 'src/sequence_sample.rs', 'self.data.extend_from_slice(data);', 'self.data.extend(data.iter().map(|v| v ^ 1));', 'sequence_samples'),
    ('sample_empty_storage', 'src/sequence_sample.rs', 'self.data.capacity() != 0', '!self.data.is_empty()', 'sequence_samples'),
    ('sample_duration', 'crates/capi/src/sequence_sample.rs', 'v.$field.duration = duration;', 'v.$field.duration = duration.wrapping_add(1);', 'sequence_samples'),
    ('sample_timestamp_presence', 'crates/capi/src/sequence_sample.rs', 'sample.timestamp = Some(copy);', 'sample.timestamp = None; let _ = copy;', 'sequence_samples'),
    ('sample_id_empty', 'crates/capi/src/sequence_sample.rs', 'if $empty_null && v.$field.content_id.is_empty()', 'if v.$field.content_id.is_empty()', 'sequence_samples'),
    ('sample_transform_metadata', 'src/image.rs', '        )?\n        .with_budget(self.budget.clone());\n        out.warnings = self.warnings.clone();\n        out.color = self.color.try_clone()?;\n        out.sensor = self.sensor.clone();\n        out.sample = self.sample.clone();', '        )?\n        .with_budget(self.budget.clone());\n        out.warnings = self.warnings.clone();\n        out.color = self.color.try_clone()?;\n        out.sensor = self.sensor.clone();\n        out.sample = crate::sequence_sample::SampleMetadata::default();', 'sequence_samples'),
    ('unc_compression_wrapper', 'src/uncompressed_compression.rs', 'b"zlib" => 4,', 'b"zlib" => 3,', 'uncompressed_units'),
    ('unc_unit_type', 'src/uncompressed_compression.rs', 'if unit > 4 {', 'if unit > 3 {', 'uncompressed_units'),
    ('unc_unit_index', 'src/uncompressed_compression.rs', '.get(tile as usize)', '.get(0)', 'uncompressed_units'),
    ('unc_implied_offset', 'src/uncompressed_compression.rs', 'implied += size;', 'implied += 0;', 'uncompressed_units'),
    ('unc_unit_overflow', 'src/uncompressed_compression.rs', 'size >= u64::MAX - offset', 'size > u64::MAX - offset', 'uncompressed_units'),
    ('unc_range_origin', 'src/uncompressed_compression.rs', 'data.copy_within(start as usize..(start + size) as usize, 0);', 'data.copy_within(0..size as usize, 0);', 'uncompressed_units'),
    ('unc_pixel_value', 'src/uncompressed_decode.rs', 'let bytes = value.to_ne_bytes();', 'let bytes = (value ^ 1).to_ne_bytes();', 'uncompressed_pixels'),
    ('unc_row_alignment', 'src/uncompressed_decode.rs', 'bits.align(u64::from(align), start);', 'bits.align(0, start);', 'uncompressed_pixels'),
    ('unc_block_padding', 'src/uncompressed_decode.rs', 'let v = (value >> shift) & ((1u64 << n) - 1);', 'let v = (value >> shift) & ((1u64 << n) - 2);', 'uncompressed_pixels'),
    ('unc_component_format', 'src/uncompressed.rs', 'desc.datatype = i32::from(c.format);', 'desc.datatype = 0;', 'uncompressed_pixels'),
    ('unc_preferred_chroma_output', 'crates/capi/src/context.rs', 'if !chroma.is_null() && !preserve_chroma {', 'if !chroma.is_null() { let _ = preserve_chroma;', 'uncompressed_pixels'),
    ('subbyte_rgb_replication', 'src/conversion.rs', '((v as u32 * factor) >> 8) as i32', '((v as u32 * factor) >> 9) as i32', 'uncompressed_pixels'),

    ("filetype_enum", "src/brands.rs", "Supported = 1,", "Supported = 7,", "brands"),
    ("error_code", "src/error.rs", 'Self::new(5, 2001, c"NULL argument passed")', 'Self::new(2, 2001, c"NULL argument passed")', "brands"),
    ("brand_box_truncation", "src/box_probe.rs", "matches!(read_box(&mut r, 0), Err((Failure::End, _)))", "matches!(read_box(&mut r, 0), Err((Failure::Other, _)))", "brand_boxes"),
    ("brand_optional_child", "src/box_probe.rs", "if let Err((error, false)) = read_box(r, level)", "if let Err((error, _)) = read_box(r, level)", "brand_boxes"),
    ("brand_parent_boundary", "src/box_probe.rs", "if end > r.input.len() as u64", "if end > r.end as u64", "brand_boxes"),
    ("item_failed_add_id", "src/items.rs", "self.items.remove(&id);", "self.items.remove(&id);\n                let _ = self.mint();", "items"),
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
    ('cmpd_count_limit', 'src/uncompressed.rs', 'count > max_components', 'count >= max_components', 'uncompressed_config'),
    ('cmpd_uri_terminator', 'src/uncompressed.rs', '.unwrap_or(self.data.len().saturating_sub(1))', '.unwrap_or(self.data.len())', 'uncompressed_config'),
    ('uncc_pixel_limit_version', 'src/uncompressed.rs', 'limits.version >= 4', 'true', 'uncompressed_config'),
    ('uncc_profile_components', 'src/uncompressed.rs', 'b"rgb3" => (&[4, 5, 6],', 'b"rgb3" => (&[6, 5, 4],', 'uncompressed_config'),
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
    ("avc_mono_chroma", "vendor/rusty_h264-decoder/src/mb16.rs", "let mut u = vec![128u8; cdw * cdh];", "let mut u = vec![127u8; cdw * cdh];", "avc"),
    ("avc_level_prefix_limit", "vendor/rusty_h264-common/src/cavlc.rs", "if level_prefix > 15 {", "if level_prefix > 16 {", "avc"),
    ("avc_profile_gate", "src/avc_openh264.rs", "if !matches!(profile, 66 | 77 | 83 | 86 | 88 | 100) {", "if !matches!(profile, 66 | 77 | 83 | 86 | 88 | 100 | 244) {", "avc"),
    ("avc_mono_intra_cbp", "vendor/rusty_h264-decoder/src/mb16.rs", "const T: [u8; 16] = [15, 0, 7, 11, 13, 14, 3, 5, 10, 12, 1, 2, 4, 8, 6, 9];", "const T: [u8; 16] = [15, 7, 0, 11, 13, 14, 3, 5, 10, 12, 1, 2, 4, 8, 6, 9];", "avc"),
    ("avc_decoder_error_text", "src/avc.rs", 'plugin_error(0, "OpenH264 decoder error")', 'plugin_error(0, "OpenH264 decoding error")', "avc"),
    ("avc_cabac_end_of_data", "vendor/rusty_h264-decoder/src/cabac.rs", "self.over || consumed > self.end_bits\n", "self.over || consumed > self.end_bits + 8\n", "avc_errors"),
    ("avc_intra_mode_validity", "vendor/rusty_h264-decoder/src/mb16.rs", "            4..=6 => top && left && self.block_corner_ok(bx, by, top, left),\n            _ => false,\n        };\n        self.intra_invalid |= !ok;", "            4..=6 => top && left && self.block_corner_ok(bx, by, top, left),\n            _ => false,\n        };\n        let _ = ok;", "avc_errors"),
    ("avc_cavlc_end_of_slice", "vendor/rusty_h264-decoder/src/mb16.rs", "                if used > r.stop_pos() {", "                if used > r.stop_pos() + 8 {", "avc_errors"),
    ("avc_coeff_token_fallback", "vendor/rusty_h264-common/src/cavlc.rs", "c.skip(if NC_TABLE[(nc as usize).min(16)] == 3 { 6 } else { 8 })?;", "c.skip(if NC_TABLE[(nc as usize).min(16)] == 3 { 6 } else { 7 })?;", "avc_errors"),
    ("avc_level_gate", "src/avc_openh264.rs", "let (max_fs, max_dpb) = level_limits(level, constraint[3]).ok_or(Rejected)?;", "let (max_fs, max_dpb) = level_limits(level, constraint[3]).unwrap_or((36864, 184320));", "avc_errors"),
    ("avc_hrd_return_code", "src/avc_openh264.rs", "        Err(ReadError(code)) => code,", "        Err(ReadError(_)) => 0,", "avc_errors"),
    ("avc_scaling_factor_wrap", "vendor/rusty_h264-common/src/transform.rs", "(((weight * NORM_ADJUST[m][POS_GROUP_FLAT[idx]]) << (qp / 6)) as u16) as i32", "(weight * NORM_ADJUST[m][POS_GROUP_FLAT[idx]]) << (qp / 6)", "avc"),
    ("avc_scaling_qp51", "vendor/rusty_h264-common/src/transform.rs", "    if qp >= 51 {\n        return 0;\n    }", "    if qp >= 52 {\n        return 0;\n    }", "avc"),
    ("avc_coefficient_wrap", "vendor/rusty_h264-common/src/transform.rs", "            wrap16((raster[i].wrapping_mul(self.ls[i]).wrapping_add(self.add)) >> self.sr)", "            (raster[i].wrapping_mul(self.ls[i]).wrapping_add(self.add)) >> self.sr", "avc_errors"),
    ("avc_early_construction", "src/avc_openh264.rs", "            6 | 9 if pending_slices => {", "            6 if pending_slices => {", "avc_errors"),
    ("avc_macroblock_count", "vendor/rusty_h264-decoder/src/lib.rs", "        if pic.mb_count != pic.total_mb {\n            return Ok(None); // picture not yet complete", "        if pic.next_mb < pic.total_mb {\n            return Ok(None); // picture not yet complete", "avc_errors"),
    ("avc_cr_qp_offset", "vendor/rusty_h264-decoder/src/lib.rs", "fd.set_chroma_qp_offset_cr(pps.second_chroma_qp_index_offset);", "fd.set_chroma_qp_offset_cr(pps.chroma_qp_index_offset);", "avc_errors"),
    ("error_field_order", "crates/capi/src/lib.rs", "pub code: c_int,\n    pub subcode: c_int,", "pub subcode: c_int,\n    pub code: c_int,", "abi"),
]


def execute(command, cwd, env, log):
    run = subprocess.run(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=1200)
    log.write_bytes(run.stdout)
    return run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-build", required=True)
    parser.add_argument("--plugin-reference-build", default=".build/reference-plugins")
    parser.add_argument("--av1-reference-build", default=".build/reference-av1")
    parser.add_argument("--jpeg2000-reference-build", default=".build/reference-jpeg2000")
    parser.add_argument("--jpeg-reference-build", default=".build/reference-jpeg")
    parser.add_argument("--avc-reference-build", default=".build/reference-avc")
    parser.add_argument("--candidate", default="target/release/libheifer.so")
    parser.add_argument("--output", default=".build/mutations-report.json")
    parser.add_argument("--only", choices=[m[0] for m in MUTATIONS], action="append", help="Run selected defects; default runs the complete mutation set")
    parser.add_argument("--shard", help="K/N: run the K-th (1-based) of N contiguous slices of the selected defects; the union of all N shards is the full set")
    args = parser.parse_args()
    mutations = [m for m in MUTATIONS if args.only is None or m[0] in args.only]
    if args.shard:
        index, count = map(int, args.shard.split("/"))
        if not 1 <= index <= count:
            raise SystemExit("--shard must be K/N with 1 <= K <= N")
        # Contiguous slices keep defects that share a suite together, so each
        # shard runs few baselines.
        mutations = mutations[len(mutations) * (index - 1) // count:len(mutations) * index // count]
    root = Path.cwd()
    evidence = root / ".build/mutations"
    evidence.mkdir(parents=True, exist_ok=True)
    reference = str(Path(args.reference_build).resolve())
    candidate = str(Path(args.candidate).resolve())
    def oracle(suite):
        if suite.startswith("jpeg2000_"):
            return str(Path(args.jpeg2000_reference_build).resolve())
        if suite in ("avc", "avc_errors"):
            return str(Path(args.avc_reference_build).resolve())
        if suite.startswith("jpeg_"):
            return str(Path(args.jpeg_reference_build).resolve())
        if suite in ('av1', 'av1_limits', 'mini_reader'):
            return str(Path(args.av1_reference_build).resolve())
        return str(Path(args.plugin_reference_build).resolve()) if suite == "dynamic_plugins" else reference
    # Baselines must pass on this tree before a rejected mutant is meaningful.
    for suite in dict.fromkeys(m[4] for m in mutations if m[4] != "abi"):
        run = execute([sys.executable, f"tools/test_{suite}.py", "--reference-build", oracle(suite), "--candidate", candidate, *(["--work", str(evidence / "decode")] if suite.startswith("decode_") or suite in ("hevc_limits", "jpeg2000_tiles", "jpeg2000_debug", "jpeg2000_pixels", "jpeg2000_errors", "jpeg2000_sampling") else []), "--output", str(evidence / f"baseline-{suite}.json")], root, os.environ, evidence / f"baseline-{suite}.log")
        if run.returncode:
            raise SystemExit(f"Baseline {suite} failed; see {evidence}")
    run = execute(["cargo", "test", "--locked", "-p", "libheifer-capi", "--test", "abi"], root, dict(os.environ, CARGO_TARGET_DIR=str(root / ".build/abi-baseline")), evidence / "baseline-abi.log")
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
                    run = execute([sys.executable, f"tools/test_{suite}.py", "--reference-build", oracle(suite), "--candidate", str(library), *(["--work", str(evidence / "decode")] if suite.startswith("decode_") or suite in ("hevc_limits", "jpeg2000_tiles", "jpeg2000_debug", "jpeg2000_pixels", "jpeg2000_errors", "jpeg2000_sampling") else []), "--output", str(report_path)], root, os.environ, evidence / f"{name}.log")
                    report = json.loads(report_path.read_text()) if report_path.exists() else {}
                    record["mismatches"] = report.get("mismatches", 0)
                    record["process_failures"] = report.get("process_failures", [])
                    record["detected"] = run.returncode == 1 and record["mismatches"] > 0 and not record["process_failures"]
                results.append(record)
                print(json.dumps(record), flush=True)
            finally:
                path.write_text(original)
    report = {"scope": f"{len(mutations)} deliberate defects; not comprehensive mutation coverage", "shard": args.shard, "mutations": results, "all_detected": all(r["detected"] for r in results)}
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
    if not report["all_detected"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
