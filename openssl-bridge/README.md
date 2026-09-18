# openssl-bridge

Independent Rust abstractions for the OpenSSL operations used by
`pyca/cryptography`, using header-generated FFI for OpenSSL, LibreSSL, AWS-LC,
and BoringSSL. The included patch replaces both Rust OpenSSL crates throughout
the pinned cryptography Rust implementation. The full acceptance matrix passes;
see [the results and exact scope](validation/acceptance/README.md). The API is
operation-oriented rather than a drop-in copy of rust-openssl.

## Acceptance requirements

1. Replace the `openssl` and `openssl-sys` dependencies without depending on or
   re-exporting either crate. Keep cryptography's Rust 1.83 minimum.
2. Encapsulate FFI ownership, lifetimes, state, checked lengths, and backend
   differences. Never require callers to prove output buffer sizes with unsafe.
3. Integrate the entire Rust surface used by cryptography; run its existing
   Python and Rust suites, including pinned Wycheproof and X.509 Limbo vectors,
   on the backend/version matrix. Do not hide regressions behind new skips.
4. Include the reproducible cryptography patch and validation evidence in the
   primary PR. Do not open it before the complete replacement and test matrix pass.
5. After the primary task is complete, replace the CFFI/TLS surface and validate
   against pyOpenSSL's test suite, recording that integration patch separately.

## API design

- Generated FFI is confined to `openssl-bridge-sys` and the abstraction crate.
- Public APIs expose values and slices rather than foreign pointers.
- Each native allocation has exactly one owning wrapper and a matching destructor.
- Cloning native state is explicitly fallible (`try_clone`).
- Digest/MAC finalization consumes the context. Failed updates poison it.
- Non-thread-safe native contexts are not marked `Sync` by default.
- Length conversions and output capacities are checked before entering C.
- AEAD plaintext must not escape one-shot decryption before tag verification.
- Stateful cipher APIs must validate legal transitions, including AEAD setup.
- Backend capability differences remain explicit, with errors for unavailable
  algorithms rather than substituting different algorithms or implementations.

## Build

The workspace contains two separate Cargo packages: `openssl-bridge` is the
safe dependency used by cryptography, and `openssl-bridge-sys` generates the
private native bindings. The integration patch uses a path dependency so the
exact code under review is built:

```toml
[dependencies]
openssl-bridge = { path = "../clanker-experiments/openssl-bridge/crates/bridge" }
```

Requires Rust, libclang for bindgen, and a supported backend installation.
Set `OPENSSL_DIR`, optionally `OPENSSL_INCLUDE_DIR`, `OPENSSL_LIB_DIR`, and
`OPENSSL_STATIC`. Without an explicit installation, discovery uses pkg-config.

The baseline cryptography revision is recorded in `compatibility/sources.json`.
Cryptography's `AGENTS.md` requires builds and checks through nox sessions.

## Integration and results

[GitHub Actions](../.github/workflows/openssl-bridge.yml) runs standalone Cargo
tests, formatting, and Clippy against system OpenSSL, OpenSSL 4, LibreSSL,
BoringSSL, and AWS-LC, including Rust 1.83 checks. A separate matrix applies the
integration patch to the pinned cryptography revision and runs its full
`nox -e local` check across all 17 recorded backend/version/configuration rows.
It includes Wycheproof and X.509 Limbo, rejects missing tests and new upstream
skips, and uploads logs, source identities, and JUnit results even after failure.
Native sources, test inputs, and action revisions are pinned; native installations
are cached by their source and build recipe. The aggregate `openssl-bridge CI`
check passes only when both matrices pass. The five core integration rows
also require the full pyOpenSSL suite with CFFI physically uninstalled.

Apply [cryptography.patch](compatibility/cryptography.patch) to the pinned
revision in [sources.json](compatibility/sources.json). Keep the two repositories
next to one another as described in [the integration instructions](compatibility/README.md).
The migration includes its API changes, changelog entries, and regression tests.

[SAFETY.md](SAFETY.md) describes ownership, state, bounds, secret storage,
concurrency, and backend assumptions. The checked-in tests and integration
results provide evidence for these invariants; they are not an independent audit.
The subsequent CFFI/TLS replacement also passes its complete acceptance gate:
all 17 cryptography rows, eight standalone crate jobs, and five full pyOpenSSL
suites with CFFI removed. See [the TLS acceptance report](validation/tls/acceptance/README.md)
and [pyopenssl.patch](compatibility/pyopenssl.patch). The original primary
acceptance patch is preserved as `compatibility/cryptography-primary.patch`.
