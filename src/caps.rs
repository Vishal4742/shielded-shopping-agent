//! Spend limits (SRS BR-1..BR-4, design §3.6). Pure: no I/O, the clock is a parameter.

use crate::model::Operator;

pub const MIN_INR: i64 = 10;
pub const MAX_INR: i64 = 50;
pub const USER_DAILY_INR: i64 = 100;
pub const GLOBAL_DAILY_INR: i64 = 200;
pub const MIN_INTERVAL_SECS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    ProductNotAllowed,
    AmountOutOfRange,
    TooSoon,
    UserDailyLimit,
    GlobalDailyLimit,
}

/// First failing check wins, in enum order. `spent_*` come from SQL over the BR-5 states.
pub fn check(
    operator: Operator,
    inr: i64,
    user_spent_today_inr: i64,
    global_spent_today_inr: i64,
    last_purchase_at: Option<i64>,
    now: i64,
) -> Decision {
    // Exhaustive on purpose: adding an operator forces a decision here.
    let allowed = match operator {
        Operator::Jio | Operator::Airtel | Operator::Vi => true,
    };
    if !allowed {
        return Decision::ProductNotAllowed;
    }
    if !(MIN_INR..=MAX_INR).contains(&inr) {
        return Decision::AmountOutOfRange;
    }
    if last_purchase_at.is_some_and(|last| now.saturating_sub(last) < MIN_INTERVAL_SECS) {
        return Decision::TooSoon;
    }
    if user_spent_today_inr.saturating_add(inr) > USER_DAILY_INR {
        return Decision::UserDailyLimit;
    }
    if global_spent_today_inr.saturating_add(inr) > GLOBAL_DAILY_INR {
        return Decision::GlobalDailyLimit;
    }
    Decision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000;

    fn ok(inr: i64, user: i64, global: i64, last: Option<i64>) -> Decision {
        check(Operator::Jio, inr, user, global, last, NOW)
    }

    #[test]
    fn allows_a_normal_purchase_for_every_operator() {
        for op in [Operator::Jio, Operator::Airtel, Operator::Vi] {
            assert_eq!(check(op, 20, 0, 0, None, NOW), Decision::Allow);
        }
    }

    #[test]
    fn amount_boundaries_9_10_50_51() {
        assert_eq!(ok(9, 0, 0, None), Decision::AmountOutOfRange);
        assert_eq!(ok(10, 0, 0, None), Decision::Allow);
        assert_eq!(ok(50, 0, 0, None), Decision::Allow);
        assert_eq!(ok(51, 0, 0, None), Decision::AmountOutOfRange);
    }

    #[test]
    fn zero_and_negative_amounts_are_out_of_range() {
        assert_eq!(ok(0, 0, 0, None), Decision::AmountOutOfRange);
        assert_eq!(ok(-20, 0, 0, None), Decision::AmountOutOfRange);
        assert_eq!(ok(i64::MIN, 0, 0, None), Decision::AmountOutOfRange);
    }

    #[test]
    fn too_soon_boundary_is_30_seconds() {
        assert_eq!(ok(20, 0, 0, Some(NOW - 29)), Decision::TooSoon);
        assert_eq!(ok(20, 0, 0, Some(NOW - 30)), Decision::Allow);
        assert_eq!(ok(20, 0, 0, Some(NOW)), Decision::TooSoon);
    }

    #[test]
    fn missing_last_purchase_never_trips_too_soon() {
        assert_eq!(ok(20, 0, 0, None), Decision::Allow);
    }

    #[test]
    fn last_purchase_in_the_future_counts_as_too_soon() {
        assert_eq!(ok(20, 0, 0, Some(NOW + 5)), Decision::TooSoon);
    }

    #[test]
    fn user_daily_limit_allows_exactly_100() {
        assert_eq!(ok(50, 50, 50, None), Decision::Allow);
        assert_eq!(ok(20, 80, 80, None), Decision::Allow);
        assert_eq!(ok(21, 80, 80, None), Decision::UserDailyLimit);
        assert_eq!(ok(10, 100, 100, None), Decision::UserDailyLimit);
    }

    #[test]
    fn global_daily_limit_allows_exactly_200() {
        assert_eq!(ok(20, 0, 180, None), Decision::Allow);
        assert_eq!(ok(21, 0, 180, None), Decision::GlobalDailyLimit);
        assert_eq!(ok(10, 0, 200, None), Decision::GlobalDailyLimit);
    }

    #[test]
    fn rejections_follow_priority_order() {
        // Amount out of range beats everything below it.
        assert_eq!(ok(51, 100, 200, Some(NOW)), Decision::AmountOutOfRange);
        // Too soon beats both daily limits.
        assert_eq!(ok(20, 100, 200, Some(NOW)), Decision::TooSoon);
        // User limit beats global limit.
        assert_eq!(ok(20, 90, 190, None), Decision::UserDailyLimit);
        // Global alone.
        assert_eq!(ok(20, 0, 190, None), Decision::GlobalDailyLimit);
    }

    #[test]
    fn spend_sum_does_not_overflow() {
        assert_eq!(ok(20, i64::MAX, 0, None), Decision::UserDailyLimit);
        assert_eq!(ok(20, 0, i64::MAX, None), Decision::GlobalDailyLimit);
    }
}
