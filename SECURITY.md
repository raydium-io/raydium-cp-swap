# Raydium CP-Swap (CPMM) Bug Bounty Program

Raydium's full bug bounty program with ImmuneFi can be found at: https://immunefi.com/bounty/raydium/

## Rewards by Threat Level

Rewards are distributed according to the impact of the vulnerability based on the Immunefi Vulnerability Severity Classification System V2.3. This is a simplified 5-level scale, focusing on the impact of the vulnerability reported.

### Smart Contracts

| Severity | Bounty                    |
| -------- | ------------------------- |
| Critical | USD 50,000 to USD 505,000 |
| High     | USD 40,000                |
| Medium   | USD 5,000                 |

All bug reports must include a Proof of Concept (PoC) demonstrating how the vulnerability can be exploited to impact an asset-in-scope to be eligible for a reward. Critical and High severity bug reports should also include a suggestion for a fix. Explanations and statements are not accepted as PoC and code is required.

Rewards for critical smart contract bug reports will be further capped at 10% of direct funds at risk if the bug discovered is exploited. However, there is a minimum reward of USD 50,000.

Bugs in `raydium-sdk` and other code outside of the smart contract will be assessed on a case-by-case basis.

## Report Submission

Please email security@reactorlabs.io with a detailed description of the attack vector. For high- and critical-severity reports, please include a proof of concept. We will reach back out within 24 hours with additional questions or next steps on the bug bounty.

## Payout Information

Payouts are handled by the Raydium team directly and are denominated in USD. Payouts can be done in RAY, SOL, or USDC.

## Out of Scope & Rules

The following vulnerabilities are excluded from the rewards for this bug bounty program:

- Attacks that the reporter has already exploited themselves, leading to damage
- Attacks requiring access to leaked keys/credentials
- Attacks requiring access to privileged addresses (governance, strategist)
- Incorrect data supplied by third party oracles (not excluding oracle manipulation/flash loan attacks)
- Basic economic governance attacks (e.g. 51% attack)
- Lack of liquidity
- Best practice critiques
- Sybil attacks
- Centralization risks
- Any UI bugs
- Bugs in the core Solana runtime (please submit these to [Solana's bug bounty program](https://github.com/solana-labs/solana/security/policy))
- Vulnerabilities that require a validator to execute them
- Vulnerabilities requiring access to privileged keys/credentials
- MEV vectors the team is already aware of

## AMM Assets in Scope

| Target                                                                                                                    | Type                                  |
| ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/lib.rs                                     | Smart Contract - lib                  |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/error.rs                                   | Smart Contract - error                |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/admin/collect_fund_fee.rs     | Smart Contract - collect_fund_fee     |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/admin/collect_protocol_fee.rs | Smart Contract - collect_protocol_fee |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/admin/create_config.rs        | Smart Contract - create_config        |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/admin/mod.rs                  | Smart Contract - admin mod            |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/admin/update_config.rs        | Smart Contract - update_config        |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/admin/update_pool_status.rs   | Smart Contract - update_pool_status   |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/deposit.rs                    | Smart Contract - deposit              |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/initialize.rs                 | Smart Contract - initialize           |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/mod.rs                        | Smart Contract - instructions mod     |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/swap_base_input.rs            | Smart Contract - swap_base_input      |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/swap_base_output.rs           | Smart Contract - swap_base_output     |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/instructions/withdraw.rs                   | Smart Contract - withdraw             |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/states/config.rs                           | Smart Contract - config               |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/states/events.rs                           | Smart Contract - events               |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/states/mod.rs                              | Smart Contract - states mod           |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/states/pool.rs                             | Smart Contract - pool                 |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/utils/math.rs                              | Smart Contract - math                 |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/utils/mod.rs                               | Smart Contract - utils mod            |
| https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/utils/token.rs                             | Smart Contract - utils token          |

## Additional Information

A public testnet of Raydium's CPMM can be found at A public testnet of Raydium’s AMM can be found at https://explorer.solana.com/address/CPMDWBwJDtYax9qW7AyRuVC19Cc4L4Vcy4n2BHAbHkCW?cluster=devnet

A public testnet of OpenBook's Central Limit Order Book can be found at https://explorer.solana.com/address/EoTcMgcDRTJVZDMZWBoU6rhYHZfkNTVEAfz3uUJRcYGj

If a Critical Impact can be caused to any other asset managed by Raydium that isn't on this table but for which the impact is in the Impacts in Scope section below, you are encouraged to submit it for consideration by the project. This only applies to Critical impacts.
