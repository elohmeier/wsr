//! AI c [runtime resource wording] , physical commodities, commodity ETFs and position limits.
//!
//! Submodules:
//! - [`limits`]: exchange limits and market impact (`CalcCommodTradeLimit`, `CalcFuturesPositionRoom`,
//!   `ApplyFuturesTradeImpact`, `EnforceCommodityPositionLimits`).
//! - [`physical`]: [runtime resource wording]  crypto (`BuyPhysCommod`, `SellPhysCommod`,
//!   `AiTradePhysCommod`, `ChargeCommodStorageCosts`).
//! - [`futures_ai`]: futures expiry and the AI futures traders (`SettleExpiredFutures`,
//!   `AiFuturesTrading`, `CommodityEtfTrading`, `LeveragedEtfFuturesUpdate`).
//! - [`desk`]: the commodity desk's futures orders (`FuturesBuyOrCover`, `FuturesSellOrShort`).
//!
//! Records as in [`crate::logic::futures`]: `FuturesRecs(i)` = `"COMMODITY|owner|qty|month|year|cost|price"`,
//! `PhysCommodRec(i)` = `"COMMODITY|owner|units|cost"`.
//!
//! Numeric conventions as in `logic::futures`: `EXT` uses software x87 arithmetic, SINGLE stores are rounded with `as f32`,
//! SINGLE pool constants of the original (`fmul dword [0x75e…]`) are written `sf(0.01)`. Values that
//! end up in `STR$` with 15 digits or decide a `FISTP` tie are kept in 80 bits ([`crate::pb::ext80::Ext80`]).


pub mod desk;
pub mod futures_ai;
pub mod limits;
pub mod physical;

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
        CaseMode, instr, lcase, len, ltrim, mcase, mid, parse_any, remove, remove_any, replace, right,
        rtrim, ucase,
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

    /// `PbStr` from a literal.
    #[allow(dead_code)]
    pub(crate) fn s(x: &str) -> PbStr {
        PbStr::from(x)
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

    /// `RTRIM$(LTRIM$(STR$(x)))` (15 digits: LONG/QUAD/EXT/DOUBLE).
    #[allow(dead_code)]
    pub(crate) fn str_trim(x: Ext) -> PbStr {
        trim(str_num(x, 15).as_bytes())
    }

    /// `STR$(x)` (15 digits).
    #[allow(dead_code)]
    pub(crate) fn str_s(x: Ext) -> PbStr {
        PbStr::from(str_num(x, 15).as_str())
    }

    /// `PARSE$(rec, ANY "|", n)`.
    #[allow(dead_code)]
    pub(crate) fn field(rec: &PbStr, n: i32) -> PbStr {
        parse_any(rec.as_bytes(), b"|", n)
    }

    /// `VAL(PARSE$(rec, ANY "|", n))`.
    #[allow(dead_code)]
    pub(crate) fn field_val(rec: &PbStr, n: i32) -> Ext {
        val(field(rec, n).as_bytes())
    }

    /// `LCASE$(x)`.
    #[allow(dead_code)]
    pub(crate) fn lc(x: &PbStr) -> PbStr {
        lcase(x.as_bytes(), CaseMode::Ansi)
    }

    /// `MCASE$(x)`.
    #[allow(dead_code)]
    pub(crate) fn mc(x: &PbStr) -> PbStr {
        mcase(x.as_bytes(), CaseMode::Ansi)
    }

    /// `UCASE$(x)`.
    #[allow(dead_code)]
    pub(crate) fn uc(x: &PbStr) -> PbStr {
        ucase(x.as_bytes(), CaseMode::Ansi)
    }

    /// `x = "BITCOIN" OR x = "ETHEREUM"`.
    #[allow(dead_code)]
    pub(crate) fn is_crypto(x: &PbStr) -> bool {
        *x == "BITCOIN" || *x == "ETHEREUM"
    }

    #[allow(unused_imports)]
    pub(crate) use crate::pb::ext80::Ext80;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::{str_num_ext, val_ext};

    /// `RTRIM$(LTRIM$(STR$(x)))` of an 80-bit value.
    #[allow(dead_code)]
    pub(crate) fn str_trim_ext(x: Ext80) -> PbStr {
        trim(str_num_ext(x, 15).as_bytes())
    }
}
