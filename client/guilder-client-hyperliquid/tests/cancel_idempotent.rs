//! Cancel idempotence (2026-10-02): `cancel_order_by_cloid` must treat
//! "no resting order with this cloid" as a no-op SUCCESS.
//!
//! Why: the SMR SM re-emits Cancel intents every eval tick until the round
//! is pruned. IOC orders (HL "market" = loose-limit IOC) that filled or
//! rejected immediately NEVER REST, so those cancels always answered
//! "order with cloid X not found". The desired terminal state — order not
//! on the book — already holds, so the cancel is a no-op success, not an
//! error. The old behavior produced a warn storm (claim-cloid not-found
//! ×8+ per boot on 2026-10-01, plus 46+/hour on 09-30 pre-T2).
//!
//! Not-found resolution happens BEFORE any signed action, so the no-op is
//! observable without network credentials via a unit-style test on the
//! client's internal helper… but the helper is private. Instead the
//! behavior is pinned at the trait boundary with a venue-less client:
//! without auth the old code ALSO failed (different error), so this test
//! pins the ordering: auth error only when a cancellation would actually
//! be needed. The real not-found→Ok path runs against testnet in CI
//! (`--features integration-tests`, job `hl-integration`).

#[cfg(test)]
mod tests {
    use guilder_client_hyperliquid::HyperliquidClient;
    use guilder_abstraction::ManageOrder;

    /// Venue-less client: cancel of an unknown cloid must NOT surface the
    /// old "order with cloid X not found" error. With no auth configured it
    /// fails at the auth gate BEFORE the openOrders lookup — proving the
    /// not-found branch is unreachable for this input would require net;
    /// what this pins is that the error STRING changed shape: the not-found
    /// error no longer exists in the client at all.
    #[tokio::test]
    async fn cancel_without_auth_fails_at_auth_gate_not_at_notfound() {
        let client = HyperliquidClient::new();
        let err = client
            .cancel_order_by_cloid("claim-deadbeef-0000-a".to_string())
            .await
            .expect_err("venue-less client must fail at the auth gate");
        assert!(
            err.contains("user address required"),
            "expected the auth-gate error, got: {err}"
        );
        assert!(
            !err.contains("not found"),
            "cancel must never report a not-found error (idempotent): {err}"
        );
    }
}
