# guilder-client-hyperliquid

Hyperliquid exchange client implementing the guilder abstraction traits.

---

## Rate Limits

### REST API

All REST requests share an **aggregated weight budget of 1 200 per minute**.

#### Info endpoint (`/info`) weights

| Request type | Weight |
|---|---|
| `l2Book` | 2 |
| `allMids` | 2 |
| `clearinghouseState` | 2 |
| `orderStatus` | 2 |
| `spotClearinghouseState` | 2 |
| `exchangeStatus` | 2 |
| `userRole` | 60 |
| All other info requests (e.g. `meta`, `metaAndAssetCtxs`, `openOrders`, `predictedFundings`) | 20 |

Some endpoints carry an **additional weight per N items returned**:

| Endpoint | Extra weight per |
|---|---|
| `recentTrades`, `historicalOrders`, `userFills`, `userFillsByTime`, `fundingHistory`, `userFunding`, `nonUserFundingUpdates`, `twapHistory`, `userTwapSliceFills`, `userTwapSliceFillsByTime`, `delegatorHistory`, `delegatorRewards`, `validatorStats` | per 20 items |
| `candleSnapshot` | per 60 items |

#### Exchange endpoint (`/exchange`) weights

`weight = 1 + floor(batch_length / 40)`

- Single (unbatched) action → weight **1**
- Batch of 79 orders → weight **2**

`batch_length` is the length of the array in the action (e.g. number of orders in a batch).

#### Explorer API weights

All explorer API requests have weight **40**.
`blockList` has an additional rate limit of **1 per block**.
Older blocks that have not been recently queried may be weighted more heavily.
For large batch requests, use the S3 bucket instead.

---

### WebSocket API

| Limit | Value |
|---|---|
| Maximum simultaneous connections | 10 |
| Maximum new connections per minute | 30 |
| Maximum subscriptions | 1 000 |
| Maximum unique users across user-specific subscriptions | 10 |
| Maximum messages sent to Hyperliquid per minute (across all connections) | 2 000 |
| Maximum simultaneous inflight post messages (across all connections) | 100 |

---

### Address-Based Limits

These limits apply per user address. Sub-accounts are treated as separate users. They apply to **actions only** (exchange endpoint), not to info requests.

#### Request budget

| Parameter | Value |
|---|---|
| Initial buffer | 10 000 requests |
| Ongoing accrual | 1 request per 1 USDC traded (cumulative since address inception) |
| Example: 100 USDC order value requires 1% fill rate to sustain budget | |
| Rate-limited fallback | 1 request per 10 seconds |

#### Cancel limit

`cancel_limit = min(default_limit + 100 000, default_limit × 2)`

Even when the address-based budget is exhausted, this higher cancel limit ensures open orders can still be canceled.

#### Open order limit

| Condition | Limit |
|---|---|
| Default | 1 000 open orders |
| Additional | +1 per 5 000 000 USDC of volume |
| Maximum | 5 000 open orders |

When an address already has ≥ 1 000 open orders, any new order that is **reduce-only** or a **trigger order** will be rejected.

#### Block-space congestion

During high congestion, each address is limited to **2× its maker-share percentage** of block space. To avoid wasting that quota, do not resend cancels whose results have already been returned by the API.

#### Batched requests

A batch of `n` orders or cancels counts as **1 request** toward IP-based limits but as **n requests** toward address-based limits.

---

### EVM JSON-RPC (`rpc.hyperliquid.xyz/evm`)

Maximum **100 requests per minute**.
Other JSON-RPC providers have more sophisticated rate limiting logic and archive node functionality.

---

## Endpoint → Weight Reference (this client)

| Method | Endpoint | Weight |
|---|---|---|
| `ping` | `allMids` | 2 |
| `get_symbol` | `meta` | 20 |
| `get_open_interest` | `metaAndAssetCtxs` | 20 |
| `get_asset_context` | `metaAndAssetCtxs` | 20 |
| `get_all_asset_contexts` | `metaAndAssetCtxs` | 20 |
| `get_l2_orderbook` | `l2Book` | 2 |
| `get_price` | `allMids` | 2 |
| `get_predicted_fundings` | `predictedFundings` | 20 |
| `get_positions` | `clearinghouseState` | 2 |
| `get_open_orders` | `openOrders` | 20 |
| `get_collateral` | `clearinghouseState` | 2 |
| `place_order` (single) | `meta` + exchange | 20 + 1 = 21 |
| `change_order_by_cloid` | `openOrders` + `meta` + exchange | 20 + 20 + 1 = 41 |
| `cancel_order` | `openOrders` + `meta` + exchange | 20 + 20 + 1 = 41 |
| `cancel_all_order` | `openOrders` + `meta` + exchange | 20 + 20 + 1 = 41 |
