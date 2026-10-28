//! Corporate finance actions: bonds, stock offerings, buybacks, loans, dividends, policies, lawsuits.
//!
//! Submodules by routine family:
//! - [`bank`]: `BankLiquidityAndCapitalCheck` (0x5021e2) and its helpers (subprime sales, buyer bank).
//! - [`bonds`]: bond issues and buybacks, convertible pricing, stock split.
//! - [`stock`]: stock offerings, buybacks, greenmail, extraordinary dividends, write-offs, reserves.
//! - [`loans`]: bank loan trading (`BuySellLoans` and its dialog), borrowing/repaying, lending bank,
//!   income-tax prepayment.
//! - [`lending`]: advances and loan calls, freezing credit, corporate policy, autopilot.
//! - [`misc`]: ETF adviser, lawsuits, rumors, CEO, company name.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::if_same_then_else,
    clippy::needless_late_init
)]


pub mod bank;
pub mod bonds;
pub mod lending;
pub mod loans;
pub mod misc;
pub mod stock;

/// Common imports of the submodules (the same as `sim_entity`'s).
pub(crate) mod prelude {

    #[allow(unused_imports)]
    pub(crate) use crate::logic::sim_entity::prelude::*;
}
