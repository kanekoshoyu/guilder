use guilder_abstraction::{self, L2Update, Fill};
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
    async fn ping(&self) -> bool {
        unimplemented!()
    }

    async fn get_server_time(&self) -> i64 {
        unimplemented!()
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::GetMarketData for ExchangeClient {
    async fn get_symbol(&self) -> Vec<String> {
        unimplemented!()
    }

    async fn get_price(&self, symbol: String) -> f64 {
        unimplemented!()
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::ManageOrder for ExchangeClient {
    async fn place_order(&self, symbol: String, price: i32, volume: i32) -> i64 {
        unimplemented!()
    }

    async fn change_order_by_cloid(&self, cloid: i64, price: i32, volume: i32) -> i64 {
        unimplemented!()
    }

    async fn cancel_order(&self, cloid: i64) -> i64 {
        unimplemented!()
    }

    async fn cancel_all_order(&self) -> bool {
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

}

