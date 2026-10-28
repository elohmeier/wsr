//! Stock, option, short, bond, T-bill and futures trading actions; company pick lists and report links.
//!
//! * [`select`]: report links and selection — `FollowReportLink` (0x46c9f4), `SelectActingEntity`
//!   (0x4721dc), `LookupSymbol` (0x422b3b), `ShowCompanyPickList` (0x5825fc), `PickFromStreamingList`
//!   (0x5de64f), `GetSelectedPlayerEtf` (0x652b17), `SelectIndustryComboItem` (0x46df33).
//! * [`stock`]: the stock trade primitives `BuyStock` (0x4efe60), `CalcTakeoverPremiumPrice` (0x528736),
//!   the player's `SellStock` (0x5ab449) and `PrivateBuyoutOffer` (0x5bcd00).
//! * [`borrow`]: `BorrowToCoverPurchase` (0x5246b6), the financing question of every purchase.
//! * [`buy_menu`]: the player's stock purchase `BuyStockMenu` (0x58d471) and its GOSUBs.
//! * [`options_menu`]: `BuyOptions` (0x5e340d), `ShowOptionChain` (0x5f0388),
//!   `CheckCircularOptionHolding` (0x606e94).
//! * [`options_sell`]: SellOptions (0x5e95d8), ExerciseOptionsMenu (0x600b42).
//! * [`short_menu`]: `SellShort` (0x5e0ea7), `CoverShortPosition` (0x5e2654).
//! * [`bonds`]: `TradeCorpBonds` (0x5b6dbe), `TradeTBills` (0x69af92).
//! * [`futures_trade`]: `SelectFuturesContract` (0x624c1b), `FindFuturesCounterparty` (0x63389c).
//!
//! Player dialogs go through [`crate::ui::Ui`] only (`msgbox`, `inputbox`, `report` pick lists with
//! `@C`/`@I` links, `choose` for custom option dialogs). Window plumbing of the original (labels, focus,
//! enabling controls) is not ported; each such spot is marked `(UI)`.


pub mod select;
pub mod stock;
pub mod borrow;
pub mod buy_menu;
pub mod options_menu;
pub mod options_sell;
pub mod short_menu;
pub mod bonds;
pub mod futures_trade;

pub use borrow::borrow_to_cover_purchase;
pub use options_menu::{buy_options, check_circular_option_holding, show_option_chain};
pub use select::{
    follow_report_link, get_selected_player_etf, lookup_symbol, pick_from_streaming_list,
    select_acting_entity, select_industry_combo_item, show_company_pick_list,
};
pub use stock::{buy_stock, calc_takeover_premium_price, private_buyout_offer, sell_stock};

/// Shared imports and small helpers of the trading routines.
#[allow(unused_imports)]
pub(crate) mod prelude {
use crate::pb::real::{Ext, cast};

    pub(crate) use crate::logic::news::post_news;
    pub(crate) use crate::logic::report::{show_quick_message, show_text_report};
    pub(crate) use crate::pb::numeric::format;
    pub(crate) use crate::pb::numeric::{str_num, val};
    pub(crate) use crate::pb::pbstr::PbStr;
    pub(crate) use crate::pb::rng::Rng;
    pub(crate) use crate::pb::string::{
        instr, instr_any, lcase, left, len, ltrim, mid, parse_any, remove, remove_any, replace, right,
        rtrim, space, string_fill, ucase, CaseMode,
    };
    pub(crate) use crate::pb::numeric::{ceil, int, to_integer, to_long, to_quad};
    pub(crate) use crate::state::Globals;
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

    /// `RTRIM$(LTRIM$(s))`.
    #[allow(dead_code)]
    pub(crate) fn trim(x: &[u8]) -> PbStr {
        rtrim(ltrim(x).as_bytes())
    }

    /// `FORMAT$(x, mask)`.
    #[allow(dead_code)]
    pub(crate) fn fmt(x: Ext, mask: &str) -> PbStr {
        PbStr(format(x, mask.as_bytes()))
    }

    /// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
    #[allow(dead_code)]
    pub(crate) fn fmt_trim(x: Ext, mask: &str) -> PbStr {
        trim(&format(x, mask.as_bytes()))
    }

    /// `STR$(x)` of a LONG/QUAD/EXT value (15 digits).
    #[allow(dead_code)]
    pub(crate) fn str_s(x: Ext) -> PbStr {
        PbStr::from(str_num(x, 15).as_str())
    }

    /// `RTRIM$(LTRIM$(STR$(x)))`.
    #[allow(dead_code)]
    pub(crate) fn str_trim(x: Ext) -> PbStr {
        trim(str_num(x, 15).as_bytes())
    }

    /// `RIGHT$("000" + RTRIM$(LTRIM$(STR$(id))), 4)`: the 4-digit id of a `@C` link code.
    #[allow(dead_code)]
    pub(crate) fn link_id(id: i32) -> PbStr {
        let t = s("000") + &str_trim(cast::<Ext, _>(id));
        right(t.as_bytes(), 4)
    }

    /// `x MOD y` on the x87 (`PB_FMod` 0x6be6ef, `fprem`): truncating remainder.
    #[allow(dead_code)]
    pub(crate) fn fmod(x: Ext, y: Ext) -> Ext {
        x % y
    }
}
