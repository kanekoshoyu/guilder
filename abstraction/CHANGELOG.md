# Changelog
All notable changes to both codegen and trading.yaml will be documented in this file.

## [0.1.24] — 2026-05-18

### Changed
- **Breaking**: Migrated all traits to use `#[async_trait]` for better cross-platform `Send` guarantees
  - `TestServer`, `GetMarketData`, `ManageOrder`, `SubscribeMarketData`, `GetAccountSnapshot`, `SubscribeUserEvents`, `SubscribeMarketDataOps`
  - Method signatures changed from `fn xxx() -> impl Future<Output = ...> + Send + '_` to `async fn xxx()`
  - This ensures async futures are `Send` when used across thread boundaries (required for stable Rust compilation in Docker)
- **Breaking**: Removed `use std::future::Future` and `use std::pin::Pin` from trait definitions (now handled by `async-trait`)

### Added
- `async-trait = "0.1"` dependency for async trait support

### Fixed
- Docker compilation failures on stable Rust — async futures now properly implement `Send`

## [0.1.23] - 2026-04-24
### Changed
- **Breaking**: replace `Balance` struct with `AccountBalance` — adds margin health fields (`safe`, `usable`, `margin_used`, `maintenance`)
- **Breaking**: consolidate `get_collateral`, `get_spot_balance`, `get_collateral_balance` into single `get_balance` returning `Vec<AccountBalance>`
- **Breaking**: `subscribe_spot_balance` / `subscribe_spot_balance_with_address` now return `Stream<Result<Vec<AccountBalance>, String>>`
### Added
- `unsubscribe_user_events` to `SubscribeAccount` trait
- `SubscribeMarketDataOps` trait with `unsubscribe_market_data`
### Added
- `L2Level` struct (`price: Decimal`, `volume: Decimal`)
- `L2Snapshot` struct (`symbol`, `bids: Vec<L2Level>`, `asks: Vec<L2Level>`, `sequence`)
- `subscribe_l2_snapshot` to `SubscribeMarketData` trait — returns `Stream<Result<L2Snapshot, String>>`
### Changed
- `GetMarketData::get_l2_orderbook` return type: `Result<Vec<L2Update>, String>` → `Result<L2Snapshot, String>`

## [0.1.4] - 2026-03-07
### Added
- `SubscribeMarketData` trait with `subscribe_l2_update` and `subscribe_fill` (async, WebSocket)
- `Stream<T>` as first-class DSL type mapping to `impl futures_core::Stream<Item=T>` in Rust and `AsyncIterator[T]` in Python
- `Iterator<T>` as first-class DSL type mapping to `impl Iterator<Item=T>` in Rust and `Iterator[T]` in Python
- `async: true` trait-level flag in DSL for async-only traits
- New enums: `Side`, `OrderSide`, `MarketType`, `OrderType`, `TimeInForce`, `VolumeDenomination`, `AssetClass`
- New structs: `L2Update`, `Trade`
- Enum codegen for Rust (`#[derive(Debug, Clone, PartialEq)]`) and Python (`class Foo(Enum)`)
### Fixed
- `Unit` type now correctly renders as `()` in Rust (was `"unit"`)
- `HashMap` comma parsing now handles nested generics correctly
- Generated Rust structs now have `#[derive(Debug, Clone)]`

## [0.1.3] - 2024-08-28
### Added
- support custom struct in yaml

## [0.1.2] - 2024-08-26
### Added
- support description
- support map
### Fixed
- python docstring position

## [0.1.1] - 2024-08-21
### Added
- support to list
- support no args

## [0.1.0] - 2024-08-20
### Added
- initial release
