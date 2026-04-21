# Changelog

## guilder-core 0.7.1 — 2026-04-22
- Fix the orderbook reconciliation deadlock by replacing the hidden `DashMap` guard pattern with explicit `RwLock<HashMap<...>>` state
- Improve orderbook warmup/reconnect behavior with initialization-aware timeout gating, transport-reset reconnects, and shared engine-status tracing

## guilder-client-hyperliquid 0.5.1 — 2026-04-22
- Add a connection-level idle watchdog and improved diagnostics for shared market WebSocket recovery
- Remove the last `DashMap` usage from the Hyperliquid client cache

## guilder-abstraction 0.1.22 — 2026-04-21
- Add `L2Snapshot` / `L2Level` and `subscribe_l2_snapshot` to support snapshot-native exchanges
- Change `get_l2_orderbook` to return `L2Snapshot` instead of `Vec<L2Update>`

## guilder-client-hyperliquid 0.4.19 — 2026-04-17
- Update `cancel_order_by_cloid` to use `String` cloid and `cancelByCloid` action
- Fix exit order placement and cancel order
- Bump `guilder-abstraction` dependency to `0.1.21`
