//! AI disposal of holdings and AI option trades: `AiSellOrMergeHolding` (0x4c64ef),
//! `AiCreateOptionPosition` (0x5fef2c) and `FindOptionCounterparty` (0x602925).
//!
//! SINGLE pool constants are written `sf(..)`; every float → integer store uses the x87 rounding helpers.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;

pub use super::etf::{ai_create_option_position, find_option_counterparty};
use super::transactions::sell_stock_holding;
use crate::logic::control::update_corp_control;
use crate::logic::holdings::find_holding_slot;
use crate::logic::reports_corp::divest_stock;

/// Port of AiSellOrMergeHolding (0x4c64ef).
/// The AI disposes of `holder_id`'s holding of `held_id`: a 100% subsidiary (difficulty above 1, holder
/// not a fund) is divested ([`divest_stock`], reason 4); otherwise, if cash plus T-bills exceed ten times
/// `MarketCap × pct` (the stake's value in thousands), the stake is sold on the market
/// ([`sell_stock_holding`] mode 2; more than 50% of a non-fund holder sells only 20..40 points), else the
/// holder writes calls on 6..10% (below the public float) of the stock ([`ai_create_option_position`]).
pub fn ai_sell_or_merge_holding(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    holder_id: i32,
    held_id: i32,
) {
    let slot = find_holding_slot(g, holder_id, held_id);
    if cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]) == 100
        && 1 < g.difficulty_level
        && cast::<i32, _>(g.industry_num[holder_id]) != 71
    {
        divest_stock(g, ui, rng, held_id, holder_id, 4);
        return;
    }
    if 0 < cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]) {
        let held_pct = cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]);
        let liquid = g.t_bills[holder_id] + g.cash[holder_id];
        let stake = cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]) * g.market_cap[held_id] * Ext::from_f64(10.0);
        if !(stake >= liquid) {
            g.tmp_ext1 = cast::<Ext, _>(holder_id);
            g.tmp_ext2 = cast::<Ext, _>(held_pct);
            if g.tmp_ext2 > Ext::from_f64(50.0) && cast::<i32, _>(g.industry_num[holder_id]) != 71 {
                g.tmp_ext2 = cast::<Ext, _>(rng.rnd_range(40, 20));
            }
            g.tmp_ext3 = Ext::from_f64(2.0);
            sell_stock_holding(g, ui, rng, held_id);
        } else {
            update_corp_control(g, ui, rng, held_id);
            if (cast::<i32, _>(g.public_pct)) < 2 {
                return;
            }
            let mut opt_pct = rng.rnd_range(10, 6);
            if cast::<i32, _>(g.public_pct) <= opt_pct {
                opt_pct = cast::<i32, _>(g.public_pct) - 1;
            }
            ai_create_option_position(g, ui, rng, holder_id, held_id, opt_pct, 2, 0);
        }
    }
}
