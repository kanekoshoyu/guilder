# Guilder

Unopinionated multi-language cross-exchange crypto trading library in Rust.

## Architecture

| Component | Path | Description |
|---|---|---|
| Abstraction | `abstraction/` | Trading traits defined in `trading.yaml`, auto-generated into Rust/Python |
| Codegen | `abstraction/codegen/` | Reads `trading.yaml` and generates trait code |
| Core | `core/` | Common trading components (`Orderbook`, `CurrencyPair`) built on abstraction |
| Clients | `client/` | Exchange implementations (Binance, Hyperliquid) |

## How It Works

1. `abstraction/trading.yaml` defines traits, enums, and structs in a language-neutral format
2. `abstraction/codegen` generates `abstraction/target/rust/` and `abstraction/target/python/`
3. Exchange clients implement the generated traits against real APIs
4. Strategies coded against the traits work across any exchange

## trading.yaml Structure

- **traits**: `TestServer`, `GetMarketData`, `ManageOrder`
- **enums**: `Status` (Success, InProgress, Completed, Failed)
- **structs**: `Orderbook` (asks/bids as `HashMap<f64, f64>`)

## Design Principles

- Sync by default, async as a feature flag
- `trading.yaml` uses only primitives, custom enums/structs — no external types
- Generated code uses only the standard library
- Core uses only standard + networking libraries by default; variants (dashmap, tokio) are feature-gated

## Key Dependencies

- `ordered-float` — ordered f64 for orderbook keys
- `dashmap` — concurrent hashmap (feature)
- `serde` / `serde_yaml` — YAML parsing in codegen
- `reqwest` — REST API calls in clients
