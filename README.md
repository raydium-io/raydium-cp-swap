# raydium-cp-swap

A revamped constant product AMM program optimized for straightforward pool deployment along with additional features and integrations:
- No Openbook market ID is required for pool creation
- Token22 is supported
- Built-in price oracle
- Optimized in Anchor

The program has been audited by [MadShield](https://www.madshield.xyz/). The report can be found [here](https://github.com/raydium-io/raydium-docs/tree/master/audit/MadShield%20Q1%202024).

The program assets are in-scope for Raydium’s [Immunefi bug bounty program](https://immunefi.com/bug-bounty/raydium/).

## Environment Setup

1. Install `Rust`

   ```shell
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup default 1.81.0
   ```

2. Install `Solana `

   ```shell
   sh -c "$(curl -sSfL https://release.anza.xyz/v2.1.0/install)"
   ```

   then run `solana-keygen new` to create a keypair at the default location.

3. install `Anchor`

   ```shell
   # Installing using Anchor version manager (avm) 
   cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
   # Install anchor
   avm install 0.31.0
   ```

## Quickstart

Clone the repository and test the program.

```shell

git clone https://github.com/raydium-io/raydium-cp-swap
cd raydium-cp-swap && yarn && anchor test
```

## License

Raydium constant product swap is licensed under the Apache License, Version 2.0.
