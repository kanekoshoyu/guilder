# Migrate subscribe_user_fills to WsMux

## Goal
`subscribe_user_fills` (client.rs:1181) currently calls standalone `ws_subscribe()` which opens its own WS connection. Migrate it to use `self.ws_mux.subscribe()` instead.

## What to change

**File:** `client/guilder-client-hyperliquid/src/client.rs`

Replace the `subscribe_user_fills` implementation (lines 1181–1221):

**Before:** calls `ws_subscribe(sub, |env| { ... })` which opens a dedicated WS connection.

**After:** calls `self.ws_mux.subscribe(key, sub)` which returns a `BoxStream<String>`, then maps each raw JSON string through the same parse logic.

New implementation pattern:
```rust
fn subscribe_user_fills(&self) -> BoxStream<Result<UserFill, String>> {
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
    Box::pin(raw_stream.filter_map(|text| {
        let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else {
            return futures_util::future::ready(None);
        };
        if env.channel != "userEvents" {
            return futures_util::future::ready(None);
        }
        let Ok(event) = serde_json::from_value::<WsUserEvent>(env.data) else {
            return futures_util::future::ready(None);
        };
        // Convert fills to Ok items, flatten
        let items: Vec<_> = event.fills.unwrap_or_default().into_iter().filter_map(|fill| {
            // ... existing parse logic unchanged ...
        }).collect();
        futures_util::future::ready(if items.is_empty() { None } else { Some(stream::iter(items.into_iter().map(Ok))) })
    }).flatten())
}
```

## Acceptance criteria
- `subscribe_user_fills` uses `self.ws_mux` instead of `ws_subscribe`
- Parse logic for fills is unchanged
- Returns `BoxStream<Result<UserFill, String>>` (same signature)
- Compiles

## Notes
- Depends on task 01 (WsMux routing key support) being done first
- The `WsMux` stream returns raw JSON strings, not `Result` — errors (reconnect etc.) are handled by the mux internally. The returned stream only yields successfully parsed items wrapped in `Ok`.
- `subscribe_user_fills` and `subscribe_funding_payments` both subscribe to `userEvents` with the same user — they will share one WS subscription via the mux automatically (same SubKey)
