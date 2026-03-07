# Changelog
All notable changes to both codegen and trading.yaml will be documented in this file.

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
