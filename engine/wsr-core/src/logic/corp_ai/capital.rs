//! Capital-structure moves of the scheduled corporate AI: stock splits and share-count rounding
//! (`ProcessStockSplitsAll`), the bond-for-bond recapitalisation of weak issuers
//! (`AiBondRecapitalization`), AI stock purchases and liquidity dividend raises (`AiCorpBuysStock`),
//! analysts' earnings-estimate revisions (`ReviseEarningsEstimates`) and random bond issues
//! (`AiRandomBondIssue`).
//!
//! SINGLE pool constants of the original (`fmul dword [0x75e…]`) are written `sf(0.5)`.

#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::if_same_then_else,
    clippy::float_cmp
)]
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;

use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::holdings::find_holding_slot;
use crate::logic::market::rebuild_streaming_quotes_panel;
use crate::logic::news::is_in_recent_headlines;
use crate::logic::reports_lists::update_streaming_quotes;
use crate::logic::sim_entity::asset_sales::ask_autopilot_permission;
use crate::logic::sim_entity::bonds::{count_bond_issuers, issue_bonds};
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::trading::buy_stock;
use crate::logic::valuation::calc_dividend_yield;

/// The share-count rounding ladder of `ProcessStockSplitsAll`: a count in `lo..=hi` becomes `value`
/// (first match wins). The bounds and values that are not whole numbers are SINGLE pool constants.
#[allow(non_snake_case)]
fn SHARE_LADDER(__content: &crate::content::ContentPack) -> [(f32, f32, f32); 32] { [
    (__content.long(14466) as f32, __content.long(14467) as f32, __content.long(14468) as f32),
    (__content.long(14469) as f32, __content.single(14470), __content.long(14471) as f32),
    (__content.long(14472) as f32, __content.single(14473), __content.long(14474) as f32),
    (__content.long(14475) as f32, __content.single(14476), __content.long(14477) as f32),
    (__content.long(14478) as f32, __content.single(14479), __content.long(14480) as f32),
    (__content.long(14481) as f32, __content.single(14482), __content.long(14483) as f32),
    (__content.long(14484) as f32, __content.single(14485), __content.single(14486)),
    (__content.long(14487) as f32, __content.single(14488), __content.long(14489) as f32),
    (__content.long(14490) as f32, __content.single(14491), __content.long(14492) as f32),
    (__content.long(14493) as f32, __content.single(14494), __content.long(14495) as f32),
    (__content.long(14496) as f32, __content.single(14497), __content.long(14498) as f32),
    (__content.long(14499) as f32, __content.single(14500), __content.long(14501) as f32),
    (__content.long(14502) as f32, __content.single(14503), __content.long(14502) as f32),
    (__content.long(14504) as f32, __content.single(14505), __content.long(14506) as f32),
    (__content.long(14507) as f32, __content.single(14508), __content.long(14509) as f32),
    (__content.single(14510), __content.single(14511), __content.long(14512) as f32),
    (__content.single(14513), __content.single(14514), __content.single(14510)),
    (__content.single(14515), __content.single(14516), __content.single(14517)),
    (__content.single(14518), __content.single(14519), __content.long(14520) as f32),
    (__content.single(14521), __content.single(14522), __content.long(14523) as f32),
    (__content.long(14524) as f32, __content.single(14525), __content.single(14521)),
    (__content.long(14526) as f32, __content.single(14527), __content.single(14528)),
    (__content.single(14529), __content.single(14530), __content.long(14531) as f32),
    (__content.single(14532), __content.single(14533), __content.single(14534)),
    (__content.single(14535), __content.single(14536), __content.single(14537)),
    (__content.single(14538), __content.single(14539), __content.single(14535)),
    (__content.single(14540), __content.single(14541), __content.single(14538)),
    (__content.single(14542), __content.single(14543), __content.single(14544)),
    (__content.single(14545), __content.single(14546), __content.single(14547)),
    (__content.single(14548), __content.single(14549), __content.single(14550)),
    (__content.single(14551), __content.single(14552), __content.single(14553)),
    (__content.single(14554), __content.single(14555), __content.single(14556)),
] }

/// Port of ProcessStockSplitsAll (0x4afc92).
/// For every active corp 11..1600: clamps tiny share counts (0.01 with "M" units, 0.001 with "B"), then
/// — unless the corp is managed by a human player at a price 3..5000, or is BKH/GOOG/AAPL/AMZN inside
/// its own price band — splits 2/3/4-for-1 when the price exceeds `RND * 125 + 75` (one corp in five, or
/// always for the selected corp or the current player's corp; the split ends the whole routine, with a
/// headline posted only for the current player's corp). Otherwise cheap stocks get fewer shares (fixed
/// counts for caps under 4.5, 6..10, 17..80), a 1-for-10 reverse split (one in three of the caps over
/// 125 trading under `minPrice`; headline unless a "reverse stock" headline is recent and it is not the
/// player's corp) or a cut to 20% of the shares (price under `RND * 0.4 + 0.5`, funds under 12).
/// Every non-split corp then has its share count rounded to a round number ([`SHARE_LADDER`]); the
/// dividend per share follows the change, and a corp on the current player's watch list refreshes the
/// streaming quotes.
pub fn process_stock_splits_all(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("SplitStock") (error path not ported)
    for corp_id in 11..=1600 {
        if g.entity_status[corp_id] != 1 {
            continue;
        }
        // minPrice = RND * 5 + 10 (always overwritten below before it is read)
        let mut min_price = rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(10.0);
        if g.shares_outstanding[corp_id] < sf(0.01f32) && g.million_prefix_uc == "M" {
            g.shares_outstanding[corp_id] = sf(0.01f32);
        }
        if g.shares_outstanding[corp_id] < sf(0.001f32) && g.million_prefix_uc == "B" {
            g.shares_outstanding[corp_id] = sf(0.001f32);
        }
        let old_shares = g.shares_outstanding[corp_id];
        // DOUBLE local
        let mut price = crate::pb::numeric::double(g.market_cap[corp_id] / g.shares_outstanding[corp_id]);
        let sym = g.ent_symbol[corp_id].clone();
        let mut skip = false;
        if get_managing_player(g, corp_id) > 0 && Ext::from_f64(3.0) < price && Ext::from_f64(5000.0) > price {
            skip = true;
        } else if sym == "BKH" && Ext::from_f64(2500.0) < price && Ext::from_f64(5000.0) > price {
            skip = true;
        } else if sym == "GOOG" && Ext::from_f64(1000.0) < price && Ext::from_f64(2000.0) > price {
            skip = true;
        } else if sym == "AAPL" && Ext::from_f64(800.0) < price && Ext::from_f64(1750.0) > price {
            skip = true;
        } else if sym == "AMZN" && Ext::from_f64(600.0) < price && Ext::from_f64(1200.0) > price {
            skip = true;
        }
        if !skip {
            let _ = min_price;
            min_price = Ext::from_f64(50.0);
            if !(sym == "BKH" || sym == "GOOG" || sym == "AAPL" || sym == "AMZN") {
                min_price = rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(10.0);
            }
            if !(rng.rnd() * Ext::from_f64(125.0) + Ext::from_f64(75.0) >= price) {
                let mut split_ratio = 2;
                if !(Ext::from_f64(125.0) >= price) {
                    split_ratio = 3;
                }
                if !(Ext::from_f64(180.0) >= price) {
                    split_ratio = 4;
                }
                if rng.rnd() * Ext::from_f64(5.0) < Ext::from_f64(1.0)
                    || corp_id == g.sel_entity
                    || g.cur_player == g.corp_controller[corp_id]
                {
                    g.shares_outstanding[corp_id] = cast::<Ext, _>(split_ratio) * g.shares_outstanding[corp_id];
                    if split_ratio > 0 {
                        g.dividend_per_share[corp_id] = g.dividend_per_share[corp_id] / cast::<Ext, _>(split_ratio);
                    }
                    g.news_msg = g.ent_name[corp_id].clone()
                        + " ("
                        + &g.ent_symbol[corp_id]
                        + __content.string(2795)
                        + &str_s(cast::<Ext, _>(split_ratio))
                        + __content.string(2796);
                    if g.cur_player == g.corp_controller[corp_id] {
                        post_news(g, ui, 0, 0);
                    }
                    return;
                }
            } else {
                // DOUBLE local
                let mut new_shares = crate::pb::numeric::double(g.shares_outstanding[corp_id]);
                let cap_val = g.market_cap[corp_id];
                if !(cap_val >= sf(4.5f32)) {
                    if !(sf(0.6f32) >= new_shares) {
                        new_shares = crate::pb::numeric::double(sf(0.25f32));
                    }
                } else if !(cap_val < Ext::from_f64(6.0)) && !(cap_val > Ext::from_f64(10.0)) {
                    if !(Ext::from_f64(1.0) >= new_shares) {
                        new_shares = crate::pb::numeric::double(sf(0.5f32));
                    }
                } else if !(cap_val < Ext::from_f64(17.0)) && !(cap_val > Ext::from_f64(80.0)) {
                    if !(Ext::from_f64(2.0) >= new_shares) {
                        new_shares = crate::pb::numeric::double(Ext::from_f64(1.0));
                    }
                }
                g.shares_outstanding[corp_id] = new_shares;
                price = crate::pb::numeric::double(g.market_cap[corp_id] / g.shares_outstanding[corp_id]);
                let mut reverse_done = false;
                if !(min_price <= price) && !(g.market_cap[corp_id] <= Ext::from_f64(125.0)) && !(rng.rnd() * Ext::from_f64(3.0) >= Ext::from_f64(1.0)) {
                    reverse_done = true;
                    new_shares = crate::pb::numeric::double(sf(0.1f32) * new_shares);
                    g.shares_outstanding[corp_id] = new_shares;
                    if !(g.shares_outstanding[corp_id] <= Ext::from_f64(23.0)) && !(g.shares_outstanding[corp_id] >= Ext::from_f64(27.0)) {
                        g.shares_outstanding[corp_id] = Ext::from_f64(25.0);
                    }
                    g.news_msg = g.ent_name[corp_id].clone()
                        + " ("
                        + &g.ent_symbol[corp_id]
                        + __content.string(2797);
                    let mut news_corp = 0;
                    if is_in_recent_headlines(g, __content.bytes(2798)) == 0 {
                        news_corp = corp_id;
                    }
                    if g.cur_player == g.corp_controller[corp_id] {
                        news_corp = corp_id;
                    }
                    if news_corp > 0 {
                        post_news(g, ui, news_corp, 0);
                    }
                }
                if !reverse_done {
                    if !(rng.rnd() * sf(0.4f32) + sf(0.5f32) <= price) {
                        g.shares_outstanding[corp_id] = sf(0.2f32) * g.shares_outstanding[corp_id];
                    } else if !(Ext::from_f64(12.0) <= price) && g.industry_num[corp_id] == 71 {
                        g.shares_outstanding[corp_id] = sf(0.2f32) * g.shares_outstanding[corp_id];
                    }
                }
            }
        }

        // ---- 0x4b0849: round the share count
        let x = g.shares_outstanding[corp_id];
        if let Some(&(_, _, v)) = SHARE_LADDER(&__content)
            .iter()
            .find(|&&(lo, hi, _)| !(x < cast::<Ext, _>(lo)) && !(x > cast::<Ext, _>(hi)))
        {
            g.shares_outstanding[corp_id] = cast::<Ext, _>(v);
        }
        if old_shares != g.shares_outstanding[corp_id] && g.shares_outstanding[corp_id] > Ext::from_f64(0.0) {
            // DOUBLE local
            let ratio = crate::pb::numeric::double(old_shares / g.shares_outstanding[corp_id]);
            g.dividend_per_share[corp_id] = ratio * g.dividend_per_share[corp_id];
        }
        let mut not_watched = 1;
        let mut watch_idx = 1;
        while watch_idx < 16 {
            if corp_id == g.watch_list[(watch_idx, g.cur_player)] {
                not_watched = 0;
                watch_idx = 15;
            }
            watch_idx += 1;
        }
        if not_watched != 1 && old_shares != g.shares_outstanding[corp_id] {
            update_streaming_quotes(g, ui, rng, corp_id - 1, corp_id + 1);
            rebuild_streaming_quotes_panel(g, ui, 2);
        }
    }
}

/// Port of AiBondRecapitalization (0x4b8da9).
/// An active, not human-controlled issuer with at least 25 bonds outstanding (over 2 years to maturity,
/// credit rating under 4, liquid funds at most 70% of the issue's market value, coupon up to 10.65, a
/// convertible only when out of the money) offers its bondholders new 5-year bonds: 1 new for 2 old at
/// twice the coupon, or 2 for 3 at 1.5x (3-for-2 when the net worth is positive or the swap leaves it
/// positive). A holding of another human player (directly or through a corp he controls) blocks the
/// offer; when the current human player holds bonds he is asked (MSGBOX YES/NO); NO cancels with a
/// headline. Otherwise `BondIssueInfo` is rewritten, the forgiven principal booked to `ExtraordinaryPL`,
/// `BondsIssued` and every holding scaled (holdings rounded to 0 are paid out at `BondPrice`), the
/// holdings packed, a headline posted, and `BondsIssued` raised to at least the sum of the holdings
/// (set to it when `BondsPublicHeld` is 0).
pub fn ai_bond_recapitalization(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_ref: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("RecapDebt") (error path not ported)
    let corp_id = corp_ref;
    if corp_id < 11 || corp_id > 1600 {
        return;
    }
    if g.entity_status[corp_id] != 1 {
        return;
    }
    if !(cast::<Ext, _>(g.bonds_issued[corp_id]) >= Ext::from_f64(25.0)) {
        return;
    }
    if get_controller_type(g, corp_id) == 1 {
        return;
    }
    let mut years_left = to_long(val(mid(g.bond_issue_info[corp_id].as_bytes(), 1, 4).as_bytes()) - cast::<Ext, _>(g.cur_year));
    if years_left < 3 {
        return;
    }
    calc_entity_financials(g, ui, rng, corp_id);
    if !(3 >= g.credit_rating_num[corp_id]) {
        return;
    }
    let liquid = g.t_bills[corp_id] + g.cash[corp_id];
    let issue_value = cast::<Ext, _>(g.bonds_issued[corp_id]) * sf(0.7f32) * sf(0.01f32) * g.bond_price[corp_id].get();
    if !(issue_value >= liquid) {
        return;
    }
    let old_coupon = cast::<f32, _>(val(right(g.bond_issue_info[corp_id].as_bytes(), 5).as_bytes()));
    let mut cv_prefix = PbStr::new();
    if instr(1, g.bond_issue_info[corp_id].as_bytes(), b"cv") > 0 {
        let conv = val(parse_any(g.bond_issue_info[corp_id].as_bytes(), b"|", 2).as_bytes());
        if !(conv >= g.market_cap[corp_id]) {
            return;
        }
        cv_prefix = PbStr::from("cv|") + &parse_any(g.bond_issue_info[corp_id].as_bytes(), b"|", 2) + "|";
    }
    let mut swap_ratio = 2;
    if !(Ext::from_f64(8.0) >= cast::<Ext, _>(old_coupon)) {
        swap_ratio = 3;
    }
    if !(sf(10.65f32) >= cast::<Ext, _>(old_coupon)) {
        return;
    }
    if swap_ratio == 2 && g.net_worth < Ext::from_f64(0.0) {
        if !(cast::<Ext, _>(g.bonds_issued[corp_id]) * sf(0.5f32) + g.net_worth >= Ext::from_f64(0.0)) {
            return;
        }
    }
    if swap_ratio == 3 && g.net_worth < Ext::from_f64(0.0) {
        if !(cast::<Ext, _>(g.bonds_issued[corp_id]) * sf(0.333f32) + g.net_worth >= Ext::from_f64(0.0)) {
            return;
        }
    }
    if !(g.net_worth < Ext::from_f64(0.0)) {
        swap_ratio = 3;
    }
    if swap_ratio == 2 && g.net_worth < Ext::from_f64(0.0) {
        if !(cast::<Ext, _>(g.bonds_issued[corp_id]) * sf(0.333f32) + g.net_worth <= Ext::from_f64(0.0)) {
            swap_ratio = 3;
        }
    }
    let mut human_holder = 0;
    for holder_id in 1..=1583 {
        if g.entity_status[holder_id] != 1 {
            continue;
        }
        for slot in 1..=15 {
            if corp_id != g.hold_bond_id[(slot, holder_id)] {
                continue;
            }
            years_left = g.corp_controller[holder_id];
            if g.num_players >= holder_id {
                years_left = holder_id;
            }
            if g.cur_player == years_left {
                human_holder = years_left;
            } else if g.num_players >= years_left && years_left > 0 && g.is_computer_player[years_left] == 0 {
                return;
            }
        }
    }
    if human_holder > 0 && g.num_players >= human_holder && g.is_computer_player[human_holder] == 1 {
        human_holder = 0;
    }
    let new_coupon: f32;
    let old_units: &str;
    let new_units: &str;
    if swap_ratio == 2 {
        new_coupon = cast::<f32, _>(old_coupon * Ext::from_f32(2.0f32));
        old_units = "2";
        new_units = "1";
    } else {
        new_coupon = cast::<f32, _>(cast::<Ext, _>(old_coupon) * sf(1.5f32));
        old_units = "3";
        new_units = "2";
    }
    if human_holder > 0 {
        let cur_sym = trim(g.currency_symbol.as_bytes());
        let msg_text = g.ent_name[corp_id].clone()
            + __content.string(2799)
            + __content.string(2800)
            + &cur_sym
            + new_units
            + &g.currency_suffix
            + __content.string(2801)
            + &trim(fmt(cast::<Ext, _>(new_coupon), b"* ##.00").as_bytes())
            + "% "
            + __content.string(2802)
            + &cur_sym
            + old_units
            + &g.currency_suffix
            + __content.string(2803)
            + &trim(fmt(cast::<Ext, _>(old_coupon), b"* ##.00").as_bytes())
            + __content.string(2804)
            + &mid(g.bond_issue_info[corp_id].as_bytes(), 1, 4)
            + "."
            + "\r"
            + "\r"
            + __content.string(2805)
            + &trim(fmt(g.bond_price[corp_id].get(), b"* ###.00").as_bytes())
            + "."
            + "\r"
            + "\r"
            + __content.string(2806)
            + "\r"
            + "\r"
            + __content.string(2807)
            + &g.ent_symbol[corp_id]
            + " "
            + __content.string(2808);
        let title = PbStr::from(__content.string(2809)) + &g.ent_symbol[corp_id];
        let answer = ui.msgbox(&msg_text, 8483, &title);
        if answer != 6 {
            g.news_msg = g.ent_symbol[corp_id].clone()
                + __content.string(2810);
            post_news(g, ui, corp_id, 0);
            return;
        }
    }
    let year = g.cur_year + 5;
    let year_str = trim(str_s(cast::<Ext, _>(year)).as_bytes());
    let coupon_str = PbStr::from("000") + &trim(fmt(cast::<Ext, _>(new_coupon), b"* ##.00").as_bytes());
    let coupon_str = right(coupon_str.as_bytes(), 5);
    g.bond_issue_info[corp_id] = year_str + &cv_prefix + "@" + &coupon_str;
    if swap_ratio == 2 {
        g.extraordinary_pl[corp_id] =
            to_quad(cast::<Ext, _>(g.bonds_issued[corp_id]) * sf(0.5f32) + cast::<Ext, _>(g.extraordinary_pl[corp_id]));
        g.bonds_issued[corp_id] = to_quad(sf(0.5f32) * cast::<Ext, _>(g.bonds_issued[corp_id]));
    } else {
        g.extraordinary_pl[corp_id] =
            to_quad(cast::<Ext, _>(g.bonds_issued[corp_id]) / Ext::from_f64(3.0) + cast::<Ext, _>(g.extraordinary_pl[corp_id]));
        g.bonds_issued[corp_id] = to_quad(Ext::from_f64(2.0) * cast::<Ext, _>(g.bonds_issued[corp_id]) / Ext::from_f64(3.0));
    }
    pack_holding_arrays(g, ui);
    let mut total_held: i64 = 0;
    for holder_id in 1..=1583 {
        if g.entity_status[holder_id] != 1 {
            continue;
        }
        if g.hold_bond_id[(0, holder_id)] == 0 {
            continue;
        }
        for slot in 1..=15 {
            if corp_id != g.hold_bond_id[(slot, holder_id)] {
                continue;
            }
            if swap_ratio == 2 {
                g.hold_bond_face[(slot, holder_id)] = to_quad(sf(0.5f32) * cast::<Ext, _>(g.hold_bond_face[(slot, holder_id)]));
            } else {
                g.hold_bond_face[(slot, holder_id)] = to_quad(Ext::from_f64(2.0) * cast::<Ext, _>(g.hold_bond_face[(slot, holder_id)]) / Ext::from_f64(3.0));
            }
            if g.hold_bond_face[(slot, holder_id)] == 0 {
                g.hold_bond_id[(slot, holder_id)] = 0;
                g.bond_hold_cost[(slot, holder_id)] = Ext::from_f64(0.0);
                g.cash[holder_id] = g.bond_price[corp_id].get() + g.cash[holder_id];
                g.cash[corp_id] = g.cash[corp_id] - g.bond_price[corp_id].get();
            }
            total_held = to_quad(cast::<Ext, _>(g.hold_bond_face[(slot, holder_id)]) + cast::<Ext, _>(total_held));
        }
    }
    pack_holding_arrays(g, ui);
    g.news_msg = g.ent_symbol[corp_id].clone()
        + __content.string(2811)
        + new_units
        + "-for-"
        + old_units
        + __content.string(2812);
    post_news(g, ui, corp_id, 0);
    while !(cast::<Ext, _>(g.bonds_issued[corp_id]) >= cast::<Ext, _>(total_held)) {
        g.bonds_issued[corp_id] = to_quad(cast::<Ext, _>(g.bonds_issued[corp_id]) + Ext::from_f64(1.0));
    }
    if g.bonds_public_held == 0 {
        g.bonds_issued[corp_id] = total_held;
    }
}

/// Port of AiCorpBuysStock (0x4bd8b4).
/// A random buyer (from a range rotating with `SimCoCount`; the selected corp one time in 15 unless it
/// just acquired) that is active, not personally managed, not another human's corp, not a fund below
/// 1586, liquid (banks 5000/1500 x exchange rate, all 1000, operating corps 30% of their assets, and
/// 20..30% of total assets) and rated 6+ adds to one of its holdings (a random one with a public
/// float, else — funds and fully held stocks — another holding) or buys into the takeover candidate
/// (when it holds nothing and the stake costs under 0.45/25 of its funds): 90% of the float (funds: up
/// to 10%), reduced until the price is under 35% of its funds (at least 3 points and 20% total).
/// A human's autopilot corp asks (AskAutopilotPermission); a computer player's corp must be under
/// `CpuMaxCorps`. The purchase goes to `BuyStock(buyer, target)` with `TmpExt1` = points and `TmpExt2`
/// = 2 (0 for the current player's corp). Afterwards a well-rated, profitable, not human-controlled
/// buyer with few holdings and no tax losses raises its dividend (x1.1..x5, or sets a 1% yield when it
/// pays none) with a " [runtime resource wording] " headline (dropped when one is recent and it is
/// not the current player's corp), outside a tight monetary stance.
///
/// Bug (reproduced, BUGS.md #194): the "raises dividend" headline is also posted when the yield is
/// already over `RND * 3 + 6` and the dividend is left unchanged. #195 (to verify): `LastAcqCorp` is set
/// when the stake did *not* change after `BuyStock`.
///
/// `time_secs`: seconds of `TIME$` (for AskAutopilotPermission and BuyStock).
pub fn ai_corp_buys_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut cur_pct: i32 = 0;
    let mut buy_status: i32 = 0;
    let mut target_id: i32 = 0;
    let mut slot: i32 = 0;
    let mut pct_to_buy: i16;
    let mut range_lo = 550;
    let mut range_hi = 1000;
    if g.sim_co_count > 550 && g.sim_co_count < 1000 {
        range_lo = 1001;
        range_hi = 1600;
    }
    if !(g.sim_co_count < 1000) {
        range_lo = 86;
        range_hi = 550;
    }
    let mut buyer_id = rng.rnd_range(range_hi, range_lo);
    if g.sel_entity > 10 && !(rng.rnd() * Ext::from_f64(15.0) <= Ext::from_f64(14.0)) && g.sel_entity != g.last_acq_corp {
        buyer_id = g.sel_entity;
    }
    if g.entity_status[buyer_id] != 1 {
        return;
    }
    if get_managing_player(g, buyer_id) > 0 {
        return;
    }
    if get_controller_type(g, buyer_id) == 1 && g.cur_player != g.corp_controller[buyer_id] {
        return;
    }
    let industry = g.industry_num[buyer_id];
    if industry == 71 && buyer_id < 1586 {
        return;
    }
    if industry == 2 {
        g.ai_acq_min_cash = 5000;
        if get_controller_type(g, buyer_id) == 2 {
            g.ai_acq_min_cash = 1500;
        }
    }
    if industry == 2 {
        if !(g.exchange_rate * cast::<Ext, _>(g.ai_acq_min_cash) <= g.t_bills[buyer_id] + g.cash[buyer_id]) {
            return;
        }
    }
    if !(g.exchange_rate * Ext::from_f64(1000.0) <= g.t_bills[buyer_id] + g.cash[buyer_id]) {
        return;
    }
    if industry > 2 && industry < 70 {
        if !(g.business_assets[buyer_id] * sf(0.3f32) <= g.t_bills[buyer_id] + g.cash[buyer_id]) {
            return;
        }
    }
    calc_entity_financials(g, ui, rng, buyer_id);
    let liquid = g.t_bills[buyer_id] + g.cash[buyer_id];
    let total_assets = g.total_assets;
    if !((rng.rnd() * sf(0.1f32) + sf(0.2f32)) * total_assets <= liquid) {
        return;
    }
    if !(6 <= g.credit_rating_num[buyer_id]) {
        return;
    }
    let mut num_holdings = cast::<i32, _>(g.hold_stock_id[(0, buyer_id)]);
    if num_holdings == 1
        && (cast::<i32, _>(g.hold_stock_pct[(1, buyer_id)])) < 10
        && g.takeover_candidate_id > 10
        && !(rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0))
    {
        num_holdings = 0;
    }
    if num_holdings > 0 {
        slot = rng.rnd_range(num_holdings, 1);
        let mut held_id = cast::<i32, _>(g.hold_stock_id[(slot, buyer_id)]);
        if held_id <= 10 {
            return;
        }
        update_corp_control(g, ui, rng, held_id);
        if industry < 71 && g.public_pct > 0 {
            target_id = held_id;
            cur_pct = cast::<i32, _>(g.hold_stock_pct[(slot, buyer_id)]);
        } else if num_holdings > 1 {
            let limit = num_holdings;
            let mut i = 1;
            while i <= limit {
                let mut take = false;
                if slot != i {
                    if industry == 71 {
                        if !(cast::<i32, _>(g.hold_stock_pct[(i, buyer_id)]) > 9) && !(rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0)) {
                            take = true;
                        }
                    } else if cast::<i32, _>(g.hold_stock_id[(i, buyer_id)]) > 0 && cast::<i32, _>(g.hold_stock_pct[(i, buyer_id)]) != 100 {
                        take = true;
                    }
                }
                if take {
                    held_id = cast::<i32, _>(g.hold_stock_id[(i, buyer_id)]);
                    update_corp_control(g, ui, rng, held_id);
                    if g.public_pct > 3 {
                        target_id = held_id;
                        slot = i;
                        cur_pct = cast::<i32, _>(g.hold_stock_pct[(slot, buyer_id)]);
                        break;
                    }
                }
                i += 1;
            }
        }
    } else if num_holdings == 0 && g.takeover_candidate_id > 10 {
        let tc = g.takeover_candidate_id;
        update_corp_control(g, ui, rng, tc);
        if g.public_pct > 19
            && !(Ext::from_f64(25.0) * g.market_cap[g.takeover_candidate_id] >= (g.t_bills[buyer_id] + g.cash[buyer_id]) * sf(0.45f32))
        {
            target_id = g.takeover_candidate_id;
            slot = find_holding_slot(g, buyer_id, target_id);
            if slot < 1 {
                target_id = 0;
            }
        }
    }
    if target_id > 10 {
        if industry == 71 {
            pct_to_buy = cast::<i16, _>(10 - cur_pct);
        } else {
            pct_to_buy = to_integer(cast::<Ext, _>(g.public_pct) * sf(0.9f32));
        }
        if cur_pct + (cast::<i32, _>(pct_to_buy)) < 20 && industry != 71 {
            return;
        }
        while buy_status == 0 {
            let liquid = g.t_bills[buyer_id] + g.cash[buyer_id];
            if !(sf(1.2f32) * cast::<Ext, _>(pct_to_buy) * g.market_cap[target_id] >= liquid * sf(0.35f32)) {
                buy_status = 1;
                break;
            }
            pct_to_buy = pct_to_buy.wrapping_sub(1);
            if (cast::<i32, _>(pct_to_buy)) < 3 && industry != 71 {
                buy_status = -1;
            }
            if cur_pct + (cast::<i32, _>(pct_to_buy)) < 20 && industry != 71 {
                buy_status = -1;
            }
            if pct_to_buy == 0 {
                buy_status = -1;
            }
        }
        if buy_status == 1 && get_controller_type(g, buyer_id) == 1 {
            buy_status = ask_autopilot_permission(g, ui, rng, buyer_id, target_id, pct_to_buy, cur_pct, 0, time_secs);
        }
        if buy_status == 1 && get_controller_type(g, buyer_id) == 2 {
            g.tmp_acquirer_ctrl = g.corp_controller[buyer_id];
            if g.tmp_acquirer_ctrl != g.corp_controller[target_id]
                && !(g.cpu_max_corps > g.player_corps_controlled[g.tmp_acquirer_ctrl])
            {
                buy_status = 0;
            }
        }
        if buy_status == 1 {
            if g.takeover_candidate_id == target_id {
                g.takeover_candidate_id = 0;
            }
            g.tmp_ext2 = Ext::from_f64(2.0);
            if g.cur_player == g.corp_controller[buyer_id] {
                g.tmp_ext2 = Ext::from_f64(0.0);
            }
            g.tmp_ext1 = cast::<Ext, _>(pct_to_buy);
            buy_stock(g, ui, rng, buyer_id, target_id, time_secs);
            if cast::<i32, _>(g.hold_stock_pct[(slot, buyer_id)]) == cur_pct {
                g.last_acq_corp = buyer_id;
            }
            if get_controller_type(g, buyer_id) != 1 {
                calc_entity_financials(g, ui, rng, buyer_id);
            }
        }
    }

    // ---- 0x4be43e: dividend raise of a liquid corp
    if get_controller_type(g, buyer_id) == 1 || g.monetary_stance == -1 {
        return;
    }
    if !((cast::<i32, _>(g.hold_stock_id[(0, buyer_id)])) < 4) {
        return;
    }
    if industry == 71 || buyer_id == g.last_div_raise_corp {
        return;
    }
    if g.tax_loss_carry[buyer_id] < Ext::from_f64(0.0) || g.earnings_y1[buyer_id] < Ext::from_f64(0.0) {
        return;
    }
    if !(8 <= g.credit_rating_num[buyer_id]) {
        return;
    }
    // DOUBLE local
    let div_yield = crate::pb::numeric::double(calc_dividend_yield(g, buyer_id));
    if div_yield == Ext::from_f64(0.0) {
        g.dividend_per_share[buyer_id] = g.market_cap[buyer_id] / g.shares_outstanding[buyer_id] * sf(0.01f32);
        return;
    }
    if !(sf(0.5f32) <= div_yield) {
        g.dividend_per_share[buyer_id] = Ext::from_f64(5.0) * g.dividend_per_share[buyer_id];
    } else if !(rng.rnd() * sf(0.5f32) + Ext::from_f64(1.0) <= div_yield) {
        g.dividend_per_share[buyer_id] = Ext::from_f64(3.0) * g.dividend_per_share[buyer_id];
    } else if !(Ext::from_f64(3.0) <= div_yield) {
        g.dividend_per_share[buyer_id] = Ext::from_f64(2.0) * g.dividend_per_share[buyer_id];
    } else if !(rng.rnd() * Ext::from_f64(3.0) + Ext::from_f64(3.0) <= div_yield) {
        g.dividend_per_share[buyer_id] = sf(1.15f32) * g.dividend_per_share[buyer_id];
    } else if !(rng.rnd() * Ext::from_f64(3.0) + Ext::from_f64(6.0) <= div_yield) {
        g.dividend_per_share[buyer_id] = sf(1.1f32) * g.dividend_per_share[buyer_id];
    }
    g.news_msg = g.ent_symbol[buyer_id].clone() + __content.string(2813);
    if !(val(parse_any(g.ent_misc_data[buyer_id].as_bytes(), b"|", 11).as_bytes()) <= Ext::from_f64(0.0)) {
        g.news_msg = g.news_msg.clone() + " again";
    }
    if is_in_recent_headlines(g, __content.bytes(2814)) > 0 && g.cur_player != g.corp_controller[buyer_id] {
        g.news_msg = PbStr::new();
    } else {
        post_news(g, ui, buyer_id, 0);
    }
    if g.sel_entity == buyer_id {
        g.last_div_raise_corp = buyer_id;
    }
}

/// Port of ReviseEarningsEstimates (0x4b8397).
/// Posts a pending `NewsMsg`, then recomputes `ProjQtrEarnings` (CalcProjectedEarnings) of every
/// active corp 11..1600 except funds above 1580 (while `SimCoCount` < 86 only up to `SimCoCount`;
/// inactive corps get 0). For the first watched corp (active, selected or last selected) whose
/// projection moves by more than 10% (and while no headline is pending) the market cap and investor
/// sentiment react (+1% / +10..20, or -1.5% / x0.85) and — at 0.15+ per share with a public float — an
/// analyst (an industry-37 corp seen on the way, else a random bank) "raises/lowers … earnings
/// estimate" (or the company its guidance, verb by `RandSeed`), posted at the end.
pub fn revise_earnings_estimates(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut news_corp: i32 = 0;
    let mut has_news: i32 = 0;
    let mut analyst_id: i32 = 0;
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    for corp_id in 11..=1600 {
        if g.entity_status[corp_id] != 1 {
            g.proj_qtr_earnings[corp_id] = Ext::from_f64(0.0);
            continue;
        }
        if corp_id > 1580 && g.industry_num[corp_id] == 71 {
            continue;
        }
        if g.sim_co_count < 86 && g.sim_co_count < corp_id {
            continue;
        }
        let old_earnings = g.proj_qtr_earnings[corp_id];
        g.proj_qtr_earnings[corp_id] = calc_projected_earnings(g, ui, rng, corp_id);
        if !g.news_msg.is_empty() {
            continue;
        }
        let mut is_watched = 0;
        if old_earnings == Ext::from_f64(0.0) {
            continue;
        }
        if g.active_entity == corp_id || g.sel_entity == corp_id || g.last_sel_entity == corp_id {
            is_watched = 1;
        }
        if is_watched == 0 && !(rng.rnd() * Ext::from_f64(8.0) <= Ext::from_f64(7.0)) && g.industry_num[corp_id] == 37 {
            analyst_id = corp_id;
        }
        if is_watched == 0 {
            continue;
        }
        let new_earnings = g.proj_qtr_earnings[corp_id];
        let mut verb = PbStr::new();
        if !(new_earnings <= Ext::from_f64(0.0)) && !(new_earnings <= old_earnings * sf(1.1f32)) {
            verb = PbStr::from("raise");
            if g.rand_seed % 5 == 1 {
                verb = PbStr::from("boost");
            }
            if g.rand_seed % 5 == 2 {
                verb = PbStr::from("lift");
            }
            if g.rand_seed % 5 == 3 {
                verb = PbStr::from("hike");
            }
            if g.rand_seed % 5 == 4 {
                verb = PbStr::from("up");
            }
            g.market_cap[corp_id] = sf(1.01f32) * g.market_cap[corp_id];
            let s = Ext::from_f64(10.0) + cast::<Ext, _>(g.investor_sentiment[corp_id]) + rng.rnd() * Ext::from_f64(10.0);
            g.investor_sentiment[corp_id] = cast::<i32, _>(to_quad(s));
        }
        if !(old_earnings <= Ext::from_f64(0.0)) && !(old_earnings <= new_earnings * sf(1.1f32)) {
            verb = PbStr::from("lower");
            if g.rand_seed % 7 == 3 {
                verb = PbStr::from("cut");
            }
            if g.rand_seed % 7 == 5 {
                verb = PbStr::from("slashe");
            }
            if g.rand_seed % 7 == 6 {
                verb = PbStr::from("reduce");
            }
            g.market_cap[corp_id] = sf(0.985f32) * g.market_cap[corp_id];
            g.investor_sentiment[corp_id] = cast::<i32, _>(to_quad(sf(0.85f32) * cast::<Ext, _>(g.investor_sentiment[corp_id])));
        }
        if verb.is_empty() {
            continue;
        }
        if !(g.shares_outstanding[corp_id] > Ext::from_f64(0.0)) {
            // ERROR 151 -> ON ERROR handler: ShowRuntimeError and exit (error path not ported)
            return;
        }
        let eps_estimate = new_earnings / g.shares_outstanding[corp_id] * sf(0.01f32);
        if !(eps_estimate >= sf(0.15f32)) {
            continue;
        }
        update_corp_control(g, ui, rng, corp_id);
        if g.public_pct == 0 {
            continue;
        }
        if analyst_id == 0 {
            analyst_id = rng.rnd_range(60, 11);
        }
        has_news = 1;
        news_corp = corp_id;
        let eps_str = trim(fmt(new_earnings / g.shares_outstanding[corp_id] * sf(0.01f32), b"* ####.00").as_bytes());
        g.news_msg = g.ent_name[analyst_id].clone()
            + " "
            + &verb
            + __content.string(2815)
            + &g.ent_symbol[corp_id]
            + " to "
            + &eps_str;
        if g.rand_seed % 3 == 1 {
            g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(2816), b"target");
        }
        if verb == "raise" || verb == "reduce" {
            let eps_str = trim(fmt(new_earnings / g.shares_outstanding[corp_id] * sf(0.01f32), b"* ####.00").as_bytes());
            g.news_msg = g.ent_symbol[corp_id].clone()
                + " ("
                + &g.ent_name[corp_id]
                + ") "
                + &verb
                + __content.string(2817)
                + &eps_str;
        }
        if len(g.news_msg.as_bytes()) < 58 {
            g.news_msg = g.news_msg.clone() + __content.string(2818);
        }
    }
    if !g.news_msg.is_empty() && has_news == 1 {
        let active = g.active_entity;
        post_news(g, ui, active, news_corp);
    }
}

/// Port of AiRandomBondIssue (0x4f77b2).
/// While at most `RND(0, 200) + 600` corps have bonds out: the corp `SimCoCount` (active, no fund
/// below 1586, no bonds, not personally managed) issues bonds with a chance of 1 in 4 (high cash and
/// rates over ~7.3..8.3 never; base rate over 6.35 only 1 in 10), if rated 5+ (funds: 9+, with liquid
/// funds over 7% of `TmpSubsidStockValue` and a base rate up to 5): 10..30% of its net worth (funds 60%
/// of that), between `RND(125, 200)` x exchange rate and `MaxBondIssue`, rounded to 100 in `TmpExt1`,
/// through [`issue_bonds`] (automatic issue).
pub fn ai_random_bond_issue(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // ON ERROR GOTO -> ShowRuntimeError("RandBondIssue") (error path not ported)
    let limit = rng.rnd_range(200, 0) + 600;
    if !(count_bond_issuers(g) <= limit) {
        return;
    }
    let mut issuer_id = g.sim_co_count;
    if issuer_id < 11 || issuer_id > 1600 {
        return;
    }
    if g.entity_status[issuer_id] != 1 {
        return;
    }
    if g.industry_num[issuer_id] == 71 && issuer_id < 1586 {
        return;
    }
    let bir = cast::<Ext, _>(g.base_interest_rate);
    if !(g.cash[issuer_id] <= g.exchange_rate * Ext::from_f64(500.0)) {
        if !(rng.rnd() + sf(7.3f32) >= bir) {
            return;
        }
    }
    if !(sf(6.35f32) >= bir) {
        if !(rng.rnd() * Ext::from_f64(10.0) <= Ext::from_f64(1.0)) {
            return;
        }
    }
    if !(rng.rnd() * Ext::from_f64(4.0) <= Ext::from_f64(1.0)) {
        return;
    }
    issuer_id = g.sim_co_count;
    if !(cast::<Ext, _>(g.bonds_issued[issuer_id]) <= Ext::from_f64(0.0)) {
        return;
    }
    if get_managing_player(g, issuer_id) > 0 {
        return;
    }
    calc_entity_financials(g, ui, rng, issuer_id);
    if g.industry_num[issuer_id] == 71 {
        if 9 > g.credit_rating_num[issuer_id] {
            return;
        }
        let liquid = g.t_bills[issuer_id] + g.cash[issuer_id];
        if g.tmp_subsid_stock_value * sf(0.07f32) < liquid {
            return;
        }
        if !(Ext::from_f64(5.0) >= cast::<Ext, _>(g.base_interest_rate)) {
            return;
        }
    }
    if 5 > g.credit_rating_num[issuer_id] {
        return;
    }
    let mut issue_amt = g.net_worth * sf(0.1f32) + g.net_worth * (rng.rnd() * sf(0.2f32));
    if g.industry_num[issuer_id] == 71 {
        issue_amt = issue_amt * sf(0.6f32);
    }
    let floor = g.exchange_rate * cast::<Ext, _>(rng.rnd_range(200, 125));
    if issue_amt < floor {
        return;
    }
    if !(issue_amt <= cast::<Ext, _>(g.max_bond_issue)) {
        return;
    }
    let hundreds = to_quad(issue_amt * sf(0.01f32));
    g.tmp_ext1 = cast::<Ext, _>(to_quad(cast::<Ext, _>(hundreds) * Ext::from_f64(100.0)));
    issue_bonds(g, ui, rng, issuer_id, Ext::from_f64(0.0), 0);
}
