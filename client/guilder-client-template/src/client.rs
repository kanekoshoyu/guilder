use guilder_abstraction::{self, Result<bool, String>, Result<i64, String>, Result<Vec<String>, String>, Result<f64, String>, L2Update, Fill, AssetContext, Liquidation};
use futures_core::Stream;
use futures_util::stream;
use reqwest::Client;

pub struct ExchangeClient {
    client: Client,
}

impl ExchangeClient {
    pub fn new() -> Self {
        ExchangeClient { client: Client::new() }
    }
}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::TestServer for ExchangeClient {
    async fn ping(&self) -> Result<bool, String> {
        unimplemented!()
    }

    async fn get_server_time(&self) -> Result<i64, String> {
        unimplemented!()
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::GetMarketData for ExchangeClient {
    async fn get_symbol(&self) -> Result<Vec<String>, String> {
        unimplemented!()
    }

    async fn get_price(&self, symbol: String) -> Result<f64, String> {
        unimplemented!()
    }

    async fn get_open_interest(&self, symbol: String) -> Result<f64, String> {
        unimplemented!()
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::ManageOrder for ExchangeClient {
    async fn place_order(&self, symbol: String, price: f64, volume: f64) -> Result<i64, String> {
        unimplemented!()
    }

    async fn change_order_by_cloid(&self, cloid: i64, price: f64, volume: f64) -> Result<i64, String> {
        unimplemented!()
    }

    async fn cancel_order(&self, cloid: i64) -> Result<i64, String> {
        unimplemented!()
    }

    async fn cancel_all_order(&self) -> Result<bool, String> {
        unimplemented!()
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::SubscribeMarketData for ExchangeClient {
    fn subscribe_l2_update(&self, symbol: String) -> impl Stream<Item = L2Update> {
        stream::pending()
    }

    fn subscribe_fill(&self, symbol: String) -> impl Stream<Item = Fill> {
        stream::pending()
    }

    fn subscribe_asset_context(&self, symbol: String) -> impl Stream<Item = AssetContext> {
        stream::pending()
    }

    fn subscribe_liquidation(&self, user: String) -> impl Stream<Item = Liquidation> {
        stream::pending()
    }

}

