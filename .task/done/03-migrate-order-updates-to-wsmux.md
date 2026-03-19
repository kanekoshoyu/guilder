# Migrate subscribe_order_updates to WsMux

## Goal
`subscribe_order_updates` (client.rs:1223) opens its own WS connection via `ws_subscribe()`. Migrate to `self.ws_mux.subscribe()`.

## What to change

**File:** `client/guilder-client-hyperliquid/src/client.rs`

Replace lines 1223–1265. Same pattern as task 02 but for `orderUpdates` channel:

```rust
fn subscribe_order_updates(&self) -> BoxStream<Result<OrderUpdate, String>> {
    let Some(addr) = self.user_address else {
        return Box::pin(stream::empty());
    };
    let sub = serde_json::json!({
        "method": "subscribe",
        "subscription": {"type": "orderUpdates", "user": format!("{:#x}", addr)}
    });
    let key = crate::ws::SubKey {
        channel: "orderUpdates".to_string(),
        routing_key: format!("{:#x}", addr),
    };
    let raw_stream = self.ws_mux.subscribe(key, sub);
    // Map raw JSON strings through the existing parse logic (WsOrderUpdate deserialization)
}
```

Keep the existing `WsOrderUpdate` → `OrderUpdate` mapping logic exactly as-is.

## Acceptance criteria
- `subscribe_order_updates` uses `self.ws_mux` instead of `ws_subscribe`
- Parse logic unchanged
- Compiles

## Notes
- Depends on task 01
- `orderUpdates` uses a different channel name than `userEvents`, so it gets its own SubKey
