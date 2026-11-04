//! Auto-pay (agent "works while you sleep"): the user approves a rule once with ✅, then the
//! agent quotes and buys on schedule. Every run still goes through `orders::confirm_and_buy`,
//! so caps, the kill switch and the ledger apply exactly as for a manual ✅.

use std::sync::Arc;
use std::time::Duration;

use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::db::Rule;
use crate::intent::{self, Intent};
use crate::model::Operator;
use crate::orders::{self, App, QuoteOutcome};

pub const MIN_EVERY_SECS: i64 = 60;
pub const MAX_RULES_PER_USER: usize = 3;
/// Warn this long before a run, with a [Skip] button.
pub const WARN_LEAD_SECS: i64 = 60;
const TICK: Duration = Duration::from_secs(10);
const DAY: i64 = 86_400;

/// What the user asked for in `/auto …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSpec {
    pub operator: Operator,
    pub phone: String,
    pub inr: i64,
    pub every_secs: i64,
}

/// `/auto jio 9876543210 10 weekly` · `… daily` · `… monthly` · `… every 2m` / `every 3 hours`.
/// Operator, phone and amount reuse the intent parser; the schedule is parsed here.
pub fn parse_rule(text: &str) -> Option<RuleSpec> {
    let lower = text.to_lowercase();
    let (head, every_secs) = parse_every(&lower)?;
    match intent::parse_regex(head) {
        Intent::TopUp {
            operator,
            phone,
            inr,
        } => Some(RuleSpec {
            operator,
            phone,
            inr,
            every_secs,
        }),
        _ => None,
    }
}

/// Splits off the schedule; returns the text before it and the period in seconds.
fn parse_every(text: &str) -> Option<(&str, i64)> {
    for (word, secs) in [("daily", DAY), ("weekly", 7 * DAY), ("monthly", 30 * DAY)] {
        if let Some(i) = text.find(word) {
            return Some((&text[..i], secs));
        }
    }
    let i = text.find("every")?;
    let rest = text[i + "every".len()..].trim();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let n: i64 = if digits.is_empty() {
        1
    } else {
        digits.parse().ok()?
    };
    let unit = rest[digits.len()..].trim_start();
    let per = match unit.chars().next()? {
        'm' if unit.starts_with("mo") => 30 * DAY,
        'm' => 60,
        'h' => 3600,
        'd' => DAY,
        'w' => 7 * DAY,
        _ => return None,
    };
    let secs = n.checked_mul(per)?;
    (secs >= MIN_EVERY_SECS).then_some((&text[..i], secs))
}

pub fn every_label(secs: i64) -> String {
    match secs {
        s if s % (30 * DAY) == 0 => plural(s / (30 * DAY), "month"),
        s if s % (7 * DAY) == 0 => plural(s / (7 * DAY), "week"),
        s if s % DAY == 0 => plural(s / DAY, "day"),
        s if s % 3600 == 0 => plural(s / 3600, "hour"),
        s => plural(s / 60, "minute"),
    }
}

fn plural(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("every {unit}")
    } else {
        format!("every {n} {unit}s")
    }
}

/// Next slot strictly after `now`, staying on the rule's grid (no burst of catch-up buys
/// after downtime: missed slots are skipped, never bought late in a batch).
pub fn next_slot(run_at: i64, every_secs: i64, now: i64) -> i64 {
    if now < run_at {
        return run_at + every_secs;
    }
    let missed = (now - run_at) / every_secs + 1;
    run_at + missed * every_secs
}

pub fn rule_keyboard(rule_id: i64) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new([[
        InlineKeyboardButton::callback("✅ Allow auto-pay", format!("rule:{rule_id}")),
        InlineKeyboardButton::callback("❌ Cancel", format!("stop:{rule_id}")),
    ]])
}

pub fn skip_keyboard(rule_id: i64) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new([[
        InlineKeyboardButton::callback("⏭ Skip this one", format!("skip:{rule_id}")),
        InlineKeyboardButton::callback("🛑 Stop rule", format!("stop:{rule_id}")),
    ]])
}

/// Every `TICK`: warn about runs due within `WARN_LEAD_SECS`, then run what's due.
pub async fn run_loop(app: Arc<App>, bot: Bot) {
    loop {
        if let Err(e) = tick(&app, &bot).await {
            tracing::warn!(error = %e.to_string().replace(bot.token(), "<token>"), "autopay tick failed");
        }
        tokio::time::sleep(TICK).await;
    }
}

async fn tick(app: &App, bot: &Bot) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    for rule in app.db.rules_due_by(now + WARN_LEAD_SECS)? {
        if rule.next_run_at > now {
            if rule.warned_for != rule.next_run_at && !rule.skip_next {
                app.db.mark_rule_warned(rule.id, rule.next_run_at)?;
                let text = format!(
                    "⏰ Auto-pay in about a minute: {}",
                    crate::telegram::rule_line(&rule)
                );
                notify(bot, rule.tg_id, text, Some(skip_keyboard(rule.id))).await;
            }
            continue;
        }
        run_rule(app, bot, &rule, now).await?;
    }
    Ok(())
}

async fn run_rule(app: &App, bot: &Bot, rule: &Rule, now: i64) -> anyhow::Result<()> {
    let next = next_slot(rule.next_run_at, rule.every_secs, now);
    // Claim the slot first: if this conditional update fails, another tick already ran it.
    // Atomic claim: re-checks active/approved and reads the CURRENT skip flag, so a Skip or
    // Stop that lands after the scheduler read the rule still wins.
    let Some(skip) = app.db.claim_rule_slot(rule.id, rule.next_run_at, next)? else {
        return Ok(());
    };
    if skip {
        notify(
            bot,
            rule.tg_id,
            format!(
                "⏭ Skipped this auto-pay. Next run {}.",
                crate::telegram::ist_time(next)
            ),
            None,
        )
        .await;
        return Ok(());
    }
    let Some(operator) = Operator::parse(&rule.operator) else {
        return Ok(());
    };
    tracing::info!(
        rule_id = rule.id,
        tg_id = rule.tg_id,
        inr = rule.inr,
        "autopay run"
    );
    let quote = orders::create_quote(app, rule.tg_id, operator, &rule.phone, rule.inr, now).await?;
    let text = match quote {
        QuoteOutcome::Quoted(order) => {
            app.db.advance_rule(rule.id, next, next, Some(&order.id))?;
            let outcome = orders::confirm_and_buy(app, &order.id, rule.tg_id, now).await?;
            format!(
                "🤖 Auto-pay ran: {}\n\n{}",
                crate::telegram::rule_line(rule),
                crate::telegram::buy_text(&outcome, now)
            )
        }
        other => format!(
            "🤖 Auto-pay didn't run: {}",
            crate::telegram::quote_text(&other, rule.inr, now)
        ),
    };
    notify(
        bot,
        rule.tg_id,
        format!("{text}\n\nNext run {}.", crate::telegram::ist_time(next)),
        None,
    )
    .await;
    Ok(())
}

async fn notify(bot: &Bot, tg_id: i64, text: String, keyboard: Option<InlineKeyboardMarkup>) {
    let req = bot.send_message(ChatId(tg_id), text);
    let res = match keyboard {
        Some(k) => req.reply_markup(k).await,
        None => req.await,
    };
    if let Err(e) = res {
        tracing::warn!(tg_id, error = %e.to_string().replace(bot.token(), "<token>"), "autopay notify failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules_with_each_schedule_form() {
        let spec = |every_secs| RuleSpec {
            operator: Operator::Jio,
            phone: "9876543210".into(),
            inr: 10,
            every_secs,
        };
        assert_eq!(parse_rule("jio 9876543210 10 weekly"), Some(spec(7 * DAY)));
        assert_eq!(parse_rule("Jio 9876543210 10 daily"), Some(spec(DAY)));
        assert_eq!(
            parse_rule("jio 9876543210 10 monthly"),
            Some(spec(30 * DAY))
        );
        assert_eq!(parse_rule("jio 9876543210 10 every 2m"), Some(spec(120)));
        assert_eq!(
            parse_rule("jio 9876543210 10 every 3 hours"),
            Some(spec(3 * 3600))
        );
        assert_eq!(
            parse_rule("jio 9876543210 10 every month"),
            Some(spec(30 * DAY))
        );
        assert_eq!(
            parse_rule("jio 9876543210 10 every week"),
            Some(spec(7 * DAY))
        );
    }

    #[test]
    fn rejects_rules_without_schedule_or_too_frequent() {
        assert_eq!(parse_rule("jio 9876543210 10"), None);
        assert_eq!(parse_rule("jio 9876543210 10 every 30s"), None);
        assert_eq!(parse_rule("jio 9876543210 10 every 0m"), None);
        assert_eq!(parse_rule("weekly"), None);
        assert_eq!(parse_rule("foo 9876543210 10 weekly"), None);
    }

    #[test]
    fn schedule_number_is_not_taken_as_the_amount() {
        // "every 2m" comes after the amount; only the text before the schedule is parsed.
        assert_eq!(
            parse_rule("airtel 98765 43210 20 every 5 minutes")
                .unwrap()
                .inr,
            20
        );
    }

    #[test]
    fn labels_read_naturally() {
        assert_eq!(every_label(120), "every 2 minutes");
        assert_eq!(every_label(60), "every minute");
        assert_eq!(every_label(7 * DAY), "every week");
        assert_eq!(every_label(30 * DAY), "every month");
        assert_eq!(every_label(3 * 3600), "every 3 hours");
    }

    #[test]
    fn next_slot_skips_missed_runs_instead_of_bursting() {
        assert_eq!(next_slot(1000, 120, 1000), 1120);
        assert_eq!(next_slot(1000, 120, 1119), 1120);
        assert_eq!(next_slot(1000, 120, 1120), 1240);
        // Down for an hour: one run now, next on the grid, no 30 back-to-back buys.
        assert_eq!(next_slot(1000, 120, 4600), 4720);
        assert_eq!(next_slot(1000, 120, 900), 1120);
    }
}
