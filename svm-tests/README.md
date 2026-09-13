# SVM tests for token collections

Full-coverage integration tests for the collection module, run against the compiled program with
[LiteSVM] on top of real mainnet state (`fixtures/cpmm_wsol_pump.json`: the live WSOL / `73ed…pump`
pool, its config, vaults, observation, mints, and two real pump.fun bonding curves, one standard and
one mayhem-mode).

```sh
# admin-gated instructions need a known admin: build with the test admin key
CPSWAP_LOCALNET_ADMIN=$(solana-keygen pubkey svm-tests/fixtures/test-admin.json) \
  cargo build-sbf --manifest-path programs/cp-swap/Cargo.toml --features localnet
cargo test --manifest-path svm-tests/Cargo.toml
```

[LiteSVM]: https://github.com/LiteSVM/litesvm
