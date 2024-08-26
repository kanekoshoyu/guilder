use guilder_abstraction;
use reqwest::Client;
// TODO set up below from exchange_yaml
// use binance_model;

pub struct BinanceClient {
    client: Client,
}
impl BinanceClient {
    pub fn new() -> Self {
        BinanceClient {
            client: Client::new(),
        }
    }
}
impl guilder_abstraction::GetMarketData for BinanceClient {
    fn get_symbol(&self) -> Vec<String> {
        todo!()
    }

    fn get_price(&self, symbol: String) -> f64 {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::BinanceClient;

    #[test]
    fn test_get_market_data() {
        use guilder_abstraction::GetMarketData;
        let client = BinanceClient::new();
        assert!(client.get_symbol().len() > 0);
    }
}
