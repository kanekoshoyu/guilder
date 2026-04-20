/// Minimal session: manages `WsTransport` connection and fans incoming messages
/// to per-subscription broadcast channels.
use super::inbound::HyperliquidWsInboundMessage;
use super::outbound::HyperliquidWsOutboundMessage;
use super::sub_key::SubKey;
use super::transport::{HyperliquidWs, WsTransport};
use guilder_abstraction::BoxStream;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

type Subs = Arc<RwLock<HashMap<SubKey, broadcast::Sender<HyperliquidWsInboundMessage>>>>;

#[derive(Clone)]
pub(crate) struct WsSession {
    ws: Arc<RwLock<HyperliquidWs>>,
    subs: Subs,
}

impl WsSession {
    pub(crate) fn new() -> Self {
        Self {
            ws: Arc::new(RwLock::new(HyperliquidWs::new())),
            subs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub(crate) fn subscribe(
        &self,
        key: SubKey,
        sub_msg: HyperliquidWsOutboundMessage,
    ) -> BoxStream<HyperliquidWsInboundMessage> {
        let ws = self.ws.clone();
        let subs = self.subs.clone();
        let subs2 = self.subs.clone();

        Box::pin(async_stream::stream! {
            // Connect once, spawn a single read loop.
            let already = {
                let mut w = ws.write().await;
                let connected = w.is_connected();
                if !connected {
                    let _ = w.connect().await;
                }
                connected
            };
            if !already {
                let ws2 = ws.clone();
                let subs2 = subs.clone();
                tokio::spawn(async move { read_loop(ws2, subs2).await });
            }

            // Register subscriber.
            let mut rx = {
                let mut map = subs2.write().await;
                let tx = map.entry(key.clone()).or_insert_with(|| {
                    broadcast::channel::<HyperliquidWsInboundMessage>(256).0
                }).clone();
                tx.subscribe()
            };

            // Send subscription message.
            {
                let mut w = ws.write().await;
                let _ = w.send(sub_msg).await;
            }

            // Yield messages.
            loop {
                match rx.recv().await {
                    Ok(msg) => yield msg,
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    }
}

async fn read_loop(ws: Arc<RwLock<HyperliquidWs>>, subs: Subs) {
    loop {
        let msg = {
            let mut w = ws.write().await;
            match w.recv().await {
                Some(Ok(m)) => m,
                _ => break,
            }
        };
        if let Some(key) = routing_key_from_message(&msg) {
            let map = subs.read().await;
            if let Some(tx) = map.get(&key) {
                let _ = tx.send(msg);
            }
        }
    }
}

fn routing_key_from_message(msg: &HyperliquidWsInboundMessage) -> Option<SubKey> {
    let channel = msg.channel_name();
    let routing_key = msg.routing_key()?;
    Some(SubKey {
        channel: channel.to_string(),
        routing_key,
    })
}
