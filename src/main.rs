//! Shielded Shopping Agent. `ssa` runs the bot; `ssa invite <CODE> <USES>` adds an invite code.

mod audit;
mod autopay;
mod bitrefill;
mod caps;
mod config;
mod db;
mod intent;
mod model;
mod order_fsm;
mod orders;
mod price;
mod quote;
mod refill;
mod telegram;
mod workers;
mod zec;
mod zip321;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;

const RESOLVE_EVERY: Duration = Duration::from_secs(30);

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.into())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,teloxide=warn,reqwest=warn".into()),
        )
        .init();

    let cfg = config::Config::from_env()?;
    let db_path = PathBuf::from(env_or("SSA_DB", "data/ssa.sqlite"));
    let db =
        Arc::new(db::Db::open(&db_path).with_context(|| format!("open {}", db_path.display()))?);
    db.migrate()?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("refill") {
        return refill_cmd(&args[1..]).await;
    }
    if let [cmd, birthday] = args.as_slice() {
        anyhow::ensure!(
            cmd == "audit",
            "usage: ssa audit <BIRTHDAY_HEIGHT> (key in SSA_AUDIT_UFVK)"
        );
        let ufvk =
            std::env::var("SSA_AUDIT_UFVK").context("set SSA_AUDIT_UFVK to the viewing key")?;
        let lines = audit::audit(
            &cfg.devtool,
            &ufvk,
            birthday.parse().context("birthday height")?,
        )
        .await?;
        print!("{}", audit::to_csv(&lines));
        return Ok(());
    }
    if let [cmd, code, uses] = args.as_slice() {
        anyhow::ensure!(cmd == "invite", "usage: ssa [invite <CODE> <USES>]");
        db.add_invite(code, uses.parse().context("USES must be a number")?)?;
        println!("invite {code} has {uses} uses");
        return Ok(());
    }
    anyhow::ensure!(args.is_empty(), "usage: ssa [invite <CODE> <USES>]");

    let zec = Arc::new(zec::Zec::new(
        cfg.devtool.clone(),
        env_or("SSA_WALLET_DIR", "tools/wallets/ssa").into(),
        env_or("SSA_IDENTITY", "tools/wallets/identity.age").into(),
        env_or("SSA_TOR", "0") == "1",
    ));
    let bitrefill = bitrefill::Bitrefill::new(
        bitrefill::DEFAULT_BASE,
        cfg.bitrefill_api_key.clone(),
        cfg.bitrefill_dry_run,
    )?;
    let stats_addr = cfg.stats_addr;
    tracing::info!(dry_run = cfg.bitrefill_dry_run, "starting ssa");
    let app = Arc::new(orders::App {
        cfg,
        env_path: PathBuf::from(".env"),
        db,
        zec,
        bitrefill,
        prices: price::PriceCache::new()?,
    });

    let alerts = orders::recover(&app, chrono::Utc::now().timestamp()).await?;
    for o in &alerts {
        tracing::warn!(order_id = %o.id, "order needs operator review after restart");
    }

    tokio::spawn(workers::sync_loop(app.clone()));
    tokio::spawn(workers::receipt_loop(app.clone()));
    if let Some(addr) = stats_addr {
        tokio::spawn(workers::stats_server(app.clone(), addr));
    }
    let resolver = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(RESOLVE_EVERY).await;
            match orders::resolve_uncertain(&resolver).await {
                Ok(changed) => {
                    for o in changed {
                        tracing::info!(order_id = %o.id, status = %o.status, "uncertain order resolved");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "resolve_uncertain failed"),
            }
        }
    });

    telegram::run(app).await
}

/// `ssa refill <USD> <usdc-base|btc> <BITREFILL_DEPOSIT_ADDR> <ZEC_REFUND_ADDR>` prints a live
/// 1Click deposit address + exact ZEC amount; `ssa refill status <DEPOSIT_ADDR>` tracks it.
async fn refill_cmd(args: &[String]) -> anyhow::Result<()> {
    const USAGE: &str = "usage: ssa refill <USD> <usdc-base|btc> <BITREFILL_DEPOSIT_ADDR> <ZEC_REFUND_ADDR> [--dry]\n       ssa refill status <DEPOSIT_ADDR>";
    if let [cmd, deposit] = args {
        anyhow::ensure!(cmd == "status", USAGE);
        println!("{:?}", refill::status(deposit).await?);
        return Ok(());
    }
    let (usd, dest, recipient, refund, dry) = match args {
        [u, d, r, f] => (u, d, r, f, false),
        [u, d, r, f, flag] if flag == "--dry" => (u, d, r, f, true),
        _ => anyhow::bail!(USAGE),
    };
    let usd_cents =
        quote::parse_usd_cents(usd).map_err(|e| anyhow::anyhow!("bad USD amount: {e:?}"))?;
    let dest = refill::Dest::parse(dest).context(USAGE)?;
    let prices = price::PriceCache::new()?
        .prices(chrono::Utc::now().timestamp())
        .await?;
    let q = refill::quote(
        dest,
        usd_cents,
        prices.btc_usd_cents,
        recipient,
        refund,
        dry,
    )
    .await?;
    println!(
        "Swap {} ZEC -> {} to Bitrefill, about {} min.",
        q.amount_in_formatted,
        q.amount_out_formatted,
        q.time_estimate_secs / 60 + 1
    );
    match (&q.deposit_address, refill::payment_uri(&q)) {
        (Some(addr), Some(uri)) => {
            println!("Send exactly {} ZEC to: {addr}", q.amount_in_formatted);
            println!("Or scan/paste this payment URI: {uri}");
            println!("Then track it: ssa refill status {addr}");
        }
        _ => println!("(dry run: no deposit address reserved)"),
    }
    Ok(())
}
