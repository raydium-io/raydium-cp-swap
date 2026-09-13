# Token collections + rebalance swaps for CP-Swap

## the pitch

a stable is a stable is a stable, more or less. an lst is an lst is an lst. a meme is a meme is a meme. the market already prices these as *sets*, but every AMM still treats each pair as an island. Curve figured out the first half of this a long time ago: put things that are supposed to trade near each other in one pool and make the pool itself the cheapest place to keep them there. Andre's contribution was the second half: pay the people who do the work. Solidly did not invent new curve math, it made fees and emissions flow to whoever keeps the system honest, and let everyone else build on top permissionlessly.

this PR takes that framing and applies it to CP-Swap without touching a single existing instruction:

1. **a token collection is a set, not a list.** anyone can create one. membership is decided by an admission *rule*, and anyone can register a mint by supplying the on-chain proof that the rule holds. the first rule shipped here is "standard pump.fun launch": bonding curve owned by pump, anchor type matches, PDA matches the mint, not a mayhem-mode coin. the collection's quote mint (WSOL for a meme collection, USDC for stables, SOL for LSTs) is always a member.
2. **rulesets are admin-defined, then free to use.** Raydium decides what "a stable", "an lst", "a standard meme launch" means on-chain, once. everyone composes on top of that.
3. **intra-collection rebalancing is cheap; extra-collection trading is unchanged.** `rebalance_swap_base_input` is `swap_base_input` with the LP fee divided by the collection's divisor (1/100 or 1/1000 of the config rate) and one extra rule: the trade must strictly reduce the pool's imbalance against the members' rates. same direction, overshoot, or an already balanced pool are rejected. arbs and keepers get paid in fee discount for pulling pools back toward fair; everyone else pays the normal rate. no emissions, no new token, no oracle in the swap path.

with transaction v1 (4,096 bytes, mainnet epoch 1035) the whole lifecycle fits in one transaction, and a route across a whole collection fits in one transaction. this is the primitive that makes "a collection of pools" behave like one pool.

## what changes

Additive only. No existing instruction, account layout, or PDA is modified. `swap_base_input`'s body moved into `swap_base_input_with_fee(accounts, amount_in, minimum_amount_out, trade_fee_rate, rebalance)`; the existing entry point calls it with the config rate and `None`, so behaviour is byte-for-byte identical.

New accounts (`states/collection.rs`)
- `Ruleset` `["ruleset", index]`: kind (0 any, 1 pump.fun launch, 2 immutable mint), flags, proof program id. Admin only.
- `TokenCollection` `["token_collection", authority, index]`: ruleset, quote mint, rebalance fee divisor, member count. Permissionless.
- `CollectionMember` `["collection_member", collection, mint]`: mint, rate (1e9 = 1.0), who registered it. Permissionless, rule-checked.

New instructions
- `create_ruleset`, `update_ruleset` (admin)
- `create_token_collection`, `update_token_collection`, `set_collection_member_rate` (collection authority)
- `register_collection_member` (anyone; remaining accounts carry the proof, e.g. the pump.fun bonding curve)
- `rebalance_swap_base_input` (anyone; `Swap` accounts + `collection`, `input_member`, `output_member`)

Rule evaluation lives in `instructions/collection/rules.rs` and reads only the mint and proof accounts, so membership is a pure function of chain state.

## verified

Against a mainnet fork (Surfpool, and Agave 4.3 `solana-test-validator` with cloned accounts) on the live WSOL / `73ed…pump` CP-Swap pool `2higKRf25Q9WMcYfgyK96AAuFVv5zucfDAFHHDuVETcq`:

| step | result |
|---|---|
| register `73ed…pump` with its bonding curve | ok |
| register a mayhem-mode pump coin | `RuleCheckFailed` |
| register with another coin's bonding curve | `RuleCheckFailed` |
| rebalance swap on a balanced pool | `NotRebalancing` |
| `swap_base_input` 20 WSOL | trade fee 2500 ppm |
| rebalance swap same direction | `NotRebalancing` |
| `rebalance_swap_base_input` back toward balance | trade fee 25 ppm |
| rebalance swap that overshoots | `NotRebalancing` |

Compute: register member ~14k CU, rebalance swap ~45k CU (vs ~33k for the plain swap; the extra is two member account loads and the imbalance check).

## open questions for maintainers

- rates today are set by the collection authority; the obvious follow-up is an oracle-fed `set_collection_member_rate` (stake pool exchange rate for LSTs, pyth for stables).
- whether the rebalance divisor should be bounded by the `AmmConfig` rather than left to the collection creator.
- `ImmutableMint` and `Any` rule kinds are included as the smallest useful generic rules; happy to drop or extend.
