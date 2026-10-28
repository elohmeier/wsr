//! Mergers, liquidations of subsidiaries, LBO tender offers, stock-for-stock mergers, capital contributions.
//!
//! * [`merge`]: `MergeCompanies` (0x509293) with its GOSUB blocks (`Merger_*`), `AdjustOptionsForMerger`,
//!   `DisableMainControls`.
//! * [`liquidate_sub`]: `LiquidateSubIntoParent` (0x512aa9) with its GOSUB blocks (`LiquidateSub_*`) and the
//!   transfers of futures, [runtime resource wording]  ETF contracts.
//! * [`capital`]: `ContributeCapitalToSub` (0x596b50), its dialog and `UpdateCtrlSinceInMiscData`;
//!   [`dlg_util`]: the generic dialog/tooltip helpers it uses.
//! * [`tender`]: `LboTenderOffer` (0x51f21c), `NegotiatePremiumBuyout` (0x528aef).
//! * [`menus`]: the player menus `StockForStockMerger` (0x5a8815) and `LiquidateSubsidiary` (0x5a7387).
//!
//! GOSUB blocks run on the caller's stack frame: they are ported as private fns over the routine's locals.
//! SINGLE pool constants of the original (`fmul dword [..]`) are written `sf(0.05)`.

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


pub mod capital;
pub mod dlg_util;
pub mod liquidate_sub;
pub mod menus;
pub mod merge;
pub mod tender;

#[allow(unused_imports)]
pub use capital::*;
#[allow(unused_imports)]
pub use dlg_util::*;
pub use liquidate_sub::*;
#[allow(unused_imports)]
pub use menus::*;
pub use merge::*;
#[allow(unused_imports)]
pub use tender::*;

/// Common imports of the submodules.
pub(crate) mod prelude {
use crate::pb::real::{Ext, cast};

    #[allow(unused_imports)]
    pub(crate) use crate::logic::news::post_news;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::format;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::{str_num, val};
    #[allow(unused_imports)]
    pub(crate) use crate::pb::pbstr::PbStr;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::rng::Rng;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::string::{instr, instr_any, len, ltrim, mid, parse_any, replace, right, rtrim};
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::{int, to_integer, to_long, to_quad};
    #[allow(unused_imports)]
    pub(crate) use crate::state::Globals;
    #[allow(unused_imports)]
    pub(crate) use crate::ui::Ui;

    /// A SINGLE constant of the original's pool (`fmul dword [..]`): the f32 value, widened.
    #[allow(dead_code)]
    pub(crate) fn sf(x: f32) -> Ext {
        cast::<Ext, _>(x)
    }

    /// A string literal.
    #[allow(dead_code)]
    pub(crate) fn s(x: &str) -> PbStr {
        PbStr::from(x)
    }

    /// `FORMAT$(x, mask)`.
    #[allow(dead_code)]
    pub(crate) fn fmt(x: Ext, mask: &[u8]) -> PbStr {
        PbStr(format(x, mask))
    }

    /// `RTRIM$(LTRIM$(s))`.
    #[allow(dead_code)]
    pub(crate) fn trim(s: &[u8]) -> PbStr {
        rtrim(ltrim(s).as_bytes())
    }

    /// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
    #[allow(dead_code)]
    pub(crate) fn fmt_trim(x: Ext, mask: &[u8]) -> PbStr {
        trim(&format(x, mask))
    }

    /// `STR$(x)` of a LONG/QUAD/EXT value.
    #[allow(dead_code)]
    pub(crate) fn str_s(x: Ext) -> PbStr {
        PbStr::from(str_num(x, 15).as_str())
    }
}
