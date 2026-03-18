# Hyperliquid WebSocket Ping/Pong Keepalive

## Problem

All WebSocket subscriptions drop after ~1 hour. The read loop pattern:

```rust
while let Some(Ok(Message::Text(text))) = ws.next().await { ... }
```

silently discards non-text frames (Ping, Pong, Close). `tokio-tungstenite` does **not** auto-reply to server pings — the caller must respond manually.

Additionally, no client-side keepalive ping is sent, so the server closes idle connections.

## Fix Required

In every `subscribe_*` method in `client/guilder-client-hyperliquid/src/client.rs`:

1. **Handle server Ping frames** — match `Message::Ping(data)` and reply with `ws.send(Message::Pong(data)).await`.

2. **Send application-level pings** — use `tokio::select!` with a `tokio::time::interval` (~50s) to send `{"method": "ping"}` JSON pings to keep the connection alive.

## Affected Methods

All `subscribe_*` methods in `HyperliquidClient`:
- `subscribe_l2_update`
- `subscribe_asset_context`
- `subscribe_liquidation`
- `subscribe_fill`
- `subscribe_order_update`
- `subscribe_ledger_update` (deposits/withdrawals)
- any others sharing the same `ws.next()` loop pattern

## Notes

- The loop refactor is the same for every subscription — consider extracting a shared WS read helper to avoid repetition.
- Hyperliquid docs recommend a ping every ~50 seconds.
