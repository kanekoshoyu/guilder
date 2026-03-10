use guilder_abstraction::{self, Result<bool, String>, Result<i64, String>, Result<Vec<String>, String>, Result<Decimal, String>, Result<OrderPlacement, String>, OrderSide, OrderType, TimeInForce, L2Update, Fill, AssetContext, Liquidation, Result<Vec<Position>, String>, Result<Vec<OpenOrder>, String>, UserFill, OrderUpdate, FundingPayment, Deposit, Withdrawal};
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
        Err("not implemented".to_string())
    }

    async fn get_server_time(&self) -> Result<i64, String> {
        Err("not implemented".to_string())
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::GetMarketData for ExchangeClient {
    async fn get_symbol(&self) -> Result<Vec<String>, String> {
        Err("not implemented".to_string())
    }

    async fn get_price(&self, symbol: String) -> Result<Decimal, String> {
        Err("not implemented".to_string())
    }

    async fn get_open_interest(&self, symbol: String) -> Result<Decimal, String> {
        Err("not implemented".to_string())
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::ManageOrder for ExchangeClient {
    async fn place_order(&self, symbol: String, side: OrderSide, price: Decimal, volume: Decimal, order_type: OrderType, time_in_force: TimeInForce) -> Result<OrderPlacement, String> {
        Err("not implemented".to_string())
    }

    async fn change_order_by_cloid(&self, cloid: i64, price: Decimal, volume: Decimal) -> Result<i64, String> {
        Err("not implemented".to_string())
    }

    async fn cancel_order(&self, cloid: i64) -> Result<i64, String> {
        Err("not implemented".to_string())
    }

    async fn cancel_all_order(&self) -> Result<bool, String> {
        Err("not implemented".to_string())
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::SubscribeMarketData for ExchangeClient {
    fn subscribe_l2_update(&self, symbol: String) -> BoxStream<L2Update> {
        Box::pin(stream::empty())
    }

    fn subscribe_fill(&self, symbol: String) -> BoxStream<Fill> {
        Box::pin(stream::empty())
    }

    fn subscribe_asset_context(&self, symbol: String) -> BoxStream<AssetContext> {
        Box::pin(stream::empty())
    }

    fn subscribe_liquidation(&self, user: String) -> BoxStream<Liquidation> {
        Box::pin(stream::empty())
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::GetAccountSnapshot for ExchangeClient {
    async fn get_positions(&self) -> Result<Vec<Position>, String> {
        Err("not implemented".to_string())
    }

    async fn get_open_orders(&self) -> Result<Vec<OpenOrder>, String> {
        Err("not implemented".to_string())
    }

    async fn get_collateral(&self) -> Result<Decimal, String> {
        Err("not implemented".to_string())
    }

}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::SubscribeUserEvents for ExchangeClient {
    fn subscribe_user_fills(&self) -> BoxStream<UserFill> {
        Box::pin(stream::empty())
    }

    fn subscribe_order_updates(&self) -> BoxStream<OrderUpdate> {
        Box::pin(stream::empty())
    }

    fn subscribe_funding_payments(&self) -> BoxStream<FundingPayment> {
        Box::pin(stream::empty())
    }

    fn subscribe_deposits(&self) -> BoxStream<Deposit> {
        Box::pin(stream::empty())
    }

    fn subscribe_withdrawals(&self) -> BoxStream<Withdrawal> {
        Box::pin(stream::empty())
    }

}

