//! Scheduled corporate AI decisions (spin-offs, recapitalisations, splits, bank actions, ...).
//!
//! Submodules: [`spinoff`] (spin-offs of subsidiaries, player dialog and AI), [`capital`] (stock splits,
//! recapitalisation, AI stock buying, earnings estimates, bond issues), [`distress`] (bank bailouts,
//! distressed assets, covenants, AI holding-company moves), [`misc`] (small quarterly bookkeeping
//! routines). `CalcStockPrice_42c72a` (0x42c72a) is the routine already ported as
//! [`crate::logic::valuation::calc_stock_price`] (same address), re-exported here.
//!
//! SINGLE pool constants of the original (`fmul dword [0x75e…]`) are written `sf(0.05)`.


pub mod capital;
pub mod distress;
pub mod misc;
pub mod spinoff;

pub use crate::logic::valuation::calc_stock_price;

/// Common imports of the submodules.
#[allow(unused_imports)]
pub(crate) mod prelude {
use crate::pb::real::{Ext, cast};

    pub(crate) use crate::logic::news::post_news;
    pub(crate) use crate::pb::numeric::format;
    pub(crate) use crate::pb::numeric::{str_num, val};
    pub(crate) use crate::pb::pbstr::PbStr;
    pub(crate) use crate::pb::rng::Rng;
    pub(crate) use crate::pb::string::{
        instr, instr_any, len, ltrim, mid, parse_any, replace, right, rtrim,
    };
    pub(crate) use crate::pb::numeric::{int, to_integer, to_long, to_quad};
    pub(crate) use crate::state::Globals;
    pub(crate) use crate::ui::Ui;

    /// A SINGLE constant of the original's pool (`fmul dword [..]`): the f32 value, widened.
    #[allow(dead_code)]
    pub(crate) fn sf(x: f32) -> Ext {
        cast::<Ext, _>(x)
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
