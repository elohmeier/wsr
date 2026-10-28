//! Price / commodity / rate alerts (`SetPriceAlert` with its alert-type dialog) and the bank asset
//! allocation dialog (`ShowDlgBankAssetAlloc`).

pub mod price_alerts;
pub mod bank_alloc;

pub use bank_alloc::*;
pub use price_alerts::*;

use crate::pb::pbstr::PbStr;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}
