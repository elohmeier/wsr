//! Industry outlook texts: `BuildIndustryOutlookText` (0x56f33b) builds `IndOutlookText`, the analyst
//! commentary on an industry (interest rates, GDP, oil and metal prices, the subprime crisis, regional
//! factors, long-term growth, special taxes), with its sentence builders `IndOutlook_GrowthCommentary`
//! (0x583605), `IndOutlook_SlowEconomyCommentary` (0x586441) and `IndOutlook_CostCommentary` (0x5878e4).
//!
//! The 78 KB routine is split into consecutive pieces that share its locals ([`Ctx`]):
//! `financial` (banks, insurers, holding companies, funds), `head` (the first paragraph of an operating
//! industry), `detail` (industry-specific cost/price factors) and `tail` (fads, long-term growth,
//! forecast, special taxes, tech-bubble aftermath).
//!
//! Numeric conventions as in `logic::research`: SINGLE locals are `f32`, SINGLE pool constants are
//! written `sf(x)`, float → integer stores use `to_long`/`to_integer`, comparisons keep the branch
//! taken for unordered operands.
use crate::pb::real::{Ext, cast};


mod commentary;
mod detail;
mod financial;
mod head;
mod tail;

pub use commentary::{ind_outlook_cost_commentary, ind_outlook_growth_commentary, ind_outlook_slow_economy_commentary};

use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::state::Globals;
use crate::ui::Ui;

pub(crate) fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// A SINGLE constant of the original (`dword [0x75e…]`), widened to EXT.
#[allow(dead_code)]
pub(crate) fn sf(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

/// The locals of `BuildIndustryOutlookText` (names from tools/locals.tsv).
#[derive(Default, Debug, Clone)]
pub(crate) struct Ctx {
    /// `industryId` (INTEGER copy of the BYREF argument).
    pub industry_id: i16,
    /// `industryId + CurQuarter` (INTEGER, stored at the start; picks the opening phrase).
    pub ind_plus_q: i16,
    pub top_corp_id: i32,
    pub cur_slack_pct: f32,
    pub proj_slack_pct: f32,
    pub proj_supply: Ext,
    pub net_margin: f32,
    pub proj_margin: f32,
    pub month_val: Ext,
    pub month_num: i32,
    pub top_corp_share_pct: i32,
    /// `PriceTrendDirection(1604, 3, 3)` (GDP growth trend), stored at the start ([ebp-0x118]).
    pub gdp_trend: i32,
    pub ind_name: PbStr,
    pub ind_name_lc: PbStr,
    pub growth_trend: i32,
    pub outlook_rating: i32,
    pub trend_val: i32,
    pub price_streak: i32,
    pub price_trend: i32,
    pub pct_change: i32,
    pub pct_change_a: i32,
    pub pct_change_b: i32,
    pub cond_flag: i32,
    pub comment_count: i32,
    pub rand_pick: i32,
    pub trend_dir: i32,
    pub body_text: PbStr,
    pub head_text: PbStr,
    pub phrase_text: PbStr,
    pub para_text: PbStr,
    pub clause_text: PbStr,
    pub recovery_text: PbStr,
    pub cmp_val: i32,
}

/// Port of BuildIndustryOutlookText (0x56f33b).
/// Builds `IndOutlookText` for `industry_arg` (INTEGER part of the BYREF LONG): banks and insurers (rates,
/// stock market, subprime crisis), holding companies and funds, and for operating industries
/// (3..69) several paragraphs on profitability, supply/demand, the economy, commodity prices,
/// industry-specific factors, long-term growth, forecasts and special taxes (if `SelEntity` > 10).
/// Draws RND; may call `UpdateMetalsPriceBase`, `CalcEntityFinancials(SelEntity)` and `CalcCapitalTaxes`.
pub fn build_industry_outlook_text(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, industry_arg: i32) {
    let mut c = Ctx { industry_id: cast::<i16, _>(industry_arg), ..Ctx::default() };
    c.ind_plus_q = cast::<i16, _>(cast::<i32, _>(c.industry_id) + g.cur_quarter);
    g.ind_outlook_text = PbStr::new();
    if c.industry_id < 3 || c.industry_id == 70 || c.industry_id == 71 {
        financial::financial_industries(g, ui, rng, &mut c);
        return;
    }
    if !head::first_paragraph(g, ui, rng, &mut c) {
        return;
    }
    detail::industry_detail(g, ui, rng, &mut c);
    tail::tail(g, ui, rng, &mut c);
}
