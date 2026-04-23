# Changelog

## 0.6.0 — 2026-04-24

- **Breaking**: adapt to `AccountBalance` struct replacing `Balance` — `get_balance` now returns `Vec<AccountBalance>` with margin health fields (`safe`, `usable`, `margin_used`, `maintenance`)
- **Breaking**: remove `get_collateral`, `get_spot_balance`, `get_collateral_balance` in favor of unified `get_balance`
- **Breaking**: `subscribe_spot_balance` / `subscribe_spot_balance_with_address` now return `Stream<Result<Vec<AccountBalance>, String>>`
- Add `unsubscribe_user_events` to `SubscribeAccount` impl
- Add `SubscribeMarketDataOps` impl with `unsubscribe_market_data`
- Add `check_balance.rs` example

## 0.5.1 — 2026-04-22

- Improve shared WebSocket connection recovery with a connection-level idle watchdog for half-open/silent sockets
- Add throughput/reconnect diagnostics around the market WS manager to support orderbook freeze investigations
- Remove `DashMap` from the remaining client-side manager cache in favor of explicit `RwLock<HashMap<...>>`

## 0.5.0 — 2026-04-21

- **Breaking**: adapt to `L2Snapshot` return type from `get_l2_orderbook` (was `Vec<L2Update>`)
- **Breaking**: update `SubscribeMarketData` impl for new `subscribe_l2_snapshot` method
- Remove `dashmap` dependency
- Remove verbose `info!` logging and rate-reporting interval from WS manager
- Remove commented-out debug logging from transport

## 0.4.20 — 2026-04-17

- Fix: swap bid/ask side mapping in `get_l2_orderbook` and `subscribe_l2_update`
  - Hyperliquid's `l2Book` returns `[bids, asks]` — index 0 is bids, index 1 is asks
  - Was: `levels[0]` → `Side::Ask`, `levels[1]` → `Side::Bid` (crossed book)
  - Now: `levels[0]` → `Side::Bid`, `levels[1]` → `Side::Ask`

## 0.4.18 — 2026-04-16

- Fix `cancel_order_by_cloid` HTTP 422 deserialization error
  - Was: `{"type": "cancel", "cancels": [{"a": asset_idx, "cloid": cloid}]}`
  - Now: `{"type": "cancelByCloid", "cancels": [{"asset": asset_idx, "cloid": cloid}]}`
  - The Hyperliquid API requires the distinct `cancelByCloid` action type and camelCase `asset` field name

## 0.4.16 — 2026-04-16

- Remove keccak256 hashing of cloid — client now passes cloid through verbatim to Hyperliquid
  - Was: `cloid = "0x" + keccak256(c.as_bytes())[:16]` — hashed the client order ID before signing
  - Now: cloid string passed through unchanged, Hyperliquid echoes it back verbatim on fills
  - Enables end-to-end intent tracing: strategy UUID → cloid → fill.cloid → trade_intent_uuid resolution

## 0.4.11 — 2026-04-14

- Fix order submission 422: add `expiresAfter` field to JSON payload
  - Python SDK always includes `"expiresAfter": null` in `_post_action`; omitting it caused server-side deserialization failure
  - Fixed in both `place_order` (direct POST) and `submit_signed_action` (cancel, modify, etc.)

## 0.4.10 — 2026-04-14

- Fix EIP-712 signing for order placement and cancel: msgpack field order now matches Python SDK exactly
  - Order wire keys: `a, b, p, s, r, t, c(opt at end)` — was `a, b, c, p, s, r, t`
  - Cloid is now appended at the end of the order dict, matching `order_request_to_order_wire` in the official Python SDK
- Enable `preserve_order` feature on `serde_json` so `serde_json::Value::Object` uses `IndexMap` (insertion order) instead of `BTreeMap` (alphabetical)
  - Required for signing all actions (cancel, batchModify, etc.) where msgpack must preserve JSON key insertion order
- Fix `value_to_msgpack` integer encoding to handle full i64/u64 range
  - Was truncating integers > 65535 to u16, causing cancel order signing to fail for large OIDs
  - Now encodes up to 64-bit integers with proper msgpack uint32/uint64/int32/int64 markers
- Add integration test for order placement and cancellation (`tests/order.rs`)
  - Env-var gated: skips when `HYPERLIQUID_WALLET_ADDRESS` and `HYPERLIQUID_WALLET_KEY` are not set
  - Tests full lifecycle: place limit order → verify in open orders → cancel → verify removed
- Cloid conversion: user-provided cloid strings are hashed via keccak256 and truncated to 16 bytes (0x + 32 hex chars) to meet Hyperliquid's 128-bit cloid format requirement
