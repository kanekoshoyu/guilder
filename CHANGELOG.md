# Changelog

## guilder-abstraction 0.1.21 — 2026-04-17
- Rename `cancel_order` → `cancel_order_by_cloid`, change `cloid` from `i64` to `String`
- Fix `cancel_order_by_cloid`: use `cancelByCloid` action type
- Fix exit order placement and cancel order

## guilder-client-hyperliquid 0.4.19 — 2026-04-17
- Update `cancel_order_by_cloid` to use `String` cloid and `cancelByCloid` action
- Fix exit order placement and cancel order
- Bump `guilder-abstraction` dependency to `0.1.21`
