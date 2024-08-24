# guilder
cross exchange, multi-language crypto trading

## how it works
- **guilder-abstraction** defines the common traits for cryoto trading. It's specs are defined in a single YAML and trait codes are auto-generated.
- As we implement trading strategies purely using these traits, we can switch to new exchange with minimal marginal effort.
- We can either auto-generate cross-language FFI bindings in the future, or we can just implement them natively in different languages.
- Each exchange has to implement guilder-abstraction, and we use exchange-yaml models to create the client with ease. We can also implement the traits on any custom models. 


## code structure

| component                              | description                                                |
| -------------------------------------- | ---------------------------------------------------------- |
| [abstraction](./abstraction/README.md) | trading traits that abstract out each exchange             |
| [core](./core/README.md)               | key trading components built on top of guilder-abstraction |
| [client](./client/README.md)           | exchange clients that implement the abstraction            |
