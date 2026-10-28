//! Option writing of funds and corporations, callees of the per-entity simulation (see [`super`]):
//! `GetEtfSmallestHolding`, `PickCoveredCallFund`, `EtfWriteCoveredCalls`, `AiCreateOptionPosition`,
//! `FindOptionCounterparty` and `AiCorpSellNakedPuts`.
//!
//! Option records (`OptionRecs()`): `corp|type (1 call, 2 put)|holder|pct (negative = written)|strike
//! (MarketCap units)|exp month|exp year|premium`.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::{balance_cash_t_bills, calc_cur_month_of_year};
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::newgame_helpers::pick_etf_holding_stock;
use crate::logic::news::is_in_recent_headlines;
use crate::logic::options::{add_or_remove_option_rec, calc_option_price, get_plr_naked_liab};
use crate::logic::valuation::calc_analyst_rating;
use crate::pb::numeric::{Ext80, str_num_ext, val_ext};

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn field(rec: &PbStr, n: i32) -> Ext {
    val(parse_any(rec.as_bytes(), b"|", n).as_bytes())
}

/// `VAL(PARSE$(rec, ANY "|", n))` at full 80-bit precision.
fn field_ext(rec: &PbStr, n: i32) -> Ext80 {
    val_ext(parse_any(rec.as_bytes(), b"|", n).as_bytes())
}

/// `STR$(x)` of an EXT value.
fn str_e(x: Ext80) -> PbStr {
    PbStr::from(str_num_ext(x, 15).as_str())
}

fn ext(x: Ext) -> Ext80 {
    Ext80::from_ext(x)
}

/// Index of the first empty `OptionRecs(1..1000)` slot, 0 if none.
fn free_option_slot(g: &Globals) -> i32 {
    let mut free_slot = 0;
    let mut scan_idx = 1;
    loop {
        if g.option_recs[scan_idx].is_empty() {
            free_slot = scan_idx;
            scan_idx = 1000;
        }
        scan_idx = scan_idx + 1;
        if scan_idx > 1000 {
            break;
        }
    }
    free_slot
}

/// Port of GetEtfSmallestHolding (0x64c9d1).
/// For an active entity `etf_id > 10` that is not controlled by a human player and holds 15 stocks: the
/// holding with the smallest positive value (`MarketCap * HoldStockPct`) among those on which it has not
/// written calls (`FindOptionPositions(etf, stock, 1, 0)` not -1 / 2), else 0.
pub fn get_etf_smallest_holding(g: &mut Globals, ui: &mut dyn Ui, etf_id: i32) -> i32 {
    let mut result = 0;
    let mut smallest_id = 0;
    let mut min_value: Ext = Ext::from_f64(0.0);
    if g.entity_status[etf_id] != 1 || !(10 < etf_id) {
        return result;
    }
    let self_id = etf_id;
    if get_controller_type(g, self_id) == 1 {
        return result;
    }
    if (cast::<i32, _>(g.hold_stock_id[(0, self_id)])) < 15 {
        return result;
    }
    for slot in 1..=15 {
        if 0 < g.hold_stock_id[(slot, self_id)] {
            let stock_id = cast::<i32, _>(g.hold_stock_id[(slot, self_id)]);
            let pos = find_option_positions(g, ui, self_id, stock_id, 1, 0);
            if pos == -1 || pos == 2 {
                continue;
            }
            let value = cast::<Ext, _>(g.hold_stock_pct[(slot, self_id)]) * g.market_cap[stock_id];
            if value > Ext::from_f64(0.0) && min_value == Ext::from_f64(0.0) {
                min_value = value;
                smallest_id = stock_id;
            } else if value > Ext::from_f64(0.0) && value < min_value {
                min_value = value;
                smallest_id = stock_id;
            }
        }
    }
    result = smallest_id;
    result
}

/// Port of PickCoveredCallFund (0x64cc22).
/// Keeps `CoveredCallFundId` if it is an active fund (> 1585, industry 71) without a human adviser
/// (`GetEtfAdviserHumanStatus = 0`), else clears it and tries 10 random funds 1586..1600 (active, industry
/// 71, no bonds, at least 12 holdings).
pub fn pick_covered_call_fund(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    if 1585 < g.covered_call_fund_id {
        let id = g.covered_call_fund_id;
        if g.entity_status[id] == 1
            && g.industry_num[id] == 71
            && !(0 < get_etf_adviser_human_status(g, ui, rng, id))
        {
        } else {
            g.covered_call_fund_id = 0;
        }
        if 1585 < g.covered_call_fund_id {
            return;
        }
    }
    let mut tries = 10;
    loop {
        let id = rng.rnd_range(1600, 1586);
        if g.entity_status[id] == 1
            && g.industry_num[id] == 71
            && g.bonds_issued[id] <= 0
            && 12 <= g.hold_stock_id[(0, id)]
        {
            g.covered_call_fund_id = id;
            return;
        }
        tries = tries - 1;
        if tries == 0 {
            return;
        }
    }
}

/// Port of EtfWriteCoveredCalls (0x603825).
/// Once per quarter (not in the first quarter of the game): picks the covered-call fund
/// ([`pick_covered_call_fund`]); with fewer than 13 holdings it may buy calls on a new stock
/// (`AiCreateOptionPosition(fund, stock, RND(1,7)+3, 1, 0)` if its liquid funds cover 5x the stock's
/// market cap and it has no bank debt; stops if the stock's public float is below 11%), then makes 10
/// tries at writing calls on half (rounded) of a random holding.
pub fn etf_write_covered_calls(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if g.start_year == g.cur_year && g.cur_quarter == 1 {
        return;
    }
    pick_covered_call_fund(g, ui, rng);
    if g.covered_call_fund_id < 1586 {
        return;
    }
    let ccf = g.covered_call_fund_id;
    if !(g.entity_status[ccf] == 1 && g.industry_num[ccf] == 71) {
        g.covered_call_fund_id = 0;
        return;
    }
    let fund_id = ccf;
    if (cast::<i32, _>(g.hold_stock_id[(0, fund_id)])) < 13 {
        let stock_id = pick_etf_holding_stock(g, ui, rng, fund_id, 0);
        if 10 < stock_id {
            update_corp_control(g, ui, rng, stock_id);
            if (cast::<i32, _>(g.public_pct)) < 11 {
                return;
            }
            let pct = rng.rnd_range(7, 1) + 3;
            let funds = (g.cash[fund_id] + g.t_bills[fund_id]) - cast::<Ext, _>(g.extraordinary_pl[fund_id]);
            if !(funds < g.market_cap[stock_id] * Ext::from_f64(5.0)) && g.bank_debt[fund_id] <= Ext::from_f64(0.0) {
                ai_create_option_position(g, ui, rng, fund_id, stock_id, pct, 1, 0);
            }
        }
    }
    let mut tries_left = 10;
    loop {
        let hold_slot = rng.rnd_range(15, 1);
        if g.hold_stock_id[(hold_slot, fund_id)] != 0 && 1 <= g.hold_stock_pct[(hold_slot, fund_id)]
        {
            let stock_id = cast::<i32, _>(g.hold_stock_id[(hold_slot, fund_id)]);
            let mut pct_to_write = cast::<i32, _>(g.hold_stock_pct[(hold_slot, fund_id)]);
            if 2 < pct_to_write {
                pct_to_write = to_long(cast::<Ext, _>(pct_to_write) / Ext::from_f64(2.0));
            }
            let pos = find_option_positions(g, ui, fund_id, stock_id, 1, 0);
            if pos != -1 && pos != 2 {
                update_corp_control(g, ui, rng, stock_id);
                let public_pct = cast::<i32, _>(g.public_pct);
                if 2 <= public_pct {
                    if public_pct < pct_to_write {
                        pct_to_write = public_pct - 1;
                    }
                    ai_create_option_position(g, ui, rng, fund_id, stock_id, pct_to_write, 2, 1);
                }
            }
        }
        tries_left = tries_left - 1;
        if tries_left == 0 {
            return;
        }
    }
}

/// Port of AiCreateOptionPosition (0x5fef2c).
/// Buys (`tran_type = 1`) or writes (`2`, recorded as negative pct) calls on `pct_arg`% of `corp_arg` for
/// `buyer_arg`; `is_etf_arg = 1` for a fund's covered calls (strike 95..110% of the market cap, expiry
/// 8..20 months out, needs autopilot under a human adviser), else strike 80..87%, expiry about 3 months
/// out. Not in the first quarter of the game (before progress 501), not for buyers < 131 buying, only for
/// share prices 5..940, no buying with an existing long position or `pct >= PublicPct`. Books the premium
/// (cash), asks [`find_option_counterparty`] for the other side and posts the news. Returns `tran_type` on
/// success, 0 otherwise.
///
/// A record whose price comes out `<= 0` stays in `OptionRecs()` with premium 0 (like the original).
pub fn ai_create_option_position(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    buyer_arg: i32,
    corp_arg: i32,
    pct_arg: i32,
    tran_type_arg: i32,
    is_etf_arg: i32,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let result = 0;
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 501 {
        return result;
    }
    if buyer_arg < 131 && tran_type_arg == 1 {
        return result;
    }
    let buyer_id = buyer_arg;
    let corp_id = corp_arg;
    let tran_type = tran_type_arg;
    let mut qty = pct_arg;
    if tran_type_arg == 2 {
        qty = -pct_arg;
    }
    let share_price = g.market_cap[corp_id] / g.shares_outstanding[corp_id];
    if share_price < Ext::from_f64(5.0) || !(share_price <= Ext::from_f64(940.0)) {
        return result;
    }
    if is_etf_arg == 1
        && 0 < get_etf_adviser_human_status(g, ui, rng, buyer_id)
        && g.auto_pilot[g.fund_adviser[buyer_id]] == 0
    {
        return result;
    }
    if 0 < find_option_positions(g, ui, buyer_id, corp_id, 1, 0) && tran_type == 1 {
        return result;
    }
    update_corp_control(g, ui, rng, corp_id);
    if !(cast::<i32, _>(g.public_pct) > qty) && tran_type == 1 {
        return result;
    }
    let sep = PbStr::from("|");
    let mut new_rec = str_s(cast::<Ext, _>(corp_id))
        + &sep
        + "1|"
        + &str_s(cast::<Ext, _>(buyer_id))
        + &sep
        + &str_s(cast::<Ext, _>(qty))
        + &sep;
    let (c1, c2) = if is_etf_arg == 1 {
        (sf(0.15f32), sf(0.95f32))
    } else {
        (sf(0.07f32), sf(0.8f32))
    };
    let mut strike =
        ext(g.market_cap[corp_id]).fmul(Ext80::fadd(ext(Ext::from(rng.rnd())).fmul(ext(c1)), ext(c2)));
    if strike.into_ext() < Ext::from_f64(0.0) {
        let text = PbStr::from("TGT =")
            + &g.ent_symbol[corp_id]
            + __content.string(10831)
            + &str_e(strike)
            + __content.string(10832)
            + &str_s(cast::<Ext, _>(buyer_arg))
            + "\r"
            + __content.string(10833)
            + &str_s(cast::<Ext, _>(tran_type))
            + __content.string(10834)
            + &str_s(cast::<Ext, _>(qty));
        ui.msgbox(&text, 0x2040, &PbStr::from(__content.string(10835)));
    }
    strike = Ext80::abs(strike);
    new_rec = new_rec + &str_e(strike) + &sep;
    let mut exp_month = to_long(Ext::from_f64(3.0) + calc_cur_month_of_year(g, g.sim_co_count).ceil());
    if is_etf_arg == 1 {
        exp_month = exp_month + 8;
    }
    let mut exp_year = g.cur_year;
    if 12 < exp_month {
        exp_month = exp_month - 12;
        exp_year = exp_year + 1;
    }
    if is_etf_arg == 1 && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
        exp_year = exp_year + 1;
    }
    new_rec = new_rec + &str_s(cast::<Ext, _>(exp_month)) + &sep + &str_s(cast::<Ext, _>(exp_year)) + &sep;
    let free_slot = free_option_slot(g);
    if free_slot == 0 {
        return result;
    }
    g.option_recs[free_slot] = new_rec.clone() + " 0";
    g.has_options_outstanding[corp_id] = 1;
    let opt_price = calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, free_slot);
    if !(opt_price > Ext::from_f64(0.0)) {
        return result;
    }
    let cost = ext(opt_price).fmul(Ext80::from_i64(cast::<i64, _>(qty)));
    let cost_abs = Ext80::abs(cost);
    new_rec = new_rec + &str_e(cost_abs);
    g.option_recs[free_slot] = new_rec.clone();
    g.cash[buyer_id] = Ext::from(Ext80::fadd(
        ext(g.cash[buyer_id]),
        Ext80 {
            se: cost.se ^ 0x8000,
            mant: cost.mant,
        },
    )
    .into_ext());
    find_option_counterparty(g, ui, rng, &new_rec, Ext::from(cost_abs.into_ext()), sf(0.26f32));
    let mut verb = PbStr::from(__content.string(10836));
    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
        verb = PbStr::from(" buy");
    }
    if tran_type == 2 {
        verb = PbStr::from(" sell");
    }
    g.news_msg = g.ent_symbol[buyer_id].clone()
        + &verb
        + __content.string(10837)
        + &str_s((cast::<Ext, _>(qty)).abs())
        + "% of "
        + &g.ent_symbol[corp_id]
        + " stock.";
    let head = mid(g.news_msg.as_bytes(), 1, 15);
    if 0 < is_in_recent_headlines(g, head.as_bytes()) && !(g.corp_controller[buyer_id] <= 10) {
        g.news_msg = PbStr::new();
    } else {
        post_news(g, ui, 0, 0);
    }
    tran_type
}

/// Port of FindOptionCounterparty (0x602925).
/// Finds a computer-run corporation [runtime resource wording] of `option_rec` (premium `premium_in`):
/// 100 random tries 131..1600, then a scan from 865 up or down (by the last random pick). A writer
/// (the record buys) needs credit rating >= 7 and `NakedLiab + premium <= NetWorth * risk_frac`; a buyer
/// (the record writes) rating >= 7, `premium <= LiquidFunds * risk_frac`, enough cash and no long options
/// (the random tries also skip anyone with options when `risk_frac < 0.24`). With `risk_frac < 0.25` a
/// counterparty rated below 9 backs out one time in 10 (returned, nothing booked). Otherwise books the
/// premium, adds the mirror record (pct negated, premium +/-2%) via `AddOrRemoveOptionRec` and posts
/// the news if the record's holder is the current player (or controlled by it). Returns the
/// counterparty (0 if none).
///
/// ERROR 151 (`SharesOutstanding = 0` of the stock) goes to the ON ERROR handler (ShowRuntimeError,
/// result 0); the port returns 0 there without UI.
pub fn find_option_counterparty(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    option_rec: &PbStr,
    premium_in: Ext,
    risk_frac: Ext,
) -> i32 {
    let __content = crate::content::current();
    let rec = option_rec.clone();
    let premium = premium_in.abs();
    let risk: Ext = crate::pb::numeric::double(risk_frac); // stored as DOUBLE
    let stock_id = to_long(field(&rec, 1));
    let opt_type = to_long(field(&rec, 2));
    let mut type_text = PbStr::from("call");
    if opt_type == 2 {
        type_text = PbStr::from("put");
    }
    let holder_id = to_long(field(&rec, 3));
    let mut opt_pct = to_long(field(&rec, 4));
    let mut trade_side = 1;
    if 0 < opt_pct {
        trade_side = 2;
    }
    let mut counterparty_id = 0;
    let mut candidate_id;
    let mut tries_left = 100;
    loop {
        candidate_id = rng.rnd_range(1600, 131);
        if counterparty_ok_random(
            g,
            ui,
            rng,
            candidate_id,
            stock_id,
            holder_id,
            opt_type,
            trade_side,
            premium,
            risk,
        ) {
            counterparty_id = candidate_id;
            break;
        }
        tries_left = tries_left - 1;
        if tries_left == 0 {
            break;
        }
    }
    if counterparty_id == 0 {
        let mut scan_dir = 1;
        if (cast::<Ext, _>(candidate_id)) < sf(865.5f32) {
            scan_dir = -1;
        }
        candidate_id = 865;
        'scan: loop {
            loop {
                if 1580 <= candidate_id || candidate_id <= 131 {
                    break 'scan;
                }
                candidate_id = candidate_id + scan_dir;
                if stock_id != candidate_id {
                    break;
                }
            }
            if 1580 < candidate_id && g.industry_num[candidate_id] == 71 {
                continue;
            }
            if g.entity_status[candidate_id] != 1 {
                continue;
            }
            if 0 < get_controller_type(g, candidate_id) {
                continue;
            }
            if trade_side == 1 {
                if 0 < find_option_positions(g, ui, candidate_id, stock_id, opt_type, 0) {
                    continue;
                }
                calc_entity_financials(g, ui, rng, candidate_id);
                if g.credit_rating_num[candidate_id] < 7 {
                    continue;
                }
                if premium > g.liquid_funds * risk {
                    continue;
                }
                if premium > sf(0.3f32) * (g.cash[candidate_id] + g.t_bills[candidate_id]) {
                    continue;
                }
                if g.long_options_value > Ext::from_f64(0.0) {
                    continue;
                }
                counterparty_id = candidate_id;
                break;
            } else {
                let pos = find_option_positions(g, ui, candidate_id, stock_id, opt_type, 0);
                if pos < 0 || pos == 2 || pos == 32 {
                    continue;
                }
                calc_entity_financials(g, ui, rng, candidate_id);
                if g.credit_rating_num[candidate_id] < 7 {
                    continue;
                }
                let limit = g.net_worth * risk;
                let liab = get_plr_naked_liab(g, ui, rng, candidate_id, 0, 0);
                if liab + premium <= limit {
                    counterparty_id = candidate_id;
                    break;
                }
            }
        }
    }
    if !(10 < counterparty_id) {
        return counterparty_id;
    }
    candidate_id = counterparty_id;
    if risk_frac < sf(0.25f32) && rng.rnd() * Ext::from_f64(10.0) > Ext::from_f64(9.0) && g.credit_rating_num[candidate_id] < 9 {
        return counterparty_id;
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let strike = Ext80::abs(field_ext(&rec, 5));
    if g.shares_outstanding[stock_id] == Ext::from_f64(0.0) {
        // ERROR 151 -> ON ERROR handler (ShowRuntimeError "MirrorOption"), result 0
        return 0;
    }
    let strike_text = fmt_trim(
        Ext::from(strike.fdiv(ext(g.shares_outstanding[stock_id])).into_ext()),
        b"* ###.00",
    );
    if trade_side == 1 {
        g.cash[candidate_id] = g.cash[candidate_id] - premium;
        g.news_msg = g.ent_symbol[candidate_id].clone()
            + __content.string(2062)
            + &type_text
            + __content.string(10838)
            + &str_s((cast::<Ext, _>(opt_pct)).abs())
            + "% of "
            + &g.ent_symbol[stock_id]
            + " at "
            + &strike_text;
    } else {
        g.cash[candidate_id] = g.cash[candidate_id] + premium;
        g.news_msg = g.ent_symbol[candidate_id].clone()
            + __content.string(10839)
            + &type_text
            + __content.string(10838)
            + &str_s((cast::<Ext, _>(opt_pct)).abs())
            + "% of "
            + &g.ent_symbol[stock_id]
            + " at "
            + &strike_text;
    }
    balance_cash_t_bills(g, candidate_id);
    if 0 < opt_pct {
        opt_pct = -opt_pct;
    } else {
        opt_pct = to_long((cast::<Ext, _>(opt_pct)).abs());
    }
    let exp_month = to_long(field(&rec, 6));
    let exp_year = to_long(field(&rec, 7));
    let mut rec_premium = field_ext(&rec, 8);
    if trade_side == 1 {
        rec_premium = ext(sf(1.02f32)).fmul(rec_premium);
    } else {
        rec_premium = ext(sf(0.98f32)).fmul(rec_premium);
    }
    let sep = "|";
    let new_rec = trim(str_s(cast::<Ext, _>(stock_id)).as_bytes())
        + sep
        + &str_s(cast::<Ext, _>(opt_type))
        + sep
        + &str_s(cast::<Ext, _>(counterparty_id))
        + sep
        + &str_s(cast::<Ext, _>(opt_pct))
        + sep
        + &str_e(strike)
        + sep
        + &str_s(cast::<Ext, _>(exp_month))
        + sep
        + &str_s(cast::<Ext, _>(exp_year))
        + sep
        + &str_e(rec_premium);
    add_or_remove_option_rec(g, ui, &new_rec, 0);
    if g.cur_player == holder_id || g.cur_player == g.corp_controller[holder_id] {
        post_news(g, ui, 0, 0);
    } else {
        g.news_msg = PbStr::new();
    }
    counterparty_id
}

/// The checks of one random candidate in [`find_option_counterparty`] (0x602ab3..0x602dfb).
fn counterparty_ok_random(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    candidate_id: i32,
    stock_id: i32,
    holder_id: i32,
    opt_type: i32,
    trade_side: i32,
    premium: Ext,
    risk: Ext,
) -> bool {
    if stock_id == candidate_id || holder_id == candidate_id {
        return false;
    }
    if g.entity_status[candidate_id] != 1 {
        return false;
    }
    if 0 < get_controller_type(g, candidate_id) {
        return false;
    }
    if 1580 < candidate_id && g.industry_num[candidate_id] == 71 {
        return false;
    }
    if sf(0.24f32) > risk && find_option_positions(g, ui, candidate_id, 0, 0, 0) != 0 {
        return false;
    }
    if trade_side == 1 && 0 < find_option_positions(g, ui, candidate_id, stock_id, opt_type, 0) {
        return false;
    }
    if trade_side == 2 {
        let pos = find_option_positions(g, ui, candidate_id, stock_id, opt_type, 0);
        if pos < 0 || pos == 2 || pos == 32 {
            return false;
        }
    }
    calc_entity_financials(g, ui, rng, candidate_id);
    if trade_side == 1 {
        !(7 > g.credit_rating_num[candidate_id])
            && !(premium > g.liquid_funds * risk)
            && !(premium * sf(0.3f32) > g.cash[candidate_id] + g.t_bills[candidate_id])
            && !(g.long_options_value > Ext::from_f64(0.0))
    } else {
        if 7 > g.credit_rating_num[candidate_id] {
            return false;
        }
        let limit = g.net_worth * risk;
        let liab = get_plr_naked_liab(g, ui, rng, candidate_id, 0, 0);
        liab + premium <= limit
    }
}

/// Port of AiCorpSellNakedPuts (0x5fe20b).
/// `seller_id` (131..1600; not in the first quarter of the game before progress 501) writes puts on 2..5%
/// of a stock at 105% of its market cap, expiring 2..6 months after the current month, books the premium
/// and finds a counterparty ([`find_option_counterparty`], risk 0.26). The stock is `target` if > 10, else:
/// for a fund 1586..1600 (autopilot under a human adviser, no bank debt or bonds, enough cash or fewer than
/// 15 holdings) a random holding <= 5% (15 holdings) or a new pick (`target = 0`, naked liabilities within
/// a quarter of its cash); for others a random 45..78% holding rated at most 2 by analysts with a public
/// float >= 2%, if the seller has a voting controller (not human), credit rating >= 7 and no naked
/// liabilities. Needs a free `OptionRecs(250)` and a share price of 5..940.
///
/// ERROR 151 (`SharesOutstanding = 0`) goes to the ON ERROR handler (ShowRuntimeError); the port just
/// returns.
pub fn ai_corp_sell_naked_puts(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    seller_ref: i32,
    target_ref: i32,
) {
    let __content = crate::content::current();
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 501 {
        return;
    }
    let seller_id = seller_ref;
    if seller_id < 131 || 1600 < seller_id {
        return;
    }
    let mut corp_id = 0;
    if 1585 < seller_id && g.industry_num[seller_id] == 71 {
        if 0 < get_etf_adviser_human_status(g, ui, rng, seller_id)
            && g.auto_pilot[g.fund_adviser[seller_id]] == 0
        {
            return;
        }
        if g.bank_debt[seller_id] > Ext::from_f64(0.0) || g.bonds_issued[seller_id] > 0 {
            return;
        }
        calc_entity_financials(g, ui, rng, seller_id);
        if g.tmp_subsid_stock_value * sf(0.45f32) > g.cash[seller_id] + g.t_bills[seller_id]
            && g.hold_stock_id[(0, seller_id)] == 15
        {
            return;
        }
        if g.hold_stock_id[(0, seller_id)] == 15 {
            let slot = rng.rnd_range(15, 1);
            corp_id = cast::<i32, _>(g.hold_stock_id[(slot, seller_id)]);
            if 5 < g.hold_stock_pct[(slot, seller_id)] {
                return;
            }
        } else if target_ref == 0 {
            corp_id = pick_etf_holding_stock(g, ui, rng, seller_id, 0);
            if corp_id == 0 {
                return;
            }
            let limit = (g.cash[seller_id] + g.t_bills[seller_id]) * sf(0.25f32);
            let liab = get_plr_naked_liab(g, ui, rng, seller_id, 0, 0);
            if liab > limit {
                return;
            }
        }
        if corp_id == 0 {
            return;
        }
        if g.liquid_funds < g.market_cap[corp_id] * Ext::from_f64(5.0) {
            return;
        }
    } else if 1580 < seller_id && g.industry_num[seller_id] == 71 {
        return;
    }
    if g.entity_status[seller_id] != 1 {
        return;
    }
    if !g.option_recs[250].is_empty() {
        return;
    }
    if 10 < target_ref {
        corp_id = target_ref;
    } else if corp_id == 0 {
        if g.hold_stock_id[(0, seller_id)] == 0 {
            return;
        }
        let slot = rng.rnd_range(cast::<i32, _>(g.hold_stock_id[(0, seller_id)]), 1);
        if slot == 0 {
            return;
        }
        update_corp_control(g, ui, rng, seller_id);
        if g.voting_controller == 0 {
            return;
        }
        if get_controller_type(g, seller_id) == 1 {
            return;
        }
        corp_id = cast::<i32, _>(g.hold_stock_id[(slot, seller_id)]);
        if corp_id < 11 {
            return;
        }
        let held_pct = cast::<i32, _>(g.hold_stock_pct[(slot, seller_id)]);
        if held_pct < 45 || 78 < held_pct {
            return;
        }
        if 2 < calc_analyst_rating(g, ui, rng, corp_id) {
            return;
        }
        update_corp_control(g, ui, rng, corp_id);
        if (cast::<i32, _>(g.public_pct)) < 2 {
            return;
        }
        let naked_liab = get_plr_naked_liab(g, ui, rng, seller_id, 0, 0);
        if g.credit_rating_num[seller_id] < 7 || naked_liab > Ext::from_f64(0.0) {
            return;
        }
    }
    if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
        // ERROR 151 -> ON ERROR handler (ShowRuntimeError)
        return;
    }
    let share_price = g.market_cap[corp_id] / g.shares_outstanding[corp_id];
    if share_price < Ext::from_f64(5.0) || !(share_price <= Ext::from_f64(940.0)) {
        return;
    }
    update_corp_control(g, ui, rng, corp_id);
    let mut pct = rng.rnd_range(5, 2);
    if (cast::<i32, _>(g.public_pct)) < 6 {
        pct = cast::<i32, _>(g.public_pct) - 1;
    }
    if pct < 2 {
        return;
    }
    let sep = PbStr::from("|");
    let mut new_rec = str_s(cast::<Ext, _>(corp_id))
        + &sep
        + "2|"
        + &str_s(cast::<Ext, _>(seller_id))
        + &sep
        + &str_s(-(cast::<Ext, _>(pct)))
        + &sep;
    let strike = Ext80::abs(ext(g.market_cap[corp_id]).fmul(ext(sf(1.05f32))));
    new_rec = new_rec + &str_e(strike) + &sep;
    let month = calc_cur_month_of_year(g, g.sim_co_count).ceil();
    let mut exp_month = to_long((Ext::from_f64(2.0) + month) + (Ext::from_f64(5.0) * rng.rnd()).floor());
    let mut exp_year = g.cur_year;
    if 12 < exp_month {
        exp_month = exp_month - 12;
        exp_year = g.cur_year + 1;
    }
    new_rec = new_rec + &str_s(cast::<Ext, _>(exp_month)) + &sep + &str_s(cast::<Ext, _>(exp_year)) + &sep;
    let free_slot = free_option_slot(g);
    g.option_recs[free_slot] = new_rec.clone() + " 0";
    g.has_options_outstanding[corp_id] = 1;
    let opt_price = calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, free_slot);
    if !(opt_price > Ext::from_f64(0.0)) {
        return;
    }
    let premium = Ext80::abs(Ext80::from_i64(cast::<i64, _>(pct)).fmul(ext(opt_price)));
    new_rec = new_rec + &str_e(premium);
    g.option_recs[free_slot] = new_rec.clone();
    g.cash[seller_id] = Ext::from(Ext80::fadd(ext(g.cash[seller_id]), premium).into_ext());
    if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) && !(0 < is_in_recent_headlines(g, __content.bytes(10840))) {
        g.news_msg = g.ent_symbol[seller_id].clone()
            + __content.string(10841)
            + &str_s(cast::<Ext, _>(pct))
            + "% of "
            + &g.ent_symbol[corp_id]
            + " stock.";
        post_news(g, ui, 0, 0);
    }
    find_option_counterparty(g, ui, rng, &new_rec, Ext::from(premium.into_ext()), sf(0.26f32));
}
