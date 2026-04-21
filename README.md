# Guilder

[![crates](https://img.shields.io/crates/v/guilder-abstraction)](https://crates.io/crates/guilder-abstraction)
[![license](https://img.shields.io/github/license/kanekoshoyu/guilder)](https://github.com/kanekoshoyu/guilder/blob/master/LICENSE)
[![discord](https://img.shields.io/discord/1153997271294283827)](https://discord.gg/q3j5MYdwnm)

Unopinionated multi-language cross-exchange crypto trading library in Rust.

## The idea

Every crypto exchange has a different API, but they all do the same things: get prices, place orders, stream market data. Guilder defines those operations as a shared set of traits in a YAML file, auto-generates the trait code, and lets exchange clients implement them. Strategies written against the traits work on any exchange with no changes.

## Crates

| Crate | Description | crates.io |
|---|---|---|
| [`guilder-abstraction`](abstraction/target/rust/README.md) | Auto-generated trading traits, structs, and enums | [![crates](https://img.shields.io/crates/v/guilder-abstraction)](https://crates.io/crates/guilder-abstraction) |
| [`guilder-core`](core/README.md) | Reusable trading components — orderbook, currency pair, live engine | [![crates](https://img.shields.io/crates/v/guilder-core)](https://crates.io/crates/guilder-core) |
| [`guilder-client-hyperliquid`](client/guilder-client-hyperliquid/README.md) | Hyperliquid exchange client | [![crates](https://img.shields.io/crates/v/guilder-client-hyperliquid)](https://crates.io/crates/guilder-client-hyperliquid) |
| [`guilder-client-binance`](client/guilder-client-binance/) | Binance exchange client | not yet published |

## Architecture

```
trading.yaml → guilder-abstraction → guilder-core → your strategy
                                   ↗
              guilder-client-* ────
```

See [DESIGN.md](DESIGN.md) for detailed design documentation.

## Repository layout

```
abstraction/
  trading.yaml          # source of truth — traits, structs, enums in a language-neutral DSL
  codegen/              # Rust binary that reads trading.yaml and writes generated code
  target/
    rust/               # generated Rust traits (published as guilder-abstraction)
    python/             # generated Python abstract base classes

core/                   # reusable trading components built on the traits
  src/
    data/               # Orderbook, CurrencyPair, IndexOrderbook
    engine/             # live orderbook sync engine (feature-gated)

client/
  guilder-client-template/    # generated starting point for a new exchange client
  guilder-client-binance/     # Binance implementation
  guilder-client-hyperliquid/ # Hyperliquid implementation
```

## Quick start

```toml
[dependencies]
guilder-abstraction = "0.1"
guilder-core = { version = "0.2", features = ["engine"] }
guilder-client-hyperliquid = "0.4"
```

```rust
use guilder_client_hyperliquid::HyperliquidClient;
use guilder_core::OrderbookEngine;
use guilder_abstraction::Side;

let client = HyperliquidClient::new(/* config */);

// live orderbook sync across all symbols
let engine = OrderbookEngine::new(client);
engine.track_all().await?;

// query analytics on the live book
let liquidity = engine.liquidity("BTC", Side::Bid, 0.01);
let imbalance = engine.imbalance("BTC", Some(10));
```

## Traits

| Trait | Description |
|---|---|
| `TestServer` | Ping and server time |
| `GetMarketData` | Symbols, prices, orderbook snapshots, open interest, funding rates |
| `GetAccountSnapshot` | Account state — collateral, positions, open orders |
| `ManageOrder` | Place, modify, cancel orders |
| `SubscribeMarketData` | Stream L2 updates, fills, asset context, liquidations |
| `SubscribeUserEvents` | Stream user fills, order updates, funding, deposits, withdrawals |

## Implementation status

| Trait | Hyperliquid | Binance |
|---|---|---|
| `TestServer` | Complete | Not started |
| `GetMarketData` | Complete | Not started |
| `GetAccountSnapshot` | Complete | Not started |
| `ManageOrder` | Complete | Not started |
| `SubscribeMarketData` | Complete | Not started |
| `SubscribeUserEvents` | Complete | Not started |

## How codegen works

1. Edit `abstraction/trading.yaml` to add or change traits, structs, or enums.
2. Run the codegen:
   ```sh
   cd abstraction/codegen && cargo run
   ```
3. Codegen writes:
   - `abstraction/target/rust/src/guilder_abstraction.rs` — Rust trait and type definitions
   - `abstraction/target/python/guilder_abstraction.py` — Python abstract base classes
   - `client/guilder-client-template/` — fresh client scaffold

Never edit generated files directly — they are overwritten on each codegen run.

## Adding a new exchange client

1. Run the codegen to get an up-to-date template.
2. Copy `client/guilder-client-template/` to `client/guilder-client-<exchange>/`.
3. Rename the package in `Cargo.toml` and the struct (e.g. `BinanceClient`).
4. Implement each method — they all start as `unimplemented!()`.

## Design constraints

- `trading.yaml` uses only primitives and types defined within the YAML — no external crate types.
- Generated code depends only on the standard library (plus `futures-core` for `Stream`).
- All traits are async. Subscription methods return `BoxStream` for object safety.
- Reconnection is a client responsibility — WebSocket streams auto-reconnect so consumers see a logically persistent stream.
- In `guilder-core`, shared orderbook maps intentionally use `RwLock<HashMap<...>>` so lock scopes stay explicit and we avoid unnecessary hidden guard-lifetime problems in core trading state.

## Concurrency note

For core shared trading state, Guilder intentionally prefers `RwLock<HashMap<...>>` over specialized concurrent-map crates. This keeps lock scopes explicit in the source, makes code review easier, and avoids subtle guard-lifetime bugs where a map entry handle can outlive the programmer's mental model.

## Why "Guilder"?

The Dutch Guilder was the currency of the East India Company for 500+ years — the original cross-exchange trading infrastructure. It also rhymes with *builder*.

## See also

- [exchange-collection](https://github.com/kanekoshoyu/exchange-collection) — Crypto exchange OpenAPI specs and generated models
