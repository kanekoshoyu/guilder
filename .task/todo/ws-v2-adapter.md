# WS v2: WsTransport-Based Refactor

## Context

The v1 refactor (`client/guilder-client-hyperliquid/src/ws/`) split the monolithic `ws.rs` into `actor.rs`, `inbound.rs`, `outbound.rs`, `mod.rs` — typed messages, `Arc<InboundMessage>` routing, etc.

Problems with the current design:
- `WsMux.subscribe(key: SubKey, subscription: Value)` — caller must construct raw exchange JSON
- `client.rs` is full of `serde_json::json!({"type": "l2Book", "coin": symbol, ...})`
- Session management (reconnect, backoff, ping/pong) is hidden inside the actor — guilder layer can't control when to reconnect, resnapshot, or pause

## Design

### `WsTransport` trait

```rust
pub trait WsTransport {
    type Inbound;
    type Outbound;
    type Error;

    async fn connect(&mut self) -> Result<(), Self::Error>;
    async fn send(&mut self, msg: Self::Outbound) -> Result<(), Self::Error>;
    async fn recv(&mut self) -> Result<Self::Inbound, Self::Error>;
    async fn close(&mut self) -> Result<(), Self::Error>;
    fn is_connected(&self) -> bool;
}
```

Generic transport contract. Each exchange implements it with its own `Inbound`/`Outbound` types.

### `impl WsTransport for HyperliquidWs`

Exchange-specific transport using `tokio_tungstenite`:
- `Inbound = InboundMessage` (already exists in `inbound.rs`)
- `Outbound = OutboundMessage` (already exists in `outbound.rs`, just needs typed variants replacing the `Value`-based `Subscribe`)
- Handles raw socket I/O, wire serialization/deserialization, ping/pong

### Generic `WsSession<T: WsTransport>`

Reusable across exchanges. Owns:
- Connection lifecycle (connect, reconnect, backoff)
- Heartbeat
- Subscription routing (typed `InboundMessage` → subscribers)
- `GuilderSub` enum → mapped to `Outbound` via transport

### Key Changes

1. **Add `WsTransport` trait** in `core` or a new `ws-transport` crate
2. **Implement `WsTransport` for `HyperliquidWs`** in `ws/transport.rs`
3. **Generic `WsSession<T>`** in `ws/session.rs` — replaces actor + WsMux
4. **Type `OutboundMessage` variants** — replace `Subscribe { subscription: Value }` with typed params
5. **`client.rs`** — uses `WsSession<HyperliquidWs>` with typed subs, removes all `serde_json::json!` construction

### Files to Change

| File | Change |
|---|---|
| `ws/transport.rs` (new) | `impl WsTransport for HyperliquidWs` |
| `ws/session.rs` (new) | Generic `WsSession<T: WsTransport>` |
| `ws/outbound.rs` | Replace `Value`-based `Subscribe` with typed variants |
| `ws/inbound.rs` | No change — already typed |
| `ws/actor.rs` | Delete — replaced by session + transport |
| `ws/mod.rs` | Re-export session, transport, inbound, outbound |
| `client.rs` | Remove JSON sub construction; use session API |

### Not Changing

- `sync_loop`, `engine.rs`, `types.rs`, `storage.rs` — core orderbook layer
- `convert.rs` — apply_update, to_book_update
- `InboundMessage` parsing — already correct
