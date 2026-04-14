# Changelog

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
