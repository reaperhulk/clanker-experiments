# Rust-only rav1d integration

Upstream: memorysafety/rav1d v1.1.0, commit
`782dab2135ea64a057c097088a13eb8ed3cc3320` (BSD-2-Clause).

Only Rust implementation sources and license/notices are vendored. The assembly
features, assembly/C build script and cc/nasm dependencies are removed, including
from all-features builds. The C compatibility entry points retain Rust code but
lose unmangled external symbols, preventing accidental dav1d ABI exports.
A safe internal API owns decoder lifetime and copies complete active sample
planes. The core does not invoke foreign decoder APIs or native codec libraries.
Native dav1d remains an independent test oracle only.

Validation and dependency review are in progress; this is not a conformance claim.
