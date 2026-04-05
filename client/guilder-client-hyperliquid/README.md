# guilder-client-hyperliquid

[![crates](https://img.shields.io/crates/v/guilder-client-hyperliquid)](https://crates.io/crates/guilder-client-hyperliquid)
[![license](https://img.shields.io/github/license/kanekoshoyu/guilder)](https://github.com/kanekoshoyu/guilder/blob/master/LICENSE)

[Hyperliquid](https://hyperliquid.xyz) exchange client for the [guilder](https://github.com/kanekoshoyu/guilder) multi-exchange crypto trading library.

Implements all guilder abstraction traits against the Hyperliquid REST and WebSocket APIs, so strategies written against the traits work on Hyperliquid with no exchange-specific code.

## Implementation status

| Trait | Status |
|---|---|
| `TestServer` | Complete |
| `GetMarketData` | Complete |
| `GetAccountSnapshot` | Complete |
| `ManageOrder` | Complete |
| `SubscribeMarketData` | Complete |
| `SubscribeUserEvents` | Complete |

## Usage

```toml
[dependencies]
guilder-client-hyperliquid = "0.4"
```

```rust
use guilder_client_hyperliquid::HyperliquidClient;
use guilder_abstraction::{TestServer, GetMarketData};

let client = HyperliquidClient::new(/* config */);

// all guilder traits are available
let symbols = client.get_symbol().await?;
let book = client.get_l2_orderbook("BTC".into()).await?;
```

### With the orderbook engine

```rust
use guilder_core::OrderbookEngine;

let engine = OrderbookEngine::new(client);
engine.track(vec!["BTC".into(), "ETH".into()]).await?;
```

## Features

- **Auto-reconnecting WebSocket streams** — `subscribe_*` methods return logically persistent streams that auto-reconnect on connection loss (5s delay). Consumers see transient `Err` items but the stream never terminates due to a dropped connection.
- **WebSocket multiplexing** — multiple subscriptions share a single connection where possible.
- **Configurable rate limiting** — respects Hyperliquid's weight-based rate limits with configurable budgets.

## Rate limits

### REST API

All REST requests share an **aggregated weight budget of 1,200 per minute**.

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
| All other info requests | 20 |

#### Exchange endpoint (`/exchange`) weights

`weight = 1 + floor(batch_length / 40)`

### WebSocket API

| Limit | Value |
|---|---|
| Max simultaneous connections | 10 |
| Max new connections per minute | 30 |
| Max subscriptions | 1,000 |
| Max messages sent per minute (all connections) | 2,000 |

### Method weight reference

| Method | Endpoint | Weight |
|---|---|---|
| `ping` | `allMids` | 2 |
| `get_symbol` | `meta` | 20 |
| `get_l2_orderbook` | `l2Book` | 2 |
| `get_price` | `allMids` | 2 |
| `get_open_interest` | `metaAndAssetCtxs` | 20 |
| `get_asset_context` | `metaAndAssetCtxs` | 20 |
| `get_predicted_fundings` | `predictedFundings` | 20 |
| `get_positions` | `clearinghouseState` | 2 |
| `get_open_orders` | `openOrders` | 20 |
| `get_collateral` | `clearinghouseState` | 2 |
| `place_order` | `meta` + exchange | 21 |
| `change_order_by_cloid` | `openOrders` + `meta` + exchange | 41 |
| `cancel_order` | `openOrders` + `meta` + exchange | 41 |
| `cancel_all_order` | `openOrders` + `meta` + exchange | 41 |

## Part of guilder

```
trading.yaml → guilder-abstraction → guilder-core → your strategy
                                   ↗
              guilder-client-* ────
```

See the [guilder repository](https://github.com/kanekoshoyu/guilder) for the full architecture.
