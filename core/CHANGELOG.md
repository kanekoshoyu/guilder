# Changelog

## 0.8.0 — 2026-04-24

- **Breaking**: adapt to `AccountBalance` replacing `Balance` in abstraction layer
- Suppress orderbook sync warnings when engine is not `Active` (avoids noise during initialization/shutdown)

## 0.7.1 — 2026-04-22

- Fix orderbook reconciliation self-deadlock by removing the read-then-write `DashMap` pattern and moving shared orderbook state to `RwLock<HashMap<...>>`
- Add startup/status gating so symbols stay `Initializing` until the first real item arrives, suppressing misleading timeout warnings during warmup
- Add orderbook stream timing and reconnect improvements, including fast reconnect on transport reset
- Add shared `EngineStatus` / `StatusHandle` transition tracing support for top-level engines

## 0.7.0 — 2026-04-21

- **Breaking**: replace `DashMap` with `RwLock<HashMap>` in `OrderbookEngine` — fewer dependencies, simpler locking
- **Breaking**: adapt to `L2Snapshot` return type from `get_l2_orderbook` (was `Vec<L2Update>`)
- Remove verbose `info!`/`debug!` logging and heartbeat ticker from `track()` and `spawn_staleness_monitor()`
- Remove `TRACK_HEARTBEAT_INTERVAL` constant

## 0.6.1 — 2026-04-17

- Fix: refresh `last_updated` on every successful REST validation (not just on correction), so the staleness gate doesn't reject low-activity coins that are reconciling cleanly

## 0.6.0 — 2026-04-17

- Add optional orderbook reconciliation — periodic REST vs WS book comparison
  - `OrderbookEngine::with_reconciliation(interval)` — enables the feature
  - `OrderbookEngine::set_reconciliation(interval)` — post-construction setter
  - `OrderbookEngine::spawn_reconciliation()` — starts the background loop (uses `spawn_local`, requires `LocalSet`)
  - On drift detection, the local book is automatically replaced with the REST snapshot
  - `OrderbookEngine::total_drifts` / `total_corrections` — atomic counters
  - `OrderbookEngine::reconciliation_health()` — per-symbol health with drift/correction status
- New types: `ReconciliationHealth`, `ReconciliationHealthView` (serialisable)

## 0.4.0 — 2026-04-10

- Restructure `core/src/` modules for consistent file layout:
  - `data/` and `engine/` merged into `orderbook/` — matches `account/` naming convention
  - `orderbook/` now uses the same `mod.rs` + `engine.rs` + `types.rs` + `error.rs` pattern as `account/`
  - `currency_pair.rs` moved to top-level (single-file module, no folder needed)
- Breaking: public module paths changed (`guilder_core::engine::` → `guilder_core::orderbook::`)
  - Re-exports at crate root remain unchanged — `use guilder_core::{Orderbook, OrderbookEngine, ..}` still works

## 0.3.4 — 2026-04-05

- Disable sequence gap detection and REST re-snapshot in `sync_loop` when `skip_initial_snapshot` is enabled
  - Hyperliquid's WS `time` field is a millisecond timestamp, not a monotonic counter — gap detection was triggering on every message, causing continuous REST calls
  - On stream error, simply resubscribe (next WS message is a full snapshot)

## 0.3.3 — 2026-04-05

- Add `with_skip_initial_snapshot(bool)` builder method to `OrderbookEngine`
  - When enabled, skips the REST snapshot burst on `track()`/`track_all()` startup
  - First WS message seeds each orderbook instead — ideal for exchanges like Hyperliquid that deliver full snapshots over WebSocket
- `sync_loop` now lazily creates orderbook entries on first WS update when no initial snapshot exists

## 0.3.2 — 2026-04-05

- Throttle concurrent REST calls in `OrderbookEngine` to prevent rate-limiting
  - `snapshot_symbols()` now uses `buffer_unordered(10)` instead of `join_all`
  - `sync_loop` gap-recovery REST calls share a semaphore (capacity 10) to prevent thundering herd on WebSocket reconnects

## 0.3.1

- Make `track`/`track_all` take `&self` for `Arc` compatibility

## 0.3.0

- Add `snapshot`, health tracking, hot-add symbols, update broadcast, parallel init, `EngineError`

## 0.2.0

- Restructure core into `data/` and `engine/` modules
