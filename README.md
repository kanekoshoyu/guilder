# Guilder: Unopinionated Cross-Exchange Crypto Trading Library
[![crates](https://img.shields.io/crates/v/guilder-abstraction)](https://crates.io/crates/guilder-abstraction)
[![license](https://img.shields.io/github/license/kanekoshoyu/guilder)](https://github.com/kanekoshoyu/guilder/blob/master/LICENSE)
[![discord](https://img.shields.io/discord/1153997271294283827)](https://discord.gg/q3j5MYdwnm)  

an unopinionated multi-language, cross-exchange crypto trading library

## how it works
- **guilder-abstraction** defines the common traits for crypto trading. It's specs are defined in a single YAML and trait codes are auto-generated.
- As we implement trading strategies purely using these traits, we can switch to new exchange with minimal marginal effort.
- We can either auto-generate cross-language (e.g. python) FFI bindings in the future, or we can just implement them natively in different languages. Main support will be in Rust.
- Each exchange has to implement **guilder-abstraction**, and we use exchange-yaml models to create the client with ease. We can also implement the traits on any custom models. 


## code structure

| component                              | description                                                                                                                     |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| [abstraction](./abstraction/README.md) | trading traits that abstract out each exchange                                                                                  |
| [core](./core/README.md)               | key trading components built on top of guilder-abstraction                                                                      |
| [client](./client/README.md)           | exchange clients that implement the abstraction using models from [exchange_yaml](https://github.com/kanekoshoyu/exchange_yaml) |

## guidelines to maintain unopinionated code
- sync as default, async as feature.
- abstraction only use standard libraries.
- abstraction only use traits and primitive data type (no custom structs).
- core only use standard and networking libraries in default. 
- any variants (e.g. dashmap, tokio) should be defined with feature.

## why the name?
**Dutch Guilder** was the currency used for 500+ years across the East India Company era for trading. I hope guilder trading library will be used by a lot of people for years. It also rhymes with **builder**.

In order to achieve it, we should make guilder:
- support more exchanges
- keep it versatile
  

## partnership
I keep this project opensource so that everyone can take part of it. 
If you want to get an exchange integrated, I can help get that up for an one-off cost in 3 weeks, just enough to pay my freelancing partner to get it done.
Please contact [Sho Kaneko](https://github.com/kanekoshoyu) for details.

## see also
- [exchange_yaml](https://github.com/kanekoshoyu/exchange_yaml) - collection of exchange OpenAPI/AsyncAPI docs and its generated models