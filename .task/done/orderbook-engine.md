# Orderbook Engine

## Problem

Core has `Orderbook` and `IndexOrderbook` as passive data structures, but no component
that keeps them in sync with a live exchange. Strategies need real-time orderbook state
and derived analytics (liquidity at slippage, imbalance) without manually wiring up
snapshots and delta streams.

## Goal

A new `OrderbookEngine` in `guilder-core` that:
- Takes any client implementing `GetMarketData + SubscribeMarketData`
- Syncs orderbooks for all symbols (via `get_symbol()` + `get_l2_orderbook` + `subscribe_l2_update`)
- Exposes query methods for analytics — no raw book access, no locks for callers

## Design

### Sync Lifecycle (per symbol)

1. Fetch snapshot via `get_l2_orderbook(symbol)`
2. Subscribe to `subscribe_l2_update(symbol)` stream
3. Apply deltas to internal `Orderbook` (`update_ask` / `update_bid`, remove level when volume = 0)
4. Detect sequence gaps → re-snapshot

### Symbol Tracking

- `track_all()` — calls `get_symbol()`, subscribes to every symbol
- `track(symbols: Vec<String>)` — subscribe to a specific set
- Internal state: `DashMap<String, Orderbook>` (lock-free concurrent reads)

### Query Interface

Callers never touch the orderbook directly. All reads go through engine methods:

- `liquidity(symbol, side, slippage_pct) -> Result<f64, _>`
  Walk from best ask/bid outward up to the slippage boundary.
  `best_ask * (1 + slippage)` for asks, `best_bid * (1 - slippage)` for bids.
  Sum `price * volume` (quote currency) for all levels within boundary.

- `imbalance(symbol, top_n: Option<usize>) -> Result<f64, _>`
  `(B - A) / (B + A)` where B = total bid liquidity, A = total ask liquidity.
  `top_n` limits to top N levels per side. `None` sums the full book.
  Returns value in [-1.0, 1.0]. Positive = bid-heavy, negative = ask-heavy.

### Internal Architecture

Engine owns a background tokio task per symbol that consumes the `BoxStream<L2Update>`
and writes into `DashMap`. Query methods read from the DashMap directly — no channel
round-trip, no locks for callers.

```
OrderbookEngine
  ├── client: Arc<C>  (C: GetMarketData + SubscribeMarketData)
  ├── books: DashMap<String, Orderbook>
  └── tasks: JoinSet<()>  (one per symbol)
```

### Feature Gating

Depends on `tokio` and `dashmap`, both already feature-gated in core. The engine
itself should be behind a `engine` feature flag.

## Tasks

- [ ] Add `liquidity()` and `imbalance()` methods to `Orderbook` (pure computation, no engine needed)
- [ ] Add `OrderbookEngine` struct with `new(client)`, `track_all()`, `track(symbols)`
- [ ] Implement sync lifecycle: snapshot + subscribe + delta apply + sequence gap detection
- [ ] Wire up query methods that delegate to the underlying `Orderbook` analytics
- [ ] Add `engine` feature flag to `core/Cargo.toml`
- [ ] Tests: unit tests for `liquidity()` / `imbalance()` on a static orderbook
