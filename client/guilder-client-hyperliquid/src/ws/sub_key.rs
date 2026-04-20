/// Subscription key for routing WebSocket messages.
///
/// Combines channel name with a routing key (e.g. symbol or user address)
/// to uniquely identify a subscription within `WsMux`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SubKey {
    pub(crate) channel: String,
    pub(crate) routing_key: String,
}
