//! Shared domain types (notes/coordination.md §2). Money is always integer: zat, cents, rupees.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator {
    Jio,
    Airtel,
    Vi,
}

impl Operator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jio => "jio",
            Self::Airtel => "airtel",
            Self::Vi => "vi",
        }
    }

    /// Lowercase only; trims nothing.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "jio" => Some(Self::Jio),
            "airtel" => Some(Self::Airtel),
            "vi" => Some(Self::Vi),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_round_trips_and_rejects_other_case() {
        for op in [Operator::Jio, Operator::Airtel, Operator::Vi] {
            assert_eq!(Operator::parse(op.as_str()), Some(op));
        }
        assert_eq!(Operator::parse("Jio"), None);
        assert_eq!(Operator::parse(" jio"), None);
    }
}
