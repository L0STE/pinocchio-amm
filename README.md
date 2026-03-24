# Pinocchio AMM

Constant-product AMM for Solana built with [pinocchio](https://github.com/anza-xyz/pinocchio). Uses batch CPI from [pinocchio-token](https://github.com/anza-xyz/pinocchio/tree/main/programs/token) to pack multiple token operations into a single cross-program invocation.

## Batch CPI

The token program supports a `0xFF` discriminator that batches multiple sub-instructions (transfer, mint_to, burn) into one CPI call. Instead of paying the CPI overhead per operation, a swap executes both its transfer-in and transfer-out in a single invoke:

```rust
batch::<2, 6>(
    [
        TokenOp::transfer(user_from, vault_from, user, result.deposit),
        TokenOp::transfer(vault_to, user_to, config, result.withdraw),
    ],
    token_program,
    &signers,
)?;
```

The const generics (`N` operations, `A` accounts) let the compiler monomorphize and fully unroll each call site. Wire format per sub-instruction: `[num_accts: u8] [data_len: u8] [disc: u8] [amount: u64 LE]`.

## Instructions

| Disc | Instruction | Accounts | Description |
|------|------------|----------|-------------|
| 0 | Initialize | 13 | Create pool, mint LP, seed initial liquidity |
| 1 | Deposit | 11 | Proportional deposit, mint LP tokens |
| 2 | Withdraw | 11 | Burn LP, proportional withdrawal |
| 3 | Swap | 9 | Constant-product swap with fee |
| 4 | UpdateAuthority | 2 | Transfer pool authority |
| 5 | UpdateFee | 2 | Change fee (max 10,000 bps) |
| 6 | UpdateLock | 2 | Toggle pool lock |
| 7 | RemoveAuthority | 2 | Renounce authority (irreversible) |

## State

Single account type: `Config` (110 bytes).

```
[0]      discriminator (1)
[1..9]   seed (u64)
[9..41]  authority (pubkey)
[41..73] mint_x (pubkey)
[73..105] mint_y (pubkey)
[105..107] fee (u16, bps)
[107]    locked (bool)
[108]    lp_bump (u8)
[109]    bump (u8)
```

Zero-copy via `#[repr(C)]` — no deserialization, direct pointer cast.

## Build & Test

```
cargo build-sbf
cargo test
```

## Dependencies

- `pinocchio` 0.10.2
- `pinocchio-token` 0.5.0
- `pinocchio-system` 0.5.0
- `pinocchio-associated-token-account` 0.3.0
