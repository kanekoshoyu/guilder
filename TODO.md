# Guilder TODO

## ✅ Multiplex WebSocket subscriptions over a single shared connection

**Problem**: `ws_subscribe` in `guilder-client-hyperliquid` opens a separate WebSocket connection per call. With ~200 coins × 2 subscriptions (fills + asset contexts) = ~400 concurrent connections to `wss://api.hyperliquid.xyz/ws`. Hyperliquid rate-limits/closes them, causing constant reconnection churn.

**Solution**: Multiplex all subscriptions over one shared WebSocket connection. Route incoming messages by `channel` + `coin`. Public `BoxStream` API stays unchanged.

### Completed Steps

- [x] **1. Create `ws.rs` with types** — Defined `SubKey`, `SubRequest`, `WsMux` structs in `src/ws.rs`. Added `pub(crate) mod ws;` to `lib.rs`.

- [x] **2. Implement `WsMux::new()` connection actor** — Spawned a tokio task that owns one WS connection, accepts `SubRequest`s, sends subscription JSONs, routes incoming messages to subscribers by `(channel, coin)`. Reconnects with exponential backoff (1s→60s, max 10 attempts). Re-subscribes all active subs on reconnect. Sends keepalive ping every 50s.

- [x] **3. Implement `WsMux::subscribe()`** — Returns `BoxStream<String>`. Creates an unbounded channel, sends `SubRequest` to the actor, wraps the receiver as a stream.

- [x] **4. Wire into `HyperliquidClient`** — Added `ws_mux: WsMux` field. Rewrote `subscribe_fill`, `subscribe_asset_context`, and `subscribe_l2_update` to use `self.ws_mux.subscribe(...)` instead of `ws_subscribe(...)`. Parse logic for messages preserved.

- [x] **5. Test and bump version** — Tests passing (L2 updates working), bumped to `0.3.0`. Built and verified with `cargo check`.

### Implementation Notes
- `src/ws.rs` handles multiplexing logic, reconnection, keepalive, and message routing
- All subscription messages routed by extracting `coin` from envelope data
- Existing parse logic moved into client methods that consume WsMux streams
- Tests updated to handle Result types from streams
