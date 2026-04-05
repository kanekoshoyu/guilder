# Guilder Design

## Layer overview

```
┌─────────────────────────────────────────────────┐
│  Strategy / Application                         │
│  (user code — written against traits, not APIs) │
├─────────────────────────────────────────────────┤
│  guilder-core                                   │
│  ├── data/      — Orderbook, CurrencyPair       │
│  └── engine/    — OrderbookEngine (feature-gated)│
├─────────────────────────────────────────────────┤
│  guilder-client-*                               │
│  (one crate per exchange: Binance, Hyperliquid) │
├─────────────────────────────────────────────────┤
│  guilder-abstraction                            │
│  (generated traits, structs, enums)             │
├─────────────────────────────────────────────────┤
│  trading.yaml                                   │
│  (language-neutral source of truth)             │
└─────────────────────────────────────────────────┘
```

## guilder-abstraction

Defines **what** every exchange must do, with no opinion on **how**.

- Source: `abstraction/trading.yaml` — traits, structs, and enums in a language-neutral DSL.
- Codegen (`abstraction/codegen/`) reads the YAML and writes Rust traits + Python ABCs.
- Generated output uses only the standard library — no external crate types leak into the interface.
- Published to crates.io so clients and core can depend on it as a normal crate.

## guilder-core

Reusable trading components built **on top of** the abstraction traits.

### `core/data/`

Domain models that don't depend on any exchange:

- **Orderbook** — sorted `BTreeMap<OrderedFloat<f64>, f64>` for asks/bids with update, merge, liquidity, and imbalance queries.
- **CurrencyPair** — base/quote representation with parsing from hyphen, underscore, and no-separator formats.

### `core/engine/` (feature-gated: `engine`)

Live orderbook sync engine. Takes any client that implements `GetMarketData + SubscribeMarketData`, snapshots orderbooks, then streams incremental L2 updates with sequence-gap detection and automatic re-snapshot on errors.

The engine feature pulls in heavier async dependencies (`tokio`, `dashmap`, `futures`, `tokio-stream`), so it is opt-in.

## guilder-client-*

One crate per exchange. Each client struct implements the abstraction traits against the exchange's real REST + WebSocket APIs.

- Clients depend on `guilder-abstraction` for trait definitions.
- Clients own reconnection logic — WebSocket streams auto-reconnect so consumers see a logically persistent stream.
- Codegen produces a template crate (`guilder-client-template/`) with all methods stubbed as `unimplemented!()`.

## Dependency direction

```
trading.yaml
    ↓  (codegen)
guilder-abstraction
    ↓
guilder-core        ←  guilder-client-*
    ↓                       ↓
  (both used by strategy / application code)
```

Key rule: **abstraction never depends on core or clients.** Core depends on abstraction. Clients depend on abstraction. Application code depends on all three.

## Design decisions

### Why YAML, not a Rust macro?
Traits need to generate code in multiple languages (Rust, Python). A proc-macro can only emit Rust. YAML keeps the definition language-neutral and the codegen straightforward.

### Why feature-gate the engine?
The data models (`Orderbook`, `CurrencyPair`) are useful on their own without an async runtime. Feature-gating `engine` keeps the default dependency footprint small.

### Why `BTreeMap` for the orderbook?
Asks are iterated lowest-first, bids highest-first. `BTreeMap` gives sorted iteration with O(log n) insert/remove — a natural fit for price levels.

### Why reconnection lives in clients, not the engine?
Each exchange has different reconnection quirks (rate limits, auth refresh, sequence resets). Pushing reconnection into the client keeps the engine and strategies generic.
