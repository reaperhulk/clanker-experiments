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
        "#include <libheif/heif.h>\n#include <stddef.h>\n#include <stdio.h>\nint main(void) {\n",
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
    layout!(heif_error, heifer::HeifError, code, subcode, message);
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
