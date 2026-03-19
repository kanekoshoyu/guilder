# Hyperliquid WebSocket Connection Multiplexing

## Problem

Every `subscribe_*` call opens its own WebSocket connection. A strategy subscribing to
many symbols (e.g. 10× `subscribe_l2_update` + 4 user event streams) creates 14 separate
connections, which risks hitting server-side connection limits.

## Idea

Hyperliquid's WS protocol natively supports multiple subscriptions per connection — each
`{"method":"subscribe",...}` message can be sent over the same socket, and envelopes are
routed back by `channel` (+ coin/user key). Use a single shared connection with a routing
layer inside the client.

## Design Sketch

```
HyperliquidClient
  └── Arc<WsSession>  (lazily initialized on first subscribe)
        ├── tokio task: owns the single WS connection loop
        ├── cmd_tx: mpsc::Sender<SessionCmd>   (Subscribe / Unsubscribe)
        └── active_subs: HashMap<SubKey, Vec<mpsc::Sender<WsEnvelope>>>
```

- `subscribe_l2_update("BTC")` creates an `mpsc::channel`, sends `Subscribe` cmd, returns a stream wrapping the receiver.
- The session task routes each incoming envelope by `(channel, coin/user)` to all matching senders.
- Ping/pong and reconnect are handled once centrally.
- On reconnect, the task replays all active subscriptions from `active_subs`.

## Design Decisions to Resolve

1. **Lazy vs eager connection** — open WS on first `subscribe_*` call, not on `new()`.
   Many usages are REST-only and shouldn't pay the WS overhead.

2. **Reconnect re-subscription** — session task must maintain the full list of active
   `SubKey`s so it can replay `{"method":"subscribe",...}` after reconnect.

3. **Subscriber drop / unsubscribe** — when a returned stream is dropped, detect via
   closed sender and send `{"method":"unsubscribe",...}`, then remove the entry.

4. **`userEvents` fan-out** — `subscribe_liquidation`, `subscribe_user_fills`, and
   `subscribe_funding_payments` all use `userEvents` for the same user address. They should
   share one WS subscription; the router filters by event type on the consumer side.

5. **Interior mutability** — traits take `&self`, so prefer a message-passing design
   (cmd channel into the session task) over `Mutex` to avoid holding locks across await points.

## Files Affected

- `client/guilder-client-hyperliquid/src/client.rs` — replace `ws_subscribe` free function
  with a `WsSession` struct; subscription methods send commands instead of opening connections.
