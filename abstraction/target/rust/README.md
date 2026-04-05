# guilder-abstraction

[![crates](https://img.shields.io/crates/v/guilder-abstraction)](https://crates.io/crates/guilder-abstraction)
[![license](https://img.shields.io/github/license/kanekoshoyu/guilder)](https://github.com/kanekoshoyu/guilder/blob/master/LICENSE)

Auto-generated trading traits for the [guilder](https://github.com/kanekoshoyu/guilder) multi-exchange crypto trading library.

This crate defines the interface contract that every exchange client implements. Strategies coded against these traits work on any exchange — Binance, Hyperliquid, or any future integration — with zero code changes.

## Traits

| Trait | Description |
|---|---|
| `TestServer` | Connectivity check — ping and server time |
| `GetMarketData` | Symbols, prices, L2 orderbook snapshots, open interest, funding rates |
| `GetAccountSnapshot` | Account state — collateral, positions, open orders |
| `ManageOrder` | Place, modify, and cancel orders |
| `SubscribeMarketData` | Stream L2 updates, fills, asset context, liquidations |
| `SubscribeUserEvents` | Stream user fills, order updates, funding payments, deposits, withdrawals |

All traits are async. Subscription methods return `BoxStream<Result<T, String>>` for object safety.

## Key types

### Structs

| Struct | Description |
|---|---|
| `L2Update` | Orderbook level-2 update — side, price, volume, sequence number |
| `Fill` | Trade fill — side, price, volume, timestamp |
| `AssetContext` | Per-asset market snapshot — mark price, open interest, funding rate, 24h volume |
| `Liquidation` | Liquidation event for an address |
| `UserFill` | User-specific trade fill |
| `OrderUpdate` | Order status change (placed, filled, cancelled) |
| `FundingPayment` | Funding payment received or paid |
| `Deposit` / `Withdrawal` | Account deposit and withdrawal events |

### Enums

| Enum | Description |
|---|---|
| `OrderSide` | `Buy` / `Sell` |
| `OrderType` | `Limit` / `Market` |
| `Side` | `Ask` / `Bid` (for orderbook levels) |

### Stream type

```rust
pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;
```

## Usage

```toml
[dependencies]
guilder-abstraction = "0.1"
```

### Implementing a client

```rust
use guilder_abstraction::{GetMarketData, L2Update};

pub struct MyExchangeClient { /* ... */ }

impl GetMarketData for MyExchangeClient {
    async fn get_symbol(&self) -> Result<Vec<String>, String> {
        // call your exchange's REST API
    }

    async fn get_l2_orderbook(&self, symbol: String) -> Result<Vec<L2Update>, String> {
        // fetch L2 snapshot
    }

    // ... other methods
}
```

### Writing exchange-agnostic code

```rust
use guilder_abstraction::GetMarketData;

async fn print_symbols(client: &impl GetMarketData) {
    let symbols = client.get_symbol().await.unwrap();
    println!("{:?}", symbols);
}
```

## How it's generated

This crate is **auto-generated** from [`trading.yaml`](https://github.com/kanekoshoyu/guilder/blob/main/abstraction/trading.yaml) — a language-neutral DSL that defines traits, structs, and enums using only primitives and self-referencing types. The codegen produces idiomatic code for each target language.

**Do not edit the generated source directly.** Edit `trading.yaml` and regenerate:

```sh
cd abstraction/codegen && cargo run
```

## Design rules

- `trading.yaml` uses only primitives and types defined within the YAML — no external crate types
- Generated code depends only on the standard library (plus `futures-core` for `Stream`)
- Subscription methods return `BoxStream`, not `async fn` — keeping traits object-safe

## Exchange clients

Implement these traits for your exchange:

| Crate | Exchange | Status |
|---|---|---|
| [`guilder-client-hyperliquid`](https://crates.io/crates/guilder-client-hyperliquid) | Hyperliquid | All traits implemented |
| [`guilder-client-binance`](https://crates.io/crates/guilder-client-binance) | Binance | Not started |

Find all clients: [crates.io/search?q=guilder-client](https://crates.io/search?q=guilder-client)

## Part of guilder

```
trading.yaml → guilder-abstraction → guilder-core → your strategy
                                   ↗
              guilder-client-* ────
```

See the [guilder repository](https://github.com/kanekoshoyu/guilder) for the full architecture.
