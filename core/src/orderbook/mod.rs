use guilder_abstraction::GetMarketData;
use std::collections::HashMap;

pub struct Orderbook {
    ask: HashMap<f64, f64>,
    bid: HashMap<f64, f64>,
}
impl Orderbook {
    pub fn new() -> Self {
        Orderbook {
            ask: HashMap::new(),
            bid: HashMap::new(),
        }
    }

    pub fn init(client: impl GetMarketData) -> Self {
        let orderbook = Orderbook::new();
        // TODO fill in the data, shall we use the orderbook API?
        // client.get_price(symbol);
        orderbook
    }
}
