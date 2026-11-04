//! zcash-devtool wrapper (EI-2). This file holds the pure output parsers; the subprocess
//! runner and wallet lock land once the coordinator defines the shared error types
//! (notes/coordination.md §8 step 4). Formats were captured from devtool `0b3436b` into
//! `fixtures/devtool/`.

use std::fmt;

const ZAT_PER_ZEC: i64 = 100_000_000;
const ZEC_DECIMALS: usize = 8;
const TXID_HEX_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    BadJson(String),
    MissingField(&'static str),
    BadAmount(String),
    NoTxid,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadJson(e) => write!(f, "devtool JSON: {e}"),
            Self::MissingField(k) => write!(f, "devtool output missing {k}"),
            Self::BadAmount(s) => write!(f, "devtool amount not parseable: {s:?}"),
            Self::NoTxid => write!(f, "devtool send printed no txid"),
        }
    }
}

impl std::error::Error for ParseError {}

/// `balance --json <uuid>`. Spendable counts the shielded pools only; transparent funds
/// are never spent by the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance {
    pub tip_height: i64,
    pub spendable_zat: i64,
    pub total_zat: i64,
}

pub fn parse_balance(json: &str) -> Result<Balance, ParseError> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| ParseError::BadJson(e.to_string()))?;
    let field = |k: &'static str| {
        v.get(k)
            .and_then(|x| x.as_i64())
            .ok_or(ParseError::MissingField(k))
    };
    let spendable = [
        "ironwood_spendable",
        "orchard_spendable",
        "sapling_spendable",
    ]
    .into_iter()
    .map(field)
    .sum::<Result<i64, _>>()?;
    Ok(Balance {
        tip_height: field("chain_tip_height")?,
        spendable_zat: spendable,
        total_zat: field("total")?,
    })
}

/// `send` prints the proposal, then `Sending transaction...`, then the txid as the last line.
pub fn parse_send_txid(stdout: &str) -> Result<String, ParseError> {
    let last = stdout
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .ok_or(ParseError::NoTxid)?;
    let is_txid = last.len() == TXID_HEX_LEN && last.bytes().all(|b| b.is_ascii_hexdigit());
    if is_txid {
        Ok(last.to_owned())
    } else {
        Err(ParseError::NoTxid)
    }
}

/// Exact decimal TAZ/ZEC (`"1.00000000 TAZ"`, `"-0.001"`) to zat, no floats.
pub fn parse_zec_amount(s: &str) -> Result<i64, ParseError> {
    let bad = || ParseError::BadAmount(s.to_owned());
    let num = s
        .trim()
        .trim_end_matches("TAZ")
        .trim_end_matches("ZEC")
        .trim();
    let (negative, digits) = match num.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, num),
    };
    let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
    let all_digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || frac.len() > ZEC_DECIMALS || !all_digits(whole) || !all_digits(frac) {
        return Err(bad());
    }
    let frac_zat: i64 = format!("{frac:0<ZEC_DECIMALS$}")
        .parse()
        .map_err(|_| bad())?;
    let zat = whole
        .parse::<i64>()
        .ok()
        .and_then(|w| w.checked_mul(ZAT_PER_ZEC))
        .and_then(|w| w.checked_add(frac_zat))
        .ok_or_else(bad)?;
    Ok(if negative { -zat } else { zat })
}

/// One output of a `list-tx` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxOutput {
    pub value_zat: i64,
    pub is_change: bool,
    pub received_by: Option<String>,
    /// Text inside `Memo::Text("…")`, still Rust-debug-escaped.
    // ponytail: escapes not decoded; our memos and order ids have no `"`/`\`, decode if a caller shows memos to users
    pub memo: Option<String>,
}

/// One `list-tx` entry. A tx touching two wallet accounts is printed once per account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tx {
    pub txid: String,
    pub mined_height: Option<i64>,
    pub outputs: Vec<TxOutput>,
}

fn is_txid(s: &str) -> bool {
    s.len() == TXID_HEX_LEN && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `list-tx` text mode. The header's "N memos" count is unreliable (it says 0 for a tx with a
/// text memo), so memos come from the per-output `Memo:` lines.
pub fn parse_list_tx(text: &str) -> Result<Vec<Tx>, ParseError> {
    let mut txs: Vec<Tx> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if !raw.starts_with(' ') && is_txid(line) {
            txs.push(Tx {
                txid: line.to_owned(),
                mined_height: None,
                outputs: Vec::new(),
            });
            continue;
        }
        let Some(tx) = txs.last_mut() else { continue };
        if let Some(rest) = line.strip_prefix("Mined:") {
            let height = rest.split_whitespace().next().unwrap_or_default();
            tx.mined_height = Some(
                height
                    .parse()
                    .map_err(|_| ParseError::BadAmount(line.to_owned()))?,
            );
        } else if line.starts_with("Output ") {
            tx.outputs.push(TxOutput {
                value_zat: 0,
                is_change: false,
                received_by: None,
                memo: None,
            });
        } else if let Some(out) = tx.outputs.last_mut() {
            parse_output_line(line, out)?;
        }
    }
    Ok(txs)
}

fn parse_output_line(line: &str, out: &mut TxOutput) -> Result<(), ParseError> {
    if let Some(rest) = line.strip_prefix("Value:") {
        let (amount, tag) = rest.split_once('(').unwrap_or((rest, ""));
        out.value_zat = parse_zec_amount(amount)?;
        out.is_change = tag.starts_with("Change");
    } else if let Some(rest) = line.strip_prefix("Received by account:") {
        out.received_by = rest.split_whitespace().next().map(str::to_owned);
    } else if let Some(rest) = line.strip_prefix("Memo: Memo::Text(\"") {
        out.memo = rest.strip_suffix("\")").map(str::to_owned);
    }
    Ok(())
}

/// Receipt-queue idempotency (design §3.4 step 2): the txid of a tx whose memo contains `needle`.
pub fn find_memo_txid<'a>(txs: &'a [Tx], needle: &str) -> Option<&'a str> {
    txs.iter()
        .find(|tx| {
            tx.outputs
                .iter()
                .any(|o| o.memo.as_deref().is_some_and(|m| m.contains(needle)))
        })
        .map(|tx| tx.txid.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST_TX_MEMO: &str = include_str!("../fixtures/devtool/list-tx-memo.txt");
    const MEMO_TXID: &str = "5c5abb6f8c0fe605c9f923ab160e13769dd70a4c963ff10bb425a816c811bf5a";
    const TREASURY: &str = "093fb250-5e57-4ca4-a371-ce280b15b7be";

    #[test]
    fn list_tx_parses_received_faucet_tx() {
        let txs = parse_list_tx(LIST_TX_MEMO).unwrap();
        assert_eq!(txs.len(), 4, "faucet x2 + memo tx printed once per account");
        assert_eq!(txs[0].mined_height, Some(4_557_289));
        assert_eq!(
            txs[0].outputs,
            vec![TxOutput {
                value_zat: 100_000_000,
                is_change: false,
                received_by: Some(TREASURY.to_owned()),
                memo: None,
            }]
        );
    }

    #[test]
    fn list_tx_reads_memo_and_change_outputs() {
        let txs = parse_list_tx(LIST_TX_MEMO).unwrap();
        let memo_tx = &txs[2];
        assert_eq!(memo_tx.txid, MEMO_TXID);
        assert_eq!(memo_tx.outputs.len(), 5);
        assert_eq!(memo_tx.outputs[0].value_zat, 100_000);
        assert_eq!(
            memo_tx.outputs[0].memo.as_deref(),
            Some("SSA·00000000-test-memo·jio·₹10")
        );
        assert!(memo_tx.outputs[1..]
            .iter()
            .all(|o| o.is_change && o.memo.is_none()));
    }

    #[test]
    fn find_memo_txid_matches_order_id_only() {
        let txs = parse_list_tx(LIST_TX_MEMO).unwrap();
        assert_eq!(find_memo_txid(&txs, "00000000-test-memo"), Some(MEMO_TXID));
        assert_eq!(find_memo_txid(&txs, "11111111-other"), None);
    }

    #[test]
    fn list_tx_unmined_and_empty_input() {
        let unmined = format!("Transactions:\n{MEMO_TXID}\n    Amount:   0.1 TAZ\n");
        assert_eq!(parse_list_tx(&unmined).unwrap()[0].mined_height, None);
        assert_eq!(parse_list_tx("Transactions:\n").unwrap(), vec![]);
    }

    const BALANCE_UNCONFIRMED: &str = include_str!("../fixtures/devtool/balance-unconfirmed.json");
    const SEND_OK: &str = include_str!("../fixtures/devtool/send-ok.txt");

    #[test]
    fn balance_with_unconfirmed_receipt_has_zero_spendable() {
        let b = parse_balance(BALANCE_UNCONFIRMED).unwrap();
        assert_eq!(
            b,
            Balance {
                tip_height: 4_557_296,
                spendable_zat: 0,
                total_zat: 100_000_000
            }
        );
    }

    #[test]
    fn balance_sums_shielded_pools_and_ignores_transparent() {
        let json = r#"{"chain_tip_height":1,"ironwood_spendable":5,"orchard_spendable":7,
            "sapling_spendable":11,"total":999,"transparent_spendable":1000}"#;
        assert_eq!(parse_balance(json).unwrap().spendable_zat, 23);
    }

    #[test]
    fn balance_rejects_missing_field_and_garbage() {
        assert_eq!(
            parse_balance(r#"{"total":1}"#),
            Err(ParseError::MissingField("ironwood_spendable"))
        );
        assert!(matches!(
            parse_balance("Error: More than one account"),
            Err(ParseError::BadJson(_))
        ));
    }

    #[test]
    fn send_txid_is_last_line_of_real_output() {
        assert_eq!(
            parse_send_txid(SEND_OK).unwrap(),
            "5c5abb6f8c0fe605c9f923ab160e13769dd70a4c963ff10bb425a816c811bf5a"
        );
    }

    #[test]
    fn send_without_txid_is_an_error() {
        assert_eq!(
            parse_send_txid("Sending transaction...\n"),
            Err(ParseError::NoTxid)
        );
        assert_eq!(parse_send_txid(""), Err(ParseError::NoTxid));
    }

    #[test]
    fn zec_amounts_parse_exactly() {
        assert_eq!(parse_zec_amount("1.00000000 TAZ"), Ok(100_000_000));
        assert_eq!(parse_zec_amount("  0.00100000 TAZ"), Ok(100_000));
        assert_eq!(parse_zec_amount("0.00000001"), Ok(1));
        assert_eq!(parse_zec_amount("-0.00025 TAZ"), Ok(-25_000));
        assert_eq!(parse_zec_amount("3"), Ok(300_000_000));
    }

    #[test]
    fn zec_amounts_reject_bad_input() {
        for s in [
            "",
            "TAZ",
            "1.000000001",
            "1,5",
            ".5",
            "1.2.3",
            "abc TAZ",
            "99999999999999999999",
        ] {
            assert!(parse_zec_amount(s).is_err(), "{s:?} should fail");
        }
    }
}
