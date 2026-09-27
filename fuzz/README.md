# Parser mutation targets

Run after deterministic tests. Pure Rust; no `libfuzzer-sys`, native runtime, FFI,
nightly toolchain or additional crates. This is a reproducible mutation smoke
fuzzer, **not coverage-guided fuzzing** and not a proof of absence of panics.
The driver accepts an iteration count and optional nonzero PRNG seed. Seed it
with checked-in official recordings and upstream-generated packets. It also
repairs CRCs/frame boundaries to reach deeper parser paths.

```sh
cargo test --workspace
cargo run --manifest-path fuzz/Cargo.toml --release --bin command -- 10000
cargo run --manifest-path fuzz/Cargo.toml --release --bin data -- 10000
cargo run --manifest-path fuzz/Cargo.toml --release --bin lvx_reader -- 10000
cargo run --manifest-path fuzz/Cargo.toml --release --bin lvx_frame -- 10000
```

Coverage-guided libFuzzer integration would introduce `libfuzzer-sys` and requires
prior discussion under the user's dependency constraint. Arbitrary file inputs
use a 1 MiB frame allocation ceiling during these runs. Future scheduled fuzzing
should save failing inputs and retain coverage measurements.
