# Goal
~~Fix `cancel_order` to cancel by cloid string (hex) instead of order_id (i64).~~

## Problem
~~Current signature: `fn cancel_order(&self, cloid: i64)` — the param is named `cloid` but the implementation searches `openOrders` by `oid` (order ID). This is wrong because:~~
1. ~~Our cloids are hex strings like `"0x31a31a8e4ec24be78ba4aaf259faa915"` (128-bit), not i64~~
2. ~~The caller doesn't have the exchange-assigned order_id at cancel time — only the cloid~~
3. ~~Hyperliquid's API supports `{"type": "cancel", "cancels": [{"a": asset_idx, "o": oid}, {"a": asset_idx, "cloid": cloid_str}]}`~~

## Fix
~~Change signature to `fn cancel_order_by_cloid(&self, cloid: &str) -> Result<(), String>` and submit:~~
~~```rust~~
~~{"type": "cancel", "cancels": [{"a": asset_idx, "cloid": cloid_str}]}~~
~~```~~

~~Also update the `ManageOrder` trait in `guilder-abstraction` to match.~~

## Acceptance criteria
- ~~`cancel_order_by_cloid("0x31a31a8e4ec24be78ba4aaf259faa915")` cancels the matching order~~
- ~~No need to fetch open_orders first~~

## Done
- `trading.yaml`: renamed `cancel_order` → `cancel_order_by_cloid`, changed `cloid: i64` → `cloid: String`, return type → `Result<(), String>`
- Codegen regenerated the `ManageOrder` trait, template client, and `guilder-abstraction` crate
- `RestOpenOrder` struct: added `cloid: Option<String>` field
- `cancel_order_by_cloid` impl: fetches open orders, matches by cloid, submits `{"a": asset_idx, "cloid": cloid}` cancel action
