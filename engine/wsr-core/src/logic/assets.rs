//! Buying, selling and offering business assets; start-up companies (player actions):
//! `BuyBusinessAssets` (0x5d2f04, [`buy`]), `SellBusinessAssets` (0x5d8ab9, [`sell`]),
//! `ShowAssetSaleOffers` (0x666a27) and `OfferAssetsForSale` (0x6699ec) ([`offers`]),
//! `StartupCompany` (0x5af020, [`startup`]).
//!
//! The GOSUB blocks of these SUBs run on the caller's frame; they are ported as nested fns / closures over
//! the routine's locals. The `*_skipping` variants take a list of routine addresses not to call (tests:
//! `ShowTextReport` 0x41dfaa, so that a pick-list result such as `TargetCorp` can be preset, the way the
//! differential tests skip the call in the original).

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


pub mod buy;
pub mod offers;
pub mod sell;
pub mod startup;

/// `ShowTextReport` (0x41dfaa): the address a `*_skipping` caller may list.
pub const SHOW_TEXT_REPORT: u32 = 0x41dfaa;

/// Common imports of the submodules.
pub(crate) mod prelude {
use crate::pb::real::{Ext, cast};

    #[allow(unused_imports)]
    pub(crate) use crate::logic::news::post_news;
    #[allow(unused_imports)]
    pub(crate) use crate::logic::report::show_quick_message;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::format;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::{str_num, val};
    #[allow(unused_imports)]
    pub(crate) use crate::pb::pbstr::PbStr;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::rng::Rng;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::string::{
        instr, len, ltrim, mid, parse_any, remove, remove_any, replace, right, rtrim, string_fill,
    };
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::{fix, int, to_integer, to_long, to_quad};
    #[allow(unused_imports)]
    pub(crate) use crate::state::Globals;
    #[allow(unused_imports)]
    pub(crate) use crate::ui::Ui;

    /// A SINGLE constant of the original's pool (`fmul dword [..]`): the f32 value, widened.
    #[allow(dead_code)]
    pub(crate) fn sf(x: f32) -> Ext {
        cast::<Ext, _>(x)
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

    /// `RTRIM$(LTRIM$(STR$(x)))`.
    #[allow(dead_code)]
    pub(crate) fn str_trim(x: Ext) -> PbStr {
        trim(str_num(x, 15).as_bytes())
    }

    /// `RTRIM$(LTRIM$(s))`.
    #[allow(dead_code)]
    pub(crate) fn trim(s: &[u8]) -> PbStr {
        rtrim(ltrim(s).as_bytes())
    }

    /// `VAL(REMOVE$(s, ANY chars))`.
    #[allow(dead_code)]
    pub(crate) fn val_removing(s: &PbStr, chars: &[u8]) -> Ext {
        val(remove_any(s.as_bytes(), chars).as_bytes())
    }
}
