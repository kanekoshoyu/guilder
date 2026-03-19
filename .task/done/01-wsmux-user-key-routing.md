# WsMux: support user-address based routing (not just coin)

## Goal
The `WsMux` in `ws.rs` currently routes messages by `(channel, coin)` extracted from `env.data.coin`. User event channels (`userEvents`, `orderUpdates`, `userNonFundingLedgerUpdates`) don't have a `coin` field at the top level — they're keyed by user address. The mux needs to support routing these messages too.

## What to change

**File:** `client/guilder-client-hyperliquid/src/ws.rs`

1. Change `SubKey.coin` to a more generic name like `routing_key` (it can be a coin string OR a user address string — the mux doesn't need to care which).

2. Update the routing logic in `ws_actor` (around line 217) to extract the routing key more flexibly. Currently it only does:
   ```rust
   if let Some(coin) = env.data.get("coin").and_then(|c| c.as_str())
   ```
   Change this to also try `env.data.get("user")` as a fallback, or accept a routing key extractor. Simplest approach: try `coin` first, then `user`, then use empty string as catch-all.

3. Also handle the case where `userEvents` data is structured differently — the envelope for `userEvents` just has the event payload directly without a top-level `user` field. For these, the routing key should come from the subscription itself, not the message. One approach: when no routing key can be extracted from the message, fan out to **all** subscribers of that channel.

## Acceptance criteria
- `SubKey` uses a generic `routing_key: String` instead of `coin: String`
- Messages for channels like `userEvents`, `orderUpdates`, `userNonFundingLedgerUpdates` are correctly delivered to subscribers
- Existing market data routing (by coin) still works
- Compiles and existing tests pass

## Notes
- Don't change the `WsMux::subscribe` public API signature beyond renaming the `SubKey` field
- The `SubKey` is `pub(crate)` so only `client.rs` uses it — update callers there too
- Keep the fallback simple: try `coin`, then `user`, then broadcast to all subs for that channel
