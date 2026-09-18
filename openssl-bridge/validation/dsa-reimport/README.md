# Repeated DSA parameter import

The change retains at most 32 exact, fully validated public parameter groups.
It does not reduce primality checks on misses or bypass public-key validation.
The measurement imports parameters, builds a public key, and verifies a pinned
Wycheproof signature on each iteration. Every process includes an initial cache
miss. Each thread performs 12 iterations.

Three interleaved A/B samples show the following median wall-clock seconds:

| Group | Threads | Before | After | Reduction |
|---|---:|---:|---:|---:|
| 2048/224 | 1 | 2.919 | 0.355 | 87.8% |
| 2048/224 | 4 | 4.579 | 0.421 | 90.8% |
| 2048/256 | 1 | 3.106 | 0.337 | 89.2% |
| 2048/256 | 4 | 4.847 | 0.509 | 89.5% |
| 3072/256 | 1 | 17.835 | 1.480 | 91.7% |
| 3072/256 | 4 | 24.175 | 2.505 | 89.6% |

Other builds and tests ran concurrently. Raw variability, source and binary
hashes, CPU, compiler, native backend, and corpus revisions are in report.json.
These timings establish repeated-import behavior; they are not a general DSA
throughput claim. Allocation and instruction counts were not measured. The cache
retains at most 33,792 public payload bytes plus collection metadata.

Reproduce with `cargo build --release --locked --example dsa_reimport`, using
OpenSSL 4.0.2 and Rust 1.90.0. Preserve the executable from each recorded source
revision, then alternate `dsa_reimport 1 12` and `dsa_reimport 4 12` for each
revision. Use isolated checkouts and target directories. An initial shared-target
measurement was rejected because the candidate artifact did not contain the
cache; the recorded results use a checked, isolated build.
