//! Random corporate news events, merger/takeover events, antitrust and lawsuit processing.
//!
//! Submodules: [`random_news`] (`RandomCorpNewsEvent`, `AdjustProfitMarginWithMisc`), [`merger`]
//! (`MergerTakeoverEvents`, `InsiderTradingProbeVerdict`), [`antitrust_suit`] (the private antitrust
//! suit: `AntitrustSuitTick`, `AntitrustSuitTrial`, `AntitrustCaseRuling`), [`antitrust`] (suit queue,
//! plaintiffs, damages, government enforcement and merger review) and [`lawsuits`]
//! (`ProcessMaliciousLawsuits`).

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


pub mod antitrust;
pub mod antitrust_suit;
pub mod lawsuits;
pub mod merger;
pub mod random_news;

pub use antitrust::*;
pub use antitrust_suit::*;
pub use lawsuits::*;
pub use merger::*;
pub use random_news::*;

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
    pub(crate) use crate::pb::string::{
        instr, instr_any, len, ltrim, mid, parse_any, replace, right, rtrim,
    };
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

    /// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
    #[allow(dead_code)]
    pub(crate) fn fmt_trim(x: Ext, mask: &[u8]) -> PbStr {
        trim(&format(x, mask))
    }

    /// `FORMAT$(x, mask)`.
    #[allow(dead_code)]
    pub(crate) fn fmt(x: Ext, mask: &[u8]) -> PbStr {
        PbStr(format(x, mask))
    }

    /// `STR$(x)` of a LONG/QUAD/EXT value.
    #[allow(dead_code)]
    pub(crate) fn str_s(x: Ext) -> PbStr {
        PbStr::from(str_num(x, 15).as_str())
    }

    /// `RTRIM$(LTRIM$(s))`.
    #[allow(dead_code)]
    pub(crate) fn trim(s: &[u8]) -> PbStr {
        rtrim(ltrim(s).as_bytes())
    }
}
