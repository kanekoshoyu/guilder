/// TODO change it to derive Copy
/// currency pair model for easier conversion, default using uppercase
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CurrencyPair {
    /// BTC in BTC-USDT
    base_currency: String,
    /// USDT in BTC-USDT
    quote_currency: String,
}
impl CurrencyPair {
    pub fn new(base_currency: &str, quote_currency: &str) -> Self {
        CurrencyPair {
            base_currency: base_currency.to_string().to_uppercase(),
            quote_currency: quote_currency.to_string().to_uppercase(),
        }
    }
    pub fn from_underscore(input: &str) -> Option<Self> {
        let n = input.find("_")?;
        Some(CurrencyPair {
            base_currency: input[..n].to_string().to_uppercase(),
            quote_currency: input[n + 1..].to_string().to_uppercase(),
        })
    }
    pub fn from_hyphen(input: &str) -> Option<Self> {
        let n = input.find("-")?;
        Some(CurrencyPair {
            base_currency: input[..n].to_string().to_uppercase(),
            quote_currency: input[n + 1..].to_string().to_uppercase(),
        })
    }
    pub fn name_underscore(&self) -> String {
        format!("{}_{}", self.base_currency, self.quote_currency)
    }

    pub fn name_nospace(&self) -> String {
        format!("{}{}", self.base_currency, self.quote_currency)
    }
    pub fn name_hyphen(&self) -> String {
        format!("{}-{}", self.base_currency, self.quote_currency)
    }
}

mod tests {

    #[test]
    fn test_name() {
        use super::*;
        let pair = CurrencyPair::new("eth", "btc");
        assert_eq!(pair.name_nospace(), "ETHBTC");
        assert_eq!(pair.name_hyphen(), "ETH-BTC");
        assert_eq!(pair.name_underscore(), "ETH_BTC");
    }
    #[test]
    fn test_load() {
        use super::*;
        let pair = CurrencyPair::from_underscore("eth_btc").unwrap();
        assert_eq!(pair.name_nospace(), "ETHBTC");
        let pair = CurrencyPair::from_hyphen("eth-btc").unwrap();
        assert_eq!(pair.name_nospace(), "ETHBTC");
    }
}
