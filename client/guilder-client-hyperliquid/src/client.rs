use crate::rate_limiter::{RestRateLimiter, AddressRateLimiter};
use alloy_primitives::Address;
use futures_util::{stream, StreamExt};
use guilder_abstraction::{
    self, AssetContext, BoxStream, Deposit, Fill, FundingPayment, L2Update, Liquidation, OpenOrder,
    OrderPlacement, OrderSide, OrderStatus, OrderType, OrderUpdate, Position, PredictedFunding,
    Side, TimeInForce, UserFill, Withdrawal,
};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
const HYPERLIQUID_INFO_URL: &str = "https://api.hyperliquid.xyz/info";
const HYPERLIQUID_EXCHANGE_URL: &str = "https://api.hyperliquid.xyz/exchange";

async fn parse_response<T: for<'de> serde::Deserialize<'de>>(
    resp: reqwest::Response,
) -> Result<T, String> {
    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("{e}: {text}"))
}

pub struct HyperliquidClient {
    client: Client,
    user_address: Option<Address>,
    private_key: Option<String>,
    rest_limiter: Arc<RestRateLimiter>,
    address_limiter: Arc<AddressRateLimiter>,
    ws_mux: crate::ws::WsMux,
}

impl Default for HyperliquidClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HyperliquidClient {
    pub fn new() -> Self {
        HyperliquidClient {
            client: Client::new(),
            user_address: None,
            private_key: None,
            rest_limiter: Arc::new(RestRateLimiter::new()),
            address_limiter: Arc::new(AddressRateLimiter::new()),
            ws_mux: crate::ws::WsMux::new(),
        }
    }

    pub fn with_auth(user_address: Address, private_key: String) -> Self {
        HyperliquidClient {
            client: Client::new(),
            user_address: Some(user_address),
            private_key: Some(private_key),
            rest_limiter: Arc::new(RestRateLimiter::new()),
            address_limiter: Arc::new(AddressRateLimiter::new()),
            ws_mux: crate::ws::WsMux::new(),
        }
    }

    /// Configure rate limit budgets (rest_weight/min, address_requests).
    /// Defaults: 1200 rest weight/min, 10000 address requests.
    pub fn with_budgets(mut self, rest_weight: u32, addr_budget: u64) -> Self {
        self.rest_limiter = Arc::new(RestRateLimiter::new_with_budget(rest_weight));
        self.address_limiter = Arc::new(AddressRateLimiter::new_with_budget(addr_budget));
        self
    }

    /// POST to the info endpoint, consuming `weight` from the REST rate-limit budget.
    async fn info_post(&self, body: Value, weight: u32, call: &str) -> Result<reqwest::Response, String> {
        self.rest_limiter.acquire_blocking(weight, call).await;
        self.client
            .post(HYPERLIQUID_INFO_URL)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())
    }

    /// POST to the exchange endpoint, consuming `weight` from the REST rate-limit budget.
    /// Weight = 1 + floor(batch_length / 40).
    async fn exchange_post(&self, body: Value, weight: u32, call: &str) -> Result<reqwest::Response, String> {
        self.rest_limiter.acquire_blocking(weight, call).await;
        self.client
            .post(HYPERLIQUID_EXCHANGE_URL)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())
    }

    fn require_user_address(&self) -> Result<String, String> {
        self.user_address
            .map(|a| format!("{:#x}", a))
            .ok_or_else(|| "user address required: use HyperliquidClient::with_auth".to_string())
    }

    fn require_private_key(&self) -> Result<&str, String> {
        self.private_key
            .as_deref()
            .ok_or_else(|| "private key required: use HyperliquidClient::with_auth".to_string())
    }

    async fn get_asset_index(&self, symbol: &str) -> Result<usize, String> {
        // `meta` is an "all other info" request → weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "meta"}), 20, "get_asset_index")
            .await?;
        let meta: MetaResponse = parse_response(resp).await?;
        meta.universe
            .iter()
            .position(|a| a.name == symbol)
            .ok_or_else(|| format!("symbol {} not found", symbol))
    }

    async fn submit_signed_action(
        &self,
        action: Value,
        vault_address: Option<&str>,
    ) -> Result<Value, String> {
        let private_key = self.require_private_key()?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let (r, s, v) = sign_action(private_key, &action, vault_address, nonce)?;

        let payload = serde_json::json!({
            "action": action,
            "nonce": nonce,
            "signature": {"r": r, "s": s, "v": v},
            "vaultAddress": null
        });

        // Single unbatched action → exchange weight 1
        let resp = self.exchange_post(payload, 1, "submit_signed_action").await?;

        let body: Value = parse_response(resp).await?;
        if body["status"].as_str() == Some("err") {
            return Err(body["response"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string());
        }
        Ok(body)
    }
}

// --- REST deserialization types ---

#[derive(Deserialize)]
struct MetaResponse {
    universe: Vec<AssetInfo>,
}

#[derive(Deserialize)]
struct AssetInfo {
    name: String,
}

type MetaAndAssetCtxsResponse = (MetaResponse, Vec<RestAssetCtx>);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct RestAssetCtx {
    open_interest: String,
    funding: String,
    mark_px: String,
    day_ntl_vlm: String,
    mid_px: Option<String>,
    oracle_px: Option<String>,
    premium: Option<String>,
    prev_day_px: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClearinghouseStateResponse {
    margin_summary: MarginSummary,
    asset_positions: Vec<AssetPosition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarginSummary {
    account_value: String,
}

#[derive(Deserialize)]
struct AssetPosition {
    position: PositionDetail,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionDetail {
    coin: String,
    /// positive = long, negative = short
    szi: String,
    entry_px: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestOpenOrder {
    coin: String,
    side: String,
    limit_px: String,
    sz: String,
    oid: i64,
    orig_sz: String,
}

// predictedFundings response: Vec<(coin, Vec<(venue, entry_or_null)>)>
// The API returns null for venues that don't list the coin.
type PredictedFundingsResponse = Vec<(String, Vec<(String, Option<PredictedFundingEntry>)>)>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PredictedFundingEntry {
    funding_rate: String,
    next_funding_time: i64,
}

// --- WebSocket envelope and data shapes ---

#[derive(Deserialize)]
struct WsEnvelope {
    channel: String,
    #[serde(default)]
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
    mid_px: Option<String>,
    oracle_px: Option<String>,
    premium: Option<String>,
    prev_day_px: Option<String>,
}

#[derive(Deserialize)]
struct WsUserEvent {
    liquidation: Option<WsLiquidation>,
    fills: Option<Vec<WsUserFill>>,
    funding: Option<WsFunding>,
}

#[derive(Deserialize)]
struct WsLiquidation {
    liquidated_user: String,
    liquidated_ntl_pos: String,
    liquidated_account_value: String,
}

#[derive(Deserialize)]
struct WsUserFill {
    coin: String,
    px: String,
    sz: String,
    side: String,
    time: i64,
    oid: i64,
    fee: String,
    /// Client order ID assigned at placement, if any.
    #[serde(default)]
    cloid: Option<String>,
}

#[derive(Deserialize)]
struct WsFunding {
    time: i64,
    coin: String,
    usdc: String,
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

#[derive(Deserialize)]
struct WsOrderUpdate {
    order: WsOrderInfo,
    status: String,
    #[serde(rename = "statusTimestamp")]
    status_timestamp: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsOrderInfo {
    coin: String,
    side: String,
    limit_px: String,
    sz: String,
    oid: i64,
    orig_sz: String,
    /// Client order ID assigned at placement, if any.
    #[serde(default)]
    cloid: Option<String>,
}

// --- WebSocket ledger update shapes (deposits / withdrawals) ---

#[derive(Deserialize)]
struct WsLedgerUpdates {
    updates: Vec<WsLedgerEntry>,
}

#[derive(Deserialize)]
struct WsLedgerEntry {
    time: i64,
    delta: WsLedgerDelta,
}

#[derive(Deserialize)]
struct WsLedgerDelta {
    #[serde(rename = "type")]
    kind: String,
    usdc: Option<String>,
}

// --- Helpers ---

fn parse_decimal(s: &str) -> Option<Decimal> {
    Decimal::from_str(s).ok()
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    Keccak256::digest(data).into()
}

/// EIP-712 domain separator for Hyperliquid mainnet (Arbitrum, chainId=42161).
fn hyperliquid_domain_separator() -> [u8; 32] {
    let type_hash = keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
    );
    let name_hash = keccak256(b"Exchange");
    let version_hash = keccak256(b"1");
    let mut chain_id = [0u8; 32];
    chain_id[28..32].copy_from_slice(&42161u32.to_be_bytes());
    let verifying_contract = [0u8; 32];

    let mut data = [0u8; 160];
    data[..32].copy_from_slice(&type_hash);
    data[32..64].copy_from_slice(&name_hash);
    data[64..96].copy_from_slice(&version_hash);
    data[96..128].copy_from_slice(&chain_id);
    data[128..160].copy_from_slice(&verifying_contract);
    keccak256(&data)
}

/// Signs a Hyperliquid exchange action using EIP-712.
/// Returns (r, s, v) where r and s are "0x"-prefixed hex strings and v is 27 or 28.
fn sign_action(
    private_key: &str,
    action: &Value,
    vault_address: Option<&str>,
    nonce: u64,
) -> Result<(String, String, u8), String> {
    use k256::ecdsa::SigningKey;

    // Step 1: msgpack-encode the action, append nonce + vault flag
    let msgpack_bytes = rmp_serde::to_vec(action).map_err(|e| e.to_string())?;
    let mut data = msgpack_bytes;
    data.extend_from_slice(&nonce.to_be_bytes());
    match vault_address {
        None => data.push(0u8),
        Some(addr) => {
            data.push(1u8);
            let addr_bytes = hex::decode(addr.trim_start_matches("0x"))
                .map_err(|e| format!("invalid vault address: {}", e))?;
            data.extend_from_slice(&addr_bytes);
        }
    }
    let connection_id = keccak256(&data);

    // Step 2: hash the Agent struct
    let agent_type_hash = keccak256(b"Agent(string source,bytes32 connectionId)");
    let source_hash = keccak256(b"a"); // "a" = mainnet
    let mut struct_data = [0u8; 96];
    struct_data[..32].copy_from_slice(&agent_type_hash);
    struct_data[32..64].copy_from_slice(&source_hash);
    struct_data[64..96].copy_from_slice(&connection_id);
    let struct_hash = keccak256(&struct_data);

    // Step 3: EIP-712 final hash
    let domain_sep = hyperliquid_domain_separator();
    let mut final_data = Vec::with_capacity(66);
    final_data.extend_from_slice(b"\x19\x01");
    final_data.extend_from_slice(&domain_sep);
    final_data.extend_from_slice(&struct_hash);
    let final_hash = keccak256(&final_data);

    // Step 4: sign with secp256k1
    let key_bytes = hex::decode(private_key.trim_start_matches("0x"))
        .map_err(|e| format!("invalid private key: {}", e))?;
    let signing_key =
        SigningKey::from_bytes(key_bytes.as_slice().into()).map_err(|e| e.to_string())?;
    let (sig, recovery_id) = signing_key
        .sign_prehash_recoverable(&final_hash)
        .map_err(|e| e.to_string())?;

    let sig_bytes = sig.to_bytes();
    let r = format!("0x{}", hex::encode(&sig_bytes[..32]));
    let s = format!("0x{}", hex::encode(&sig_bytes[32..64]));
    let v = 27u8 + recovery_id.to_byte();

    Ok((r, s, v))
}

// --- Trait implementations ---

#[allow(async_fn_in_trait)]
impl guilder_abstraction::TestServer for HyperliquidClient {
    /// Sends a lightweight allMids request; returns true if the server responds 200 OK.
    async fn ping(&self) -> Result<bool, String> {
        // allMids → weight 2
        self.info_post(serde_json::json!({"type": "allMids"}), 2, "ping")
            .await
            .map(|r| r.status().is_success())
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
        // meta → weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "meta"}), 20, "get_symbol")
            .await?;
        parse_response::<MetaResponse>(resp)
            .await
            .map(|r| r.universe.into_iter().map(|a| a.name).collect())
    }

    /// Returns the current open interest for `symbol` from metaAndAssetCtxs.
    async fn get_open_interest(&self, symbol: String) -> Result<Decimal, String> {
        // metaAndAssetCtxs → weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "metaAndAssetCtxs"}), 20, "get_open_interest")
            .await?;
        let (meta, ctxs) = parse_response::<Option<MetaAndAssetCtxsResponse>>(resp)
            .await?
            .ok_or_else(|| "metaAndAssetCtxs returned null".to_string())?;
        meta.universe
            .iter()
            .position(|a| a.name == symbol)
            .and_then(|i| ctxs.get(i))
            .and_then(|ctx| parse_decimal(&ctx.open_interest))
            .ok_or_else(|| format!("symbol {} not found", symbol))
    }

    /// Returns a full AssetContext snapshot for `symbol` from metaAndAssetCtxs.
    async fn get_asset_context(&self, symbol: String) -> Result<AssetContext, String> {
        // metaAndAssetCtxs → weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "metaAndAssetCtxs"}), 20, "get_asset_context")
            .await?;
        let (meta, ctxs) = parse_response::<Option<MetaAndAssetCtxsResponse>>(resp)
            .await?
            .ok_or_else(|| "metaAndAssetCtxs returned null".to_string())?;
        let idx = meta
            .universe
            .iter()
            .position(|a| a.name == symbol)
            .ok_or_else(|| format!("symbol {} not found", symbol))?;
        let ctx = ctxs
            .get(idx)
            .ok_or_else(|| format!("symbol {} not found", symbol))?;
        Ok(AssetContext {
            symbol,
            open_interest: parse_decimal(&ctx.open_interest).ok_or("invalid open_interest")?,
            funding_rate: parse_decimal(&ctx.funding).ok_or("invalid funding")?,
            mark_price: parse_decimal(&ctx.mark_px).ok_or("invalid mark_px")?,
            day_volume: parse_decimal(&ctx.day_ntl_vlm).ok_or("invalid day_ntl_vlm")?,
            mid_price: ctx.mid_px.as_deref().and_then(parse_decimal),
            oracle_price: ctx.oracle_px.as_deref().and_then(parse_decimal),
            premium: ctx.premium.as_deref().and_then(parse_decimal),
            prev_day_price: ctx.prev_day_px.as_deref().and_then(parse_decimal),
        })
    }

    /// Fetches metaAndAssetCtxs once and returns all asset contexts in universe order.
    /// Prefer this over repeated `get_asset_context` calls to avoid rate-limiting.
    async fn get_all_asset_contexts(&self) -> Result<Vec<AssetContext>, String> {
        // metaAndAssetCtxs → weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "metaAndAssetCtxs"}), 20, "get_all_asset_contexts")
            .await?;
        let (meta, ctxs) = parse_response::<Option<MetaAndAssetCtxsResponse>>(resp)
            .await?
            .ok_or_else(|| "metaAndAssetCtxs returned null".to_string())?;
        let mut result = Vec::with_capacity(meta.universe.len());
        for (asset, ctx) in meta.universe.iter().zip(ctxs.iter()) {
            let Some(open_interest) = parse_decimal(&ctx.open_interest) else {
                continue;
            };
            let Some(funding_rate) = parse_decimal(&ctx.funding) else {
                continue;
            };
            let Some(mark_price) = parse_decimal(&ctx.mark_px) else {
                continue;
            };
            let Some(day_volume) = parse_decimal(&ctx.day_ntl_vlm) else {
                continue;
            };
            result.push(AssetContext {
                symbol: asset.name.clone(),
                open_interest,
                funding_rate,
                mark_price,
                day_volume,
                mid_price: ctx.mid_px.as_deref().and_then(parse_decimal),
                oracle_price: ctx.oracle_px.as_deref().and_then(parse_decimal),
                premium: ctx.premium.as_deref().and_then(parse_decimal),
                prev_day_price: ctx.prev_day_px.as_deref().and_then(parse_decimal),
            });
        }
        Ok(result)
    }

    /// Returns a full L2 orderbook snapshot for `symbol` from the l2Book REST endpoint.
    /// Levels are returned as individual `L2Update` items; all share the same `sequence` (timestamp).
    async fn get_l2_orderbook(&self, symbol: String) -> Result<Vec<L2Update>, String> {
        // l2Book → weight 2
        let resp = self
            .info_post(serde_json::json!({"type": "l2Book", "coin": symbol}), 2, "get_l2_orderbook")
            .await?;
        let book: Option<WsBook> = parse_response(resp).await?;
        let book = match book {
            Some(b) => b,
            None => return Ok(vec![]),
        };
        let mut levels = Vec::new();
        for level in book.levels.first().into_iter().flatten() {
            if let (Some(price), Some(volume)) =
                (parse_decimal(&level.px), parse_decimal(&level.sz))
            {
                levels.push(L2Update {
                    symbol: book.coin.clone(),
                    price,
                    volume,
                    side: Side::Ask,
                    sequence: book.time,
                });
            }
        }
        for level in book.levels.get(1).into_iter().flatten() {
            if let (Some(price), Some(volume)) =
                (parse_decimal(&level.px), parse_decimal(&level.sz))
            {
                levels.push(L2Update {
                    symbol: book.coin.clone(),
                    price,
                    volume,
                    side: Side::Bid,
                    sequence: book.time,
                });
            }
        }
        Ok(levels)
    }

    /// Returns the mid-price of `symbol` (e.g. "BTC") from allMids.
    async fn get_price(&self, symbol: String) -> Result<Decimal, String> {
        // allMids → weight 2
        let resp = self
            .info_post(serde_json::json!({"type": "allMids"}), 2, "get_price")
            .await?;
        parse_response::<HashMap<String, String>>(resp)
            .await?
            .get(&symbol)
            .and_then(|s| parse_decimal(s))
            .ok_or_else(|| format!("symbol {} not found", symbol))
    }

    /// Returns predicted funding rates for all symbols across all venues.
    /// Null venue entries (unsupported coins) are silently skipped.
    async fn get_predicted_fundings(&self) -> Result<Vec<PredictedFunding>, String> {
        // predictedFundings → weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "predictedFundings"}), 20, "get_predicted_fundings")
            .await?;
        let data: PredictedFundingsResponse = parse_response(resp).await?;
        let mut result = Vec::new();
        for (symbol, venues) in data {
            for (venue, entry) in venues {
                let Some(entry) = entry else { continue };
                if let Some(funding_rate) = parse_decimal(&entry.funding_rate) {
                    result.push(PredictedFunding {
                        symbol: symbol.clone(),
                        venue,
                        funding_rate,
                        next_funding_time_ms: entry.next_funding_time,
                    });
                }
            }
        }
        Ok(result)
    }
}

#[allow(async_fn_in_trait)]
impl guilder_abstraction::ManageOrder for HyperliquidClient {
    /// Places an order on Hyperliquid. Requires `with_auth`. Returns an `OrderPlacement` with
    /// the exchange-assigned order ID. Market orders are submitted as aggressive limit orders (IOC).
    ///
    /// If `cloid` is provided, Hyperliquid attaches it to the order lifecycle — fills and order
    /// updates will carry the same cloid back, enabling end-to-end intent tracing without a
    /// separate order_id mapping.
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
        let asset_idx = self.get_asset_index(&symbol).await?;
        let is_buy = matches!(side, OrderSide::Buy);

        let tif_str = match time_in_force {
            TimeInForce::Gtc => "Gtc",
            TimeInForce::Ioc => "Ioc",
            TimeInForce::Fok => "Fok",
        };
        // Market orders are IOC limit orders at a wide price
        let order_type_val = match order_type {
            OrderType::Limit => serde_json::json!({"limit": {"tif": tif_str}}),
            OrderType::Market => serde_json::json!({"limit": {"tif": "Ioc"}}),
        };

        let mut order_json = serde_json::json!({
            "a": asset_idx,
            "b": is_buy,
            "p": price.to_string(),
            "s": volume.to_string(),
            "r": false,
            "t": order_type_val
        });
        if let Some(ref c) = cloid {
            order_json["c"] = serde_json::json!(c);
        }

        let action = serde_json::json!({
            "type": "order",
            "orders": [order_json],
            "grouping": "na"
        });

        let resp = self.submit_signed_action(action, None).await?;
        let oid = resp["response"]["data"]["statuses"][0]["resting"]["oid"]
            .as_i64()
            .or_else(|| resp["response"]["data"]["statuses"][0]["filled"]["oid"].as_i64())
            .ok_or_else(|| format!("unexpected response: {}", resp))?;

        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        Ok(OrderPlacement {
            order_id: oid,
            symbol,
            side,
            price,
            quantity: volume,
            timestamp_ms,
            cloid,
        })
    }

    /// Modifies price and size of an existing order by its order ID. Requires `with_auth`.
    /// Fetches the order's current coin and side before submitting the modify action.
    async fn change_order_by_cloid(
        &self,
        cloid: i64,
        price: Decimal,
        volume: Decimal,
    ) -> Result<i64, String> {
        let user = self.require_user_address()?;

        // openOrders → weight 20; get_asset_index → meta weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "openOrders", "user": user}), 20, "change_order_by_cloid")
            .await?;
        let orders: Vec<RestOpenOrder> = parse_response(resp).await?;
        let order = orders
            .iter()
            .find(|o| o.oid == cloid)
            .ok_or_else(|| format!("order {} not found", cloid))?;

        let asset_idx = self.get_asset_index(&order.coin).await?;
        let is_buy = order.side == "B";

        let action = serde_json::json!({
            "type": "batchModify",
            "modifies": [{
                "oid": cloid,
                "order": {
                    "a": asset_idx,
                    "b": is_buy,
                    "p": price.to_string(),
                    "s": volume.to_string(),
                    "r": false,
                    "t": {"limit": {"tif": "Gtc"}}
                }
            }]
        });

        self.submit_signed_action(action, None).await?;
        Ok(cloid)
    }

    /// Cancels a single order by its order ID. Requires `with_auth`.
    /// Fetches open orders to resolve the coin/asset before cancelling.
    async fn cancel_order(&self, cloid: i64) -> Result<i64, String> {
        let user = self.require_user_address()?;

        // openOrders → weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "openOrders", "user": user}), 20, "cancel_order")
            .await?;
        let orders: Vec<RestOpenOrder> = parse_response(resp).await?;
        let order = orders
            .iter()
            .find(|o| o.oid == cloid)
            .ok_or_else(|| format!("order {} not found", cloid))?;

        let asset_idx = self.get_asset_index(&order.coin).await?;
        let action = serde_json::json!({
            "type": "cancel",
            "cancels": [{"a": asset_idx, "o": cloid}]
        });

        self.submit_signed_action(action, None).await?;
        Ok(cloid)
    }

    /// Cancels all open orders. Requires `with_auth`.
    /// Fetches all open orders and submits a batch cancel in a single signed request.
    async fn cancel_all_order(&self) -> Result<bool, String> {
        let user = self.require_user_address()?;

        // openOrders → weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "openOrders", "user": user}), 20, "cancel_all_order")
            .await?;
        let orders: Vec<RestOpenOrder> = parse_response(resp).await?;
        if orders.is_empty() {
            return Ok(true);
        }

        // meta → weight 20
        let meta_resp = self
            .info_post(serde_json::json!({"type": "meta"}), 20, "cancel_all_order")
            .await?;
        let meta: MetaResponse = parse_response(meta_resp).await?;

        let cancels: Vec<Value> = orders
            .iter()
            .filter_map(|o| {
                let asset_idx = meta.universe.iter().position(|a| a.name == o.coin)?;
                Some(serde_json::json!({"a": asset_idx, "o": o.oid}))
            })
            .collect();

        let action = serde_json::json!({"type": "cancel", "cancels": cancels});
        self.submit_signed_action(action, None).await?;
        Ok(true)
    }
}

#[allow(async_fn_in_trait)]
impl guilder_abstraction::SubscribeMarketData for HyperliquidClient {
    fn subscribe_l2_update(&self, symbol: String) -> BoxStream<Result<L2Update, String>> {
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": {"type": "l2Book", "coin": symbol.clone()}
        });
        let key = crate::ws::SubKey {
            channel: "l2Book".to_string(),
            routing_key: symbol,
        };
        let stream = self.ws_mux.subscribe(key, sub);
        Box::pin(async_stream::stream! {
            for await msg in stream {
                let Ok(env) = serde_json::from_str::<WsEnvelope>(&msg) else {
                    continue;
                };
                if env.channel != "l2Book" {
                    continue;
                }
                let Ok(book) = serde_json::from_value::<WsBook>(env.data) else {
                    continue;
                };
                for level in book.levels.first().into_iter().flatten() {
                    if let (Some(price), Some(volume)) =
                        (parse_decimal(&level.px), parse_decimal(&level.sz))
                    {
                        yield Ok(L2Update {
                            symbol: book.coin.clone(),
                            price,
                            volume,
                            side: Side::Ask,
                            sequence: book.time,
                        });
                    }
                }
                for level in book.levels.get(1).into_iter().flatten() {
                    if let (Some(price), Some(volume)) =
                        (parse_decimal(&level.px), parse_decimal(&level.sz))
                    {
                        yield Ok(L2Update {
                            symbol: book.coin.clone(),
                            price,
                            volume,
                            side: Side::Bid,
                            sequence: book.time,
                        });
                    }
                }
            }
        })
    }

    fn subscribe_asset_context(&self, symbol: String) -> BoxStream<Result<AssetContext, String>> {
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": {"type": "activeAssetCtx", "coin": symbol.clone()}
        });
        let key = crate::ws::SubKey {
            channel: "activeAssetCtx".to_string(),
            routing_key: symbol,
        };
        let stream = self.ws_mux.subscribe(key, sub);
        Box::pin(async_stream::stream! {
            for await msg in stream {
                let Ok(env) = serde_json::from_str::<WsEnvelope>(&msg) else {
                    continue;
                };
                if env.channel != "activeAssetCtx" {
                    continue;
                }
                let Ok(update) = serde_json::from_value::<WsAssetCtx>(env.data) else {
                    continue;
                };
                let ctx = &update.ctx;
                let (Some(open_interest), Some(funding_rate), Some(mark_price), Some(day_volume)) = (
                    parse_decimal(&ctx.open_interest),
                    parse_decimal(&ctx.funding),
                    parse_decimal(&ctx.mark_px),
                    parse_decimal(&ctx.day_ntl_vlm),
                ) else {
                    continue;
                };
                yield Ok(AssetContext {
                    symbol: update.coin,
                    open_interest,
                    funding_rate,
                    mark_price,
                    day_volume,
                    mid_price: ctx.mid_px.as_deref().and_then(parse_decimal),
                    oracle_price: ctx.oracle_px.as_deref().and_then(parse_decimal),
                    premium: ctx.premium.as_deref().and_then(parse_decimal),
                    prev_day_price: ctx.prev_day_px.as_deref().and_then(parse_decimal),
                });
            }
        })
    }

    fn subscribe_liquidation(&self, user: String) -> BoxStream<Result<Liquidation, String>> {
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": {"type": "userEvents", "user": user.clone()}
        });
        let key = crate::ws::SubKey {
            channel: "userEvents".to_string(),
            routing_key: user,
        };
        let raw_stream = self.ws_mux.subscribe(key, sub);
        Box::pin(raw_stream.filter_map(|text| async move {
            let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else {
                return None;
            };
            if env.channel != "userEvents" {
                return None;
            }
            let Ok(event) = serde_json::from_value::<WsUserEvent>(env.data) else {
                return None;
            };
            let Some(liq) = event.liquidation else {
                return None;
            };
            let (Some(notional_position), Some(account_value)) = (
                parse_decimal(&liq.liquidated_ntl_pos),
                parse_decimal(&liq.liquidated_account_value),
            ) else {
                return None;
            };
            let item = Liquidation {
                symbol: String::new(),
                side: OrderSide::Sell,
                liquidated_user: liq.liquidated_user,
                notional_position,
                account_value,
            };
            Some(stream::iter(vec![Ok(item)].into_iter()))
        }).flatten())
    }

    fn subscribe_fill(&self, symbol: String) -> BoxStream<Result<Fill, String>> {
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": {"type": "trades", "coin": symbol.clone()}
        });
        let key = crate::ws::SubKey {
            channel: "trades".to_string(),
            routing_key: symbol,
        };
        let stream = self.ws_mux.subscribe(key, sub);
        Box::pin(async_stream::stream! {
            for await msg in stream {
                let Ok(env) = serde_json::from_str::<WsEnvelope>(&msg) else {
                    continue;
                };
                if env.channel != "trades" {
                    continue;
                }
                let Ok(trades) = serde_json::from_value::<Vec<WsTrade>>(env.data) else {
                    continue;
                };
                for trade in trades {
                    let side = if trade.side == "B" {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    };
                    let price = parse_decimal(&trade.px);
                    let volume = parse_decimal(&trade.sz);
                    if let (Some(price), Some(volume)) = (price, volume) {
                        yield Ok(Fill {
                            symbol: trade.coin,
                            price,
                            volume,
                            side,
                            timestamp_ms: trade.time,
                            trade_id: trade.tid,
                        });
                    }
                }
            }
        })
    }
}

#[allow(async_fn_in_trait)]
impl guilder_abstraction::GetAccountSnapshot for HyperliquidClient {
    /// Returns open positions from `clearinghouseState`. Requires `with_auth`.
    /// Zero-size positions are filtered out. Positive `szi` = long, negative = short.
    async fn get_positions(&self) -> Result<Vec<Position>, String> {
        let user = self.require_user_address()?;
        // clearinghouseState → weight 2
        let resp = self
            .info_post(
                serde_json::json!({"type": "clearinghouseState", "user": user}),
                2,
                "get_positions",
            )
            .await?;
        let state: ClearinghouseStateResponse = parse_response(resp).await?;

        Ok(state
            .asset_positions
            .into_iter()
            .filter_map(|ap| {
                let p = ap.position;
                let size = parse_decimal(&p.szi)?;
                if size.is_zero() {
                    return None;
                }
                let entry_price = p
                    .entry_px
                    .as_deref()
                    .and_then(parse_decimal)
                    .unwrap_or_default();
                let side = if size > Decimal::ZERO {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                Some(Position {
                    symbol: p.coin,
                    side,
                    size: size.abs(),
                    entry_price,
                })
            })
            .collect())
    }

    /// Returns resting orders from Hyperliquid's `openOrders` endpoint. Requires `with_auth`.
    /// `filled_quantity` is derived as `origSz - sz` (original size minus remaining size).
    async fn get_open_orders(&self) -> Result<Vec<OpenOrder>, String> {
        let user = self.require_user_address()?;
        // openOrders → weight 20
        let resp = self
            .info_post(serde_json::json!({"type": "openOrders", "user": user}), 20, "get_open_orders")
            .await?;
        let orders: Vec<RestOpenOrder> = parse_response(resp).await?;

        Ok(orders
            .into_iter()
            .filter_map(|o| {
                let price = parse_decimal(&o.limit_px)?;
                let quantity = parse_decimal(&o.orig_sz)?;
                let remaining = parse_decimal(&o.sz)?;
                let filled_quantity = quantity - remaining;
                let side = if o.side == "B" {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                Some(OpenOrder {
                    order_id: o.oid,
                    symbol: o.coin,
                    side,
                    price,
                    quantity,
                    filled_quantity,
                })
            })
            .collect())
    }

    /// Returns total account value (collateral) from `clearinghouseState`. Requires `with_auth`.
    async fn get_collateral(&self) -> Result<Decimal, String> {
        let user = self.require_user_address()?;
        // clearinghouseState → weight 2
        let resp = self
            .info_post(
                serde_json::json!({"type": "clearinghouseState", "user": user}),
                2,
                "get_collateral",
            )
            .await?;
        let state: ClearinghouseStateResponse = parse_response(resp).await?;
        parse_decimal(&state.margin_summary.account_value)
            .ok_or_else(|| "invalid account value".to_string())
    }
}

#[allow(async_fn_in_trait)]
impl guilder_abstraction::SubscribeUserEvents for HyperliquidClient {
    fn subscribe_user_fills(&self) -> BoxStream<Result<UserFill, String>> {
        let Some(addr) = self.user_address else {
            return Box::pin(stream::empty());
        };
        let addr_str = format!("{:#x}", addr);
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": {"type": "userEvents", "user": addr_str.clone()}
        });
        let key = crate::ws::SubKey {
            channel: "userEvents".to_string(),
            routing_key: addr_str,
        };
        let raw_stream = self.ws_mux.subscribe(key, sub);
        Box::pin(raw_stream.filter_map(|text| async move {
            let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else {
                return None;
            };
            if env.channel != "userEvents" {
                return None;
            }
            let Ok(event) = serde_json::from_value::<WsUserEvent>(env.data) else {
                return None;
            };
            let items: Vec<_> = event
                .fills
                .unwrap_or_default()
                .into_iter()
                .filter_map(|fill| {
                    let side = if fill.side == "B" {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    };
                    let price = parse_decimal(&fill.px)?;
                    let quantity = parse_decimal(&fill.sz)?;
                    let fee_usd = parse_decimal(&fill.fee)?;
                    Some(UserFill {
                        order_id: fill.oid,
                        symbol: fill.coin,
                        side,
                        price,
                        quantity,
                        fee_usd,
                        timestamp_ms: fill.time,
                        cloid: fill.cloid,
                    })
                })
                .collect();
            if items.is_empty() {
                None
            } else {
                Some(stream::iter(items.into_iter().map(Ok)))
            }
        }).flatten())
    }

    fn subscribe_order_updates(&self) -> BoxStream<Result<OrderUpdate, String>> {
        let Some(addr) = self.user_address else {
            return Box::pin(stream::empty());
        };
        let addr_str = format!("{:#x}", addr);
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": {"type": "orderUpdates", "user": addr_str.clone()}
        });
        let key = crate::ws::SubKey {
            channel: "orderUpdates".to_string(),
            routing_key: addr_str,
        };
        let raw_stream = self.ws_mux.subscribe(key, sub);
        Box::pin(raw_stream.filter_map(|text| async move {
            let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else {
                return None;
            };
            if env.channel != "orderUpdates" {
                return None;
            }
            let Ok(updates) = serde_json::from_value::<Vec<WsOrderUpdate>>(env.data) else {
                return None;
            };
            let items: Vec<_> = updates
                .into_iter()
                .map(|upd| {
                    let status = match upd.status.as_str() {
                        "open" => OrderStatus::Placed,
                        "filled" => OrderStatus::Filled,
                        "canceled" | "cancelled" => OrderStatus::Cancelled,
                        _ => OrderStatus::PartiallyFilled,
                    };
                    let side = if upd.order.side == "B" {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    };
                    OrderUpdate {
                        order_id: upd.order.oid,
                        symbol: upd.order.coin,
                        status,
                        side: Some(side),
                        price: parse_decimal(&upd.order.limit_px),
                        quantity: parse_decimal(&upd.order.orig_sz),
                        remaining_quantity: parse_decimal(&upd.order.sz),
                        timestamp_ms: upd.status_timestamp,
                        cloid: upd.order.cloid,
                    }
                })
                .collect();
            if items.is_empty() {
                None
            } else {
                Some(stream::iter(items.into_iter().map(Ok)))
            }
        }).flatten())
    }

    fn subscribe_funding_payments(&self) -> BoxStream<Result<FundingPayment, String>> {
        let Some(addr) = self.user_address else {
            return Box::pin(stream::empty());
        };
        let addr_str = format!("{:#x}", addr);
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": {"type": "userEvents", "user": addr_str.clone()}
        });
        let key = crate::ws::SubKey {
            channel: "userEvents".to_string(),
            routing_key: addr_str,
        };
        let raw_stream = self.ws_mux.subscribe(key, sub);
        Box::pin(raw_stream.filter_map(|text| async move {
            let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else {
                return None;
            };
            if env.channel != "userEvents" {
                return None;
            }
            let Ok(event) = serde_json::from_value::<WsUserEvent>(env.data) else {
                return None;
            };
            let Some(funding) = event.funding else {
                return None;
            };
            let Some(amount_usd) = parse_decimal(&funding.usdc) else {
                return None;
            };
            let item = FundingPayment {
                symbol: funding.coin,
                amount_usd,
                timestamp_ms: funding.time,
            };
            Some(stream::iter(vec![Ok(item)].into_iter()))
        }).flatten())
    }

    fn subscribe_deposits(&self) -> BoxStream<Result<Deposit, String>> {
        let Some(addr) = self.user_address else {
            return Box::pin(stream::empty());
        };
        let addr_str = format!("{:#x}", addr);
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": {"type": "userNonFundingLedgerUpdates", "user": addr_str.clone()}
        });
        let key = crate::ws::SubKey {
            channel: "userNonFundingLedgerUpdates".to_string(),
            routing_key: addr_str,
        };
        let raw_stream = self.ws_mux.subscribe(key, sub);
        Box::pin(raw_stream.filter_map(|text| async move {
            let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else {
                return None;
            };
            if env.channel != "userNonFundingLedgerUpdates" {
                return None;
            }
            let Ok(ledger) = serde_json::from_value::<WsLedgerUpdates>(env.data) else {
                return None;
            };
            let items: Vec<_> = ledger
                .updates
                .into_iter()
                .filter_map(|e| {
                    if e.delta.kind != "deposit" {
                        return None;
                    }
                    let amount_usd = e.delta.usdc.as_deref().and_then(parse_decimal)?;
                    Some(Deposit {
                        asset: "USDC".to_string(),
                        amount_usd,
                        timestamp_ms: e.time,
                    })
                })
                .collect();
            if items.is_empty() {
                None
            } else {
                Some(stream::iter(items.into_iter().map(Ok)))
            }
        }).flatten())
    }

    fn subscribe_withdrawals(&self) -> BoxStream<Result<Withdrawal, String>> {
        let Some(addr) = self.user_address else {
            return Box::pin(stream::empty());
        };
        let addr_str = format!("{:#x}", addr);
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": {"type": "userNonFundingLedgerUpdates", "user": addr_str.clone()}
        });
        let key = crate::ws::SubKey {
            channel: "userNonFundingLedgerUpdates".to_string(),
            routing_key: addr_str,
        };
        let raw_stream = self.ws_mux.subscribe(key, sub);
        Box::pin(raw_stream.filter_map(|text| async move {
            let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else {
                return None;
            };
            if env.channel != "userNonFundingLedgerUpdates" {
                return None;
            }
            let Ok(ledger) = serde_json::from_value::<WsLedgerUpdates>(env.data) else {
                return None;
            };
            let items: Vec<_> = ledger
                .updates
                .into_iter()
                .filter_map(|e| {
                    if e.delta.kind != "withdraw" {
                        return None;
                    }
                    let amount_usd = e.delta.usdc.as_deref().and_then(parse_decimal)?;
                    Some(Withdrawal {
                        asset: "USDC".to_string(),
                        amount_usd,
                        timestamp_ms: e.time,
                    })
                })
                .collect();
            if items.is_empty() {
                None
            } else {
                Some(stream::iter(items.into_iter().map(Ok)))
            }
        }).flatten())
    }
}
