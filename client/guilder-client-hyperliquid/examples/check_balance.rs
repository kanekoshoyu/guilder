use guilder_client_hyperliquid::HyperliquidClient;
use guilder_abstraction::GetAccountSnapshot;

#[tokio::main]
async fn main() {
    let addr = std::env::var("HYPERLIQUID_WALLET_ADDRESS").expect("missing addr");
    let key = std::env::var("HYPERLIQUID_WALLET_KEY").expect("missing key");
    let client = HyperliquidClient::with_auth(addr, key);

    match client.get_balance().await {
        Ok(balances) => {
            println!("{} balances returned:", balances.len());
            for b in &balances {
                println!("\n--- {} ---", b.token);
                println!("  equity:     {}", b.equity);
                println!("  free:       {}", b.free);
                println!("  hold:       {}", b.hold);
                println!("  safe:       {:?}", b.safe);
                println!("  usable:     {}", b.usable);
                println!("  maintenance:{:?}", b.maintenance);
                println!("  margin_used:{:?}", b.margin_used);
            }
        }
        Err(e) => eprintln!("ERROR: {e}"),
    }
}
