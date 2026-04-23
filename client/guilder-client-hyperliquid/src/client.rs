use crate::rate_limiter::{AddressRateLimiter, RestRateLimiter};
use crate::ws::manager::{
    managed_stream, HyperliquidSubscription, HyperliquidWsManager, WsSendRateLimiter,
};
use crate::ws::{HyperliquidWsBook, HyperliquidWsInboundMessage};
use futures_util::{stream, StreamExt};
use guilder_abstraction::{
    self, AssetContext, BoxStream, Deposit, Fill, FundingPayment, L2Level, L2Snapshot, L2Update,
    Liquidation, OpenOrder, OrderPlacement, OrderSide, OrderType, OrderUpdate, Position,
    PredictedFunding, TimeInForce, UserFill, Withdrawal,
};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
const HYPERLIQUID_INFO_URL: &str = "https://api.hyperliquid.xyz/info";
const HYPERLIQUID_EXCHANGE_URL: &str = "https://api.hyperliquid.xyz/exchange";

async fn parse_response<T: for<'de> serde::Deserialize<'de>>(
    resp: reqwest::Response,
) -> Result<T, String> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response body (status {status}): {e}"))?;

    if text.is_empty() {
        return Err(format!(
            "empty response body from Hyperliquid (HTTP {status})"
        ));
    }

    serde_json::from_str(&text).map_err(|e| {
        let snippet = if text.len() > 512 {
            format!("{}... ({} bytes total)", &text[..256], text.len())
        } else {
            text.clone()
        };
        format!("deserialize error (HTTP {status}): {e}: {snippet}")
    })
}

pub struct HyperliquidClient {
    client: Client,
    user_address: Option<String>,
    private_key: Option<String>,
    rest_limiter: Arc<RestRateLimiter>,
    address_limiter: Arc<AddressRateLimiter>,
    market_ws_manager: HyperliquidWsManager,
    user_ws_managers: Arc<RwLock<HashMap<String, HyperliquidWsManager>>>,
    ws_send_limiter: WsSendRateLimiter,
}

impl Default for HyperliquidClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HyperliquidClient {
    pub fn new() -> Self {
        let ws_send_limiter = WsSendRateLimiter::new();
        HyperliquidClient {
            client: Client::new(),
            user_address: None,
            private_key: None,
            rest_limiter: Arc::new(RestRateLimiter::new()),
            address_limiter: Arc::new(AddressRateLimiter::new()),
            market_ws_manager: HyperliquidWsManager::new(None, ws_send_limiter.clone()),
            user_ws_managers: Arc::new(RwLock::new(HashMap::new())),
            ws_send_limiter,
        }
    }

    pub fn with_auth(user_address: impl Into<String>, private_key: String) -> Self {
        let ws_send_limiter = WsSendRateLimiter::new();
        HyperliquidClient {
            client: Client::new(),
            user_address: Some(user_address.into()),
            private_key: Some(private_key),
            rest_limiter: Arc::new(RestRateLimiter::new()),
            address_limiter: Arc::new(AddressRateLimiter::new()),
            market_ws_manager: HyperliquidWsManager::new(None, ws_send_limiter.clone()),
            user_ws_managers: Arc::new(RwLock::new(HashMap::new())),
            ws_send_limiter,
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
    /// Returns `Err("rate_limited: ...")` immediately if budget is exhausted — no retry.
    /// Callers should handle gracefully (skip cycle, retry later, etc.).
    async fn info_post(
        &self,
        body: Value,
        weight: u32,
        call: &str,
    ) -> Result<reqwest::Response, String> {
        self.rest_limiter.acquire(weight).await.map_err(|e| {
            format!(
                "rate_limited: info_post ({call}) budget exhausted, retry_after_ms={}",
                e.retry_after.as_millis()
            )
        })?;
        self.client
            .post(HYPERLIQUID_INFO_URL)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())
    }
    fn require_user_address(&self) -> Result<String, String> {
        self.user_address
            .clone()
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
            "vaultAddress": null,
            "expiresAfter": null
        });

        // Check both rate limiters non-blocking — fail fast, no retry.
        self.rest_limiter.acquire(1).await.map_err(|e| {
            format!(
                "rate_limited: rest_weight exhausted, retry_after_ms={}",
                e.retry_after.as_millis()
            )
        })?;
        self.address_limiter.acquire(1, false).await.map_err(|e| {
            format!(
                "rate_limited: address quota exhausted, retry_after_ms={}",
                e.retry_after.as_millis()
            )
        })?;

        let resp = self
            .client
            .post(HYPERLIQUID_EXCHANGE_URL)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.map_err(|e| e.to_string())?;
            return Err(format!("HTTP {status}: {text}"));
        }

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
    #[serde(rename = "szDecimals")]
    sz_decimals: i32,
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
#[allow(dead_code)]
struct ClearinghouseStateResponse {
    margin_summary: MarginSummary,
    asset_positions: Vec<AssetPosition>,
}

/// Kept for get_positions compatibility; margin_summary fields are unused since get_collateral was removed.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
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
    cloid: Option<String>,
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

// --- Helpers ---

fn parse_decimal(s: &str) -> Option<Decimal> {
    Decimal::from_str(s).ok()
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    Keccak256::digest(data).into()
}

/// EIP-712 domain separator for Hyperliquid L1 actions (chainId=1337).
fn hyperliquid_domain_separator() -> [u8; 32] {
    let type_hash = keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
    );
    let name_hash = keccak256(b"Exchange");
    let version_hash = keccak256(b"1");
    let mut chain_id = [0u8; 32];
    chain_id[28..32].copy_from_slice(&1337u32.to_be_bytes());
    let verifying_contract = [0u8; 32];

    let mut data = [0u8; 160];
    data[..32].copy_from_slice(&type_hash);
    data[32..64].copy_from_slice(&name_hash);
    data[64..96].copy_from_slice(&version_hash);
    data[96..128].copy_from_slice(&chain_id);
    data[128..160].copy_from_slice(&verifying_contract);
    keccak256(&data)
}

/// Convert a `serde_json::Value` to msgpack bytes, preserving the JSON map key order.
/// This avoids rmp_serde's HashMap-based serialization which reorders map keys.
fn value_to_msgpack(val: &Value) -> Vec<u8> {
    match val {
        Value::Null => vec![0xc0],
        Value::Bool(true) => vec![0xc3],
        Value::Bool(false) => vec![0xc2],
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if i >= 0 {
                    if i <= 127 {
                        vec![i as u8]
                    } else if i <= 255 {
                        vec![0xcc, i as u8]
                    } else if i <= 65535 {
                        let mut buf = vec![0xcd];
                        buf.extend_from_slice(&(i as u16).to_be_bytes());
                        buf
                    } else if i <= 4294967295 {
                        let mut buf = vec![0xce];
                        buf.extend_from_slice(&(i as u32).to_be_bytes());
                        buf
                    } else {
                        let mut buf = vec![0xcf];
                        buf.extend_from_slice(&(i as u64).to_be_bytes());
                        buf
                    }
                } else if i >= -32 {
                    vec![0xe0 | (i as u8)]
                } else if i >= -128 {
                    vec![0xd0, i as i8 as u8]
                } else if i >= -32768 {
                    let mut buf = vec![0xd1];
                    buf.extend_from_slice(&(i as i16).to_be_bytes());
                    buf
                } else if i >= -2147483648 {
                    let mut buf = vec![0xd2];
                    buf.extend_from_slice(&(i as i32).to_be_bytes());
                    buf
                } else {
                    let mut buf = vec![0xd3];
                    buf.extend_from_slice(&i.to_be_bytes());
                    buf
                }
            } else if let Some(f) = n.as_f64() {
                let mut buf = vec![0xcb];
                buf.extend_from_slice(&f.to_be_bytes());
                buf
            } else {
                let u = n.as_u64().unwrap();
                if u <= 127 {
                    vec![u as u8]
                } else if u <= 255 {
                    vec![0xcc, u as u8]
                } else if u <= 65535 {
                    let mut buf = vec![0xcd];
                    buf.extend_from_slice(&(u as u16).to_be_bytes());
                    buf
                } else if u <= 4294967295 {
                    let mut buf = vec![0xce];
                    buf.extend_from_slice(&(u as u32).to_be_bytes());
                    buf
                } else {
                    let mut buf = vec![0xcf];
                    buf.extend_from_slice(&u.to_be_bytes());
                    buf
                }
            }
        }
        Value::String(s) => {
            let bytes = s.as_bytes();
            let len = bytes.len();
            let mut buf = Vec::new();
            if len <= 31 {
                buf.push(0xa0 | (len as u8));
            } else if len <= 255 {
                buf.push(0xd9);
                buf.push(len as u8);
            } else if len <= 65535 {
                buf.push(0xda);
                buf.extend_from_slice(&(len as u16).to_be_bytes());
            } else {
                buf.push(0xdb);
                buf.extend_from_slice(&(len as u32).to_be_bytes());
            }
            buf.extend_from_slice(bytes);
            buf
        }
        Value::Array(arr) => {
            let len = arr.len();
            let mut buf = Vec::new();
            if len <= 15 {
                buf.push(0x90 | (len as u8));
            } else if len <= 65535 {
                buf.push(0xdc);
                buf.extend_from_slice(&(len as u16).to_be_bytes());
            } else {
                buf.push(0xdd);
                buf.extend_from_slice(&(len as u32).to_be_bytes());
            }
            for item in arr {
                buf.extend_from_slice(&value_to_msgpack(item));
            }
            buf
        }
        Value::Object(map) => {
            let len = map.len();
            let mut buf = Vec::new();
            if len <= 15 {
                buf.push(0x80 | (len as u8));
            } else if len <= 65535 {
                buf.push(0xde);
                buf.extend_from_slice(&(len as u16).to_be_bytes());
            } else {
                buf.push(0xdf);
                buf.extend_from_slice(&(len as u32).to_be_bytes());
            }
            for (key, value) in map {
                buf.extend_from_slice(&value_to_msgpack(&Value::String(key.clone())));
                buf.extend_from_slice(&value_to_msgpack(value));
            }
            buf
        }
    }
}

/// Convert action to msgpack bytes preserving JSON map key insertion order
/// (matching Python's msgpack dict ordering).
fn action_to_canonical_msgpack(action: &Value) -> Result<Vec<u8>, String> {
    Ok(value_to_msgpack(action))
}

/// Build msgpack for a single order with Python SDK field order:
/// a, b, p, s, r, t, c(opt)
fn build_order_msgpack(
    asset_idx: usize,
    is_buy: bool,
    price: &str,
    size: &str,
    reduce_only: bool,
    order_kind: &str,
    tif: &[u8],
    cloid: Option<&str>,
) -> Vec<u8> {
    let field_count = if cloid.is_some() { 7 } else { 6 };
    let mut buf = Vec::new();
    buf.push(0x80 | (field_count as u8)); // fixmap

    // "a": asset_idx
    buf.extend_from_slice(&value_to_msgpack(&Value::String("a".to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::Number(serde_json::Number::from(
        asset_idx,
    ))));

    // "b": is_buy
    buf.extend_from_slice(&value_to_msgpack(&Value::String("b".to_string())));
    buf.push(if is_buy { 0xc3 } else { 0xc2 });

    // "p": price
    buf.extend_from_slice(&value_to_msgpack(&Value::String("p".to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::String(price.to_string())));

    // "s": size (Python SDK puts s before r)
    buf.extend_from_slice(&value_to_msgpack(&Value::String("s".to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::String(size.to_string())));

    // "r": reduce_only
    buf.extend_from_slice(&value_to_msgpack(&Value::String("r".to_string())));
    buf.push(if reduce_only { 0xc3 } else { 0xc2 });

    // "t": { order_kind: { "tif": tif_str } }
    buf.extend_from_slice(&value_to_msgpack(&Value::String("t".to_string())));
    // Inner: fixmap(1) with order_kind key
    buf.push(0x81);
    buf.extend_from_slice(&value_to_msgpack(&Value::String(order_kind.to_string())));
    // Inner-inner: fixmap(1) with "tif" key
    buf.push(0x81);
    buf.extend_from_slice(&value_to_msgpack(&Value::String("tif".to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::String(
        String::from_utf8_lossy(tif).to_string(),
    )));

    // "c": cloid (optional, appended at END per Python SDK)
    if let Some(c) = cloid {
        buf.extend_from_slice(&value_to_msgpack(&Value::String("c".to_string())));
        buf.extend_from_slice(&value_to_msgpack(&Value::String(c.to_string())));
    }

    buf
}

/// Build msgpack for a trigger order (take profit / stop loss) with Python SDK field order.
/// Trigger orders use: t = { "trigger": { "isMarket": bool, "triggerPx": str, "tpsl": "tp"|"sl" } }
fn build_trigger_order_msgpack(
    asset_idx: usize,
    is_buy: bool,
    price: &str,
    size: &str,
    reduce_only: bool,
    trigger_px: &str,
    is_market: bool,
    tpsl: &str,
    cloid: Option<&str>,
) -> Vec<u8> {
    let field_count = if cloid.is_some() { 7 } else { 6 };
    let mut buf = Vec::new();
    buf.push(0x80 | (field_count as u8)); // fixmap

    // "a": asset_idx
    buf.extend_from_slice(&value_to_msgpack(&Value::String("a".to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::Number(serde_json::Number::from(
        asset_idx,
    ))));

    // "b": is_buy
    buf.extend_from_slice(&value_to_msgpack(&Value::String("b".to_string())));
    buf.push(if is_buy { 0xc3 } else { 0xc2 });

    // "p": price (use trigger_px as price for resting, or "0" for market-on-trigger)
    buf.extend_from_slice(&value_to_msgpack(&Value::String("p".to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::String(price.to_string())));

    // "s": size
    buf.extend_from_slice(&value_to_msgpack(&Value::String("s".to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::String(size.to_string())));

    // "r": reduce_only
    buf.extend_from_slice(&value_to_msgpack(&Value::String("r".to_string())));
    buf.push(if reduce_only { 0xc3 } else { 0xc2 });

    // "t": { "trigger": { "isMarket": bool, "triggerPx": str, "tpsl": str } }
    buf.extend_from_slice(&value_to_msgpack(&Value::String("t".to_string())));
    buf.push(0x81); // fixmap(1): "trigger"
    buf.extend_from_slice(&value_to_msgpack(&Value::String("trigger".to_string())));
    // trigger object: fixmap(3)
    buf.push(0x83);
    buf.extend_from_slice(&value_to_msgpack(&Value::String("isMarket".to_string())));
    buf.push(if is_market { 0xc3 } else { 0xc2 });
    buf.extend_from_slice(&value_to_msgpack(&Value::String("triggerPx".to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::String(trigger_px.to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::String("tpsl".to_string())));
    buf.extend_from_slice(&value_to_msgpack(&Value::String(tpsl.to_string())));

    // "c": cloid (optional, appended at END)
    if let Some(c) = cloid {
        buf.extend_from_slice(&value_to_msgpack(&Value::String("c".to_string())));
        buf.extend_from_slice(&value_to_msgpack(&Value::String(c.to_string())));
    }

    buf
}

/// Sign using pre-built msgpack bytes (bypassing serde_json field ordering).
fn sign_with_msgpack(
    msgpack: &[u8],
    private_key: &str,
    nonce: u64,
    vault_address: Option<&str>,
) -> Result<(String, String, u8), String> {
    use k256::ecdsa::SigningKey;

    let mut data = msgpack.to_vec();
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
    let agent_type_hash = keccak256(b"Agent(string source,bytes32 connectionId)");
    let source_hash = keccak256(b"a");
    let mut struct_data = [0u8; 96];
    struct_data[..32].copy_from_slice(&agent_type_hash);
    struct_data[32..64].copy_from_slice(&source_hash);
    struct_data[64..96].copy_from_slice(&connection_id);
    let struct_hash = keccak256(&struct_data);

    let domain_sep = hyperliquid_domain_separator();
    let mut final_data = Vec::with_capacity(66);
    final_data.extend_from_slice(b"\x19\x01");
    final_data.extend_from_slice(&domain_sep);
    final_data.extend_from_slice(&struct_hash);
    let final_hash = keccak256(&final_data);

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

/// Signs a Hyperliquid exchange action using EIP-712.
/// Returns (r, s, v) where r and s are "0x"-prefixed hex strings and v is 27 or 28.
fn sign_action(
    private_key: &str,
    action: &Value,
    vault_address: Option<&str>,
    nonce: u64,
) -> Result<(String, String, u8), String> {
    use k256::ecdsa::SigningKey;

    // Step 1: msgpack-encode the action preserving Python dict field order,
    // then append nonce + vault flag.
    let msgpack_bytes = action_to_canonical_msgpack(action)?;
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
            .info_post(
                serde_json::json!({"type": "metaAndAssetCtxs"}),
                20,
                "get_open_interest",
            )
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
            .info_post(
                serde_json::json!({"type": "metaAndAssetCtxs"}),
                20,
                "get_asset_context",
            )
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
            sz_decimals: meta.universe.get(idx).map(|a| a.sz_decimals).unwrap_or(0),
        })
    }

    /// Fetches metaAndAssetCtxs once and returns all asset contexts in universe order.
    /// Prefer this over repeated `get_asset_context` calls to avoid rate-limiting.
    async fn get_all_asset_contexts(&self) -> Result<Vec<AssetContext>, String> {
        // metaAndAssetCtxs → weight 20
        let resp = self
            .info_post(
                serde_json::json!({"type": "metaAndAssetCtxs"}),
                20,
                "get_all_asset_contexts",
            )
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
                sz_decimals: asset.sz_decimals,
            });
        }
        Ok(result)
    }

    /// Returns the number of decimal places for order size for a symbol.
    async fn get_sz_decimals(&self, symbol: String) -> Result<i32, String> {
        let all = self.get_all_sz_decimals().await?;
        all.get(&symbol)
            .copied()
            .ok_or_else(|| format!("symbol {} not found", symbol))
    }

    /// Returns sz_decimals for all symbols from the meta universe.
    async fn get_all_sz_decimals(&self) -> Result<HashMap<String, i32>, String> {
        // metaAndAssetCtxs → weight 20
        let resp = self
            .info_post(
                serde_json::json!({"type": "metaAndAssetCtxs"}),
                20,
                "get_all_sz_decimals",
            )
            .await?;
        let (meta, _) = parse_response::<Option<MetaAndAssetCtxsResponse>>(resp)
            .await?
            .ok_or_else(|| "metaAndAssetCtxs returned null".to_string())?;
        Ok(meta
            .universe
            .into_iter()
            .map(|a| (a.name, a.sz_decimals))
            .collect())
    }

    /// Returns a full L2 orderbook snapshot for `symbol` from the l2Book REST endpoint.
    async fn get_l2_orderbook(&self, symbol: String) -> Result<L2Snapshot, String> {
        // l2Book → weight 2
        let resp = self
            .info_post(
                serde_json::json!({"type": "l2Book", "coin": symbol}),
                2,
                "get_l2_orderbook",
            )
            .await?;
        let book: Option<HyperliquidWsBook> = parse_response(resp).await?;
        let book = match book {
            Some(b) => b,
            None => {
                return Ok(L2Snapshot {
                    symbol,
                    bids: vec![],
                    asks: vec![],
                    sequence: 0,
                })
            }
        };
        let bids = book
            .levels
            .first()
            .into_iter()
            .flatten()
            .filter_map(|level| {
                Some(L2Level {
                    price: parse_decimal(&level.px)?,
                    volume: parse_decimal(&level.sz)?,
                })
            })
            .collect();
        let asks = book
            .levels
            .get(1)
            .into_iter()
            .flatten()
            .filter_map(|level| {
                Some(L2Level {
                    price: parse_decimal(&level.px)?,
                    volume: parse_decimal(&level.sz)?,
                })
            })
            .collect();
        Ok(L2Snapshot {
            symbol: book.coin,
            bids,
            asks,
            sequence: book.time,
        })
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
            .info_post(
                serde_json::json!({"type": "predictedFundings"}),
                20,
                "get_predicted_fundings",
            )
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
    ///
    /// Trigger orders (`TakeProfit` / `StopLoss`) require `trigger_price` to be set. The order
    /// activates when the mark price reaches `triggerPx`, then executes as a market or limit
    /// order depending on `time_in_force` (`Ioc` = market, `Gtc` = limit).
    async fn place_order(
        &self,
        symbol: String,
        side: OrderSide,
        price: Decimal,
        volume: Decimal,
        order_type: OrderType,
        time_in_force: TimeInForce,
        trigger_price: Option<Decimal>,
        reduce_only: bool,
        cloid: Option<String>,
    ) -> Result<OrderPlacement, String> {
        // Rate limiting is handled in submit_signed_action (non-blocking).
        let asset_idx = self.get_asset_index(&symbol).await?;
        let is_buy = matches!(side, OrderSide::Buy);

        let tif_str = match time_in_force {
            TimeInForce::Gtc => "Gtc",
            TimeInForce::Ioc => "Ioc",
            TimeInForce::Fok => "Fok",
            TimeInForce::Alo => "Alo",
        };

        let cloid_hex = cloid.clone();

        // Determine if this is a trigger order
        let is_trigger = matches!(order_type, OrderType::TakeProfit | OrderType::StopLoss);

        let (order_msgpack, order_type_json) = if is_trigger {
            // --- Trigger order ---
            let trigger_px = trigger_price
                .ok_or_else(|| format!("{:?} order requires trigger_price to be set", order_type))?
                .normalize()
                .to_string();

            let tpsl = match order_type {
                OrderType::TakeProfit => "tp",
                OrderType::StopLoss => "sl",
                _ => unreachable!(),
            };

            // TimeInForce determines market vs limit on trigger:
            // Ioc = market execution (isMarket: true), Gtc/Alo/Fok = limit (isMarket: false)
            let is_market = matches!(time_in_force, TimeInForce::Ioc);

            // For trigger orders, `p` must be set to the trigger price (not "0"),
            // even for market-on-trigger. Hyperliquid validates this field.
            let price_str = if is_market {
                trigger_px.clone()
            } else {
                price.normalize().to_string()
            };

            let msgpack = build_trigger_order_msgpack(
                asset_idx,
                is_buy,
                &price_str,
                &volume.normalize().to_string(),
                reduce_only,
                &trigger_px,
                is_market,
                tpsl,
                cloid_hex.as_deref(),
            );

            let json_type = if is_market {
                format!(
                    r#"{{"trigger":{{"isMarket":true,"triggerPx":"{}","tpsl":"{}"}}}}"#,
                    trigger_px, tpsl
                )
            } else {
                format!(
                    r#"{{"trigger":{{"isMarket":false,"triggerPx":"{}","tpsl":"{}"}}}}"#,
                    trigger_px, tpsl
                )
            };

            (msgpack, json_type)
        } else {
            // --- Regular limit/market order ---
            let (order_kind, tif_bytes) = match order_type {
                OrderType::Limit => ("limit", tif_str.as_bytes()),
                OrderType::Market => ("limit", b"Ioc".as_slice()),
                _ => unreachable!(),
            };

            let price_str = price.normalize().to_string();
            let size_str = volume.normalize().to_string();

            let msgpack = build_order_msgpack(
                asset_idx,
                is_buy,
                &price_str,
                &size_str,
                reduce_only,
                order_kind,
                tif_bytes,
                cloid_hex.as_deref(),
            );

            let json_type = match order_type {
                OrderType::Limit => format!(r#"{{"limit":{{"tif":"{tif_str}"}}}}"#),
                OrderType::Market => r#"{"limit":{"tif":"Ioc"}}"#.to_string(),
                _ => unreachable!(),
            };

            (msgpack, json_type)
        };

        // Build the action-level msgpack with Python SDK field order (insertion order):
        // type → orders → grouping
        let mut action_msgpack = Vec::new();
        action_msgpack.push(0x83); // fixmap(3)
        action_msgpack.extend_from_slice(&value_to_msgpack(&Value::String("type".to_string())));
        action_msgpack.extend_from_slice(&value_to_msgpack(&Value::String("order".to_string())));
        action_msgpack.extend_from_slice(&value_to_msgpack(&Value::String("orders".to_string())));
        action_msgpack.push(0x91); // fixarray(1)
        action_msgpack.extend_from_slice(&order_msgpack);
        action_msgpack.extend_from_slice(&value_to_msgpack(&Value::String("grouping".to_string())));
        action_msgpack.extend_from_slice(&value_to_msgpack(&Value::String("na".to_string())));

        let cloid_json = if let Some(ref c) = cloid_hex {
            format!(r#","c":"{c}""#)
        } else {
            String::new()
        };

        let reduce_json = if reduce_only { "true" } else { "false" };

        let price_for_json = if is_trigger && matches!(time_in_force, TimeInForce::Ioc) {
            // For market-on-trigger, use trigger price (same as msgpack)
            if let Some(ref tp) = trigger_price {
                tp.normalize().to_string()
            } else {
                price.normalize().to_string()
            }
        } else {
            price.normalize().to_string()
        };

        let action_json_str = format!(
            r#"{{"type":"order","orders":[{{"a":{asset_idx},"b":{is_buy},"p":"{price}","s":"{size}","r":{reduce_json},"t":{order_type_json}{cloid_json}}}],"grouping":"na"}}"#,
            price = price_for_json,
            size = volume.normalize().to_string(),
        );

        // Sign using the canonical msgpack (matching Python's field order)
        let private_key = self.require_private_key()?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let (r, s, v) = sign_with_msgpack(&action_msgpack, private_key, nonce, None)?;

        let payload_str = format!(
            r#"{{"action":{},"nonce":{},"signature":{{"r":"{}","s":"{}","v":{}}},"vaultAddress":null,"expiresAfter":null}}"#,
            action_json_str, nonce, r, s, v
        );

        self.rest_limiter.acquire(1).await.map_err(|e| {
            format!(
                "rate_limited: rest_weight exhausted, retry_after_ms={}",
                e.retry_after.as_millis()
            )
        })?;
        self.address_limiter.acquire(1, false).await.map_err(|e| {
            format!(
                "rate_limited: address quota exhausted, retry_after_ms={}",
                e.retry_after.as_millis()
            )
        })?;

        let resp = self
            .client
            .post(HYPERLIQUID_EXCHANGE_URL)
            .header("Content-Type", "application/json")
            .body(payload_str)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.map_err(|e| e.to_string())?;
            return Err(format!("HTTP {status}: {text}"));
        }

        let body: Value = parse_response(resp).await?;
        if body["status"].as_str() == Some("err") {
            return Err(body["response"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string());
        }
        let statuses = &body["response"]["data"]["statuses"][0];

        let (oid, returned_cloid, timestamp_ms) = if let Some(resting) = statuses.get("resting") {
            let oid = resting["oid"]
                .as_i64()
                .ok_or_else(|| format!("resting status missing oid: {}", body))?;
            let returned_cloid = resting["cloid"].as_str().map(|s: &str| s.to_string());
            // resting doesn't include a timestamp
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            (oid, returned_cloid, ts)
        } else if let Some(filled) = statuses.get("filled") {
            let oid = filled["oid"]
                .as_i64()
                .ok_or_else(|| format!("filled status missing oid: {}", body))?;
            // filled doesn't include cloid
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            (oid, None, ts)
        } else if let Some(error) = statuses.get("error") {
            return Err(error
                .as_str()
                .unwrap_or("order rejected with unknown error")
                .to_string());
        } else {
            return Err(format!("unexpected order status: {}", body));
        };

        Ok(OrderPlacement {
            order_id: oid,
            symbol,
            side,
            price,
            quantity: volume,
            timestamp_ms,
            cloid: returned_cloid.or(cloid),
            order_type,
            trigger_price,
            reduce_only,
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
            .info_post(
                serde_json::json!({"type": "openOrders", "user": user}),
                20,
                "change_order_by_cloid",
            )
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

    /// Cancels a single order by its client order ID (cloid). Requires `with_auth`.
    /// Fetches open orders to resolve the order ID for the matching cloid, then
    /// submits a cancel action using the order ID — this works for all order types
    /// including trigger orders (TakeProfit/StopLoss).
    async fn cancel_order_by_cloid(&self, cloid: String) -> Result<(), String> {
        let user = self.require_user_address()?;

        // openOrders → weight 20
        let resp = self
            .info_post(
                serde_json::json!({"type": "openOrders", "user": user}),
                20,
                "cancel_order_by_cloid",
            )
            .await?;
        let orders: Vec<RestOpenOrder> = parse_response(resp).await?;
        let order = orders
            .iter()
            .find(|o| o.cloid.as_ref() == Some(&cloid))
            .ok_or_else(|| format!("order with cloid {} not found", cloid))?;

        // meta → weight 20
        let meta_resp = self
            .info_post(
                serde_json::json!({"type": "meta"}),
                20,
                "cancel_order_by_cloid",
            )
            .await?;
        let meta: MetaResponse = parse_response(meta_resp).await?;

        let asset_idx = meta
            .universe
            .iter()
            .position(|a| a.name == order.coin)
            .ok_or_else(|| format!("asset {} not found in meta", order.coin))?;

        // Use the same "cancel" action type as cancel_all_order, which is
        // proven to work for all order types including trigger orders.
        let action = serde_json::json!({
            "type": "cancel",
            "cancels": [{"a": asset_idx, "o": order.oid}]
        });

        self.submit_signed_action(action, None).await?;
        Ok(())
    }

    /// Cancels all open orders. Requires `with_auth`.
    /// Fetches all open orders and submits a batch cancel in a single signed request.
    async fn cancel_all_order(&self) -> Result<bool, String> {
        let user = self.require_user_address()?;

        // openOrders → weight 20
        let resp = self
            .info_post(
                serde_json::json!({"type": "openOrders", "user": user}),
                20,
                "cancel_all_order",
            )
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
        Box::pin(stream::iter(vec![Err(format!(
            "subscribe_l2_update is unsupported for {symbol}; use subscribe_l2_snapshot"
        ))]))
    }

    fn subscribe_l2_snapshot(&self, symbol: String) -> BoxStream<Result<L2Snapshot, String>> {
        let subscription = HyperliquidSubscription::L2Book { coin: symbol };
        Box::pin(managed_stream(
            self.market_ws_manager.clone(),
            subscription,
            |msg: HyperliquidWsInboundMessage| {
                if let Some(snapshot) = msg.as_l2_snapshot() {
                    vec![Ok(snapshot)]
                } else {
                    vec![]
                }
            },
        ))
    }

    fn subscribe_asset_context(&self, symbol: String) -> BoxStream<Result<AssetContext, String>> {
        let subscription = HyperliquidSubscription::ActiveAssetCtx { coin: symbol };
        Box::pin(managed_stream(
            self.market_ws_manager.clone(),
            subscription,
            |msg: HyperliquidWsInboundMessage| {
            if let Some(ctx) = msg.as_asset_context() {
                vec![Ok(ctx)]
            } else {
                vec![]
            }
        }))
    }

    fn subscribe_liquidation(&self, user: String) -> BoxStream<Result<Liquidation, String>> {
        subscribe_user_stream(
            self,
            user.clone(),
            HyperliquidSubscription::UserEvents { user_addr: user },
            |msg: HyperliquidWsInboundMessage| {
                if let Some(liq) = msg.as_liquidation() {
                    vec![Ok(liq)]
                } else {
                    vec![]
                }
            },
        )
    }

    fn subscribe_fill(&self, symbol: String) -> BoxStream<Result<Fill, String>> {
        let subscription = HyperliquidSubscription::Trades { coin: symbol };
        Box::pin(managed_stream(
            self.market_ws_manager.clone(),
            subscription,
            |msg: HyperliquidWsInboundMessage| {
            if let Some(fills) = msg.as_trades() {
                fills.into_iter().map(Ok).collect()
            } else {
                vec![]
            }
        }))
    }
}

fn subscribe_user_stream<T, F>(
    client: &HyperliquidClient,
    user_addr: String,
    subscription: HyperliquidSubscription,
    parse: F,
) -> BoxStream<Result<T, String>>
where
    T: Send + 'static,
    F: Fn(HyperliquidWsInboundMessage) -> Vec<Result<T, String>> + Send + Sync + 'static,
{
    let manager = get_or_create_user_manager(
        &client.user_ws_managers,
        client.ws_send_limiter.clone(),
        user_addr,
    );
    Box::pin(async_stream::stream! {
        let stream = managed_stream(manager, subscription, parse);
        tokio::pin!(stream);
        while let Some(item) = stream.next().await {
            yield item;
        }
    })
}

fn get_or_create_user_manager(
    user_ws_managers: &RwLock<HashMap<String, HyperliquidWsManager>>,
    ws_send_limiter: WsSendRateLimiter,
    user_addr: String,
) -> HyperliquidWsManager {
    {
        let managers = user_ws_managers.read().unwrap_or_else(|e| e.into_inner());
        if let Some(manager) = managers.get(&user_addr) {
            return manager.clone();
        }
    }

    let mut managers = user_ws_managers.write().unwrap_or_else(|e| e.into_inner());
    managers
        .entry(user_addr.clone())
        .or_insert_with(|| HyperliquidWsManager::new(Some(user_addr), ws_send_limiter))
        .clone()
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
            .info_post(
                serde_json::json!({"type": "openOrders", "user": user}),
                20,
                "get_open_orders",
            )
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
                    order_type: None, // openOrders REST endpoint doesn't return order type
                    trigger_price: None, // trigger info not included in openOrders response
                    reduce_only: false, // default; REST doesn't expose this field
                })
            })
            .collect())
    }

    /// Returns all per-asset balances from `spotClearinghouseState` with margin health.
    /// Requires `with_auth`.
    async fn get_balance(&self) -> Result<Vec<guilder_abstraction::AccountBalance>, String> {
        let user = self.require_user_address()?;
        // spotClearinghouseState → weight 15
        let resp = self
            .info_post(
                serde_json::json!({"type": "spotClearinghouseState", "user": user}),
                15,
                "get_balance",
            )
            .await?;

        #[derive(Deserialize)]
        struct SpotStateResponse {
            balances: Vec<SpotBalance>,
            #[serde(rename = "tokenToAvailableAfterMaintenance")]
            token_to_available_after_maintenance: Vec<(i32, String)>,
        }

        #[allow(dead_code)]
        #[derive(Deserialize)]
        struct SpotBalance {
            coin: String,
            total: String,
            hold: String,
            #[serde(default)]
            token: Option<i32>,
            #[serde(default)]
            #[serde(rename = "entryNtl")]
            entry_ntl: Option<String>,
        }

        let state: SpotStateResponse = parse_response(resp).await?;

        // Build a safe lookup map from token ID → available after maintenance.
        // Not every token appears in the map — absence means zero maintenance impact.
        let safe_map: HashMap<i32, Decimal> = state
            .token_to_available_after_maintenance
            .into_iter()
            .filter_map(|(token_id, value)| {
                parse_decimal(&value).map(|v| (token_id, v))
            })
            .collect();

        state
            .balances
            .into_iter()
            .map(|balance| {
                let equity = parse_decimal(&balance.total)
                    .ok_or_else(|| "invalid total balance".to_string())?;
                let hold = parse_decimal(&balance.hold)
                    .ok_or_else(|| "invalid hold balance".to_string())?;
                let free = equity - hold;

                let safe = balance.token.and_then(|token_id| safe_map.get(&token_id).copied());
                let usable = match safe {
                    Some(s) => std::cmp::min(free, s),
                    None => free,
                };
                let maintenance = safe.map(|s| equity - s);

                Ok(guilder_abstraction::AccountBalance {
                    token: balance.coin,
                    balance: equity,
                    free,
                    safe,
                    usable,
                    hold,
                    margin_used: None,
                    maintenance,
                })
            })
            .collect()
    }

    /// Returns the user's address-level API rate limit budget.
    /// Queries Hyperliquid's `userRateLimit` info endpoint for authoritative server-side counts.
    async fn get_user_rate_limit(&self) -> Result<guilder_abstraction::UserRateLimit, String> {
        let user = self.require_user_address()?;
        let resp = self
            .info_post(
                serde_json::json!({"type": "userRateLimit", "user": user}),
                20,
                "get_user_rate_limit",
            )
            .await?;
        let val = parse_response::<Value>(resp).await?;

        let cumulative_volume = val["cumVlm"]
            .as_str()
            .and_then(parse_decimal)
            .ok_or_else(|| "missing or invalid cumVlm".to_string())?;
        let requests_used = val["nRequestsUsed"]
            .as_i64()
            .ok_or_else(|| "missing or invalid nRequestsUsed".to_string())?;
        let requests_cap = val["nRequestsCap"]
            .as_i64()
            .ok_or_else(|| "missing or invalid nRequestsCap".to_string())?;
        let requests_surplus = val["nRequestsSurplus"]
            .as_i64()
            .ok_or_else(|| "missing or invalid nRequestsSurplus".to_string())?;

        Ok(guilder_abstraction::UserRateLimit {
            cumulative_volume,
            requests_used,
            requests_cap,
            requests_surplus,
        })
    }
}

#[allow(async_fn_in_trait)]
impl guilder_abstraction::SubscribeUserEvents for HyperliquidClient {
    fn subscribe_user_fills(&self) -> BoxStream<Result<UserFill, String>> {
        let Some(addr) = self.user_address.as_ref() else {
            return Box::pin(stream::iter(vec![Err(
                "user address not registered".to_string()
            )]));
        };
        subscribe_user_stream(
            self,
            addr.clone(),
            HyperliquidSubscription::UserEvents {
                user_addr: addr.clone(),
            },
            |msg: HyperliquidWsInboundMessage| {
                if let Some(fills) = msg.as_user_fills() {
                    fills.into_iter().map(Ok).collect()
                } else {
                    vec![]
                }
            },
        )
    }

    fn subscribe_order_updates(&self) -> BoxStream<Result<OrderUpdate, String>> {
        let Some(addr) = self.user_address.as_ref() else {
            return Box::pin(stream::iter(vec![Err(
                "user address not registered".to_string()
            )]));
        };
        subscribe_user_stream(
            self,
            addr.clone(),
            HyperliquidSubscription::OrderUpdates {
                user_addr: addr.clone(),
            },
            |msg: HyperliquidWsInboundMessage| {
                if let Some(updates) = msg.as_order_updates() {
                    updates.into_iter().map(Ok).collect()
                } else {
                    vec![]
                }
            },
        )
    }

    fn subscribe_funding_payments(&self) -> BoxStream<Result<FundingPayment, String>> {
        let Some(addr) = self.user_address.as_ref() else {
            return Box::pin(stream::iter(vec![Err(
                "user address not registered".to_string()
            )]));
        };
        subscribe_user_stream(
            self,
            addr.clone(),
            HyperliquidSubscription::UserEvents {
                user_addr: addr.clone(),
            },
            |msg: HyperliquidWsInboundMessage| {
                if let Some(p) = msg.as_funding_payment() {
                    vec![Ok(p)]
                } else {
                    vec![]
                }
            },
        )
    }

    fn subscribe_deposits(&self) -> BoxStream<Result<Deposit, String>> {
        let Some(addr) = self.user_address.as_ref() else {
            return Box::pin(stream::iter(vec![Err(
                "user address not registered".to_string()
            )]));
        };
        subscribe_user_stream(
            self,
            addr.clone(),
            HyperliquidSubscription::NonFundingLedger {
                user_addr: addr.clone(),
            },
            |msg: HyperliquidWsInboundMessage| {
                if let Some(deps) = msg.as_deposits() {
                    deps.into_iter().map(Ok).collect()
                } else {
                    vec![]
                }
            },
        )
    }

    fn subscribe_withdrawals(&self) -> BoxStream<Result<Withdrawal, String>> {
        let Some(addr) = self.user_address.as_ref() else {
            return Box::pin(stream::iter(vec![Err(
                "user address not registered".to_string()
            )]));
        };
        subscribe_user_stream(
            self,
            addr.clone(),
            HyperliquidSubscription::NonFundingLedger {
                user_addr: addr.clone(),
            },
            |msg: HyperliquidWsInboundMessage| {
                if let Some(wds) = msg.as_withdrawals() {
                    wds.into_iter().map(Ok).collect()
                } else {
                    vec![]
                }
            },
        )
    }

    /// Subscribe to spot wallet balance updates for the registered user address.
    fn subscribe_spot_balance(
        &self,
    ) -> BoxStream<Result<Vec<guilder_abstraction::AccountBalance>, String>> {
        let Some(addr) = self.user_address.as_ref() else {
            return Box::pin(stream::iter(vec![Err(
                "user address not registered".to_string()
            )]));
        };
        self.subscribe_spot_balance_with_address(addr.clone())
    }

    /// Subscribe to spot wallet balance updates for a specific address.
    fn subscribe_spot_balance_with_address(
        &self,
        address: String,
    ) -> BoxStream<Result<Vec<guilder_abstraction::AccountBalance>, String>> {
        subscribe_user_stream(
            self,
            address.clone(),
            HyperliquidSubscription::UserEvents { user_addr: address },
            |msg: HyperliquidWsInboundMessage| {
                if let Some(balances) = msg.as_spot_balance() {
                    vec![Ok(balances)]
                } else {
                    vec![]
                }
            },
        )
    }

    async fn unsubscribe_user_events(&self) {
        // Unsubscribe from the market-level user manager if we have a user address.
        if let Some(addr) = &self.user_address {
            self.market_ws_manager.unsubscribe_user(addr);
        }
        // Also unsubscribe from all per-user managers.
        let managers = self.user_ws_managers.read().unwrap_or_else(|e| e.into_inner());
        for (addr, manager) in managers.iter() {
            manager.unsubscribe_user(addr);
        }
    }
}

impl guilder_abstraction::SubscribeMarketDataOps for HyperliquidClient {
    async fn unsubscribe_market_data(&self, symbol: String) {
        self.market_ws_manager.unsubscribe_by_coin(&symbol);
    }
}

#[cfg(test)]
mod msgpack_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_msgpack_null() {
        let result = value_to_msgpack(&Value::Null);
        assert_eq!(result, vec![0xc0]);
    }

    #[test]
    fn test_msgpack_bool() {
        assert_eq!(value_to_msgpack(&Value::Bool(true)), vec![0xc3]);
        assert_eq!(value_to_msgpack(&Value::Bool(false)), vec![0xc2]);
    }

    #[test]
    fn test_msgpack_positive_fixint() {
        // 0–127: positive fixint
        assert_eq!(value_to_msgpack(&json!(0)), vec![0x00]);
        assert_eq!(value_to_msgpack(&json!(1)), vec![0x01]);
        assert_eq!(value_to_msgpack(&json!(127)), vec![0x7f]);
    }

    #[test]
    fn test_msgpack_uint8() {
        // 128–255: uint8
        assert_eq!(value_to_msgpack(&json!(128)), vec![0xcc, 0x80]);
        assert_eq!(value_to_msgpack(&json!(255)), vec![0xcc, 0xff]);
    }

    #[test]
    fn test_msgpack_uint16() {
        // 256–65535: uint16
        assert_eq!(value_to_msgpack(&json!(256)), vec![0xcd, 0x01, 0x00]);
        assert_eq!(value_to_msgpack(&json!(65535)), vec![0xcd, 0xff, 0xff]);
    }

    #[test]
    fn test_msgpack_uint32() {
        // 65536–4294967295: uint32
        assert_eq!(
            value_to_msgpack(&json!(65536)),
            vec![0xce, 0x00, 0x01, 0x00, 0x00]
        );
        assert_eq!(
            value_to_msgpack(&json!(4294967295u64)),
            vec![0xce, 0xff, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    fn test_msgpack_uint64() {
        // >4294967295: uint64
        let big: u64 = 4294967296;
        assert_eq!(
            value_to_msgpack(&json!(big)),
            vec![0xcf, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn test_msgpack_negative_fixint() {
        // -1 to -32: negative fixint
        assert_eq!(value_to_msgpack(&json!(-1)), vec![0xff]);
        assert_eq!(value_to_msgpack(&json!(-32)), vec![0xe0]);
    }

    #[test]
    fn test_msgpack_int8() {
        // -33 to -128: int8
        assert_eq!(value_to_msgpack(&json!(-33)), vec![0xd0, 0xdf]);
        assert_eq!(value_to_msgpack(&json!(-128)), vec![0xd0, 0x80]);
    }

    #[test]
    fn test_msgpack_int16() {
        // -129 to -32768: int16
        assert_eq!(value_to_msgpack(&json!(-129)), vec![0xd1, 0xff, 0x7f]);
        assert_eq!(value_to_msgpack(&json!(-32768)), vec![0xd1, 0x80, 0x00]);
    }

    #[test]
    fn test_msgpack_int32() {
        // -32769 to -2147483648: int32
        assert_eq!(
            value_to_msgpack(&json!(-32769)),
            vec![0xd2, 0xff, 0xff, 0x7f, 0xff]
        );
        assert_eq!(
            value_to_msgpack(&json!(-2147483648i64)),
            vec![0xd2, 0x80, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn test_msgpack_int64() {
        let val: i64 = -2147483649;
        let result = value_to_msgpack(&json!(val));
        assert_eq!(result[0], 0xd3); // int64 marker
        assert_eq!(result.len(), 9);
    }

    #[test]
    fn test_msgpack_float() {
        let result = value_to_msgpack(&json!(3.14));
        assert_eq!(result[0], 0xcb); // float64 marker
        assert_eq!(result.len(), 9);
    }

    #[test]
    fn test_msgpack_fixstr() {
        // 0–31 bytes: fixstr
        assert_eq!(value_to_msgpack(&json!("")), vec![0xa0]);
        assert_eq!(value_to_msgpack(&json!("hello")), {
            let mut expected = vec![0xa5];
            expected.extend_from_slice(b"hello");
            expected
        });
        let s = "a".repeat(31);
        let result = value_to_msgpack(&json!(s));
        assert_eq!(result[0], 0xbf); // 0xa0 | 31
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_msgpack_str8() {
        let s = "a".repeat(32);
        let result = value_to_msgpack(&json!(s));
        assert_eq!(result[0], 0xd9); // str8 marker
        assert_eq!(result[1], 32);
        assert_eq!(result.len(), 34);
    }

    #[test]
    fn test_msgpack_fixarray() {
        // 0–15 elements: fixarray
        assert_eq!(value_to_msgpack(&json!([])), vec![0x90]);
        let result = value_to_msgpack(&json!([1, 2, 3]));
        assert_eq!(result[0], 0x93);
        assert_eq!(result, vec![0x93, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_msgpack_fixmap() {
        // 0–15 entries: fixmap
        assert_eq!(value_to_msgpack(&json!({})), vec![0x80]);
        let result = value_to_msgpack(&json!({"a": 1}));
        assert_eq!(result[0], 0x81); // fixmap(1)
        assert_eq!(result, {
            let mut expected = vec![0x81];
            expected.extend_from_slice(&value_to_msgpack(&json!("a")));
            expected.extend_from_slice(&value_to_msgpack(&json!(1)));
            expected
        });
    }

    #[test]
    fn test_msgmap_preserves_insertion_order() {
        // Verify keys are serialized in JSON insertion order, not sorted
        let val = json!({
            "z": 1,
            "a": 2,
            "m": 3
        });
        let result = value_to_msgpack(&val);
        // fixmap(3)
        assert_eq!(result[0], 0x83);
        // First key should be "z" (insertion order), not "a" (sorted)
        assert_eq!(result[1], 0xa1); // fixstr(1)
        assert_eq!(result[2], b'z');
    }

    #[test]
    fn test_msgpack_mixed_array() {
        let val = json!([null, true, false, 42, "hi", [1, 2]]);
        let result = value_to_msgpack(&val);
        assert_eq!(result[0], 0x96); // fixarray(6)
        assert_eq!(result[1], 0xc0); // null
        assert_eq!(result[2], 0xc3); // true
        assert_eq!(result[3], 0xc2); // false
        assert_eq!(result[4], 0x2a); // 42
                                     // "hi" = fixstr(2) + "hi"
        assert_eq!(result[5], 0xa2);
        assert_eq!(result[6], b'h');
        assert_eq!(result[7], b'i');
    }

    #[test]
    fn test_build_order_msgpack_without_cloid() {
        let result = build_order_msgpack(
            0,       // asset index
            true,    // is_buy
            "1000",  // price
            "0.1",   // size
            false,   // reduce_only
            "limit", // order_kind
            b"gtc",  // tif
            None,    // cloid
        );
        // fixmap(6)
        assert_eq!(result[0], 0x86);
    }

    #[test]
    fn test_build_order_msgpack_with_cloid() {
        let result = build_order_msgpack(
            0,                // asset index
            true,             // is_buy
            "1000",           // price
            "0.1",            // size
            false,            // reduce_only
            "limit",          // order_kind
            b"gtc",           // tif
            Some("my-cloid"), // cloid
        );
        // fixmap(7)
        assert_eq!(result[0], 0x87);
    }

    #[test]
    fn test_action_to_canonical_msgpack() {
        let action = json!({
            "type": "order",
            "orders": [{"a": 0, "b": true, "p": "1000", "s": "0.1", "r": false, "t": {"limit": {"tif": "gtc"}}}],
            "grouping": "na"
        });
        let result = action_to_canonical_msgpack(&action).unwrap();
        // fixmap(3)
        assert_eq!(result[0], 0x83);
    }

    #[test]
    fn test_msgpack_matches_rmp_serde_for_simple_values() {
        // Verify our encoding matches rmp_serde for simple scalar values
        use rmp_serde::to_vec;

        for val in [
            json!(0),
            json!(127),
            json!(255),
            json!(1000),
            json!(-1),
            json!(-32),
            json!(-128),
        ] {
            let ours = value_to_msgpack(&val);
            let theirs = to_vec(&val).unwrap();
            assert_eq!(
                ours, theirs,
                "mismatch for {}: ours={:?}, rmp={:?}",
                val, ours, theirs
            );
        }
    }

    #[test]
    fn test_msgpack_string_encoding() {
        use rmp_serde::to_vec;
        for val in [
            json!(""),
            json!("a"),
            json!("hello world"),
            json!("BTC-USD"),
        ] {
            let ours = value_to_msgpack(&val);
            let theirs = to_vec(&val).unwrap();
            assert_eq!(
                ours, theirs,
                "mismatch for {}: ours={:?}, rmp={:?}",
                val, ours, theirs
            );
        }
    }

    #[test]
    fn test_msgpack_bool_encoding() {
        use rmp_serde::to_vec;
        let theirs = to_vec(&json!(true)).unwrap();
        assert_eq!(value_to_msgpack(&json!(true)), theirs);
        let theirs = to_vec(&json!(false)).unwrap();
        assert_eq!(value_to_msgpack(&json!(false)), theirs);
    }

    #[test]
    fn test_msgpack_null_encoding() {
        use rmp_serde::to_vec;
        let theirs = to_vec(&Value::Null).unwrap();
        assert_eq!(value_to_msgpack(&Value::Null), theirs);
    }

    #[test]
    fn test_msgpack_nested_object() {
        let val = json!({
            "outer": {
                "inner": 42
            }
        });
        let result = value_to_msgpack(&val);
        assert_eq!(result[0], 0x81); // fixmap(1)
    }

    #[test]
    fn test_msgpack_empty_containers() {
        assert_eq!(value_to_msgpack(&json!([])), vec![0x90]);
        assert_eq!(value_to_msgpack(&json!({})), vec![0x80]);
    }
}
