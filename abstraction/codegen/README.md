# guilder-abstraction-codegen

Reads `../trading.yaml` and generates code for three targets:

| Target | Path | Description |
|---|---|---|
| Rust abstraction | `../target/rust/src/guilder_abstraction.rs` | Trait and type definitions |
| Python abstraction | `../target/python/guilder_abstraction.py` | Abstract base classes |
| Client template | `../../client/guilder-client-template/` | Starting point for a new exchange client |

## Running

```
cargo run
```

## What gets generated

**Abstraction files** are always overwritten. Do not edit them — edit `../trading.yaml` and re-run.

**The client template** (`client/guilder-client-template/`) is also always overwritten. It contains:
- `Cargo.toml` — dependency list with placeholder package name `guilder-client-<exchange>`
- `src/client.rs` — impl stubs for every trait using a generic `ExchangeClient` struct

To start a new exchange client, copy the template directory, rename the package and struct, and implement the methods.

## trading.yaml structure

- **traits** — groups of related methods; each trait can be marked `async: true`
- **structs** — data types built from primitives and enums defined in the same file
- **enums** — simple value enumerations

Only primitives and types defined within the YAML are allowed — no external crate types.

## Async rules (Rust)

- Non-streaming methods in an `async: true` trait → `async fn`
- `Stream<T>` return types stay as plain `fn` returning `impl Stream<Item = T>` regardless of the trait's async flag
