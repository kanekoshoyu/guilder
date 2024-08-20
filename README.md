# guilder-dsl
GuilderDSL: Crypto Trading DSL (domain specific language) in YAML

define generic interfaces for different programming languages  
official support: rust, python


# code structure

| component                       | description                                                  |
| ------------------------------- | ------------------------------------------------------------ |
| [trading.yaml](./trading.yaml)  | definition of trading traits                                 |
| [/codegen](./codegen/README.md) | codegen for different programming lanugages, written in rust |
| [/target](./target/README.md)   | generated trading trait code                                 |
