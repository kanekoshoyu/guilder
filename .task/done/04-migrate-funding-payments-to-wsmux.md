# Migrate subscribe_funding_payments to WsMux

## Goal
`subscribe_funding_payments` (client.rs:1267) opens its own WS connection via `ws_subscribe()`. Migrate to `self.ws_mux.subscribe()`.

## What to change

**File:** `client/guilder-client-hyperliquid/src/client.rs`

Replace lines 1267–1294. Uses `userEvents` channel (same as user_fills):

```rust
fn subscribe_funding_payments(&self) -> BoxStream<Result<FundingPayment, String>> {
    let Some(addr) = self.user_address else {
        return Box::pin(stream::empty());
    };
    let sub = serde_json::json!({
        "method": "subscribe",
        "subscription": {"type": "userEvents", "user": format!("{:#x}", addr)}
    });
    let key = crate::ws::SubKey {
        channel: "userEvents".to_string(),
        routing_key: format!("{:#x}", addr),
    };
    let raw_stream = self.ws_mux.subscribe(key, sub);
    // Map raw JSON through existing WsUserEvent.funding parse logic
}
```

## Acceptance criteria
- Uses `self.ws_mux` instead of `ws_subscribe`
- Parse logic unchanged
- Compiles

## Notes
- Depends on task 01
- Same SubKey as `subscribe_user_fills` (`userEvents` + same addr) — the mux will fan out the same messages to both subscribers. This is correct: each stream filters for its own event type.
