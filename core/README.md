# guilder-core

[![crates](https://img.shields.io/crates/v/guilder-core)](https://crates.io/crates/guilder-core)
[![license](https://img.shields.io/github/license/kanekoshoyu/guilder)](https://github.com/kanekoshoyu/guilder/blob/master/LICENSE)

Reusable trading components for the [guilder](https://github.com/kanekoshoyu/guilder) multi-exchange crypto trading library.

guilder-core sits between the abstraction layer and your application code, providing battle-tested data structures and an optional live orderbook engine that works with any exchange client implementing the guilder traits.

## Modules

### `data` — Trading data structures

Domain models that are exchange-agnostic:

| Struct | Description |
|---|---|
| `Orderbook` | Sorted L2 orderbook backed by `BTreeMap<OrderedFloat<f64>, f64>` — O(log n) insert/remove with natural price ordering. Supports update, merge, liquidity calculation, and imbalance ratio queries. |
| `CurrencyPair` | Base/quote currency pair (e.g. BTC-USDT) with parsing from hyphen (`ETH-BTC`), underscore (`ETH_BTC`), and concatenated (`ETHBTC`) formats. |
| `IndexOrderbook` | Multi-pair orderbook index — look up best bid/ask across currency pairs. |

### `engine` — Live orderbook sync (feature-gated)

A real-time orderbook synchronization engine that takes any client implementing `GetMarketData + SubscribeMarketData` and keeps local orderbooks in sync.

| Feature | Detail |
|---|---|
| Snapshot + incremental sync | Fetches L2 snapshot, then applies streamed updates |
| Sequence gap detection | Detects gaps in update sequences and automatically re-snapshots |
| Stream error recovery | Re-snapshots and resubscribes on stream errors |
| Concurrent symbol tracking | Tracks multiple symbols simultaneously via `select_all` |
| Live analytics | Query liquidity within a slippage boundary, or bid/ask imbalance ratio, on a live book |

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
guilder-core = "0.2"

# enable the live orderbook engine (pulls in tokio, futures)
guilder-core = { version = "0.2", features = ["engine"] }
```

### Data structures

```rust
use guilder_core::{Orderbook, CurrencyPair};

let mut book = Orderbook::new();
book.update_ask(100.0, 1.5);
book.update_bid(99.0, 2.0);

let pair = CurrencyPair::new("btc", "usdt");
assert_eq!(pair.name_hyphen(), "BTC-USDT");
```

### Orderbook engine

```rust
use guilder_core::OrderbookEngine;
use guilder_abstraction::Side;

// client: any struct implementing GetMarketData + SubscribeMarketData
let engine = OrderbookEngine::new(client);

// track specific symbols
engine.track(vec!["BTC".into(), "ETH".into()]).await?;

// query live analytics
let liq = engine.liquidity("BTC", Side::Bid, 0.01);  // liquidity within 1% slippage
let imb = engine.imbalance("BTC", Some(10));           // top-10 level imbalance
```

## Feature flags

| Flag | Default | Description |
|---|---|---|
| `engine` | off | Enables `OrderbookEngine` with async runtime dependencies (`tokio`, `futures`, `tokio-stream`, `rust_decimal`) |

The base crate (data structures only) depends on `guilder-abstraction` and `ordered-float` — nothing else.

## Part of guilder

```
trading.yaml → guilder-abstraction → guilder-core → your strategy
                                   ↗
              guilder-client-* ────
```

See the [guilder repository](https://github.com/kanekoshoyu/guilder) for the full architecture.
