# Changelog

## guilder-abstraction 0.1.23 — 2026-04-24
- **Breaking**: replace `Balance` with `AccountBalance` struct, adding margin health fields (`safe`, `usable`, `margin_used`, `maintenance`)
- **Breaking**: consolidate `get_collateral`, `get_spot_balance`, `get_collateral_balance` into single `get_balance` returning `Vec<AccountBalance>`
- `subscribe_spot_balance` / `subscribe_spot_balance_with_address` now return `Stream<Result<Vec<AccountBalance>, String>>`
- Add `unsubscribe_user_events` to `SubscribeAccount`
- Add `SubscribeMarketDataOps` trait with `unsubscribe_market_data`

## guilder-client-hyperliquid 0.6.0 — 2026-04-24
- **Breaking**: adapt to `AccountBalance` replacing `Balance` across all balance methods
- Implement `unsubscribe_user_events` and `SubscribeMarketDataOps::unsubscribe_market_data`
- Add `check_balance.rs` example

## guilder-client-binance 0.2.0 — 2026-04-24
- **Breaking**: adapt to `AccountBalance` replacing `Balance` in abstraction layer

## guilder-core 0.8.0 — 2026-04-24
- **Breaking**: adapt to `AccountBalance` replacing `Balance` in abstraction layer
- Suppress orderbook sync warnings when engine is not `Active`

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
