# raydium-cp-swap

Raydium constant product AMM Program without place order to OpenBook. The curve algorithm is inspired by Solana's official token-swap.

## Environment Setup

1. Install Rust.
2. Install Solana and then run solana-keygen new to create a keypair at the default location.
3. Install Anchor.

## Quickstart

Clone the repository and enter the source code directory.

```shell

git clone https://github.com/raydium-io/raydium-cp-swap
cd raydium-cp-swap
```

Build And deploy

```shell
anchor build
anchor deploy
```

Attention, check your configuration and confirm the environment you want to deploy.

## License

Raydium constant product swap is licensed under the Apache License, Version 2.0.
