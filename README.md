# zk-scribble

Trace mutation fuzzer for [Hekate](https://github.com/oumuamua-labs/hekate) ZK programs and chiplets.

Tampers your valid trace, runs preflight checks,
panics if the tamper goes undetected. If scribble
doesn't panic, your constraints have a hole.

## What it does

- Flips bits, swaps rows, toggles selectors, injects out-of-range values
- Validates using preflight (row-by-row constraint eval + bus multiset check)
- Proptest integration, shrinks failures to the smallest mutation that escapes your AIR
- Runs in debug mode in seconds, not minutes

## What it doesn't do

- **No proofs.** Scribble never calls the prover or verifier. It checks your constraints on the concrete trace, not through the ZK pipeline. This is why it's fast.
- **No protocol-level testing.** Transcript binding, Fiat-Shamir, evaluation arguments, Brakedown, those need the real prover/verifier e2e tests.
- **No soundness guarantees.** Passing scribble means every random mutation was caught. It doesn't prove your constraints are complete, only that the ones you wrote are wired up.

## Quick start

```rust
use zk_scribble::{ScribbleConfig, assert_all_caught};

#[test]
fn my_chiplet_survives_chaos() {
    let (air, instance, witness) = setup_my_chiplet();
    assert_all_caught(&air, &instance, &witness, ScribbleConfig::default());
}
```

## Requirements

Depends on `hekate-math`, `hekate-sdk`, `hekate-core`, `hekate-program`.

## License

MIT