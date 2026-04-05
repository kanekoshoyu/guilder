# Guilder

[![crates](https://img.shields.io/crates/v/guilder-abstraction)](https://crates.io/crates/guilder-abstraction)
[![license](https://img.shields.io/github/license/kanekoshoyu/guilder)](https://github.com/kanekoshoyu/guilder/blob/master/LICENSE)
[![discord](https://img.shields.io/discord/1153997271294283827)](https://discord.gg/q3j5MYdwnm)

Unopinionated multi-language cross-exchange crypto trading library in Rust.

## The idea

Every crypto exchange has a different API, but they all do the same things: get prices, place orders, stream market data. Guilder defines those operations as a shared set of traits in a YAML file, auto-generates the trait code, and lets exchange clients implement them. Strategies written against the traits work on any exchange with no changes.

## Repository layout

```
abstraction/
  trading.yaml          # source of truth — traits, structs, enums defined in a language-neutral DSL
  codegen/              # Rust binary that reads trading.yaml and writes generated code
  target/
    rust/               # generated Rust trait definitions (published to crates.io as guilder-abstraction)
    python/             # generated Python abstract base classes

core/                   # reusable trading components (Orderbook, CurrencyPair) built on the traits

client/
  guilder-client-template/   # generated starting point for a new exchange client (not a real crate)
  guilder-client-binance/    # Binance implementation
  guilder-client-hyperliquid/ # Hyperliquid implementation
```

## How codegen works

1. Edit `abstraction/trading.yaml` to add or change traits, structs, or enums.
2. Run the codegen:
   ```
   cd abstraction/codegen && cargo run
   ```
3. Codegen writes:
   - `abstraction/target/rust/src/guilder_abstraction.rs` — Rust trait and type definitions
   - `abstraction/target/python/guilder_abstraction.py` — Python abstract base classes
   - `client/guilder-client-template/` — a fresh client template (see below)

Never edit the generated files directly — they will be overwritten on the next codegen run.

## Adding a new exchange client

1. Run the codegen to get an up-to-date template.
2. Copy `client/guilder-client-template/` to `client/guilder-client-<exchange>/`.
3. In the new directory:
   - Rename the package in `Cargo.toml`.
   - Replace `ExchangeClient` with your struct name (e.g. `BinanceClient`).
   - Implement each method — they all start as `unimplemented!()`.
4. Add your crate to the workspace if needed.

## Traits

| Trait | Description | Async |
|---|---|---|
| `TestServer` | Ping and server time | yes |
| `GetMarketData` | Symbols, prices, orderbook snapshots | yes |
| `GetAccountSnapshot` | Account state snapshot (collateral, positions, orders) | yes |
| `ManageOrder` | Place, modify, cancel orders | yes |
| `SubscribeMarketData` | Streaming L2 updates, fills, asset context, liquidations via `BoxStream` | yes |
| `SubscribeUserEvents` | Streaming user fills, order updates, funding, deposits, withdrawals via `BoxStream` | yes |

## Implementation status

| Trait | binance | hyperliquid |
|---|---|---|
| `TestServer` | ❌ | ✅ |
| `GetMarketData` | ❌ | ✅ |
| `GetAccountSnapshot` | ❌ | ✅ |
| `ManageOrder` | ❌ | ✅ |
| `SubscribeMarketData` | ❌ | ✅ |
| `SubscribeUserEvents` | ❌ | ✅ |

legend: ✅ complete, 🚧 partial, ❌ not started


## Design constraints

- `trading.yaml` uses only primitives and types defined within the YAML itself — no external crate types.
- Generated abstraction code uses only the standard library.
- All traits are async.
- `Stream` return types (for subscriptions) stay as `fn` returning `impl Stream`, not `async fn`.
- **Reconnection is a client responsibility.** WebSocket streams returned by `subscribe_*` methods auto-reconnect on connection loss (5 s delay). Consumers see transient `Err` items but the stream never terminates due to a dropped connection — they can treat the stream as logically persistent. This keeps reconnection logic out of higher-level engines and strategies.

## Why "Guilder"?

The Dutch Guilder was the currency of the East India Company for 500+ years — the original cross-exchange trading infrastructure. It also rhymes with *builder*.

## See also

- [exchange-collection](https://github.com/kanekoshoyu/exchange-collection) — Crypto exchange OpenAPI specs and generated models
