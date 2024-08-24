# [guilder](../README.md)-abstraction
GuilderDSL: Crypto Trading DSL (domain specific language) in YAML  
define generic interfaces for different programming languages  
official support: rust, python

## code structure

| component                       | description                                                           |
| ------------------------------- | --------------------------------------------------------------------- |
| [trading.yaml](./trading.yaml)  | definition of trading traits in a YAML DSL (domain specific language) |
| [codegen](./codegen/README.md) | codegen for different programming lanugages, written in rust          |
| [target](./target/README.md)   | generated trading trait code                                          |

## TODO
- [ ] package rust abstraction
- [ ] publish rust abstraction
- [ ] package python abstraction
- [ ] publish python abstraction
