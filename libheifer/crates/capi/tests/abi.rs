// SPDX-License-Identifier: LGPL-3.0-or-later
// Compile the C side against untouched reference headers, independently of Rust.
use std::{
    fs,
    mem::{align_of, offset_of, size_of},
    path::Path,
    process::Command,
};

#[test]
fn error_struct_matches_original_header_layout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let work = root
        .join("target")
        .join(format!("abi-{}", std::process::id()));
    fs::create_dir_all(&work).unwrap();
    let source = work.join("layout.c");
    let binary = work.join("layout");
    fs::write(
        &source,
        r#"
        #include <libheif/heif_error.h>
        #include <stddef.h>
        #include <stdio.h>
        int main(void) {
            printf("%zu %zu %zu %zu %zu", sizeof(heif_error), _Alignof(heif_error),
                   offsetof(heif_error, code), offsetof(heif_error, subcode),
                   offsetof(heif_error, message));
        }
    "#,
    )
    .unwrap();
    assert!(
        Command::new("cc")
            .arg("-std=c11")
            .arg("-Werror")
            .arg("-I")
            .arg(root.join("tests/upstream/libheif/api"))
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .status()
            .unwrap()
            .success()
    );
    let result = Command::new(&binary).output().unwrap();
    assert!(result.status.success());
    let actual = format!(
        "{} {} {} {} {}",
        size_of::<heifer::HeifError>(),
        align_of::<heifer::HeifError>(),
        offset_of!(heifer::HeifError, code),
        offset_of!(heifer::HeifError, subcode),
        offset_of!(heifer::HeifError, message)
    );
    assert_eq!(String::from_utf8(result.stdout).unwrap(), actual);
    fs::remove_dir_all(work).unwrap();
}
