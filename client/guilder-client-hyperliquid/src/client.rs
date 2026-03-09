use guilder_abstraction::{self, L2Update, Fill, AssetContext, Liquidation, BoxStream, Side, OrderSide, OrderType, TimeInForce, OrderPlacement, Position, OpenOrder, UserFill, OrderUpdate, FundingPayment, Deposit, Withdrawal};
use futures_util::{stream, SinkExt, StreamExt};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const HYPERLIQUID_INFO_URL: &str = "https://api.hyperliquid.xyz/info";
const HYPERLIQUID_WS_URL: &str = "wss://api.hyperliquid.xyz/ws";

pub struct HyperliquidClient {
    client: Client,
}

impl HyperliquidClient {
    pub fn new() -> Self {
        HyperliquidClient { client: Client::new() }
    }
}

// --- Deserialization types for Hyperliquid REST responses ---

#[derive(Deserialize)]
struct MetaResponse {
    universe: Vec<AssetInfo>,
}

#[derive(Deserialize)]
struct AssetInfo {
    name: String,
}

/// Response from metaAndAssetCtxs: [meta, [ctx, ...]]
type MetaAndAssetCtxsResponse = (MetaResponse, Vec<RestAssetCtx>);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct RestAssetCtx {
    open_interest: String,
    funding: String,
    mark_px: String,
    day_ntl_vlm: String,
}

// --- WebSocket envelope and data shapes ---

#[derive(Deserialize)]
struct WsEnvelope {
    channel: String,
    data: Value,
}

#[derive(Deserialize)]
struct WsBook {
    coin: String,
    levels: Vec<Vec<WsLevel>>,
    time: i64,
}

#[derive(Deserialize)]
struct WsLevel {
    px: String,
    sz: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsAssetCtx {
    coin: String,
    ctx: WsPerpsCtx,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsPerpsCtx {
    open_interest: String,
    funding: String,
    mark_px: String,
    day_ntl_vlm: String,
}

#[derive(Deserialize)]
struct WsUserEvent {
    liquidation: Option<WsLiquidation>,
}

#[derive(Deserialize)]
struct WsLiquidation {
    liquidated_user: String,
    liquidated_ntl_pos: String,
    liquidated_account_value: String,
}

#[derive(Deserialize)]
struct WsTrade {
    coin: String,
    side: String,
    px: String,
    sz: String,
    time: i64,
    tid: i64,
}

fn parse_decimal(s: &str) -> Option<Decimal> {
    Decimal::from_str(s).ok()
}

// --- Trait implementations ---

#[allow(async_fn_in_trait)]
impl guilder_abstraction::TestServer for HyperliquidClient {
    /// Sends a lightweight allMids request; returns true if the server responds 200 OK.
    async fn ping(&self) -> Result<bool, String> {
        self.client
            .post(HYPERLIQUID_INFO_URL)
            .json(&serde_json::json!({"type": "allMids"}))
            .send()
            .await
            .map(|r| r.status().is_success())
            .map_err(|e| e.to_string())
    }

    /// Hyperliquid has no dedicated server-time endpoint; returns local UTC ms.
    async fn get_server_time(&self) -> Result<i64, String> {
        Ok(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0))
    }
}

#[allow(async_fn_in_trait)]
impl guilder_abstraction::GetMarketData for HyperliquidClient {
    /// Returns all perpetual asset names from Hyperliquid's meta endpoint.
    async fn get_symbol(&self) -> Result<Vec<String>, String> {
        let resp = self.client
            .post(HYPERLIQUID_INFO_URL)
            .json(&serde_json::json!({"type": "meta"}))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json::<MetaResponse>()
            .await
            .map(|r| r.universe.into_iter().map(|a| a.name).collect())
            .map_err(|e| e.to_string())
    }

    /// Returns the current open interest for `symbol` from metaAndAssetCtxs.
    async fn get_open_interest(&self, symbol: String) -> Result<Decimal, String> {
        let resp = self.client
            .post(HYPERLIQUID_INFO_URL)
            .json(&serde_json::json!({"type": "metaAndAssetCtxs"}))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let (meta, ctxs) = resp.json::<MetaAndAssetCtxsResponse>()
            .await
            .map_err(|e| e.to_string())?;
        meta.universe.iter()
            .position(|a| a.name == symbol)
            .and_then(|i| ctxs.get(i))
            .and_then(|ctx| parse_decimal(&ctx.open_interest))
            .ok_or_else(|| format!("symbol {} not found", symbol))
    }

    /// Returns the mid-price of `symbol` (e.g. "BTC") from allMids.
    async fn get_price(&self, symbol: String) -> Result<Decimal, String> {
        let resp = self.client
            .post(HYPERLIQUID_INFO_URL)
            .json(&serde_json::json!({"type": "allMids"}))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.json::<HashMap<String, String>>()
            .await
            .map_err(|e| e.to_string())?
            .get(&symbol)
            .and_then(|s| parse_decimal(s))
            .ok_or_else(|| format!("symbol {} not found", symbol))
    }
}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::ManageOrder for HyperliquidClient {
    async fn place_order(&self, symbol: String, side: OrderSide, price: Decimal, volume: Decimal, order_type: OrderType, time_in_force: TimeInForce) -> Result<OrderPlacement, String> {
        unimplemented!()
    }

    async fn change_order_by_cloid(&self, cloid: i64, price: Decimal, volume: Decimal) -> Result<i64, String> {
        unimplemented!()
    }

    async fn cancel_order(&self, cloid: i64) -> Result<i64, String> {
        unimplemented!()
    }

    async fn cancel_all_order(&self) -> Result<bool, String> {
        unimplemented!()
    }
}

#[allow(async_fn_in_trait)]
impl guilder_abstraction::SubscribeMarketData for HyperliquidClient {
    /// Streams L2 orderbook updates for `symbol`. Each message from Hyperliquid is a
    /// full-depth snapshot; every level is emitted as an individual `L2Update` event.
    /// All levels in the same snapshot share the same `sequence` value.
    fn subscribe_l2_update(&self, symbol: String) -> BoxStream<L2Update> {
        Box::pin(async_stream::stream! {
            let Ok((mut ws, _)) = connect_async(HYPERLIQUID_WS_URL).await else { return; };
            let sub = serde_json::json!({
                "method": "subscribe",
                "subscription": {"type": "l2Book", "coin": symbol}
            });
            if ws.send(Message::Text(sub.to_string().into())).await.is_err() { return; }

            while let Some(Ok(Message::Text(text))) = ws.next().await {
                let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else { continue; };
                if env.channel != "l2Book" { continue; }
                let Ok(book) = serde_json::from_value::<WsBook>(env.data) else { continue; };

                for level in book.levels.first().into_iter().flatten() {
                    if let (Some(price), Some(volume)) = (parse_decimal(&level.px), parse_decimal(&level.sz)) {
                        yield L2Update { symbol: book.coin.clone(), price, volume, side: Side::Ask, sequence: book.time };
                    }
                }
                for level in book.levels.get(1).into_iter().flatten() {
                    if let (Some(price), Some(volume)) = (parse_decimal(&level.px), parse_decimal(&level.sz)) {
                        yield L2Update { symbol: book.coin.clone(), price, volume, side: Side::Bid, sequence: book.time };
                    }
                }
            }
        })
    }

    /// Streams asset context updates for `symbol` via Hyperliquid's `activeAssetCtx` subscription.
    /// Each message carries OI, funding rate, mark price, and 24h notional volume.
    fn subscribe_asset_context(&self, symbol: String) -> BoxStream<AssetContext> {
        Box::pin(async_stream::stream! {
            let Ok((mut ws, _)) = connect_async(HYPERLIQUID_WS_URL).await else { return; };
            let sub = serde_json::json!({
                "method": "subscribe",
                "subscription": {"type": "activeAssetCtx", "coin": symbol}
            });
            if ws.send(Message::Text(sub.to_string().into())).await.is_err() { return; }

            while let Some(Ok(Message::Text(text))) = ws.next().await {
                let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else { continue; };
                if env.channel != "activeAssetCtx" { continue; }
                let Ok(update) = serde_json::from_value::<WsAssetCtx>(env.data) else { continue; };
                let ctx = &update.ctx;
                if let (Some(open_interest), Some(funding_rate), Some(mark_price), Some(day_volume)) = (
                    parse_decimal(&ctx.open_interest),
                    parse_decimal(&ctx.funding),
                    parse_decimal(&ctx.mark_px),
                    parse_decimal(&ctx.day_ntl_vlm),
                ) {
                    yield AssetContext { symbol: update.coin, open_interest, funding_rate, mark_price, day_volume };
                }
            }
        })
    }

    /// Streams liquidation events for a user address via Hyperliquid's `userEvents` subscription.
    /// Note: Hyperliquid's liquidation event is account-level; `symbol` is set to empty string
    /// and `side` defaults to `OrderSide::Sell` as the data source does not provide per-position detail.
    fn subscribe_liquidation(&self, user: String) -> BoxStream<Liquidation> {
        Box::pin(async_stream::stream! {
            let Ok((mut ws, _)) = connect_async(HYPERLIQUID_WS_URL).await else { return; };
            let sub = serde_json::json!({
                "method": "subscribe",
                "subscription": {"type": "userEvents", "user": user}
            });
            if ws.send(Message::Text(sub.to_string().into())).await.is_err() { return; }

            while let Some(Ok(Message::Text(text))) = ws.next().await {
                let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else { continue; };
                if env.channel != "userEvents" { continue; }
                let Ok(event) = serde_json::from_value::<WsUserEvent>(env.data) else { continue; };
                let Some(liq) = event.liquidation else { continue; };
                if let (Some(notional_position), Some(account_value)) = (
                    parse_decimal(&liq.liquidated_ntl_pos),
                    parse_decimal(&liq.liquidated_account_value),
                ) {
                    yield Liquidation {
                        symbol: String::new(),
                        side: OrderSide::Sell,
                        liquidated_user: liq.liquidated_user,
                        notional_position,
                        account_value,
                    };
                }
            }
        })
    }

    /// Streams public trade fills for `symbol`. Maps to Hyperliquid's `trades` subscription.
    /// `side` reflects the aggressor: "B" (buyer) → `OrderSide::Buy`, otherwise → `OrderSide::Sell`.
    fn subscribe_fill(&self, symbol: String) -> BoxStream<Fill> {
        Box::pin(async_stream::stream! {
            let Ok((mut ws, _)) = connect_async(HYPERLIQUID_WS_URL).await else { return; };
            let sub = serde_json::json!({
                "method": "subscribe",
                "subscription": {"type": "trades", "coin": symbol}
            });
            if ws.send(Message::Text(sub.to_string().into())).await.is_err() { return; }

            while let Some(Ok(Message::Text(text))) = ws.next().await {
                let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else { continue; };
                if env.channel != "trades" { continue; }
                let Ok(trades) = serde_json::from_value::<Vec<WsTrade>>(env.data) else { continue; };

                for trade in trades {
                    let side = if trade.side == "B" { OrderSide::Buy } else { OrderSide::Sell };
                    if let (Some(price), Some(volume)) = (parse_decimal(&trade.px), parse_decimal(&trade.sz)) {
                        yield Fill { symbol: trade.coin, price, volume, side, timestamp_ms: trade.time, trade_id: trade.tid };
                    }
                }
            }
        })
    }
}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::GetAccountSnapshot for HyperliquidClient {
    async fn get_positions(&self) -> Result<Vec<Position>, String> {
        unimplemented!()
    }

    async fn get_open_orders(&self) -> Result<Vec<OpenOrder>, String> {
        unimplemented!()
    }

    async fn get_collateral(&self) -> Result<Decimal, String> {
        unimplemented!()
    }
}

#[allow(unused_variables)]
#[allow(async_fn_in_trait)]
impl guilder_abstraction::SubscribeUserEvents for HyperliquidClient {
    fn subscribe_user_fills(&self) -> BoxStream<UserFill> {
        Box::pin(stream::pending())
    }

    fn subscribe_order_updates(&self) -> BoxStream<OrderUpdate> {
        Box::pin(stream::pending())
    }

    fn subscribe_funding_payments(&self) -> BoxStream<FundingPayment> {
        Box::pin(stream::pending())
    }

    fn subscribe_deposits(&self) -> BoxStream<Deposit> {
        Box::pin(stream::pending())
    }

    fn subscribe_withdrawals(&self) -> BoxStream<Withdrawal> {
        Box::pin(stream::pending())
    }
}
