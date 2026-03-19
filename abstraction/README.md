# [guilder](../README.md)-abstraction

GuilderDSL: Crypto Trading DSL (domain specific language) in YAML.
Defines generic interfaces for different programming languages.
Official support: Rust, Python.

## How it works

`trading.yaml` is the single source of truth. It defines traits, structs, and enums in a language-neutral DSL. The codegen reads this file and produces idiomatic code for each target language — Rust traits and Python abstract base classes.

**Never edit generated files directly** — they are overwritten on each codegen run.

## Code structure

| Component | Description |
|---|---|
| [trading.yaml](./trading.yaml) | Trait, struct, and enum definitions in the YAML DSL |
| [codegen](./codegen/README.md) | Code generator (Rust binary) targeting multiple languages |
| [target](./target/README.md) | Generated trading trait code per language |

## Generated types

### Key structs

| Struct | Description |
|---|---|
| `L2Update` | Orderbook level-2 update (bids, asks, sequence number) |
| `Fill` | Trade fill with side, price, volume, timestamp |
| `AssetContext` | Per-asset market context: mark price, OI, funding rate, 24h volume |
| `Liquidation` | Liquidation event for a user address |
| `UserFill` | User-specific trade fill |
| `OrderUpdate` | Order status change (placed, filled, cancelled) |
| `FundingPayment` | Funding payment received/paid |
| `Deposit` / `Withdrawal` | Account deposit and withdrawal events |

### Key enums

| Enum | Description |
|---|---|
| `OrderSide` | `Buy` / `Sell` |
| `OrderType` | `Limit` / `Market` |

### Stream type

```rust
pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;
```

All `subscribe_*` methods return `BoxStream<Result<T, String>>` — a pinned, boxed, object-safe async stream. This makes the traits object-safe (`Box<dyn SubscribeMarketData>` works).

## Running codegen

```sh
cd codegen && cargo run
```

Outputs:
- `target/rust/src/guilder_abstraction.rs`
- `target/python/guilder_abstraction.py`
- `../client/guilder-client-template/` (fresh client scaffold)

## Design rules

- `trading.yaml` uses only primitives and types defined within the YAML — no external crate types.
- Generated code depends only on the standard library (plus `futures-core` for `Stream`).
- All trait methods are async.
- Subscription methods return `BoxStream`, not `async fn` — this keeps them object-safe.

## TODO

- [x] Package Rust abstraction
- [x] Publish Rust abstraction
- [ ] Package Python abstraction
- [ ] Publish Python abstraction
