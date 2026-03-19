# Migrate subscribe_deposits and subscribe_withdrawals to WsMux

## Goal
`subscribe_deposits` (client.rs:1296) and `subscribe_withdrawals` (client.rs:1329) each open their own WS connection via `ws_subscribe()`. Both subscribe to `userNonFundingLedgerUpdates`. Migrate both to `self.ws_mux.subscribe()`.

## What to change

**File:** `client/guilder-client-hyperliquid/src/client.rs`

Replace both methods. Both use the same SubKey:

```rust
let key = crate::ws::SubKey {
    channel: "userNonFundingLedgerUpdates".to_string(),
    routing_key: format!("{:#x}", addr),
};
```

- `subscribe_deposits`: filters for `e.delta.kind == "deposit"`
- `subscribe_withdrawals`: filters for `e.delta.kind == "withdraw"`

Keep existing `WsLedgerUpdates` parse logic unchanged for each.

## Acceptance criteria
- Both methods use `self.ws_mux` instead of `ws_subscribe`
- Parse logic unchanged for both
- Compiles

## Notes
- Depends on task 01
- Same SubKey for both — mux fans out, each stream filters by kind
