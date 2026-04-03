# Market-wide liquidation stream

## Goal
Replace the current per-user `subscribe_liquidation(user)` with a market-wide liquidation feed so albatross can detect liquidation pressure across all coins without needing a specific wallet address.

## Context
- Native Hyperliquid WS API only has per-user liquidations via `userEvents` — no exchange-wide subscription
- HypeRPC (`wss://rpc.hyperliquid.xyz/ws` or similar) reportedly offers a `liquidations` channel that streams all liquidation events exchange-wide
- Current `subscribe_liquidation` subscribes to `userEvents` for a single address — useless for market-wide liquidation detection
- Albatross's `LiquidationStressStrategyV1` needs aggregate liquidation volume per coin to compute stress ratios
- **Two possible data sources:** native Hyperliquid API (per-user only) vs HypeRPC (market-wide, third-party dependency)

## Tasks

### T1a: Verify MoonDev liquidations REST endpoint and document schema
**Depends on:** nothing
**Size:** small
**What:** Fetch `GET https://api.moondev.com/api/liquidations/10m.json` (requires `MOONDEV_API_KEY` header/param — get free key at moondev.com). Log raw response and document exact schema. From the example code, the response shape is:
```json
{
  "stats": {
    "total_count": 123,
    "total_value_usd": 456789,
    "long_count": 60, "short_count": 63,
    "long_value_usd": 200000, "short_value_usd": 256789,
    "by_coin": {
      "BTC": { "count": 10, "total_value_usd": 100000, "long_value_usd": 50000, "short_value_usd": 50000 },
      ...
    },
    "largest": [
      { "coin": "BTC", "side": "long", "value_usd": 50000, "price": 65000.0, "address": "0x...", "timestamp": 1711... },
      ...
    ]
  }
}
```
Confirm this schema, check auth mechanism (header vs query param), and test `10m` + `1h` timeframes.

### T1b: Verify HypeRPC WS `liquidations` channel (optional/stretch)
**Depends on:** nothing
**Size:** small
**What:** If MoonDev REST works, this becomes lower priority. But worth confirming whether HypeRPC's WS endpoint has a real-time liquidation stream for future low-latency needs. Document findings.

### T2: Update abstraction — add `poll_liquidations` to `trading.yaml`
**Depends on:** T1a (need confirmed schema)
**Size:** small
**What:**
- Add a new method to a trait in `abstraction/trading.yaml` for polling liquidation snapshots
- Design decision: this is REST polling, not a stream subscription. Two options:
  - (A) `poll_liquidations(timeframe: String) -> Result<LiquidationSnapshot, String>` — returns a point-in-time snapshot, caller manages the polling interval
  - (B) `subscribe_all_liquidations(interval_secs: u64) -> Stream<Result<Liquidation, String>>` — wraps polling in a stream internally
- Option A is simpler at the abstraction level; albatross can wrap it in a `tokio::time::interval` loop
- Update `Liquidation` struct if needed to match MoonDev schema (add `price`, `value_usd` fields?)
- Add `LiquidationSnapshot` struct if going with option A (contains `stats` + `largest` list)
- Run codegen to regenerate `guilder_abstraction.rs`

### T3: Implement MoonDev liquidation polling in hyperliquid client
**Depends on:** T2
**Size:** medium
**What:**
- In `client/guilder-client-hyperliquid/src/client.rs`, implement the new trait method
- Simple HTTP GET to `https://api.moondev.com/api/liquidations/{timeframe}.json`
- Auth: pass `MOONDEV_API_KEY` (env var) — determine if it's a header or query param from T1a
- Deserialize JSON response into `LiquidationSnapshot` / `Liquidation` structs
- No WsMux changes needed — this is pure REST
- Add `reqwest` or reuse existing HTTP client for the GET request

### T4: Deprecate or keep `subscribe_liquidation(user)`
**Depends on:** T3
**Size:** small
**What:** Decide whether to keep the per-user `subscribe_liquidation(user)` for other use cases or remove it. If keeping both, ensure naming is clear. If removing, update `trading.yaml`, codegen, and all downstream consumers (albatross `GuilderBridge`).

### T5: Update albatross to use new polling
**Depends on:** T3, T4
**Size:** small
**What:**
- In `albatross/src/engine/market/guilder.rs`, add a polling loop (e.g. every 30s) that calls the new liquidation method
- Convert each liquidation record into the existing `Update::Liquidation` / `on_liquidation()` flow
- Remove the `liquidation_user: Option<String>` parameter from `GuilderBridge::run()`
- Revert the `main.rs` change that passes `config.wallet_address` — no address needed anymore
- Need deduplication: track last-seen timestamp or set of liquidation IDs to avoid re-emitting on overlapping polls

## Dependency graph
```
T1a ── T2 ── T3 ── T4
                │    │
                └────┴── T5

T1b (optional, parallel with everything)
```

## Notes
- MoonDev REST polling at 30s is sufficient for liquidation pressure detection over a rolling window
- Free API key required from moondev.com — add `MOONDEV_API_KEY` to albatross config/env
- Deduplication in T5 is important: overlapping time windows (e.g. polling `10m.json` every 30s) will return repeated events
- Native Hyperliquid API has NO exchange-wide liquidation endpoint (REST or WS) — third-party is the only option
- T1b (HypeRPC WS) is a stretch goal for lower-latency needs in the future
- Keep the existing `Liquidation` struct stable if possible to minimize downstream changes
- MoonDev also offers multi-exchange liquidations (`/api/all_liquidations/`) and per-exchange endpoints — could be useful later
