# Safety invariants

This document records the current implementation's invariants. It is not an
independent audit or a claim that the complete requested API has been implemented.

## Ownership and lifetimes

The safe crate does not expose foreign pointers. Native allocations enter a
`NonNull` owning wrapper immediately, before any subsequent fallible operation.
Each wrapper calls exactly its matching native destructor. Initialization failure
therefore drops partially initialized allocations rather than leaking them.

Digest descriptors come exclusively from backend getters returning immutable
process-lifetime descriptors. There is no safe constructor from an arbitrary
pointer. Keys are immutable after construction; every signature, verification,
and exchange operation allocates its own operation context.

## Context state

Hash, HMAC, and CMAC finalization consumes the owning context. Native context
copying is fallible and never hidden in a `Clone` implementation. A failed update
poisons the context. Further updates, copying, and finalization return errors.
An XOF tracks whether squeezing started and refuses further absorption or a
second form of finalization.

Stateful conventional ciphers select the algorithm, direction, key, IV, and
padding at construction. No uninitialized cipher context, arbitrary control,
unchecked update, or operation after finalization is public. AEAD uses a separate
API and cannot be mistaken for an unauthenticated conventional cipher.

## Bounds and native writes

Slice lengths are checked against native integer types before FFI. Digest and
MAC output sizes come from the selected algorithm. Native in/out size parameters
receive the actual destination capacity; returned lengths are checked as well.

Conventional cipher output capacity includes a **full block** of slack, except
for stream modes. This covers intermediate writes, not just reported output.
The regression test enumerates partial-block splits in padded CBC decryption and
checks canaries around the exact supplied output slice. LibreSSL writes the
withheld block before determining the returned length, including on an empty
update. Using only `input.len() + block_size - 1` failed that test.

The AEAD implementation uses only GCM, whose update writes exactly the input
length, and checks the reported length. Authentication failure erases the pending
plaintext; callers only receive plaintext after successful tag verification.

## Concurrency and secrets

Explicit Send/Sync implementations are limited to descriptors and contexts whose
shared methods do not mutate native state. Operation methods requiring mutation
take `&mut self` or consume the owner. Mutable cipher contexts are not Sync.

Secret Curve25519 exports and shared secrets erase their storage on drop and
implement neither Debug nor Clone. X25519 rejects an all-zero shared secret even
if a backend were to return one as success. Ed25519 signing and verifying keys
are separate types, distinct from X25519 keys.

## Backend assumptions and validation

Bindgen reads the selected installation's headers. No struct layout or native
function signature is hand-transcribed. The small C shim evaluates native macros
under those same headers. The safe wrappers rely on each native implementation
honoring its documented pointer, ownership, and length contracts.

Known-answer vectors, wrong signatures/tags, split updates, boundary checks,
compile-fail lifecycle examples, and the upstream integration suites provide
different kinds of evidence. Passing one category does not substitute for another.

## RSA construction and decryption

Private RSA imports own every component separately until each successful native
set0 transfer. Constructors always require complete positive components, bounded
bit lengths, valid ranges, odd private factors, and n = p * q. Full validation
also invokes the native mathematical key check. Explicit structural validation
omits primality and exponent consistency checks, preserving cryptography's
existing validation-skip option without allowing incomplete native objects.
Public imports preserve cryptography's permissive modulus handling; native
operations can reject mathematically unsuitable public parameters.

Signature and encryption padding have distinct types. Automatic PSS salt
recovery is available only for verification. OAEP label ownership is transferred
only after successful native configuration; empty labels use NULL because
LibreSSL does not retain a zero-length allocation. Private exports and decrypted
plaintext erase their owned initialized bytes when dropped.

The caller-buffer decryption method validates capacity, erases failed native
output, and erases unused output bytes before returning. Cryptography retains
its allocation step for both successful and failed PKCS1 v1.5 decryption.
This is not a claim of constant-time PKCS1 v1.5 decryption or padding-oracle
resistance on backends without native mitigations.

Configuration discovery checks final preprocessor state, including macros that
were defined empty or later undefined. Clippy exceptions for bindgen bitfield
patterns are confined to generated bindings. ABI-dependent integer conversions
remain checked even on forks where the source and destination types coincide.
