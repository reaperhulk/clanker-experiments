# Local compatibility patch

Base: zlib-rs 0.6.8 from crates.io, archive SHA-256 `b268e58e7c693d7c271f93ffc4ba3b380412554231c85bf61ca7af91042a4112`.

Preserve the original inflate diagnostic when the fast loop enters the Bad state.
The original code overwrites it with "repeated call with bad state", even on the
first call. This patch preserves the cause of malformed-stream failures.

The packaged manifest allows dead-code/unused-import warnings from unexposed
optional internals, matching the warning suppression applied by Cargo to registry
dependencies. No behavioral or safety lint is disabled.

Default features are disabled; only std/rust-allocator are enabled. No native
codec, build script, C allocator backend, or C implementation is used.
