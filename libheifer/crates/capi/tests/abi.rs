// SPDX-License-Identifier: LGPL-3.0-or-later
// Compile the C side against untouched reference headers, independently of Rust.
use libheifer::color::*;
use std::{
    fs,
    mem::{align_of, offset_of, size_of},
    path::Path,
    process::Command,
};

#[test]
fn public_structs_match_original_header_layouts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let work = root
        .join("target")
        .join(format!("abi-{}", std::process::id()));
    fs::create_dir_all(&work).unwrap();
    let generated = work.join("include/libheif");
    fs::create_dir_all(&generated).unwrap();
    let version =
        fs::read_to_string(root.join("tests/upstream/libheif/api/libheif/heif_version.h.in"))
            .unwrap()
            .replace("@PROJECT_VERSION_MAJOR@", "1")
            .replace("@PROJECT_VERSION_MINOR@", "23")
            .replace("@PROJECT_VERSION_PATCH@", "4")
            .replace("@PLUGIN_DIRECTORY@", "");
    fs::write(generated.join("heif_version.h"), version).unwrap();
    let source = work.join("layout.c");
    let binary = work.join("layout");
    let mut c = String::from(
        "#include <libheif/heif.h>\n#include <libheif/heif_properties.h>\n#include <libheif/heif_components.h>\n#include <libheif/heif_tai_timestamps.h>\n#include <libheif/heif_sequences.h>\n#include <libheif/heif_uncompressed.h>\n#include <libheif/heif_entity_groups.h>\n#include <libheif/heif_plugin.h>\n#include <stddef.h>\n#include <stdio.h>\nint main(void) {\n",
    );
    let mut expected = String::new();
    macro_rules! layout {
        ($c:ident, $rust:ty, $($field:ident),+ $(,)?) => {{
            c.push_str(&format!("printf(\"%zu %zu\", sizeof({0}), _Alignof({0}));\n", stringify!($c)));
            expected.push_str(&format!("{} {}", size_of::<$rust>(), align_of::<$rust>()));
            $(
                c.push_str(&format!("printf(\" %zu\", offsetof({}, {}));\n", stringify!($c), stringify!($field)));
                expected.push_str(&format!(" {}", offset_of!($rust, $field)));
            )+
            c.push_str("puts(\"\");\n");
            expected.push('\n');
        }};
    }
    layout!(
        heif_encoder_plugin,
        heifer::EncoderPlugin,
        plugin_api_version,
        compression_format,
        id_name,
        priority,
        supports_lossy_compression,
        supports_lossless_compression,
        get_plugin_name,
        init_plugin,
        cleanup_plugin,
        new_encoder,
        free_encoder,
        set_parameter_quality,
        get_parameter_quality,
        set_parameter_lossless,
        get_parameter_lossless,
        set_parameter_logging_level,
        get_parameter_logging_level,
        list_parameters,
        set_parameter_integer,
        get_parameter_integer,
        set_parameter_boolean,
        get_parameter_boolean,
        set_parameter_string,
        get_parameter_string,
        query_input_colorspace,
        encode_image,
        get_compressed_data,
        query_input_colorspace2,
        query_encoded_size,
        minimum_required_libheif_version,
        start_sequence_encoding,
        encode_sequence_frame,
        end_sequence_encoding,
        get_compressed_data2,
        does_indicate_keyframes
    );
    layout!(
        heif_decoder_plugin,
        heifer::DecoderPlugin,
        plugin_api_version,
        get_plugin_name,
        init_plugin,
        deinit_plugin,
        does_support_format,
        new_decoder,
        free_decoder,
        push_data,
        decode_image,
        set_strict_decoding,
        id_name,
        decode_next_image,
        minimum_required_libheif_version,
        does_support_format2,
        new_decoder2,
        push_data2,
        flush_data,
        decode_next_image2
    );
    layout!(
        heif_decoder_plugin_options,
        heifer::DecoderPluginOptions,
        format,
        strict_decoding,
        num_threads,
        limits
    );
    layout!(
        heif_decoder_plugin_compressed_format_description,
        heifer::CompressedFormatDescription,
        format
    );
    c.push_str(
        "printf(\"%zu %zu\", sizeof(heif_encoder_parameter), _Alignof(heif_encoder_parameter));\n",
    );
    expected.push_str(&format!(
        "{} {}",
        size_of::<heifer::EncoderParameter>(),
        align_of::<heifer::EncoderParameter>()
    ));
    c.push_str("printf(\" %zu\", offsetof(heif_encoder_parameter, version));\n");
    expected.push_str(&format!(
        " {}",
        offset_of!(heifer::EncoderParameter, version)
    ));
    c.push_str("printf(\" %zu\", offsetof(heif_encoder_parameter, name));\n");
    expected.push_str(&format!(" {}", offset_of!(heifer::EncoderParameter, name)));
    c.push_str("printf(\" %zu\", offsetof(heif_encoder_parameter, type));\n");
    expected.push_str(&format!(" {}", offset_of!(heifer::EncoderParameter, kind)));
    c.push_str("printf(\" %zu\", offsetof(heif_encoder_parameter, integer));\n");
    expected.push_str(&format!(" {}", offset_of!(heifer::EncoderParameter, value)));
    c.push_str("printf(\" %zu\", offsetof(heif_encoder_parameter, has_default));\n");
    expected.push_str(&format!(
        " {}",
        offset_of!(heifer::EncoderParameter, has_default)
    ));
    c.push_str("puts(\"\");\n");
    expected.push('\n');
    c.push_str("printf(\"%zu %zu\", sizeof(heif_plugin_info), _Alignof(heif_plugin_info));\n");
    expected.push_str(&format!(
        "{} {}",
        size_of::<heifer::PluginInfo>(),
        align_of::<heifer::PluginInfo>()
    ));
    c.push_str("printf(\" %zu\", offsetof(heif_plugin_info, version));\n");
    expected.push_str(&format!(" {}", offset_of!(heifer::PluginInfo, version)));
    c.push_str("printf(\" %zu\", offsetof(heif_plugin_info, type));\n");
    expected.push_str(&format!(" {}", offset_of!(heifer::PluginInfo, kind)));
    c.push_str("printf(\" %zu\", offsetof(heif_plugin_info, plugin));\n");
    expected.push_str(&format!(" {}", offset_of!(heifer::PluginInfo, plugin)));
    c.push_str("printf(\" %zu\", offsetof(heif_plugin_info, internal_handle));\n");
    expected.push_str(&format!(
        " {}",
        offset_of!(heifer::PluginInfo, internal_handle)
    ));
    c.push_str("puts(\"\");\n");
    expected.push('\n');
    layout!(heif_writer, heifer::Writer, writer_api_version, write);
    layout!(
        heif_entity_group,
        heifer::EntityGroup,
        entity_group_id,
        entity_group_type,
        entities,
        num_entities
    );
    layout!(
        heif_encoding_options,
        heifer::EncodingOptions,
        version,
        save_alpha_channel,
        macOS_compatibility_workaround,
        save_two_colr_boxes_when_ICC_and_nclx_available,
        output_nclx_profile,
        macOS_compatibility_workaround_no_nclx_profile,
        image_orientation,
        color_conversion_options,
        prefer_uncC_short_form,
        unci_parameters
    );
    layout!(
        heif_sequence_encoding_options,
        heifer::SequenceEncodingOptions,
        version,
        output_nclx_profile,
        color_conversion_options,
        gop_structure,
        keyframe_distance_min,
        keyframe_distance_max,
        save_alpha_channel,
        content_kind
    );
    layout!(
        heif_unci_image_parameters,
        heifer::UnciParameters,
        version,
        image_width,
        image_height,
        tile_width,
        tile_height,
        compression
    );
    layout!(
        heif_tai_clock_info,
        libheifer::tai::ClockInfo,
        version,
        time_uncertainty,
        clock_resolution,
        clock_drift_rate,
        clock_type
    );
    layout!(
        heif_tai_timestamp_packet,
        libheifer::tai::Timestamp,
        version,
        tai_timestamp,
        synchronization_state,
        timestamp_generation_failure,
        timestamp_is_modified
    );
    layout!(
        heif_complex32,
        libheifer::components::Complex32,
        real,
        imaginary
    );
    layout!(
        heif_complex64,
        libheifer::components::Complex64,
        real,
        imaginary
    );
    layout!(
        heif_security_limits,
        heifer::SecurityLimits,
        version,
        max_image_size_pixels,
        max_number_of_tiles,
        max_bayer_pattern_pixels,
        max_items,
        max_color_profile_size,
        max_memory_block_size,
        max_components,
        max_iloc_extents_per_item,
        max_size_entity_group,
        max_children_per_box,
        max_total_memory,
        max_sample_description_box_entries,
        max_sample_group_description_box_entries,
        max_sequence_frames,
        max_number_of_file_brands,
        max_bad_pixels,
        max_iso23001_17_pixel_size_bytes,
        parent
    );
    layout!(
        heif_depth_representation_info,
        heifer::DepthRepresentationInfo,
        version,
        has_z_near,
        has_z_far,
        has_d_min,
        has_d_max,
        z_near,
        z_far,
        d_min,
        d_max,
        depth_representation_type,
        disparity_reference_view,
        depth_nonlinear_representation_model_size,
        depth_nonlinear_representation_model
    );
    layout!(
        heif_property_user_description,
        heifer::UserDescription,
        version,
        lang,
        name,
        description,
        tags
    );
    layout!(
        heif_camera_intrinsic_matrix,
        libheifer::camera::IntrinsicMatrix,
        focal_length_x,
        focal_length_y,
        principal_point_x,
        principal_point_y,
        skew
    );
    layout!(
        heif_bayer_pattern_pixel,
        libheifer::sensor::BayerPixel,
        component_id,
        component_gain
    );
    layout!(heif_bad_pixel, libheifer::sensor::BadPixel, row, column);
    layout!(heif_error, heifer::HeifError, code, subcode, message);
    layout!(
        heif_decoding_options,
        heifer::DecodingOptions,
        version,
        ignore_transformations,
        start_progress,
        on_progress,
        end_progress,
        progress_user_data,
        convert_hdr_to_8bit,
        strict_decoding,
        decoder_id,
        color_conversion_options,
        cancel_decoding,
        color_conversion_options_ext,
        ignore_sequence_editlist,
        output_image_nclx_profile,
        num_library_threads,
        num_codec_threads,
        autocorrect_broken_input,
        output_image_nclx_profile_passthrough
    );
    layout!(
        heif_color_profile_nclx,
        NclxProfile,
        version,
        color_primaries,
        transfer_characteristics,
        matrix_coefficients,
        full_range_flag,
        color_primary_red_x,
        color_primary_red_y,
        color_primary_green_x,
        color_primary_green_y,
        color_primary_blue_x,
        color_primary_blue_y,
        color_primary_white_x,
        color_primary_white_y
    );
    layout!(
        heif_color_conversion_options,
        ColorConversionOptions,
        version,
        preferred_chroma_downsampling_algorithm,
        preferred_chroma_upsampling_algorithm,
        only_use_preferred_chroma_algorithm
    );
    layout!(
        heif_color_conversion_options_ext,
        ColorConversionOptionsExt,
        version,
        alpha_composition_mode,
        background_red,
        background_green,
        background_blue,
        secondary_background_red,
        secondary_background_green,
        secondary_background_blue,
        checkerboard_square_size
    );
    layout!(
        heif_content_light_level,
        ContentLightLevel,
        max_content_light_level,
        max_pic_average_light_level
    );
    layout!(
        heif_mastering_display_colour_volume,
        MasteringDisplayColourVolume,
        display_primaries_x,
        display_primaries_y,
        white_point_x,
        white_point_y,
        max_display_mastering_luminance,
        min_display_mastering_luminance
    );
    layout!(
        heif_decoded_mastering_display_colour_volume,
        DecodedMasteringDisplayColourVolume,
        display_primaries_x,
        display_primaries_y,
        white_point_x,
        white_point_y,
        max_display_mastering_luminance,
        min_display_mastering_luminance
    );
    layout!(
        heif_ambient_viewing_environment,
        AmbientViewingEnvironment,
        ambient_illumination,
        ambient_light_x,
        ambient_light_y
    );
    c.push_str("printf(\"%zu\\n\", offsetof(heif_encoder_parameter, integer.default_value));\n");
    expected.push_str(&format!(
        "{}\n",
        offset_of!(heifer::EncoderParameter, value)
            + offset_of!(heifer::IntegerParameter, default_value)
    ));
    c.push_str(
        "printf(\"%zu\\n\", offsetof(heif_encoder_parameter, integer.have_minimum_maximum));\n",
    );
    expected.push_str(&format!(
        "{}\n",
        offset_of!(heifer::EncoderParameter, value)
            + offset_of!(heifer::IntegerParameter, have_minimum_maximum)
    ));
    c.push_str("printf(\"%zu\\n\", offsetof(heif_encoder_parameter, integer.minimum));\n");
    expected.push_str(&format!(
        "{}\n",
        offset_of!(heifer::EncoderParameter, value) + offset_of!(heifer::IntegerParameter, minimum)
    ));
    c.push_str("printf(\"%zu\\n\", offsetof(heif_encoder_parameter, integer.maximum));\n");
    expected.push_str(&format!(
        "{}\n",
        offset_of!(heifer::EncoderParameter, value) + offset_of!(heifer::IntegerParameter, maximum)
    ));
    c.push_str("printf(\"%zu\\n\", offsetof(heif_encoder_parameter, integer.valid_values));\n");
    expected.push_str(&format!(
        "{}\n",
        offset_of!(heifer::EncoderParameter, value)
            + offset_of!(heifer::IntegerParameter, valid_values)
    ));
    c.push_str("printf(\"%zu\\n\", offsetof(heif_encoder_parameter, integer.num_valid_values));\n");
    expected.push_str(&format!(
        "{}\n",
        offset_of!(heifer::EncoderParameter, value)
            + offset_of!(heifer::IntegerParameter, num_valid_values)
    ));
    c.push_str("printf(\"%zu\\n\", offsetof(heif_encoder_parameter, string.default_value));\n");
    expected.push_str(&format!(
        "{}\n",
        offset_of!(heifer::EncoderParameter, value)
            + offset_of!(heifer::StringParameter, default_value)
    ));
    c.push_str("printf(\"%zu\\n\", offsetof(heif_encoder_parameter, string.valid_values));\n");
    expected.push_str(&format!(
        "{}\n",
        offset_of!(heifer::EncoderParameter, value)
            + offset_of!(heifer::StringParameter, valid_values)
    ));
    c.push_str("printf(\"%zu\\n\", offsetof(heif_encoder_parameter, boolean.default_value));\n");
    expected.push_str(&format!(
        "{}\n",
        offset_of!(heifer::EncoderParameter, value)
            + offset_of!(heifer::BooleanParameter, default_value)
    ));
    c.push_str("return 0; }\n");
    fs::write(&source, c).unwrap();
    assert!(
        Command::new("cc")
            .arg("-std=c11")
            .arg("-Werror")
            .arg("-I")
            .arg(root.join("tests/upstream/libheif/api"))
            .arg("-I")
            .arg(work.join("include"))
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .status()
            .unwrap()
            .success()
    );
    let result = Command::new(&binary).output().unwrap();
    assert!(result.status.success());
    assert_eq!(String::from_utf8(result.stdout).unwrap(), expected);
    fs::remove_dir_all(work).unwrap();
}
