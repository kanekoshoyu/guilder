# Remove standalone ws_subscribe function

## Goal
After tasks 02–05, the `ws_subscribe` free function (client.rs:443–546) is no longer called. Remove it and clean up unused imports.

## What to change

**File:** `client/guilder-client-hyperliquid/src/client.rs`

1. Delete the `ws_subscribe` function (lines 443–546)
2. Remove the `HYPERLIQUID_WS_URL` constant (line 20) if no longer used in this file
3. Remove unused imports: `connect_async`, `tungstenite::Message` if no longer needed in client.rs
4. Remove `WsEnvelope` struct if it was defined in client.rs and is no longer used there — BUT check first: the mux migration streams still need to deserialize envelopes. If `WsEnvelope` is used in the new stream mapping closures, keep it.

## Acceptance criteria
- `ws_subscribe` function is deleted
- No dead code warnings related to removed items
- Compiles and tests pass

## Notes
- This is a cleanup task — do it last after all migrations are confirmed working
- Run `cargo check` to verify no remaining references
