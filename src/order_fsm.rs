//! Order state machine (design §2). Pure: no I/O, no clock.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Quoted,
    Purchasing,
    Delivered,
    Failed,
    Uncertain,
    ReceiptSending,
    Receipted,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// `Confirm` happens inside db::confirm_order's single SQL transaction and `RetryReceipt` is a
// self-loop the receipt queue never needs to write; both stay so the edge table matches design §2.
#[allow(dead_code)]
pub enum Event {
    Cancel,
    Expire,
    Confirm,
    Deliver,
    Fail,
    MarkUncertain,
    ResolveDelivered,
    ResolveFailed,
    StartReceipt,
    RetryReceipt,
    ReceiptMined,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    #[error("event {event:?} not allowed from {from:?}")]
    NotAllowed { from: Status, event: Event },
    #[error("unknown status {0:?}")]
    UnknownStatus(String),
}

/// The only way an order changes status. Callers pair it with a conditional SQL update.
pub fn apply(from: Status, event: Event) -> Result<Status, TransitionError> {
    use Event as E;
    use Status as S;
    match (from, event) {
        (S::Quoted, E::Cancel) => Ok(S::Cancelled),
        (S::Quoted, E::Expire) => Ok(S::Expired),
        (S::Quoted, E::Confirm) => Ok(S::Purchasing),
        (S::Purchasing, E::Deliver) => Ok(S::Delivered),
        (S::Purchasing, E::Fail) => Ok(S::Failed),
        (S::Purchasing, E::MarkUncertain) => Ok(S::Uncertain),
        (S::Uncertain, E::ResolveDelivered) => Ok(S::Delivered),
        (S::Uncertain, E::ResolveFailed) => Ok(S::Failed),
        (S::Delivered, E::StartReceipt) => Ok(S::ReceiptSending),
        (S::ReceiptSending, E::RetryReceipt) => Ok(S::ReceiptSending),
        (S::ReceiptSending, E::ReceiptMined) => Ok(S::Receipted),
        _ => Err(TransitionError::NotAllowed { from, event }),
    }
}

pub fn as_str(s: Status) -> &'static str {
    match s {
        Status::Quoted => "QUOTED",
        Status::Purchasing => "PURCHASING",
        Status::Delivered => "DELIVERED",
        Status::Failed => "FAILED",
        Status::Uncertain => "UNCERTAIN",
        Status::ReceiptSending => "RECEIPT_SENDING",
        Status::Receipted => "RECEIPTED",
        Status::Cancelled => "CANCELLED",
        Status::Expired => "EXPIRED",
    }
}

pub fn parse(s: &str) -> Result<Status, TransitionError> {
    match s {
        "QUOTED" => Ok(Status::Quoted),
        "PURCHASING" => Ok(Status::Purchasing),
        "DELIVERED" => Ok(Status::Delivered),
        "FAILED" => Ok(Status::Failed),
        "UNCERTAIN" => Ok(Status::Uncertain),
        "RECEIPT_SENDING" => Ok(Status::ReceiptSending),
        "RECEIPTED" => Ok(Status::Receipted),
        "CANCELLED" => Ok(Status::Cancelled),
        "EXPIRED" => Ok(Status::Expired),
        other => Err(TransitionError::UnknownStatus(other.to_string())),
    }
}

/// SRS BR-5: statuses whose price counts toward the daily caps. `db::spent_today_inr` holds
/// the same list in SQL; the test below pins it.
#[cfg(test)]
pub fn counts_toward_cap(s: Status) -> bool {
    matches!(
        s,
        Status::Purchasing
            | Status::Uncertain
            | Status::Delivered
            | Status::ReceiptSending
            | Status::Receipted
    )
}

#[cfg(test)]
pub fn is_terminal(s: Status) -> bool {
    matches!(
        s,
        Status::Receipted | Status::Cancelled | Status::Expired | Status::Failed
    )
}

#[cfg(test)]
mod tests {
    use super::Event::*;
    use super::Status::*;
    use super::*;

    const ALL_STATUSES: [Status; 9] = [
        Quoted,
        Purchasing,
        Delivered,
        Failed,
        Uncertain,
        ReceiptSending,
        Receipted,
        Cancelled,
        Expired,
    ];

    const ALL_EVENTS: [Event; 11] = [
        Cancel,
        Expire,
        Confirm,
        Deliver,
        Fail,
        MarkUncertain,
        ResolveDelivered,
        ResolveFailed,
        StartReceipt,
        RetryReceipt,
        ReceiptMined,
    ];

    /// Design §2, exactly: (from, event, to).
    const EDGES: [(Status, Event, Status); 11] = [
        (Quoted, Cancel, Cancelled),
        (Quoted, Expire, Expired),
        (Quoted, Confirm, Purchasing),
        (Purchasing, Deliver, Delivered),
        (Purchasing, Fail, Failed),
        (Purchasing, MarkUncertain, Uncertain),
        (Uncertain, ResolveDelivered, Delivered),
        (Uncertain, ResolveFailed, Failed),
        (Delivered, StartReceipt, ReceiptSending),
        (ReceiptSending, RetryReceipt, ReceiptSending),
        (ReceiptSending, ReceiptMined, Receipted),
    ];

    #[test]
    fn every_allowed_edge_moves_to_target() {
        for (from, event, to) in EDGES {
            assert_eq!(apply(from, event), Ok(to), "{from:?} --{event:?}-->");
        }
    }

    #[test]
    fn every_other_pair_is_rejected() {
        let mut rejected = 0;
        for from in ALL_STATUSES {
            for event in ALL_EVENTS {
                if EDGES.iter().any(|&(f, e, _)| f == from && e == event) {
                    continue;
                }
                assert_eq!(
                    apply(from, event),
                    Err(TransitionError::NotAllowed { from, event })
                );
                rejected += 1;
            }
        }
        assert_eq!(rejected, 9 * 11 - EDGES.len());
    }

    #[test]
    fn as_str_parse_round_trip() {
        for s in ALL_STATUSES {
            assert_eq!(parse(as_str(s)), Ok(s));
        }
    }

    #[test]
    fn as_str_matches_db_values() {
        let expected = [
            (Quoted, "QUOTED"),
            (Purchasing, "PURCHASING"),
            (Delivered, "DELIVERED"),
            (Failed, "FAILED"),
            (Uncertain, "UNCERTAIN"),
            (ReceiptSending, "RECEIPT_SENDING"),
            (Receipted, "RECEIPTED"),
            (Cancelled, "CANCELLED"),
            (Expired, "EXPIRED"),
        ];
        for (s, text) in expected {
            assert_eq!(as_str(s), text);
        }
    }

    #[test]
    fn parse_rejects_unknown_and_wrong_case() {
        for bad in ["", "quoted", "CONFIRMED", "Quoted ", "RECEIPT-SENDING"] {
            assert_eq!(
                parse(bad),
                Err(TransitionError::UnknownStatus(bad.to_string()))
            );
        }
    }

    #[test]
    fn terminal_states_are_exactly_the_four() {
        for s in ALL_STATUSES {
            let want = matches!(s, Receipted | Cancelled | Expired | Failed);
            assert_eq!(is_terminal(s), want, "{s:?}");
        }
    }

    #[test]
    fn terminal_states_have_no_outgoing_edge() {
        for s in ALL_STATUSES.into_iter().filter(|&s| is_terminal(s)) {
            for event in ALL_EVENTS {
                assert!(apply(s, event).is_err(), "{s:?} --{event:?}-->");
            }
        }
    }

    #[test]
    fn counts_toward_cap_table() {
        let expected = [
            (Quoted, false),
            (Purchasing, true),
            (Delivered, true),
            (Failed, false),
            (Uncertain, true),
            (ReceiptSending, true),
            (Receipted, true),
            (Cancelled, false),
            (Expired, false),
        ];
        for (s, want) in expected {
            assert_eq!(counts_toward_cap(s), want, "{s:?}");
        }
    }
}
