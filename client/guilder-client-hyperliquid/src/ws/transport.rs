/// Duplex WebSocket transport trait.
///
/// Abstracts connection lifecycle and message I/O. Each exchange implements
/// it with its own `Inbound`/`Outbound` types.
pub trait WsTransport {
    /// Parsed inbound message type after deserialization.
    type Inbound;
    /// Typed outbound message to serialize before sending.
    type Outbound;
    /// Transport error type.
    type Error: std::fmt::Debug;

    /// Establish the WebSocket connection.
    fn connect(&mut self) -> impl std::future::Future<Output = Result<(), Self::Error>>;

    /// Send a typed message to the server.
    fn send(
        &mut self,
        msg: Self::Outbound,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>>;

    /// Receive the next typed message from the server.
    /// Returns `None` when the connection is cleanly closed.
    fn recv(
        &mut self,
    ) -> impl std::future::Future<Output = Option<Result<Self::Inbound, Self::Error>>>;

    /// Close the WebSocket connection gracefully.
    fn close(&mut self) -> impl std::future::Future<Output = Result<(), Self::Error>>;

    /// Whether the underlying connection is currently established.
    fn is_connected(&self) -> bool;
}

/// Hyperliquid-specific WebSocket transport.
///
/// Wraps a `tokio_tungstenite` connection and implements `WsTransport`
/// with `InboundMessage` / `OutboundMessage` as the typed message pair.
use crate::ws::inbound::HyperliquidWsInboundMessage;
use crate::ws::outbound::HyperliquidWsOutboundMessage;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
const HYPERLIQUID_WS_URL: &str = "wss://api.hyperliquid.xyz/ws";

type TungsteniteStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Hyperliquid WebSocket connection.
pub(crate) struct HyperliquidWs {
    url: String,
    stream: Option<TungsteniteStream>,
    /// Set to true when the stream was closed/dropped — distinguishes
    /// "never connected" from "was connected but lost".
    closed: bool,
}

impl HyperliquidWs {
    pub(crate) fn new() -> Self {
        Self {
            url: HYPERLIQUID_WS_URL.to_string(),
            stream: None,
            closed: false,
        }
    }
}

/// Error type for Hyperliquid WebSocket operations.
#[derive(Debug)]
pub(crate) enum WsError {
    /// The WebSocket connection is not established.
    NotConnected,
    /// I/O error from the underlying transport.
    Io(String),
    /// Failed to deserialize an inbound message.
    Parse(String),
    /// The server closed the connection.
    Closed,
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsError::NotConnected => write!(f, "websocket not connected"),
            WsError::Io(e) => write!(f, "websocket I/O error: {e}"),
            WsError::Parse(e) => write!(f, "websocket parse error: {e}"),
            WsError::Closed => write!(f, "websocket connection closed"),
        }
    }
}

impl std::error::Error for WsError {}

impl WsTransport for HyperliquidWs {
    type Inbound = HyperliquidWsInboundMessage;
    type Outbound = HyperliquidWsOutboundMessage;
    type Error = WsError;

    async fn connect(&mut self) -> Result<(), Self::Error> {
        let (ws, _) = connect_async(&self.url)
            .await
            .map_err(|e| WsError::Io(e.to_string()))?;
        self.stream = Some(ws);
        self.closed = false;
        Ok(())
    }

    async fn send(&mut self, msg: Self::Outbound) -> Result<(), Self::Error> {
        let stream = self.stream.as_mut().ok_or(WsError::NotConnected)?;
        let json = msg.to_json();
        stream
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| WsError::Io(e.to_string()))
    }

    async fn recv(&mut self) -> Option<Result<Self::Inbound, Self::Error>> {
        let stream = self.stream.as_mut()?;
        loop {
            match stream.next().await {
                None => {
                    self.closed = true;
                    return Some(Err(WsError::Closed));
                }
                Some(Err(e)) => {
                    self.closed = true;
                    return Some(Err(WsError::Io(e.to_string())));
                }
                Some(Ok(Message::Text(text))) => {
                    let Ok(env) =
                        serde_json::from_str::<crate::ws::inbound::HyperliquidWsEnvelope>(&text)
                    else {
                        return Some(Err(WsError::Parse(format!(
                            "invalid envelope: {text:.100}"
                        ))));
                    };
                    match HyperliquidWsInboundMessage::try_from(env) {
                        Ok(msg) => return Some(Ok(msg)),
                        Err(e) => return Some(Err(WsError::Parse(e.to_string()))),
                    }
                }
                Some(Ok(Message::Ping(_))) => {
                    // Tungstenite auto-responds to pongs; skip silently.
                }
                Some(Ok(Message::Pong(_))) => {}
                Some(Ok(Message::Close(_))) => {
                    self.closed = true;
                    return Some(Err(WsError::Closed));
                }
                Some(Ok(Message::Binary(_))) => {
                    // Unexpected on Hyperliquid WS; skip.
                }
                Some(Ok(Message::Frame(_))) => {}
            }
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        if let Some(stream) = self.stream.as_mut() {
            stream
                .close(None)
                .await
                .map_err(|e| WsError::Io(e.to_string()))
        } else {
            Ok(())
        }
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some() && !self.closed
    }
}
