# Changelog

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
