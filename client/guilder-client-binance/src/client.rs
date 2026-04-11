use futures_util::stream;
use guilder_abstraction::{
    self, AssetContext, Balance, BoxStream, Deposit, Fill, FundingPayment, L2Update, Liquidation,
    OpenOrder, OrderPlacement, OrderSide, OrderType, OrderUpdate, Position, PredictedFunding,
    TimeInForce, UserFill, Withdrawal,
};
use reqwest::Client;
use rust_decimal::Decimal;

#[allow(dead_code)]
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

impl Default for BinanceClient {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::TestServer for BinanceClient {
    async fn ping(&self) -> Result<bool, String> {
        unimplemented!()
    }

    async fn get_server_time(&self) -> Result<i64, String> {
        unimplemented!()
    }
}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::GetMarketData for BinanceClient {
    async fn get_symbol(&self) -> Result<Vec<String>, String> {
        unimplemented!()
    }

    async fn get_price(&self, symbol: String) -> Result<Decimal, String> {
        unimplemented!()
    }

    async fn get_open_interest(&self, symbol: String) -> Result<Decimal, String> {
        unimplemented!()
    }

    async fn get_asset_context(&self, symbol: String) -> Result<AssetContext, String> {
        unimplemented!()
    }

    async fn get_all_asset_contexts(&self) -> Result<Vec<AssetContext>, String> {
        unimplemented!()
    }

    async fn get_predicted_fundings(&self) -> Result<Vec<PredictedFunding>, String> {
        unimplemented!()
    }

    async fn get_l2_orderbook(&self, symbol: String) -> Result<Vec<L2Update>, String> {
        unimplemented!()
    }
}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::ManageOrder for BinanceClient {
    async fn place_order(
        &self,
        symbol: String,
        side: OrderSide,
        price: Decimal,
        volume: Decimal,
        order_type: OrderType,
        time_in_force: TimeInForce,
        cloid: Option<String>,
    ) -> Result<OrderPlacement, String> {
        unimplemented!()
    }

    async fn change_order_by_cloid(
        &self,
        cloid: i64,
        price: Decimal,
        volume: Decimal,
    ) -> Result<i64, String> {
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
impl guilder_abstraction::SubscribeMarketData for BinanceClient {
    fn subscribe_l2_update(&self, symbol: String) -> BoxStream<Result<L2Update, String>> {
        Box::pin(stream::pending())
    }

    fn subscribe_fill(&self, symbol: String) -> BoxStream<Result<Fill, String>> {
        Box::pin(stream::pending())
    }

    fn subscribe_asset_context(&self, symbol: String) -> BoxStream<Result<AssetContext, String>> {
        Box::pin(stream::pending())
    }

    fn subscribe_liquidation(&self, user: String) -> BoxStream<Result<Liquidation, String>> {
        Box::pin(stream::pending())
    }
}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::GetAccountSnapshot for BinanceClient {
    async fn get_positions(&self) -> Result<Vec<Position>, String> {
        unimplemented!()
    }

    async fn get_open_orders(&self) -> Result<Vec<OpenOrder>, String> {
        unimplemented!()
    }

    async fn get_collateral(&self) -> Result<Decimal, String> {
        unimplemented!()
    }

    async fn get_spot_balance(&self) -> Result<Vec<Balance>, String> {
        unimplemented!()
    }

    async fn get_collateral_balance(&self, symbol: String) -> Result<Balance, String> {
        unimplemented!()
    }
}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::SubscribeUserEvents for BinanceClient {
    fn subscribe_user_fills(&self) -> BoxStream<Result<UserFill, String>> {
        Box::pin(stream::pending())
    }

    fn subscribe_order_updates(&self) -> BoxStream<Result<OrderUpdate, String>> {
        Box::pin(stream::pending())
    }

    fn subscribe_funding_payments(&self) -> BoxStream<Result<FundingPayment, String>> {
        Box::pin(stream::pending())
    }

    fn subscribe_deposits(&self) -> BoxStream<Result<Deposit, String>> {
        Box::pin(stream::pending())
    }

    fn subscribe_withdrawals(&self) -> BoxStream<Result<Withdrawal, String>> {
        Box::pin(stream::pending())
    }

    fn subscribe_spot_balance(&self) -> BoxStream<Result<Vec<Balance>, String>> {
        Box::pin(stream::pending())
    }

    fn subscribe_spot_balance_with_address(
        &self,
        address: String,
    ) -> BoxStream<Result<Vec<Balance>, String>> {
        Box::pin(stream::pending())
    }
}
