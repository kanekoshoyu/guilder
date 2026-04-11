/// WebSocket multiplexer for Hyperliquid subscriptions.
///
/// Routes all subscriptions over a single shared WebSocket connection.
/// Manages reconnection, keepalive, and message routing by (channel, coin) pairs.
use futures_util::{SinkExt, StreamExt};
use guilder_abstraction::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const HYPERLIQUID_WS_URL: &str = "wss://api.hyperliquid.xyz/ws";
const MAX_RECONNECT_ATTEMPTS: u32 = 10;
const PONG_TIMEOUT_SECS: u64 = 30;
const PING_INTERVAL_SECS: u64 = 50;

/// Unique subscription identifier (channel + routing key).
/// Routing key can be a coin string (e.g. "BTC") or user address (e.g. "0x...").
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub(crate) struct SubKey {
    pub channel: String,
    pub routing_key: String,
}

/// Subscription request sent to the WsMux actor.
pub(crate) struct SubRequest {
    pub key: SubKey,
    pub subscription: Value,
    pub tx: mpsc::UnboundedSender<String>,
}

/// WebSocket message envelope.
#[derive(Serialize, Deserialize)]
struct WsEnvelope {
    channel: String,
    #[serde(default)]
    data: Value,
}

/// WebSocket multiplexer.
///
/// Manages a single shared WebSocket connection and routes incoming messages
/// to subscribers by (channel, routing_key).
pub(crate) struct WsMux {
    /// Channel to send subscription requests to the actor task.
    req_tx: mpsc::UnboundedSender<SubRequest>,
}

impl WsMux {
    /// Creates a new WsMux and spawns the connection actor.
    pub(crate) fn new() -> Self {
        let (req_tx, req_rx) = mpsc::unbounded_channel();
        tokio::spawn(ws_actor(req_rx));
        WsMux { req_tx }
    }

    /// Subscribe to (channel, routing_key) and return a BoxStream of JSON strings.
    pub(crate) fn subscribe(&self, key: SubKey, subscription: Value) -> BoxStream<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        let req = SubRequest {
            key,
            subscription,
            tx,
        };
        let _ = self.req_tx.send(req);

        Box::pin(async_stream::stream! {
            let mut rx = rx;
            while let Some(msg) = rx.recv().await {
                yield msg;
            }
        })
    }
}

/// Actor task that manages the single WebSocket connection.
///
/// Receives SubRequests, maintains active subscriptions, routes incoming
/// messages to the correct subscriber by (channel, routing_key), and handles
/// reconnection with exponential backoff.
async fn ws_actor(mut req_rx: mpsc::UnboundedReceiver<SubRequest>) {
    let mut backoff_secs: u64 = 1;
    let mut reconnect_attempts: u32 = 0;
    // Active subscriptions: (channel, coin) -> list of senders
    let subscriptions: Arc<RwLock<HashMap<SubKey, Vec<mpsc::UnboundedSender<String>>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    // Pending subscriptions to send on next connection
    let pending_subs: Arc<RwLock<HashMap<SubKey, Value>>> = Arc::new(RwLock::new(HashMap::new()));

    loop {
        if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
            eprintln!("ws max reconnect attempts ({MAX_RECONNECT_ATTEMPTS}) reached — giving up");
            break;
        }

        // Try to connect
        let ws = match connect_async(HYPERLIQUID_WS_URL).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                reconnect_attempts += 1;
                eprintln!(
                    "ws connect failed: {e} — reconnecting in {backoff_secs}s ({reconnect_attempts}/{MAX_RECONNECT_ATTEMPTS})"
                );
                tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(60);
                continue;
            }
        };

        let (mut sink, mut stream) = ws.split();

        // Re-subscribe all pending subscriptions on reconnection
        let mut resub_failed = false;
        {
            for (_, sub_value) in pending_subs.read().await.iter() {
                if let Err(e) = sink.send(Message::Text(sub_value.to_string().into())).await {
                    eprintln!("ws subscribe failed: {e} — reconnecting in {backoff_secs}s");
                    resub_failed = true;
                    reconnect_attempts += 1;
                    break;
                }
            }
        }

        if resub_failed {
            tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(60);
            continue;
        }

        // Connected successfully — reset backoff and attempt counter
        backoff_secs = 1;
        reconnect_attempts = 0;

        let mut ping_interval = tokio::time::interval_at(
            tokio::time::Instant::now() + std::time::Duration::from_secs(PING_INTERVAL_SECS),
            std::time::Duration::from_secs(PING_INTERVAL_SECS),
        );
        let mut pong_deadline: Option<std::pin::Pin<Box<tokio::time::Sleep>>> = None;

        let should_reconnect;
        loop {
            tokio::select! {
                Some(sub_req) = req_rx.recv() => {
                    // New subscription request
                    let SubRequest { key, subscription, tx } = sub_req;

                    // Add to pending subscriptions
                    pending_subs.write().await.insert(key.clone(), subscription.clone());

                    // Add sender to active subscriptions
                    let mut subs = subscriptions.write().await;
                    subs.entry(key).or_insert_with(Vec::new).push(tx);

                    // Send subscription to server
                    if let Err(e) = sink.send(Message::Text(subscription.to_string().into())).await {
                        eprintln!("ws subscribe send failed: {e} — reconnecting in {backoff_secs}s");
                        should_reconnect = true;
                        break;
                    }
                }
                _ = ping_interval.tick() => {
                    if let Err(e) = sink.send(Message::Text(r#"{"method":"ping"}"#.to_string().into())).await {
                        eprintln!("ws ping failed: {e} — reconnecting in {backoff_secs}s");
                        should_reconnect = true;
                        break;
                    }
                    pong_deadline = Some(Box::pin(tokio::time::sleep(
                        std::time::Duration::from_secs(PONG_TIMEOUT_SECS),
                    )));
                }
                _ = async { pong_deadline.as_mut().unwrap().await }, if pong_deadline.is_some() => {
                    eprintln!("ws pong timeout ({PONG_TIMEOUT_SECS}s) — reconnecting in {backoff_secs}s");
                    should_reconnect = true;
                    break;
                }
                msg = stream.next() => {
                    match msg {
                        None => {
                            eprintln!("ws stream ended — reconnecting in {backoff_secs}s");
                            should_reconnect = true;
                            break;
                        }
                        Some(Err(e)) => {
                            eprintln!("ws error: {e} — reconnecting in {backoff_secs}s");
                            should_reconnect = true;
                            break;
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = sink.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) => {
                            eprintln!("websocket closed — reconnecting in {backoff_secs}s");
                            should_reconnect = true;
                            break;
                        }
                        Some(Ok(Message::Text(text))) => {
                            let text_str = text.to_string();
                            let Ok(env) = serde_json::from_str::<WsEnvelope>(&text_str) else {
                                continue;
                            };

                            match env.channel.as_str() {
                                "pong" => {
                                    pong_deadline = None;
                                }
                                "subscriptionResponse" => {}
                                _ => {
                                    // Extract routing key from envelope data (try coin first, then user)
                                    let routing_key = env.data.get("coin").and_then(|c| c.as_str())
                                        .or_else(|| env.data.get("user").and_then(|u| u.as_str()));

                                    let subs = subscriptions.read().await;
                                    if let Some(key_str) = routing_key {
                                        let key = SubKey {
                                            channel: env.channel.clone(),
                                            routing_key: key_str.to_string(),
                                        };
                                        if let Some(senders) = subs.get(&key) {
                                            for sender in senders {
                                                let _ = sender.send(text_str.clone());
                                            }
                                        }
                                    } else {
                                        // No routing key in message (e.g. userEvents,
                                        // orderUpdates) — fan out to all subscribers
                                        // of this channel.
                                        for (key, senders) in subs.iter() {
                                            if key.channel == env.channel {
                                                for sender in senders {
                                                    let _ = sender.send(text_str.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }

        if should_reconnect {
            tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(60);
        }
    }
}
