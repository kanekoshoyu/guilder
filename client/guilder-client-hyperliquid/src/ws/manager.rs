use super::inbound::HyperliquidWsSubscriptionResponse;
use super::transport::{HyperliquidWs, WsTransport};
use super::{HyperliquidWsInboundMessage, HyperliquidWsOutboundMessage};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::{self, Duration as TokioDuration};
use tracing::warn;

use std::collections::HashMap;

const HEARTBEAT_INTERVAL: TokioDuration = TokioDuration::from_secs(25);
const IDLE_WATCHDOG_INTERVAL: TokioDuration = TokioDuration::from_secs(5);
const MAX_IDLE_BEFORE_RECONNECT: TokioDuration = TokioDuration::from_secs(40);
const BACKOFF_MAX_SECS: u64 = 30;
const SEND_SPACING: TokioDuration = TokioDuration::from_millis(40);
const FANOUT_CAPACITY: usize = 1024;
/// Grace period after unsubscribe during which messages for that coin are silently dropped.
const UNSUBSCRIBE_GRACE_PERIOD: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub(crate) enum HyperliquidSubscription {
    L2Book { coin: String },
    Trades { coin: String },
    ActiveAssetCtx { coin: String },
    UserEvents { user_addr: String },
    OrderUpdates { user_addr: String },
    NonFundingLedger { user_addr: String },
}

impl HyperliquidSubscription {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::L2Book { coin } => format!("l2Book:{coin}"),
            Self::Trades { coin } => format!("trades:{coin}"),
            Self::ActiveAssetCtx { coin } => format!("activeAssetCtx:{coin}"),
            Self::UserEvents { user_addr } => format!("user:{user_addr}"),
            Self::OrderUpdates { user_addr } => format!("orderUpdates:{user_addr}"),
            Self::NonFundingLedger { user_addr } => {
                format!("userNonFundingLedgerUpdates:{user_addr}")
            }
        }
    }

    pub(crate) fn subscribe_message(&self) -> HyperliquidWsOutboundMessage {
        match self {
            Self::L2Book { coin } => {
                HyperliquidWsOutboundMessage::SubscribeL2Book { coin: coin.clone() }
            }
            Self::Trades { coin } => {
                HyperliquidWsOutboundMessage::SubscribeTrades { coin: coin.clone() }
            }
            Self::ActiveAssetCtx { coin } => {
                HyperliquidWsOutboundMessage::SubscribeActiveAssetCtx { coin: coin.clone() }
            }
            Self::UserEvents { user_addr } => {
                HyperliquidWsOutboundMessage::SubscribeUserEvents {
                    user_addr: user_addr.clone(),
                }
            }
            Self::OrderUpdates { user_addr } => {
                HyperliquidWsOutboundMessage::SubscribeOrderUpdates {
                    user_addr: user_addr.clone(),
                }
            }
            Self::NonFundingLedger { user_addr } => {
                HyperliquidWsOutboundMessage::SubcribeNonFundingLedger {
                    user_addr: user_addr.clone(),
                }
            }
        }
    }

    pub(crate) fn unsubscribe_message(&self) -> HyperliquidWsOutboundMessage {
        HyperliquidWsOutboundMessage::Unsubscribe {
            subscription: self.subscription_payload(),
        }
    }

    fn subscription_payload(&self) -> serde_json::Value {
        match self {
            Self::L2Book { coin } => serde_json::json!({"type": "l2Book", "coin": coin}),
            Self::Trades { coin } => serde_json::json!({"type": "trades", "coin": coin}),
            Self::ActiveAssetCtx { coin } => {
                serde_json::json!({"type": "activeAssetCtx", "coin": coin})
            }
            Self::UserEvents { user_addr } => {
                serde_json::json!({"type": "user", "user": user_addr})
            }
            Self::OrderUpdates { user_addr } => {
                serde_json::json!({"type": "orderUpdates", "user": user_addr})
            }
            Self::NonFundingLedger { user_addr } => {
                serde_json::json!({"type": "userNonFundingLedgerUpdates", "user": user_addr})
            }
        }
    }

    /// Returns the coin/user symbol for this subscription, used for tracking
    /// recent unsubscriptions so in-flight messages are silently dropped.
    fn unsubscribe_symbol(&self) -> Option<String> {
        match self {
            Self::L2Book { coin }
            | Self::Trades { coin }
            | Self::ActiveAssetCtx { coin } => Some(coin.clone()),
            Self::UserEvents { user_addr }
            | Self::OrderUpdates { user_addr }
            | Self::NonFundingLedger { user_addr } => Some(user_addr.clone()),
        }
    }

    fn matches_message(&self, msg: &HyperliquidWsInboundMessage, manager_user: Option<&str>) -> bool {
        match (self, msg) {
            (Self::L2Book { coin }, HyperliquidWsInboundMessage::L2Book(book)) => book.coin == *coin,
            (Self::Trades { coin }, HyperliquidWsInboundMessage::Trades(trades)) => {
                trades.first().map(|trade| trade.coin.as_str()) == Some(coin.as_str())
            }
            (Self::ActiveAssetCtx { coin }, HyperliquidWsInboundMessage::ActiveAssetCtx(ctx)) => {
                ctx.coin == *coin
            }
            (Self::UserEvents { user_addr }, HyperliquidWsInboundMessage::User(_)) => {
                Some(user_addr.as_str()) == manager_user
            }
            (Self::OrderUpdates { user_addr }, HyperliquidWsInboundMessage::OrderUpdates(_)) => {
                Some(user_addr.as_str()) == manager_user
            }
            (
                Self::NonFundingLedger { user_addr },
                HyperliquidWsInboundMessage::NonFundingLedger(_),
            ) => Some(user_addr.as_str()) == manager_user,
            (expected, HyperliquidWsInboundMessage::SubscriptionResponse(resp)) => {
                subscription_from_response(resp).as_ref() == Some(expected)
            }
            _ => false,
        }
    }
}

fn subscription_from_response(
    resp: &HyperliquidWsSubscriptionResponse,
) -> Option<HyperliquidSubscription> {
    match resp.subscription.get("type")?.as_str()? {
        "l2Book" => Some(HyperliquidSubscription::L2Book {
            coin: resp.subscription.get("coin")?.as_str()?.to_string(),
        }),
        "trades" => Some(HyperliquidSubscription::Trades {
            coin: resp.subscription.get("coin")?.as_str()?.to_string(),
        }),
        "activeAssetCtx" => Some(HyperliquidSubscription::ActiveAssetCtx {
            coin: resp.subscription.get("coin")?.as_str()?.to_string(),
        }),
        "user" => Some(HyperliquidSubscription::UserEvents {
            user_addr: resp.subscription.get("user")?.as_str()?.to_string(),
        }),
        "orderUpdates" => Some(HyperliquidSubscription::OrderUpdates {
            user_addr: resp.subscription.get("user")?.as_str()?.to_string(),
        }),
        "userNonFundingLedgerUpdates" => Some(HyperliquidSubscription::NonFundingLedger {
            user_addr: resp.subscription.get("user")?.as_str()?.to_string(),
        }),
        _ => None,
    }
}

#[derive(Clone)]
pub(crate) struct WsSendRateLimiter {
    request_tx: mpsc::UnboundedSender<oneshot::Sender<()>>,
}

impl WsSendRateLimiter {
    pub(crate) fn new() -> Self {
        let (request_tx, mut request_rx) = mpsc::unbounded_channel::<oneshot::Sender<()>>();

        tokio::spawn(async move {
            while let Some(reply_tx) = request_rx.recv().await {
                let _ = reply_tx.send(());
                time::sleep(SEND_SPACING).await;
            }
        });

        Self { request_tx }
    }

    async fn acquire(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(reply_tx)
            .map_err(|_| "WS send rate limiter is not running".to_string())?;
        reply_rx
            .await
            .map_err(|_| "WS send rate limiter dropped permit".to_string())
    }
}

#[derive(Clone)]
pub(crate) struct HyperliquidWsManager {
    cmd_tx: mpsc::UnboundedSender<ManagerCommand>,
}

struct ManagedSubscription {
    ref_count: usize,
    sender: broadcast::Sender<Result<HyperliquidWsInboundMessage, String>>,
    window_messages: u64,
    total_messages: u64,
}

enum ManagerCommand {
    Acquire {
        subscription: HyperliquidSubscription,
        response_tx: oneshot::Sender<broadcast::Receiver<Result<HyperliquidWsInboundMessage, String>>>,
    },
    Release {
        subscription: HyperliquidSubscription,
    },
}

impl HyperliquidWsManager {
    pub(crate) fn new(user_addr: Option<String>, send_limiter: WsSendRateLimiter) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        tokio::spawn(run_manager(cmd_rx, user_addr, send_limiter));
        Self { cmd_tx }
    }

    pub(crate) async fn subscribe(
        &self,
        subscription: HyperliquidSubscription,
    ) -> Result<broadcast::Receiver<Result<HyperliquidWsInboundMessage, String>>, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.cmd_tx
            .send(ManagerCommand::Acquire {
                subscription,
                response_tx,
            })
            .map_err(|_| "websocket manager is not running".to_string())?;
        response_rx
            .await
            .map_err(|_| "websocket manager dropped subscribe request".to_string())
    }

    pub(crate) fn unsubscribe(&self, subscription: HyperliquidSubscription) {
        let _ = self.cmd_tx.send(ManagerCommand::Release { subscription });
    }

    /// Unsubscribe all subscriptions that match the given coin/user symbol.
    /// Used by bridges during shutdown to cleanly drain subscriptions before exiting.
    pub(crate) fn unsubscribe_by_coin(&self, coin: &str) {
        // We can't inspect the manager's subscription map from here, so we
        // send a dedicated command. But for now, the bridge knows its own
        // subscriptions and calls `unsubscribe` per-subscription. This method
        // is a convenience for the case where we know the coin but not the
        // exact subscription type (e.g. L2Book vs Trades vs ActiveAssetCtx).
        // We send Release for all known subscription variants for this coin.
        let variants = vec![
            HyperliquidSubscription::L2Book { coin: coin.to_string() },
            HyperliquidSubscription::Trades { coin: coin.to_string() },
            HyperliquidSubscription::ActiveAssetCtx { coin: coin.to_string() },
        ];
        for sub in variants {
            let _ = self.cmd_tx.send(ManagerCommand::Release { subscription: sub });
        }
    }

    /// Unsubscribe all user-related subscriptions for a given user address.
    pub(crate) fn unsubscribe_user(&self, user_addr: &str) {
        let variants = vec![
            HyperliquidSubscription::UserEvents { user_addr: user_addr.to_string() },
            HyperliquidSubscription::OrderUpdates { user_addr: user_addr.to_string() },
            HyperliquidSubscription::NonFundingLedger { user_addr: user_addr.to_string() },
        ];
        for sub in variants {
            let _ = self.cmd_tx.send(ManagerCommand::Release { subscription: sub });
        }
    }
}

async fn run_manager(
    mut cmd_rx: mpsc::UnboundedReceiver<ManagerCommand>,
    user_addr: Option<String>,
    send_limiter: WsSendRateLimiter,
) {
    let mut ws = HyperliquidWs::new();
    let mut subscriptions: HashMap<HyperliquidSubscription, ManagedSubscription> = HashMap::new();
    // Coins recently unsubscribed — messages for these during grace period are silently dropped.
    let mut unsubscribed_coins: HashMap<String, Instant> = HashMap::new();
    let mut backoff_secs = 1_u64;

    loop {
        // Clean up expired unsubscribe entries on each outer loop iteration.
        unsubscribed_coins.retain(|_, since| since.elapsed() < UNSUBSCRIBE_GRACE_PERIOD);

        while subscriptions.is_empty() {
            let Some(cmd) = cmd_rx.recv().await else {
                return;
            };
            handle_command(
                cmd,
                &mut subscriptions,
                &mut unsubscribed_coins,
                &mut ws,
                &send_limiter,
            )
            .await;
        }

        if let Err(err) = ws.connect().await {
            warn!(error = ?err, "WS connect failed, backing off");
            time::sleep(Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(BACKOFF_MAX_SECS);
            continue;
        }


        if let Err(err) = replay_subscriptions(&mut ws, &subscriptions, &send_limiter).await {
            warn!(error = %err, "WS replay failed, reconnecting");
            let _ = ws.close().await;
            time::sleep(Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(BACKOFF_MAX_SECS);
            continue;
        }

        backoff_secs = 1;
        let mut heartbeat = time::interval(HEARTBEAT_INTERVAL);
        heartbeat.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
        let mut idle_watchdog = time::interval(IDLE_WATCHDOG_INTERVAL);
        idle_watchdog.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let mut last_inbound_activity = Instant::now();

        loop {
            tokio::select! {
                maybe_cmd = cmd_rx.recv() => {
                    let Some(cmd) = maybe_cmd else {
                        let _ = ws.close().await;
                        return;
                    };
                    handle_command(
                        cmd,
                        &mut subscriptions,
                        &mut unsubscribed_coins,
                        &mut ws,
                        &send_limiter,
                    )
                    .await;
                    if subscriptions.is_empty() {
                        let _ = ws.close().await;
                        break;
                    }
                }
                _ = heartbeat.tick() => {
                    if let Err(err) = send_with_limit(&mut ws, HyperliquidWsOutboundMessage::Ping, &send_limiter).await {
                        warn!(error = %err, "WS heartbeat failed, reconnecting");
                        fanout_error(&subscriptions, "websocket heartbeat failed".to_string());
                        let _ = ws.close().await;
                        break;
                    }
                }
                _ = idle_watchdog.tick() => {
                    let idle_for = last_inbound_activity.elapsed();
                    if idle_for >= MAX_IDLE_BEFORE_RECONNECT {
                        warn!(
                            idle_for_ms = idle_for.as_millis(),
                            "WS idle watchdog triggered, reconnecting"
                        );
                        fanout_error(&subscriptions, "websocket idle watchdog triggered reconnect".to_string());
                        let _ = ws.close().await;
                        break;
                    }
                }
                inbound = ws.recv() => {
                    match inbound {
                        Some(Ok(HyperliquidWsInboundMessage::Pong)) => {
                            last_inbound_activity = Instant::now();
                        }
                        Some(Ok(msg)) => {
                            last_inbound_activity = Instant::now();
                            dispatch_message(&mut subscriptions, &unsubscribed_coins, &msg, user_addr.as_deref())
                        }
                        Some(Err(err)) => {
                            warn!(error = %err, "WS recv error, reconnecting");
                            fanout_error(&subscriptions, err.to_string());
                            let _ = ws.close().await;
                            break;
                        }
                        None => {
                            warn!("WS stream ended, reconnecting");
                            fanout_error(&subscriptions, "websocket stream ended".to_string());
                            let _ = ws.close().await;
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn handle_command(
    cmd: ManagerCommand,
    subscriptions: &mut HashMap<HyperliquidSubscription, ManagedSubscription>,
    unsubscribed_coins: &mut HashMap<String, Instant>,
    ws: &mut HyperliquidWs,
    send_limiter: &WsSendRateLimiter,
) {
    match cmd {
        ManagerCommand::Acquire {
            subscription,
            response_tx,
        } => {
            if let Some(managed) = subscriptions.get_mut(&subscription) {
                managed.ref_count += 1;
                let _ = response_tx.send(managed.sender.subscribe());
                return;
            }

            let (sender, receiver) = broadcast::channel(FANOUT_CAPACITY);
            subscriptions.insert(
                subscription.clone(),
                ManagedSubscription {
                    ref_count: 1,
                    sender,
                    window_messages: 0,
                    total_messages: 0,
                },
            );
            let _ = response_tx.send(receiver);

            if ws.is_connected() {
                if let Err(err) =
                    send_with_limit(ws, subscription.subscribe_message(), send_limiter).await
                {
                    if let Some(managed) = subscriptions.get(&subscription) {
                        let _ = managed.sender.send(Err(format!(
                            "failed to subscribe {:?}: {err}",
                            subscription
                        )));
                    }
                }
            }
        }
        ManagerCommand::Release { subscription } => {
            let remove = match subscriptions.get_mut(&subscription) {
                Some(managed) if managed.ref_count > 1 => {
                    managed.ref_count -= 1;
                    false
                }
                Some(_) => true,
                None => false,
            };

            if remove {
                // Track the coin(s) this subscription was for so in-flight messages
                // during shutdown don't trigger spurious warnings.
                if let Some(coin) = subscription.unsubscribe_symbol() {
                    unsubscribed_coins.insert(coin, Instant::now());
                }

                subscriptions.remove(&subscription);
                if ws.is_connected() {
                    if let Err(err) =
                        send_with_limit(ws, subscription.unsubscribe_message(), send_limiter).await
                    {
                        warn!(error = %err, subscription = ?subscription, "WS unsubscribe failed");
                    }
                }
            }
        }
    }
}

async fn replay_subscriptions(
    ws: &mut HyperliquidWs,
    subscriptions: &HashMap<HyperliquidSubscription, ManagedSubscription>,
    send_limiter: &WsSendRateLimiter,
) -> Result<(), String> {
    for subscription in subscriptions.keys() {
        send_with_limit(ws, subscription.subscribe_message(), send_limiter).await?;
    }
    Ok(())
}

async fn send_with_limit(
    ws: &mut HyperliquidWs,
    msg: HyperliquidWsOutboundMessage,
    send_limiter: &WsSendRateLimiter,
) -> Result<(), String> {
    send_limiter.acquire().await?;
    ws.send(msg).await.map_err(|e| e.to_string())
}

fn dispatch_message(
    subscriptions: &mut HashMap<HyperliquidSubscription, ManagedSubscription>,
    unsubscribed_coins: &HashMap<String, Instant>,
    msg: &HyperliquidWsInboundMessage,
    manager_user: Option<&str>,
) {
    match msg {
        HyperliquidWsInboundMessage::SubscriptionResponse(resp) => {
            if let Some(subscription) = subscription_from_response(resp) {
                if let Some(managed) = subscriptions.get(&subscription) {
                    if !resp.success.unwrap_or(true) {
                        let _ = managed.sender.send(Err(format!(
                            "subscription rejected: {:?}",
                            subscription
                        )));
                    }
                }
            }
        }
        HyperliquidWsInboundMessage::Unknown { channel, .. } => {
            warn!(channel = %channel, "WS message ignored as unknown");
        }
        _ => {
            let mut matched = 0usize;
            for (subscription, managed) in subscriptions.iter_mut() {
                if subscription.matches_message(msg, manager_user) {
                    matched += 1;
                    managed.window_messages += 1;
                    managed.total_messages += 1;
                    let _ = managed.sender.send(Ok(msg.clone()));
                }
            }
            if matched == 0 {
                // Check if this message belongs to a recently unsubscribed coin.
                // In-flight messages during the grace period are silently dropped.
                let sym = message_symbol(msg);
                let recently_unsubscribed = sym
                    .as_ref()
                    .map(|s| {
                        unsubscribed_coins
                            .get(s)
                            .is_some_and(|since| since.elapsed() < UNSUBSCRIBE_GRACE_PERIOD)
                    })
                    .unwrap_or(false);
                if recently_unsubscribed {
                    tracing::debug!(
                        message = %message_label(msg),
                        "WS message dropped for recently unsubscribed symbol"
                    );
                } else {
                    let is_draining = subscriptions.is_empty();
                    if is_draining {
                        tracing::debug!(message = %message_label(msg), "WS message did not match any active subscription (draining)");
                    } else {
                        warn!(message = %message_label(msg), "WS message did not match any active subscription");
                    }
                }
            }
        }
    }
}

/// Extract the coin/user symbol from a message, for unsubscribe-grace lookup.
fn message_symbol(msg: &HyperliquidWsInboundMessage) -> Option<String> {
    match msg {
        HyperliquidWsInboundMessage::L2Book(book) => Some(book.coin.clone()),
        HyperliquidWsInboundMessage::ActiveAssetCtx(ctx) => Some(ctx.coin.clone()),
        HyperliquidWsInboundMessage::Trades(trades) => {
            trades.first().map(|t| t.coin.clone())
        }
        HyperliquidWsInboundMessage::User(_)
        | HyperliquidWsInboundMessage::OrderUpdates(_)
        | HyperliquidWsInboundMessage::NonFundingLedger(_) => {
            // User messages don't carry a coin — they're identified by user address.
            // The unsubscribed_coins map stores user addresses too.
            None
        }
        _ => None,
    }
}

pub(crate) fn message_label(msg: &HyperliquidWsInboundMessage) -> String {
    match msg {
        HyperliquidWsInboundMessage::Pong => "pong".to_string(),
        HyperliquidWsInboundMessage::L2Book(book) => format!("l2Book:{}", book.coin),
        HyperliquidWsInboundMessage::ActiveAssetCtx(ctx) => format!("activeAssetCtx:{}", ctx.coin),
        HyperliquidWsInboundMessage::Trades(trades) => format!(
            "trades:{}",
            trades.first().map(|trade| trade.coin.as_str()).unwrap_or("<empty>")
        ),
        HyperliquidWsInboundMessage::User(_) => "user".to_string(),
        HyperliquidWsInboundMessage::OrderUpdates(_) => "orderUpdates".to_string(),
        HyperliquidWsInboundMessage::NonFundingLedger(_) => "userNonFundingLedgerUpdates".to_string(),
        HyperliquidWsInboundMessage::SubscriptionResponse(resp) => format!(
            "subscriptionResponse:{}",
            subscription_from_response(resp)
                .map(|sub| sub.label())
                .unwrap_or_else(|| "<unknown>".to_string())
        ),
        HyperliquidWsInboundMessage::Unknown { channel, .. } => format!("unknown:{channel}"),
    }
}

fn fanout_error(
    subscriptions: &HashMap<HyperliquidSubscription, ManagedSubscription>,
    error: String,
) {
    for managed in subscriptions.values() {
        let _ = managed.sender.send(Err(error.clone()));
    }
}

pub(crate) fn managed_stream<T, F>(
    manager: HyperliquidWsManager,
    subscription: HyperliquidSubscription,
    parse: F,
) -> impl futures_core::Stream<Item = Result<T, String>> + Send + 'static
where
    T: Send + 'static,
    F: Fn(HyperliquidWsInboundMessage) -> Vec<Result<T, String>> + Send + Sync + 'static,
{
    async_stream::stream! {
        let mut receiver = match manager.subscribe(subscription.clone()).await {
            Ok(receiver) => receiver,
            Err(err) => {
                yield Err(err);
                return;
            }
        };

        struct ReleaseOnDrop {
            manager: HyperliquidWsManager,
            subscription: HyperliquidSubscription,
        }

        impl Drop for ReleaseOnDrop {
            fn drop(&mut self) {
                self.manager.unsubscribe(self.subscription.clone());
            }
        }

        let _release_on_drop = ReleaseOnDrop { manager, subscription };

        loop {
            match receiver.recv().await {
                Ok(Ok(message)) => {
                    for item in parse(message) {
                        yield item;
                    }
                }
                Ok(Err(err)) => yield Err(err),
                Err(broadcast::error::RecvError::Closed) => {
                    warn!(subscription = %_release_on_drop.subscription.label(), "managed stream receiver closed");
                    yield Err("websocket subscription closed".to_string());
                    return;
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(
                        subscription = %_release_on_drop.subscription.label(),
                        skipped = skipped,
                        "managed stream receiver lagged"
                    );
                    yield Err(format!("websocket subscription lagged by {skipped} messages"));
                }
            }
        }
    }
}
