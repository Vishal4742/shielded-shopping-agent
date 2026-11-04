//! Telegram front end (EI-1, FR-1..FR-3, FR-7, FR-8): long polling, private chats only,
//! owner-checked buttons. Handlers only translate between chat and `orders`/`db`; all limits
//! and money rules live in those modules. The LLM output is an `Intent`, never a purchase.
//! Message wording follows design §9.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use teloxide::prelude::*;
use teloxide::types::{
    BotCommand, CallbackQuery, ChatId, InlineKeyboardButton, InlineKeyboardMarkup, InputFile,
    Message, MessageId,
};

use crate::audit;
use crate::autopay;
use crate::caps::{self, Decision};
use crate::db::{Order, Rule};
use crate::intent::{self, Intent};
use crate::model::Operator;
use crate::orders::{self, App, BuyOutcome, QuoteOutcome};
use crate::zip321;

const MODEL: &str = "claude-haiku-5-5";
const ZAT_PER_ZEC: u64 = 100_000_000;
/// Longest message text sent to the intent extractor.
const MAX_INTENT_CHARS: usize = 300;
const IST_OFFSET_SECS: i64 = 5 * 3600 + 1800;
const QR_MIN_PX: u32 = 320;

const MSG_NOT_ALLOWED: &str = "This bot is invite-only. Ask the team for an invite code.";
const MSG_GROUP: &str = "Please DM me. I only work in private chats.";
const MSG_CREATING_WALLET: &str = "Creating your private wallet…";
const MSG_NO_WALLET: &str = "You don't have a wallet yet. Send /start to create one.";
const MSG_ERROR: &str = "Something went wrong, nothing was bought.";
const MSG_BUYING: &str = "Buying… usually under 15 s.";
const MSG_CANCELLED: &str = "❌ Cancelled. Nothing was bought.";
const MSG_NOT_OPEN: &str = "That order is no longer open (already used or cancelled).";
const MSG_NOT_YOURS: &str = "Not your order.";
const MSG_EXPIRED: &str = "That price expired. Send the request again for a fresh quote.";
const MSG_PAUSED: &str = "Purchases are paused by the operator right now.";
const MSG_UNCERTAIN: &str =
    "I'm checking this order with Bitrefill. Nothing to do on your side; I'll message you.";
const MSG_DRY_RUN: &str = "Dry run: nothing was bought and your budget is restored.";
const MSG_AUTO_HINT: &str = "Set a recurring top-up like this:\n/auto jio 9876543210 10 weekly\nSchedules: daily, weekly, monthly, or every 2m / every 3 hours.";
const MSG_REPORT_WORKING: &str = "Building your report from your viewing key alone (a fresh watch-only wallet). This takes about 30 s…";
const MSG_ERROR_REPORT: &str =
    "I couldn't build the report right now. Nothing was changed; try again in a minute.";
const MSG_HINT: &str = "I didn't catch that. Try: jio 9876543210 to pick a recharge,\nor jio 9876543210 10 (operator, phone number, rupees). /help for more.";
const MSG_SLOW_DOWN: &str = "That's a lot of messages. I'll be back for you in a minute.";
const MSG_REPORT_COOLDOWN: &str = "You can build one report every 2 minutes. Try again shortly.";
const MSG_REPORT_BUSY: &str = "Another report is being built right now. Try again in a minute.";
/// ponytail: one fixed site; make it config if the site moves.
const WEBSITE: &str = "https://zecshop.vercel.app";
const MSG_WEB: &str =
    "Your recharges on the website. This link logs you in for 1 hour; don't share it.";
const MSG_MENU: &str = "What would you like to do?";
const MSG_RECHARGE: &str = "Send the operator and the phone number, e.g. jio 9876543210, and I'll show the recharges you can pick.\nOr add the amount: jio 9876543210 10";
const MSG_NO_RECEIPTS: &str =
    "No recharges yet. Send jio and your number to pick one, e.g. jio 9876543210.";
const MSG_OPTIONS_UNAVAILABLE: &str = "I couldn't load the recharge list right now. Try again in a minute, or send the amount too, e.g. jio 9876543210 10.";
/// Telegram shows inline keyboards well up to about this many rows.
const MAX_OPTIONS: usize = 8;
/// Receipts per /receipts message; ~400 characters each keeps it under Telegram's 4,096.
const MAX_RECEIPTS: usize = 8;
const RECEIPT_DONE_EVERY: std::time::Duration = std::time::Duration::from_secs(15);
/// Orders that bought something real, newest receipt state last.
const BOUGHT_STATUSES: [&str; 3] = ["DELIVERED", "RECEIPT_SENDING", "RECEIPTED"];
const MSG_VIEWKEY_HELP: &str = "Read-only: it shows your deposits and receipts, nothing can be spent with it. Import it as a watch-only account in Zingo or zcash-devtool.";

/// Chats already told to DM us, so a busy group is answered once.
static NOTICED_GROUPS: Mutex<BTreeSet<i64>> = Mutex::new(BTreeSet::new());
/// Wrong invite codes per Telegram id; after 5 the id gets no more guesses (threat model T15).
/// ponytail: in memory, so a restart clears it and a guesser can rotate accounts; move it to
/// SQLite with a global counter if the bot stays public after the event.
static BAD_INVITES: Mutex<BTreeMap<i64, u32>> = Mutex::new(BTreeMap::new());
const MAX_BAD_INVITE_TRIES: u32 = 5;

fn bad_invite_tries(tg_id: i64) -> u32 {
    BAD_INVITES
        .lock()
        .map(|m| m.get(&tg_id).copied().unwrap_or(0))
        .unwrap_or(MAX_BAD_INVITE_TRIES)
}

fn note_bad_invite(tg_id: i64) {
    if let Ok(mut m) = BAD_INVITES.lock() {
        *m.entry(tg_id).or_insert(0) += 1;
    }
}

/// Flood guard: at most MAX_UPDATES_PER_MIN messages or taps per Telegram id per minute.
/// ponytail: fixed one-minute window in memory, wiped at 10,000 ids; fine for one process.
static RECENT_UPDATES: Mutex<BTreeMap<i64, (i64, u32)>> = Mutex::new(BTreeMap::new());
const MAX_UPDATES_PER_MIN: u32 = 20;
const MAX_TRACKED_IDS: usize = 10_000;

/// How many updates this id has sent in the current minute, this one included.
fn count_update(tg_id: i64, now: i64) -> u32 {
    let mut seen = RECENT_UPDATES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if seen.len() > MAX_TRACKED_IDS {
        seen.clear();
    }
    let entry = seen.entry(tg_id).or_insert((now, 0));
    if now - entry.0 >= 60 {
        *entry = (now, 0);
    }
    entry.1 += 1;
    entry.1
}

/// One `/report` at a time (each runs four devtool processes) and one per user every 2 min.
static REPORT_RUNNING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static LAST_REPORT: Mutex<BTreeMap<i64, i64>> = Mutex::new(BTreeMap::new());
const REPORT_COOLDOWN_SECS: i64 = 120;

/// Serialises wallet creation so a double `/start` cannot make two accounts for one user.
static WALLET_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Builds the bot from the config token and long-polls until Ctrl-C.
pub async fn run(app: Arc<App>) -> anyhow::Result<()> {
    let bot = Bot::new(app.cfg.telegram_bot_token.clone());
    let http = reqwest::Client::new();
    tokio::spawn(autopay::run_loop(app.clone(), bot.clone()));
    tokio::spawn(receipt_done_loop(app.clone(), bot.clone()));
    register_bot_profile(&bot).await;
    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(on_message))
        .branch(Update::filter_callback_query().endpoint(on_callback));
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![app, http])
        .default_handler(|_| async {})
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}

// ---------------------------------------------------------------- wiring

async fn on_message(
    bot: Bot,
    msg: Message,
    app: Arc<App>,
    http: reqwest::Client,
) -> ResponseResult<()> {
    if let Err(e) = handle_message(&bot, &msg, &app, &http).await {
        log_error(&app, "message handler failed", &e);
        let _ = bot.send_message(msg.chat.id, MSG_ERROR).await;
    }
    Ok(())
}

async fn on_callback(bot: Bot, q: CallbackQuery, app: Arc<App>) -> ResponseResult<()> {
    if let Err(e) = handle_callback(&bot, &q, &app).await {
        log_error(&app, "callback handler failed", &e);
        if let Some(m) = &q.message {
            let _ = bot.send_message(m.chat().id, MSG_ERROR).await;
        }
    }
    Ok(())
}

/// Logs an error with the bot token stripped (reqwest errors can carry the request URL).
fn log_error(app: &App, what: &str, err: &anyhow::Error) {
    let text = redact(&format!("{err:#}"), &app.cfg.telegram_bot_token);
    tracing::error!(error = %text, "{what}");
}

fn tg_id_of(user: &teloxide::types::User) -> i64 {
    user.id.0 as i64
}

// ---------------------------------------------------------------- messages

async fn handle_message(
    bot: &Bot,
    msg: &Message,
    app: &App,
    http: &reqwest::Client,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    let chat = msg.chat.id;
    if !msg.chat.is_private() {
        return notify_group(bot, chat).await;
    }
    let (Some(from), Some(text)) = (msg.from.as_ref(), msg.text()) else {
        return Ok(());
    };
    let tg_id = tg_id_of(from);
    let count = count_update(tg_id, now);
    if count > MAX_UPDATES_PER_MIN {
        // Say it once per minute, then stay silent so a flood costs us nothing.
        if count == MAX_UPDATES_PER_MIN + 1 {
            send(bot, chat, MSG_SLOW_DOWN.to_owned()).await?;
        }
        return Ok(());
    }
    match parse_command(text) {
        Some(("start", arg)) => cmd_start(bot, app, chat, from, arg, now).await,
        Some(("help", _)) => send_with_menu(bot, chat, help_text()).await,
        Some(("menu", _)) => send_with_menu(bot, chat, MSG_MENU.to_owned()).await,
        Some(("recharge", _)) => send(bot, chat, MSG_RECHARGE.to_owned()).await,
        Some(("balance", _)) => cmd_balance(bot, app, chat, tg_id).await,
        Some(("viewkey", _)) => cmd_viewkey(bot, app, chat, tg_id).await,
        Some(("auto", _)) => cmd_auto(bot, app, chat, tg_id, command_args(text), now).await,
        Some(("rules", _)) => cmd_rules(bot, app, chat, tg_id).await,
        Some(("report", _)) => cmd_report(bot, app, chat, tg_id).await,
        Some(("receipts", _)) => cmd_receipts(bot, app, chat, tg_id).await,
        Some(("web", _)) => cmd_web(bot, app, chat, from).await,
        Some(("stop", arg)) => cmd_stop(bot, app, chat, tg_id, arg).await,
        Some(_) => send(bot, chat, MSG_HINT.to_owned()).await,
        None => handle_text(bot, app, http, chat, tg_id, text, now).await,
    }
}

async fn send(bot: &Bot, chat: ChatId, text: String) -> anyhow::Result<()> {
    bot.send_message(chat, text).await?;
    Ok(())
}

async fn notify_group(bot: &Bot, chat: ChatId) -> anyhow::Result<()> {
    let first_time = {
        let mut groups = NOTICED_GROUPS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if groups.len() > MAX_TRACKED_IDS {
            groups.clear();
        }
        groups.insert(chat.0)
    };
    if first_time {
        send(bot, chat, MSG_GROUP.to_owned()).await?;
    }
    Ok(())
}

async fn cmd_start(
    bot: &Bot,
    app: &App,
    chat: ChatId,
    from: &teloxide::types::User,
    invite: &str,
    now: i64,
) -> anyhow::Result<()> {
    let tg_id = tg_id_of(from);
    if !app.db.is_user(tg_id)? {
        let locked_out = bad_invite_tries(tg_id) >= MAX_BAD_INVITE_TRIES;
        let admitted = !locked_out
            && !invite.is_empty()
            && app.db.admit(invite, tg_id, from.username.as_deref(), now)?;
        if !admitted {
            if !locked_out && !invite.is_empty() {
                note_bad_invite(tg_id);
            }
            return send(bot, chat, MSG_NOT_ALLOWED.to_owned()).await;
        }
    }
    let ua = ensure_wallet(bot, app, chat, tg_id).await?;
    send_deposit_address(bot, chat, &ua).await
}

/// Returns the user's deposit address, creating the wallet account first if needed.
async fn ensure_wallet(bot: &Bot, app: &App, chat: ChatId, tg_id: i64) -> anyhow::Result<String> {
    let _creating = WALLET_LOCK.lock().await;
    if let Some(ua) = app.db.get_user(tg_id)?.and_then(|u| u.deposit_ua) {
        return Ok(ua);
    }
    send(bot, chat, MSG_CREATING_WALLET.to_owned()).await?;
    let (account, ua) = app.zec.create_account(&format!("u{tg_id}")).await?;
    app.db
        .set_wallet(tg_id, &account.uuid, &account.ufvk, &ua)?;
    Ok(ua)
}

async fn send_deposit_address(bot: &Bot, chat: ChatId, ua: &str) -> anyhow::Result<()> {
    send(bot, chat, welcome_text(ua)).await?;
    // The address is already delivered; a QR failure must not look like a failed signup.
    if let Err(e) = send_qr(bot, chat, ua).await {
        tracing::warn!(error = %redact(&e.to_string(), bot.token()), "deposit QR not sent");
    }
    send_with_menu(bot, chat, MSG_MENU.to_owned()).await
}

async fn send_qr(bot: &Bot, chat: ChatId, ua: &str) -> anyhow::Result<()> {
    let png = qr_png(&zip321::deposit_uri(ua, None, None)?)?;
    bot.send_photo(chat, InputFile::memory(png).file_name("deposit.png"))
        .await?;
    Ok(())
}

fn qr_png(data: &str) -> anyhow::Result<Vec<u8>> {
    let code = qrcode::QrCode::new(data.as_bytes())?;
    let img = code
        .render::<image::Luma<u8>>()
        .min_dimensions(QR_MIN_PX, QR_MIN_PX)
        .build();
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)?;
    Ok(out)
}

/// The user's `(deposit address, UFVK)`, or a reply explaining what to do and `None`.
async fn wallet_of(
    bot: &Bot,
    app: &App,
    chat: ChatId,
    tg_id: i64,
) -> anyhow::Result<Option<(String, String)>> {
    let Some(user) = app.db.get_user(tg_id)? else {
        send(bot, chat, MSG_NOT_ALLOWED.to_owned()).await?;
        return Ok(None);
    };
    match (user.deposit_ua, user.ufvk) {
        (Some(ua), Some(ufvk)) => Ok(Some((ua, ufvk))),
        _ => {
            send(bot, chat, MSG_NO_WALLET.to_owned()).await?;
            Ok(None)
        }
    }
}

async fn cmd_balance(bot: &Bot, app: &App, chat: ChatId, tg_id: i64) -> anyhow::Result<()> {
    let Some((ua, _)) = wallet_of(bot, app, chat, tg_id).await? else {
        return Ok(());
    };
    let available = app.db.available_zat(tg_id)?;
    send(bot, chat, balance_text(available, &ua)).await
}

async fn cmd_viewkey(bot: &Bot, app: &App, chat: ChatId, tg_id: i64) -> anyhow::Result<()> {
    let Some((_, ufvk)) = wallet_of(bot, app, chat, tg_id).await? else {
        return Ok(());
    };
    // Never logged; protect_content stops forwarding it onward.
    bot.send_message(chat, viewkey_text(&ufvk))
        .protect_content(true)
        .await?;
    Ok(())
}

async fn handle_text(
    bot: &Bot,
    app: &App,
    http: &reqwest::Client,
    chat: ChatId,
    tg_id: i64,
    text: &str,
    now: i64,
) -> anyhow::Result<()> {
    if !app.db.is_user(tg_id)? {
        return send(bot, chat, MSG_NOT_ALLOWED.to_owned()).await;
    }
    let text: String = text.chars().take(MAX_INTENT_CHARS).collect();
    let intent = intent::extract(http, &app.cfg.anthropic_api_key, MODEL, &text).await;
    match intent {
        Intent::TopUp {
            operator,
            phone,
            inr,
        } => send_quote(bot, app, chat, tg_id, operator, &phone, inr, now).await,
        Intent::Choose { operator, phone } => send_options(bot, app, chat, operator, &phone).await,
        Intent::Balance => cmd_balance(bot, app, chat, tg_id).await,
        Intent::Help => send(bot, chat, help_text()).await,
        Intent::Unknown => send(bot, chat, MSG_HINT.to_owned()).await,
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_quote(
    bot: &Bot,
    app: &App,
    chat: ChatId,
    tg_id: i64,
    operator: Operator,
    phone: &str,
    inr: i64,
    now: i64,
) -> anyhow::Result<()> {
    let outcome = orders::create_quote(app, tg_id, operator, phone, inr, now).await?;
    let request = bot.send_message(chat, quote_text(&outcome, inr, now));
    match &outcome {
        QuoteOutcome::Quoted(order) => request.reply_markup(quote_keyboard(&order.id)).await?,
        _ => request.await?,
    };
    Ok(())
}

/// "jio 98…" with no amount: list the catalog packages inside the per-order limits as buttons.
/// A tap (`pick:<op>_<phone>_<inr>`) makes the normal quote, so every check still applies.
async fn send_options(
    bot: &Bot,
    app: &App,
    chat: ChatId,
    operator: Operator,
    phone: &str,
) -> anyhow::Result<()> {
    let product = match app.bitrefill.get_product(orders::slug_for(operator)).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "catalog lookup for options failed");
            return send(bot, chat, MSG_OPTIONS_UNAVAILABLE.to_owned()).await;
        }
    };
    let options = recharge_options(&product);
    let name = operator_label(operator.as_str());
    if options.is_empty() {
        let text = format!(
            "No {name} recharge between ₹{} and ₹{} right now. Try Jio.",
            caps::MIN_INR,
            caps::MAX_INR
        );
        return send(bot, chat, text).await;
    }
    let rows = options.iter().map(|(inr, label)| {
        [InlineKeyboardButton::callback(
            format!("₹{inr} · {label}"),
            format!("pick:{}_{phone}_{inr}", operator.as_str()),
        )]
    });
    bot.send_message(
        chat,
        format!(
            "Pick a {name} recharge for {}. I'll show the price before anything is bought.",
            crate::bitrefill::mask_phone(phone)
        ),
    )
    .reply_markup(InlineKeyboardMarkup::new(rows))
    .await?;
    Ok(())
}

/// Packages inside the per-order limits, one per rupee amount (the cheapest), sorted by amount.
fn recharge_options(product: &crate::bitrefill::Product) -> Vec<(i64, String)> {
    let mut amounts: Vec<i64> = product
        .packages
        .iter()
        .filter_map(|p| p.inr)
        .filter(|inr| (caps::MIN_INR..=caps::MAX_INR).contains(inr))
        .collect();
    amounts.sort_unstable();
    amounts.dedup();
    amounts
        .into_iter()
        .filter_map(|inr| orders::pick_package(product, inr).map(|p| (inr, p.value.clone())))
        .take(MAX_OPTIONS)
        .collect()
}

/// `jio_9876543210_10` → (Jio, phone, 10). Re-validated here: callback data comes from the client.
fn parse_pick(id: &str) -> Option<(Operator, &str, i64)> {
    let mut parts = id.split('_');
    let (op, phone, inr) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let operator = Operator::parse(op)?;
    let phone_ok = phone.len() == 10
        && phone.bytes().all(|b| b.is_ascii_digit())
        && matches!(phone.as_bytes()[0], b'6'..=b'9');
    let inr: i64 = inr.parse().ok()?;
    (phone_ok && (caps::MIN_INR..=caps::MAX_INR).contains(&inr)).then_some((operator, phone, inr))
}

/// One finished recharge with everything we know about it: the Done message and `/receipts`.
fn receipt_text(order: &Order) -> String {
    let mut lines = vec![
        format!(
            "🧾 {} ₹{} recharge · {}",
            operator_label(&order.operator),
            order.inr,
            receipt_status(&order.status)
        ),
        format!("Phone: {}", mask_phone(&order.phone)),
        format!(
            "When: {} · Ref {}",
            ist_datetime(order.created_at),
            short_ref(&order.id)
        ),
        format!(
            "Paid: {} (network fee {})",
            zec(order.total_zat()),
            zec(order.fee_zat)
        ),
    ];
    if let Some(invoice) = &order.bitrefill_invoice {
        lines.push(format!("Bitrefill invoice: {invoice}"));
    }
    match &order.receipt_txid {
        Some(txid) => lines.push(format!("Shielded receipt tx: {txid}")),
        None => lines.push("Shielded receipt: on its way".to_owned()),
    }
    lines.push(format!(
        "Memo (your /viewkey reads it): SSA·{}·{}·₹{}",
        order.id, order.operator, order.inr
    ));
    lines.join("\n")
}

fn receipt_status(status: &str) -> &'static str {
    match status {
        "RECEIPTED" => "receipt on chain ✓",
        "RECEIPT_SENDING" => "receipt being sent",
        _ => "delivered",
    }
}

fn ist_datetime(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts + IST_OFFSET_SECS, 0)
        .map(|t| t.format("%d %b %H:%M IST").to_string())
        .unwrap_or_default()
}

/// `/receipts`: every real recharge of this user, newest first, with its full receipt.
async fn cmd_receipts(bot: &Bot, app: &App, chat: ChatId, tg_id: i64) -> anyhow::Result<()> {
    let mut orders = Vec::new();
    for status in BOUGHT_STATUSES {
        orders.extend(
            app.db
                .orders_in_status(status)?
                .into_iter()
                .filter(|o| o.tg_id == tg_id && !o.dry_run),
        );
    }
    if orders.is_empty() {
        return send(bot, chat, MSG_NO_RECEIPTS.to_owned()).await;
    }
    orders.sort_by_key(|o| std::cmp::Reverse(o.created_at));
    let total_inr: i64 = orders.iter().map(|o| o.inr).sum();
    let shown = orders.len().min(MAX_RECEIPTS);
    let body = orders[..shown]
        .iter()
        .map(receipt_text)
        .collect::<Vec<_>>()
        .join("\n\n");
    let more = if orders.len() > shown {
        format!(" (latest {shown} below)")
    } else {
        String::new()
    };
    let text = format!(
        "Your recharges: {} · ₹{total_inr} in total{more}\n\n{body}\n\nProof from your viewing key alone: /report",
        orders.len()
    );
    send(bot, chat, text).await
}

/// Sends the "Done" message with the full receipt once an order's shielded receipt is mined.
/// ponytail: the watermark lives in memory from boot, so a receipt mined while the bot is down
/// gets no Done message (it still shows in /receipts); add a notified_at column if that matters.
async fn receipt_done_loop(app: Arc<App>, bot: Bot) {
    let mut since = chrono::Utc::now().timestamp();
    loop {
        tokio::time::sleep(RECEIPT_DONE_EVERY).await;
        let orders = match app.db.orders_in_status("RECEIPTED") {
            Ok(orders) => orders,
            Err(e) => {
                tracing::warn!(error = %e, "receipt done query failed");
                continue;
            }
        };
        for order in orders.iter().filter(|o| o.updated_at > since && !o.dry_run) {
            let text = format!("✅ Done\n\n{}", receipt_text(order));
            if let Err(e) = bot.send_message(ChatId(order.tg_id), text).await {
                tracing::warn!(error = %redact(&e.to_string(), bot.token()), "receipt done message failed");
            }
        }
        since = orders.iter().map(|o| o.updated_at).fold(since, i64::max);
    }
}

/// The `/` command list and the profile text shown before /start. Failures only cost polish.
async fn register_bot_profile(bot: &Bot) {
    let commands = [
        ("recharge", "Recharge a phone (pick from a list)"),
        ("balance", "Your budget and deposit address"),
        ("receipts", "Every recharge with its receipt"),
        (
            "auto",
            "Recharge on a schedule, e.g. /auto jio 98… 10 weekly",
        ),
        ("rules", "Your auto-pay rules"),
        ("stop", "Stop an auto-pay rule: /stop 1"),
        ("viewkey", "Your read-only viewing key"),
        ("report", "Purchases proven from your viewing key"),
        ("web", "See your recharges on the website"),
        ("menu", "Show the buttons"),
        ("help", "How it works and the limits"),
    ]
    .map(|(name, about)| BotCommand::new(name, about));
    let results = [
        bot.set_my_commands(commands).await.err(),
        bot.set_my_short_description()
            .short_description("Phone recharges from a private Zcash budget, inside fixed limits.")
            .await
            .err(),
        bot.set_my_description()
            .description(
                "I buy Jio recharges for you from a shielded Zcash testnet budget. \
                 Limits are fixed in code: ₹10–₹50 per recharge, ₹100 per day. \
                 Every purchase leaves a private receipt only your viewing key can read.",
            )
            .await
            .err(),
    ];
    for e in results.into_iter().flatten() {
        tracing::warn!(error = %redact(&e.to_string(), bot.token()), "bot profile not updated");
    }
}

fn menu_keyboard() -> InlineKeyboardMarkup {
    let b = |label: &str, id: &str| InlineKeyboardButton::callback(label, format!("menu:{id}"));
    InlineKeyboardMarkup::new([
        [b("📱 Recharge", "recharge"), b("💰 Balance", "balance")],
        [b("🧾 Receipts", "receipts"), b("🔁 Auto-pay", "rules")],
        [b("🔑 View key", "viewkey"), b("📄 Report", "report")],
        [b("🌐 Website", "web"), b("❓ Help", "help")],
    ])
}

async fn send_with_menu(bot: &Bot, chat: ChatId, text: String) -> anyhow::Result<()> {
    bot.send_message(chat, text)
        .reply_markup(menu_keyboard())
        .await?;
    Ok(())
}

/// A tap on the button menu runs the same handler as the typed command.
async fn menu_action(
    bot: &Bot,
    app: &App,
    chat: ChatId,
    from: &teloxide::types::User,
    item: &str,
) -> anyhow::Result<()> {
    let tg_id = tg_id_of(from);
    match item {
        "recharge" => send(bot, chat, MSG_RECHARGE.to_owned()).await,
        "balance" => cmd_balance(bot, app, chat, tg_id).await,
        "receipts" => cmd_receipts(bot, app, chat, tg_id).await,
        "rules" => cmd_rules(bot, app, chat, tg_id).await,
        "viewkey" => cmd_viewkey(bot, app, chat, tg_id).await,
        "report" => cmd_report(bot, app, chat, tg_id).await,
        "help" => send_with_menu(bot, chat, help_text()).await,
        "menu" => send_with_menu(bot, chat, MSG_MENU.to_owned()).await,
        "web" => cmd_web(bot, app, chat, from).await,
        _ => Ok(()),
    }
}

/// `/web`: a one-hour login link to the website's "My recharges", signed like Telegram's login
/// widget, sent only in this private chat. It needs no phone confirmation from Telegram.
async fn cmd_web(
    bot: &Bot,
    app: &App,
    chat: ChatId,
    from: &teloxide::types::User,
) -> anyhow::Result<()> {
    let tg_id = tg_id_of(from);
    if !app.db.is_user(tg_id)? {
        return send(bot, chat, MSG_NOT_ALLOWED.to_owned()).await;
    }
    let (id, auth_date) = (
        tg_id.to_string(),
        chrono::Utc::now().timestamp().to_string(),
    );
    let query = crate::tglogin::sign(
        &[
            ("id", id.as_str()),
            ("first_name", from.first_name.as_str()),
            ("auth_date", auth_date.as_str()),
        ],
        &app.cfg.telegram_bot_token,
    );
    let Ok(url) = reqwest::Url::parse(&format!("{WEBSITE}/?{query}")) else {
        return send(bot, chat, MSG_ERROR.to_owned()).await;
    };
    bot.send_message(chat, MSG_WEB)
        .protect_content(true)
        .reply_markup(InlineKeyboardMarkup::new([[InlineKeyboardButton::url(
            "🌐 Open my recharges",
            url,
        )]]))
        .await?;
    Ok(())
}

// ---------------------------------------------------------------- callbacks

async fn handle_callback(bot: &Bot, q: &CallbackQuery, app: &App) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    // Always answer first so the button stops spinning, even if we then ignore the tap.
    if let Err(e) = bot.answer_callback_query(q.id.clone()).await {
        tracing::warn!(error = %redact(&e.to_string(), bot.token()), "answer_callback_query failed");
    }
    let Some((action, order_id)) = q.data.as_deref().and_then(parse_callback) else {
        return Ok(());
    };
    let Some(card) = &q.message else {
        return Ok(());
    };
    if !card.chat().is_private() {
        return Ok(());
    }
    let (chat, card_id) = (card.chat().id, card.id());
    let tg_id = tg_id_of(&q.from);
    if count_update(tg_id, now) > MAX_UPDATES_PER_MIN {
        return Ok(());
    }
    let rule_id = || order_id.parse::<i64>().ok();
    match action {
        Action::Menu => menu_action(bot, app, chat, &q.from, order_id).await?,
        Action::Pick => {
            if let Some((operator, phone, inr)) = parse_pick(order_id) {
                send_quote(bot, app, chat, tg_id, operator, phone, inr, now).await?;
            }
        }
        Action::ApproveRule | Action::SkipRule | Action::StopRule => {
            let Some(id) = rule_id() else { return Ok(()) };
            let text = rule_callback(app, action, id, tg_id)?;
            finish_card(bot, app, chat, card_id, text).await;
        }
        Action::Cancel => {
            let text = if orders::cancel(app, order_id, tg_id, now)? {
                MSG_CANCELLED
            } else {
                MSG_NOT_OPEN
            };
            finish_card(bot, app, chat, card_id, text.to_owned()).await;
        }
        Action::Buy => {
            // Dropping the buttons first makes a second tap moot; `orders` guards it too.
            if let Err(e) = edit_card(bot, chat, card_id, MSG_BUYING).await {
                tracing::warn!(error = %redact(&e.to_string(), bot.token()), "could not show Buying…");
            }
            let text = match orders::confirm_and_buy(app, order_id, tg_id, now).await {
                Ok(outcome) => buy_text(&outcome, now),
                Err(e) => {
                    log_error(app, "confirm_and_buy failed", &e);
                    // Past QUOTED the budget was debited and an invoice may exist, so
                    // "nothing was bought" could be false. Say we're checking instead.
                    match app.db.get_order(order_id) {
                        Ok(Some(order))
                            if order.tg_id == tg_id
                                && matches!(
                                    order.status.as_str(),
                                    "PURCHASING" | "UNCERTAIN" | "DELIVERED"
                                ) =>
                        {
                            buy_text(&BuyOutcome::Uncertain(order), now)
                        }
                        _ => MSG_ERROR.to_owned(),
                    }
                }
            };
            finish_card(bot, app, chat, card_id, text).await;
        }
    }
    Ok(())
}

async fn edit_card(
    bot: &Bot,
    chat: ChatId,
    card: MessageId,
    text: &str,
) -> Result<(), teloxide::RequestError> {
    bot.edit_message_text(chat, card, text)
        .reply_markup(InlineKeyboardMarkup::default())
        .await
        .map(|_| ())
}

/// Shows the final result on the card. Never fails the caller: by now the order may have been
/// bought, so a Telegram hiccup must not turn into "nothing was bought".
async fn finish_card(bot: &Bot, app: &App, chat: ChatId, card: MessageId, text: String) {
    if edit_card(bot, chat, card, &text).await.is_ok() {
        return;
    }
    if let Err(e) = bot.send_message(chat, text).await {
        let text = redact(&e.to_string(), &app.cfg.telegram_bot_token);
        tracing::warn!(error = %text, "could not deliver final order message");
    }
}

// ---------------------------------------------------------------- audit report

/// `/report`: rebuilds a throwaway watch-only wallet from the user's viewing key ALONE and lists
/// the purchases its receipt memos prove. Same thing an auditor would get from `ssa audit`.
async fn cmd_report(bot: &Bot, app: &App, chat: ChatId, tg_id: i64) -> anyhow::Result<()> {
    let Some(user) = app.db.get_user(tg_id)? else {
        return send(bot, chat, MSG_NOT_ALLOWED.to_owned()).await;
    };
    let Some(uuid) = user.account_uuid else {
        return send(bot, chat, MSG_NO_WALLET.to_owned()).await;
    };
    let Some(account) = app.zec.account(&uuid).await? else {
        return send(bot, chat, MSG_ERROR_REPORT.to_owned()).await;
    };
    // No birthday would mean scanning from block 0; refuse instead of running for 40 minutes.
    let Some(birthday) = account.birthday else {
        return send(bot, chat, MSG_ERROR_REPORT.to_owned()).await;
    };
    let now = chrono::Utc::now().timestamp();
    let last = LAST_REPORT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&tg_id)
        .copied();
    if last.is_some_and(|t| now - t < REPORT_COOLDOWN_SECS) {
        return send(bot, chat, MSG_REPORT_COOLDOWN.to_owned()).await;
    }
    let Ok(_running) = REPORT_RUNNING.try_lock() else {
        return send(bot, chat, MSG_REPORT_BUSY.to_owned()).await;
    };
    LAST_REPORT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(tg_id, now);
    send(bot, chat, MSG_REPORT_WORKING.to_owned()).await?;
    let lines = match audit::audit(&app.cfg.devtool, &account.ufvk, birthday).await {
        Ok(lines) => lines,
        Err(e) => {
            log_error(app, "audit failed", &e);
            return send(bot, chat, MSG_ERROR_REPORT.to_owned()).await;
        }
    };
    let summary = report_summary(&lines);
    let csv = InputFile::memory(audit::to_csv(&lines).into_bytes()).file_name("ssa-receipts.csv");
    bot.send_document(chat, csv).caption(summary).await?;
    Ok(())
}

fn report_summary(lines: &[audit::AuditLine]) -> String {
    if lines.is_empty() {
        return "Viewing-key report: no purchase receipts found yet.".to_owned();
    }
    let total_inr: i64 = lines.iter().map(|l| l.inr).sum();
    format!(
        "Viewing-key report: {} purchase receipt(s), ₹{total_inr} in total.\nBuilt only from your read-only viewing key, the same view an auditor gets. It can't spend and sees nothing outside your account.",
        lines.len()
    )
}

// ---------------------------------------------------------------- auto-pay

async fn cmd_auto(
    bot: &Bot,
    app: &App,
    chat: ChatId,
    tg_id: i64,
    args: &str,
    now: i64,
) -> anyhow::Result<()> {
    if wallet_of(bot, app, chat, tg_id).await?.is_none() {
        return Ok(());
    }
    let Some(spec) = autopay::parse_rule(args) else {
        return send(bot, chat, MSG_AUTO_HINT.to_owned()).await;
    };
    if app.db.active_rules(tg_id)?.len() >= autopay::MAX_RULES_PER_USER {
        return send(
            bot,
            chat,
            format!(
                "You already have {} auto-pay rules. /stop one first.",
                autopay::MAX_RULES_PER_USER
            ),
        )
        .await;
    }
    // A demo-friendly first run: one period from now, but at least 2 minutes away so the
    // warning with [Skip] always fires first.
    let first_run = now + spec.every_secs.max(2 * autopay::WARN_LEAD_SECS);
    let id = app.db.insert_rule(
        tg_id,
        spec.operator.as_str(),
        &spec.phone,
        spec.inr,
        spec.every_secs,
        first_run,
        now,
    )?;
    let rule = app
        .db
        .get_rule(id)?
        .ok_or_else(|| anyhow::anyhow!("rule vanished"))?;
    bot.send_message(chat, rule_card(&rule))
        .reply_markup(autopay::rule_keyboard(id))
        .await?;
    Ok(())
}

async fn cmd_rules(bot: &Bot, app: &App, chat: ChatId, tg_id: i64) -> anyhow::Result<()> {
    let rules = app.db.active_rules(tg_id)?;
    if rules.is_empty() {
        return send(
            bot,
            chat,
            "No auto-pay rules. Try: /auto jio 9876543210 10 weekly".to_owned(),
        )
        .await;
    }
    let list = rules
        .iter()
        .map(|r| {
            format!(
                "#{} {}
   next run {}",
                r.id,
                rule_line(r),
                ist_time(r.next_run_at)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    send(
        bot,
        chat,
        format!("Your auto-pay rules:\n{list}\n\nStop one with /stop <number>."),
    )
    .await
}

async fn cmd_stop(bot: &Bot, app: &App, chat: ChatId, tg_id: i64, arg: &str) -> anyhow::Result<()> {
    let text = match arg.trim_start_matches('#').parse::<i64>() {
        Ok(id) if app.db.stop_rule(id, tg_id)? => format!("🛑 Auto-pay #{id} stopped."),
        Ok(_) => "No active rule with that number. See /rules.".to_owned(),
        Err(_) => "Usage: /stop <rule number> (see /rules)".to_owned(),
    };
    send(bot, chat, text).await
}

fn rule_callback(app: &App, action: Action, id: i64, tg_id: i64) -> anyhow::Result<String> {
    let Some(rule) = app.db.get_rule(id)? else {
        return Ok("That rule no longer exists.".to_owned());
    };
    if rule.tg_id != tg_id {
        return Ok("Not your rule.".to_owned());
    }
    Ok(match action {
        Action::ApproveRule if app.db.approve_rule(id, tg_id)? => format!(
            "✅ Auto-pay #{id} is on: {}\nFirst run {}. I'll ping you a minute before each run with a Skip button.\nLimits still apply to every run. /rules to see it, /stop {id} to end it.",
            rule_line(&rule),
            ist_time(rule.next_run_at),
        ),
        Action::SkipRule if app.db.skip_next_run(id, tg_id)? => {
            format!("⏭ OK, I'll skip the next run of #{id}. The rule stays on.")
        }
        Action::StopRule if app.db.stop_rule(id, tg_id)? => {
            format!("🛑 Auto-pay #{id} stopped. Nothing more will be bought by it.")
        }
        _ => "That rule isn't open for this action any more. See /rules.".to_owned(),
    })
}

/// `Jio ₹10 → xxxxxxxx10 every week`
pub(crate) fn rule_line(rule: &Rule) -> String {
    format!(
        "{} ₹{} → {} {}",
        operator_label(&rule.operator),
        rule.inr,
        mask_phone(&rule.phone),
        autopay::every_label(rule.every_secs),
    )
}

fn rule_card(rule: &Rule) -> String {
    format!(
        "🤖 New auto-pay rule\n{}\nFirst run: {}\n\nOnce you allow it, I buy this on schedule without asking each time. Every run still respects your limits (₹{}–₹{} per order, ₹{} per day) and the operator's kill switch, and I warn you a minute before with a Skip button.",
        rule_line(rule),
        ist_time(rule.next_run_at),
        caps::MIN_INR,
        caps::MAX_INR,
        caps::USER_DAILY_INR,
    )
}

/// `14:32 IST, 4 Nov`
pub(crate) fn ist_time(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts + IST_OFFSET_SECS, 0)
        .map(|t| t.format("%H:%M IST, %-d %b").to_string())
        .unwrap_or_default()
}

/// Everything after the command word: `/auto jio 98.. 10 weekly` -> `jio 98.. 10 weekly`.
fn command_args(text: &str) -> &str {
    text.trim()
        .split_once(char::is_whitespace)
        .map_or("", |(_, rest)| rest.trim())
}

// ---------------------------------------------------------------- pure helpers

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Menu,
    Pick,
    Buy,
    Cancel,
    ApproveRule,
    SkipRule,
    StopRule,
}

/// `buy:<id>` / `cancel:<id>`; anything else (or a malformed id) is ignored.
fn parse_callback(data: &str) -> Option<(Action, &str)> {
    let (action, id) = data.split_once(':')?;
    let action = match action {
        "menu" => Action::Menu,
        "pick" => Action::Pick,
        "buy" => Action::Buy,
        "cancel" => Action::Cancel,
        "rule" => Action::ApproveRule,
        "skip" => Action::SkipRule,
        "stop" => Action::StopRule,
        _ => return None,
    };
    let valid = !id.is_empty()
        && id.len() <= 60
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then_some((action, id))
}

/// `/cmd@botname args` -> `("cmd", first arg)`. `None` when the text is not a command.
fn parse_command(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim().strip_prefix('/')?;
    let (head, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let name = head.split_once('@').map_or(head, |(name, _)| name);
    let first_arg = args.split_whitespace().next().unwrap_or("");
    Some((name, first_arg))
}

/// Exact decimal ZEC, always 8 places: `8542` -> `0.00008542 ZEC`.
fn zec(zat: i64) -> String {
    let sign = if zat < 0 { "-" } else { "" };
    let abs = zat.unsigned_abs();
    format!("{sign}{}.{:08} ZEC", abs / ZAT_PER_ZEC, abs % ZAT_PER_ZEC)
}

/// Keeps only the last two digits: `9876543210` -> `xxxxxxxx10`.
fn mask_phone(phone: &str) -> String {
    let count = phone.chars().count();
    phone
        .chars()
        .enumerate()
        .map(|(i, c)| if i + 2 >= count { c } else { 'x' })
        .collect()
}

fn operator_label(slug: &str) -> String {
    match Operator::parse(slug) {
        Some(Operator::Jio) => "Jio".to_owned(),
        Some(Operator::Airtel) => "Airtel".to_owned(),
        Some(Operator::Vi) => "Vi".to_owned(),
        None => slug.to_owned(),
    }
}

fn ist_clock(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts + IST_OFFSET_SECS, 0)
        .map(|t| t.format("%H:%M IST").to_string())
        .unwrap_or_default()
}

fn short_ref(order_id: &str) -> String {
    order_id.chars().take(8).collect()
}

fn redact(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        text.to_owned()
    } else {
        text.replace(secret, "<redacted>")
    }
}

fn decision_text(decision: Decision) -> String {
    match decision {
        Decision::Allow => "That's allowed, but something went wrong. Please try again.".to_owned(),
        Decision::ProductNotAllowed => {
            "I can only top up Jio, Airtel and Vi. Try: jio 9876543210 10".to_owned()
        }
        Decision::AmountOutOfRange => format!(
            "Top-ups are ₹{} to ₹{} each. Pick an amount in that range.",
            caps::MIN_INR,
            caps::MAX_INR
        ),
        Decision::TooSoon => format!(
            "One purchase every {} seconds, please.",
            caps::MIN_INTERVAL_SECS
        ),
        Decision::UserDailyLimit => {
            format!(
                "That would pass your ₹{} daily limit.",
                caps::USER_DAILY_INR
            )
        }
        Decision::GlobalDailyLimit => {
            "Today's demo budget is used up. Try again after midnight IST.".to_owned()
        }
    }
}

fn welcome_text(ua: &str) -> String {
    format!("Your private budget is ready. Deposit testnet ZEC here:\n\n{ua}\n\n(QR below)")
}

fn balance_text(available_zat: i64, ua: &str) -> String {
    format!(
        "Available: {}\n\nDeposit testnet ZEC to:\n{ua}",
        zec(available_zat)
    )
}

fn viewkey_text(ufvk: &str) -> String {
    format!("Your viewing key (read-only):\n\n{ufvk}\n\n{MSG_VIEWKEY_HELP}")
}

fn help_text() -> String {
    format!(
        "I buy Indian mobile top-ups from your private Zcash testnet budget.\n\n\
         Send: <operator> <phone>, e.g. jio 9876543210, and pick a recharge,\n\
         or add the amount: jio 9876543210 10\n\
         Operators: Jio, Airtel, Vi (the demo package is Jio ₹{min})\n\
         Limits: ₹{min}–₹{max} per order, ₹{day} per day\n\n\
         I show a price first; nothing is bought until you tap ✅ Buy.\n\n\
         /balance: your budget and deposit address\n\
         /viewkey: your read-only viewing key\n\
         /receipts: every recharge with its full receipt\n\
         /menu: buttons for everything below\n\
         /report: your purchases, proven from the viewing key alone\n\n\
         Auto-pay (I buy on schedule, within your limits):\n\
         /auto jio 9876543210 10 weekly\n\
         /rules: your rules · /stop <n>: end one",
        min = caps::MIN_INR,
        max = caps::MAX_INR,
        day = caps::USER_DAILY_INR,
    )
}

fn quote_card(order: &Order, now: i64) -> String {
    let minutes_left = ((order.quote_expires_at - now).max(0) + 59) / 60;
    format!(
        "{} ₹{} top-up\nPhone: {}\n\nPrice: {}\nNetwork fee: {}\nTotal: {}\n\nExpires in {} min.",
        operator_label(&order.operator),
        order.inr,
        mask_phone(&order.phone),
        zec(order.price_zat),
        zec(order.fee_zat),
        zec(order.total_zat()),
        minutes_left.max(1),
    )
}

/// Reply for any `create_quote` result; the `Quoted` card gets its buttons from the caller.
pub(crate) fn quote_text(outcome: &QuoteOutcome, requested_inr: i64, now: i64) -> String {
    match outcome {
        QuoteOutcome::Quoted(order) => quote_card(order, now),
        QuoteOutcome::Refused(decision) => decision_text(*decision),
        QuoteOutcome::NoPackage { available_inr } => no_package_text(requested_inr, available_inr),
        QuoteOutcome::NotRegistered => MSG_NO_WALLET.to_owned(),
        QuoteOutcome::Unavailable(reason) => {
            format!("I can't get a price right now ({reason}). Try again in a minute.")
        }
    }
}

fn no_package_text(requested_inr: i64, available_inr: &[i64]) -> String {
    if available_inr.is_empty() {
        return format!("There's no ₹{requested_inr} package for that operator right now.");
    }
    let list = available_inr
        .iter()
        .map(|inr| format!("₹{inr}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("There's no ₹{requested_inr} package for that operator. Available: {list}.")
}

pub(crate) fn buy_text(outcome: &BuyOutcome, now: i64) -> String {
    match outcome {
        BuyOutcome::Delivered(order) => format!(
            "✅ Top-up successful\n{} ₹{} → {}\n{} · Ref {}\nYour shielded receipt is on its way; I'll send the full receipt when it's on chain. /receipts lists them all.",
            operator_label(&order.operator),
            order.inr,
            mask_phone(&order.phone),
            ist_clock(now),
            short_ref(&order.id),
        ),
        BuyOutcome::Failed(order) => format!(
            "Bitrefill couldn't complete this order. You have not been charged; your budget is restored. Order {}.",
            short_ref(&order.id)
        ),
        BuyOutcome::Uncertain(order) => format!("{MSG_UNCERTAIN}\nRef {}", short_ref(&order.id)),
        BuyOutcome::DryRun(order) => format!("{MSG_DRY_RUN}\nRef {}", short_ref(&order.id)),
        BuyOutcome::Refused(decision) => decision_text(*decision),
        BuyOutcome::Paused => MSG_PAUSED.to_owned(),
        BuyOutcome::NotOwner => MSG_NOT_YOURS.to_owned(),
        BuyOutcome::NotQuoted => MSG_NOT_OPEN.to_owned(),
        BuyOutcome::Expired => MSG_EXPIRED.to_owned(),
        BuyOutcome::InsufficientFunds {
            available_zat,
            needed_zat,
        } => format!(
            "You have {} available; this needs {}. Deposit more: /start",
            zec(*available_zat),
            zec(*needed_zat)
        ),
    }
}

fn quote_keyboard(order_id: &str) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new([[
        InlineKeyboardButton::callback("✅ Buy", format!("buy:{order_id}")),
        InlineKeyboardButton::callback("❌ Cancel", format!("cancel:{order_id}")),
    ]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_guard_counts_per_id_per_minute() {
        let id = -424_242;
        for n in 1..=MAX_UPDATES_PER_MIN {
            assert_eq!(count_update(id, 1_000), n);
        }
        assert!(count_update(id, 1_030) > MAX_UPDATES_PER_MIN, "same minute");
        assert_eq!(count_update(id, 1_060), 1, "a new minute starts over");
        assert_eq!(count_update(id - 1, 1_000), 1, "other ids are separate");
    }

    #[test]
    fn receipt_text_has_every_receipt_field() {
        let mut o = order();
        o.status = "RECEIPTED".into();
        o.bitrefill_invoice = Some("inv-123".into());
        o.receipt_txid = Some("abcdef0123".into());
        let t = receipt_text(&o);
        for want in [
            "receipt on chain ✓",
            "Bitrefill invoice: inv-123",
            "Shielded receipt tx: abcdef0123",
            "Memo (your /viewkey reads it): SSA·",
            "Paid: ",
            "IST · Ref ",
        ] {
            assert!(t.contains(want), "missing {want:?} in {t}");
        }
        assert!(!t.contains(&o.phone), "full phone must not appear");
        o.receipt_txid = None;
        assert!(receipt_text(&o).contains("Shielded receipt: on its way"));
    }

    #[test]
    fn parse_pick_accepts_only_valid_operator_phone_and_amount() {
        assert_eq!(
            parse_pick("jio_9876543210_10"),
            Some((Operator::Jio, "9876543210", 10))
        );
        for bad in [
            "jio_9876543210_5",
            "jio_9876543210_51",
            "foo_9876543210_10",
            "jio_5876543210_10",
            "jio_98765x3210_10",
            "jio_9876543210_10_1",
            "jio_9876543210",
        ] {
            assert_eq!(parse_pick(bad), None, "{bad}");
        }
    }

    #[test]
    fn recharge_options_keep_amounts_inside_limits_cheapest_first() {
        use crate::bitrefill::{Package, Product};
        let p = |v: &str, inr, price| Package {
            value: v.into(),
            inr,
            price,
        };
        let product = Product {
            slug: "s".into(),
            name: "n".into(),
            packages: vec![
                p("big", Some(500), 1),
                p("talk", Some(10), 141),
                p("talk-dup", Some(10), 150),
                p("data", Some(11), 155),
                p("tiny", Some(5), 1),
                p("ranged", None, 1),
            ],
        };
        assert_eq!(
            recharge_options(&product),
            vec![(10, "talk".to_owned()), (11, "data".to_owned())]
        );
    }

    fn order() -> Order {
        Order {
            id: "3f9a1c2e-1111-4222-8333-444455556666".into(),
            tg_id: 7,
            operator: "jio".into(),
            product_slug: "reliance-jio-india-bundles".into(),
            phone: "9876543210".into(),
            inr: 10,
            usd_cents: 11,
            price_zat: 8_542,
            fee_zat: 10_000,
            status: "QUOTED".into(),
            dry_run: false,
            bitrefill_invoice: None,
            receipt_txid: None,
            error: None,
            quote_expires_at: 1_000 + 600,
            created_at: 1_000,
            updated_at: 1_000,
        }
    }

    #[test]
    fn formats_zat_as_eight_decimal_zec() {
        assert_eq!(zec(8_542), "0.00008542 ZEC");
        assert_eq!(zec(0), "0.00000000 ZEC");
        assert_eq!(zec(150_000_000), "1.50000000 ZEC");
        assert_eq!(zec(-8_542), "-0.00008542 ZEC");
        assert_eq!(zec(i64::MIN), "-92233720368.54775808 ZEC");
    }

    #[test]
    fn masks_all_but_last_two_digits() {
        assert_eq!(mask_phone("9876543210"), "xxxxxxxx10");
        assert_eq!(mask_phone("+919876543210"), "xxxxxxxxxxx10");
        assert_eq!(mask_phone("7"), "7");
        assert_eq!(mask_phone(""), "");
    }

    #[test]
    fn quote_card_shows_masked_phone_amounts_and_expiry() {
        let card = quote_card(&order(), 1_000);
        assert!(card.contains("Jio ₹10"));
        assert!(card.contains("xxxxxxxx10"));
        assert!(!card.contains("9876543210"));
        assert!(card.contains("Price: 0.00008542 ZEC"));
        assert!(card.contains("Network fee: 0.00010000 ZEC"));
        assert!(card.contains("Total: 0.00018542 ZEC"));
        assert!(card.contains("Expires in 10 min."));
        assert!(quote_card(&order(), 1_000 + 590).contains("Expires in 1 min."));
    }

    #[test]
    fn every_refusal_decision_has_distinct_text() {
        let texts: Vec<String> = [
            Decision::ProductNotAllowed,
            Decision::AmountOutOfRange,
            Decision::TooSoon,
            Decision::UserDailyLimit,
            Decision::GlobalDailyLimit,
        ]
        .map(decision_text)
        .into();
        for (i, a) in texts.iter().enumerate() {
            assert!(texts.iter().skip(i + 1).all(|b| a != b));
        }
        assert!(decision_text(Decision::UserDailyLimit).contains("₹100"));
        assert!(decision_text(Decision::TooSoon).contains("30 seconds"));
    }

    #[test]
    fn no_package_lists_available_rupee_values() {
        let text = no_package_text(15, &[10, 20, 50]);
        assert!(text.contains("₹15"));
        assert!(text.contains("₹10, ₹20, ₹50"));
        assert!(no_package_text(15, &[]).contains("no ₹15 package"));
    }

    #[test]
    fn buy_outcomes_map_to_user_messages() {
        let done = buy_text(&BuyOutcome::Delivered(order()), 1_700_000_000);
        assert!(done.starts_with("✅ Top-up successful"));
        assert!(done.contains("Jio ₹10 → xxxxxxxx10"));
        assert!(done.contains("Ref 3f9a1c2e"));
        assert!(done.contains(" IST"));
        assert!(buy_text(&BuyOutcome::Failed(order()), 0).contains("not been charged"));
        assert!(buy_text(&BuyOutcome::Uncertain(order()), 0).contains("checking"));
        assert!(buy_text(&BuyOutcome::DryRun(order()), 0).contains("Dry run"));
        assert!(buy_text(&BuyOutcome::Paused, 0).contains("paused"));
        assert_eq!(buy_text(&BuyOutcome::NotOwner, 0), "Not your order.");
        assert!(buy_text(&BuyOutcome::Expired, 0).contains("expired"));
        assert!(buy_text(&BuyOutcome::NotQuoted, 0).contains("no longer open"));
        assert!(buy_text(&BuyOutcome::Refused(Decision::TooSoon), 0).contains("30 seconds"));
        let low = buy_text(
            &BuyOutcome::InsufficientFunds {
                available_zat: 100,
                needed_zat: 18_542,
            },
            0,
        );
        assert!(low.contains("0.00000100 ZEC") && low.contains("0.00018542 ZEC"));
    }

    #[test]
    fn quote_outcomes_map_to_user_messages() {
        assert!(quote_text(&QuoteOutcome::Quoted(order()), 10, 1_000).contains("Total:"));
        assert!(quote_text(&QuoteOutcome::NotRegistered, 10, 0).contains("/start"));
        assert!(
            quote_text(&QuoteOutcome::Unavailable("price feed".into()), 10, 0)
                .contains("price feed")
        );
        assert!(
            quote_text(&QuoteOutcome::Refused(Decision::AmountOutOfRange), 99, 0)
                .contains("₹10 to ₹50")
        );
    }

    #[test]
    fn parses_rule_callbacks_and_command_args() {
        assert_eq!(
            parse_callback("menu:balance"),
            Some((Action::Menu, "balance"))
        );
        assert_eq!(
            parse_callback("pick:jio_9876543210_10"),
            Some((Action::Pick, "jio_9876543210_10"))
        );
        assert_eq!(parse_callback("rule:12"), Some((Action::ApproveRule, "12")));
        assert_eq!(parse_callback("skip:3"), Some((Action::SkipRule, "3")));
        assert_eq!(parse_callback("stop:3"), Some((Action::StopRule, "3")));
        assert_eq!(parse_callback("delete:3"), None);
        assert_eq!(
            command_args("/auto jio 9876543210 10 weekly"),
            "jio 9876543210 10 weekly"
        );
        assert_eq!(command_args("/auto@zcashdemo_bot  jio 1 2"), "jio 1 2");
        assert_eq!(command_args("/auto"), "");
    }

    #[test]
    fn rule_line_masks_phone_and_reads_schedule() {
        let rule = Rule {
            id: 1,
            tg_id: 1,
            operator: "jio".into(),
            phone: "9876543210".into(),
            inr: 10,
            every_secs: 120,
            next_run_at: 0,
            approved: true,
            active: true,
            skip_next: false,
            warned_for: 0,
            last_order_id: None,
        };
        assert_eq!(rule_line(&rule), "Jio ₹10 → xxxxxxxx10 every 2 minutes");
        assert!(!rule_card(&rule).contains("9876543210"));
    }

    #[test]
    fn parses_only_buy_and_cancel_callbacks() {
        let id = "3f9a1c2e-1111-4222-8333-444455556666";
        assert_eq!(
            parse_callback(&format!("buy:{id}")),
            Some((Action::Buy, id))
        );
        assert_eq!(
            parse_callback(&format!("cancel:{id}")),
            Some((Action::Cancel, id))
        );
        for bad in [
            "",
            "buy",
            "buy:",
            "refund:abc",
            "buy:a b",
            "buy:../x",
            "BUY:abc",
        ] {
            assert_eq!(parse_callback(bad), None, "{bad}");
        }
        assert_eq!(parse_callback(&format!("buy:{}", "a".repeat(61))), None);
    }

    #[test]
    fn parses_commands_with_bot_suffix_and_invite() {
        assert_eq!(parse_command("/start abc123"), Some(("start", "abc123")));
        assert_eq!(
            parse_command("/start@ssa_bot abc123"),
            Some(("start", "abc123"))
        );
        assert_eq!(parse_command("/start"), Some(("start", "")));
        assert_eq!(parse_command("  /balance  "), Some(("balance", "")));
        assert_eq!(parse_command("jio 9876543210 10"), None);
    }

    #[test]
    fn help_lists_limits_operators_and_demo_package() {
        let help = help_text();
        for needle in ["₹10–₹50", "₹100", "Jio", "Airtel", "Vi", "Jio ₹10"] {
            assert!(help.contains(needle), "{needle}");
        }
    }

    #[test]
    fn redacts_the_bot_token() {
        let err = "error sending request for url (https://api.telegram.org/bot123:ABC/getMe)";
        let clean = redact(err, "123:ABC");
        assert!(!clean.contains("123:ABC"));
        assert_eq!(redact("plain", ""), "plain");
    }

    #[test]
    fn clock_is_shown_in_ist() {
        // 2026-11-04 09:00:00 UTC is 14:30 IST.
        assert_eq!(ist_clock(1_793_782_800), "14:30 IST");
    }

    #[test]
    fn deposit_qr_is_a_png() {
        let png = qr_png("zcash:utest1abc").unwrap();
        assert_eq!(&png[..4], b"\x89PNG");
    }
}
