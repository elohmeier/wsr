//! New-game and upgrade helpers: earnings history and convertible bonds of a fresh world, the "famous
//! company" tweaks, [runtime resource wording] (ETFs, ids 1581..1600) with their holdings, the country and
//! month name tables, the configuration file `cfig.wsr`, save-game upgrades that add the ETFs, and the
//! creation of banks and insurers in free entity slots.
//!
//! Numeric conventions as in `logic::valuation`: `EXT` uses software x87 arithmetic, SINGLE constants are written `s(0.8)`,
//! the association order of float expressions follows the machine code, comparisons are written so the
//! branch taken for unordered (NaN) operands matches the x87 code.
//!
//! Files: readers take `files: &dyn Fn(&str) -> Option<Vec<u8>>` (base name as the original opens it after
//! `DataPath`, e.g. `"cfig.wsr"`; `None` = the file does not exist); writers return the bytes written.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::neg_multiply,
    clippy::if_same_then_else
)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::holdings::{find_holding_slot, find_stock_holding};
use crate::logic::news::post_news;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::assign_etf_adviser;
use crate::logic::sim_entity::bonds::issue_bonds;
use crate::logic::sim_entity::transactions::etf_drop_one_adviser_contract;
use crate::logic::valuation::{
    adjust_index_divisor, calc_stock_price, check_stock_split, clear_price_history, count_active_corps,
};
use crate::pb::fileio::{InFile, OutFile};
use crate::pb::numeric::format;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, len, ltrim, mid, parse_any, parse_count_any, right, rtrim};
use crate::pb::numeric::{to_long, to_quad, Cur};
use crate::state::Globals;
use crate::ui::Ui;

/// A SINGLE constant of the original (`fld/fmul dword [const]`) as the x87 sees it.
#[inline]
fn s(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &[u8]) -> PbStr {
    rtrim(ltrim(x).as_bytes())
}

/// `RTRIM$(LTRIM$(STR$(x)))` of an EXT/LONG value.
fn str_trim(x: Ext) -> PbStr {
    trim(str_num(x, 15).as_bytes())
}

// ------------------------------------------------------------------------------------------ earnings

/// Port of InitEarningsHistory (0x47d0c7).
/// Gives every active company 131..1500 a plausible earnings history from its projected quarterly
/// earnings (two passes: `EarningsY1` = 4 x the randomized projection, the three previous years derived
/// from it with random noise; pass 2 also sets `TaxLossCarry` for losses and the first month's
/// high/low around `CapHistClose(1)`), values each at once (`ValuationMode = 3`, splitting stocks priced
/// over 130), then re-values every active company 86..1500 normally and raises its market cap by 20%.
///
/// `time_secs` = `VAL(RIGHT$(TIME$, 2))` for the final normal valuations (see
/// [`crate::logic::valuation::calc_stock_price`]).
pub fn init_earnings_history(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    for pass in 1..=2 {
        for corp_id in 131..=1500 {
            if g.entity_status[corp_id] != 1 {
                continue;
            }
            calc_projected_earnings(g, ui, rng, corp_id);
            // proj * (RND * 0.38) + proj * 0.8
            let r = rng.rnd() * s(0.38f32);
            let v = g.proj_qtr_earnings[corp_id] * r + g.proj_qtr_earnings[corp_id] * s(0.8f32);
            if !(v.abs() >= s(0.01f32)) {
                continue;
            }
            g.earnings_y1[corp_id] = v * Ext::from_f64(4.0);
            if pass == 2 && g.earnings_y1[corp_id] < Ext::from_f64(0.0) {
                g.tax_loss_carry[corp_id] = g.earnings_y1[corp_id] * s(1.7f32);
            }
            g.earnings_yr2_ago[corp_id] = g.earnings_y1[corp_id] * s(0.95f32);
            let r = rng.rnd() * s(1.2f32);
            g.earnings_yr2_ago[corp_id] = g.earnings_y1[corp_id] * r + g.earnings_yr2_ago[corp_id] * s(0.4f32);
            g.earnings_yr3_ago[corp_id] = g.earnings_yr2_ago[corp_id] * s(0.92f32);
            let r = rng.rnd() * Ext::from_f64(1.0);
            g.earnings_yr3_ago[corp_id] = g.earnings_yr2_ago[corp_id] * r + g.earnings_yr3_ago[corp_id] * s(0.5f32);
            g.earnings_yr4_ago[corp_id] = g.earnings_yr3_ago[corp_id] * s(0.9f32);
            let r = rng.rnd() * s(0.8f32);
            g.earnings_yr4_ago[corp_id] = g.earnings_yr3_ago[corp_id] * r + g.earnings_yr4_ago[corp_id] * s(0.6f32);
            g.valuation_mode = 3;
            calc_stock_price(g, ui, rng, corp_id, time_secs);
            if g.tmp_share_price > Ext::from_f64(130.0) {
                check_stock_split(g, corp_id);
            }
            if pass == 2 {
                g.price_hist_high[(1, corp_id)] = s(1.08f32) * g.cap_hist_close[(1, corp_id)];
                g.price_hist_low[(1, corp_id)] = s(0.93f32) * g.cap_hist_close[(1, corp_id)];
            }
        }
    }
    for corp_id in 86..=1500 {
        if g.entity_status[corp_id] == 1 {
            calc_projected_earnings(g, ui, rng, corp_id);
            calc_stock_price(g, ui, rng, corp_id, time_secs);
            g.market_cap[corp_id] = s(1.2f32) * g.market_cap[corp_id];
        }
    }
}

/// `"cv|" + RTRIM$(LTRIM$(FORMAT$(convPrice, "* ###.00"))) + "|@" + couponText` after the maturity.
fn convertible_info(maturity_text: &PbStr, conv_price: Ext, coupon_text: &PbStr) -> PbStr {
    maturity_text.clone() + "cv|" + &trim(&format(conv_price, b"* ###.00")) + "|@" + coupon_text
}

/// Port of InitConvertibleBondIssues (0x47d87a).
/// 25 random draws of a company 11..1500; an active one [runtime resource wording] 2 or more gets a
/// convertible bond issue: without bonds a new issue of 10..40% of its net worth (if at least
/// `ExchangeRate * 100`; cash and `BondsIssued` increase; maturity 1..7 years after `StartYear`, coupon
/// `BorrowRate - 3` rounded to 0.05 and limited to 1..9.5), otherwise its existing non-convertible issue
/// is made convertible with the coupon lowered by 3 (1 below 1.25). `BondIssueInfo` becomes
/// `"<year>cv|<conversion value>|@<coupon>"` with a conversion value of 60..150% (new) or 80..130%
/// (existing) of the market cap.
pub fn init_convertible_bond_issues(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let mut remaining = 25;
    loop {
        let corp_id = rng.rnd_range(1500, 11);
        if g.entity_status[corp_id] == 1 {
            calc_entity_financials(g, ui, rng, corp_id);
            if 2 <= g.credit_rating_num[corp_id] {
                if g.bonds_issued[corp_id] == 0 {
                    // NetWorth * 0.1 + NetWorth * (RND * 0.3)
                    let a = g.net_worth * s(0.1f32);
                    let r = rng.rnd() * s(0.3f32);
                    let issue_amt = to_quad(a + g.net_worth * r);
                    if cast::<Ext, _>(issue_amt) >= g.exchange_rate * Ext::from_f64(100.0) {
                        g.cash[corp_id] = cast::<Ext, _>(issue_amt) + g.cash[corp_id];
                        g.bonds_issued[corp_id] = issue_amt;
                        let r = rng.rnd() * Ext::from_f64(6.0);
                        let maturity_year = to_long(r + (Ext::from_f64(1.0) + cast::<Ext, _>(g.start_year)));
                        let maturity_text = str_trim(cast::<Ext, _>(maturity_year));
                        let mut coupon = Cur::from_f64(cast::<Ext, _>(g.borrow_rate) - Ext::from_f64(3.0));
                        let coupon_x20 = to_long(coupon.get() * Ext::from_f64(20.0));
                        coupon.set(cast::<Ext, _>(coupon_x20) / Ext::from_f64(20.0));
                        if !(coupon.get() >= Ext::from_f64(1.0)) {
                            coupon.set(Ext::from_f64(1.0));
                        }
                        if coupon.get() > s(9.5f32) {
                            coupon.set(s(9.5f32));
                        }
                        let coupon_text = trim(&format(coupon.get(), b"0#.00"));
                        let r = rng.rnd() * s(0.9f32);
                        let conv_price = g.market_cap[corp_id] * r + g.market_cap[corp_id] * s(0.6f32);
                        g.bond_issue_info[corp_id] = convertible_info(&maturity_text, conv_price, &coupon_text);
                    }
                } else if instr(1, g.bond_issue_info[corp_id].as_bytes(), b"cv") < 1 {
                    let mut coupon = Cur::from_f64(val(right(g.bond_issue_info[corp_id].as_bytes(), 5).as_bytes()) - Ext::from_f64(3.0));
                    if !(coupon.get() >= s(1.25f32)) {
                        coupon.set(Ext::from_f64(1.0));
                    }
                    let coupon_text = trim(&format(coupon.get(), b"0#.00"));
                    let maturity_text = mid(g.bond_issue_info[corp_id].as_bytes(), 1, 4);
                    let r = rng.rnd() * s(0.5f32);
                    let conv_price = g.market_cap[corp_id] * r + g.market_cap[corp_id] * s(0.8f32);
                    g.bond_issue_info[corp_id] = convertible_info(&maturity_text, conv_price, &coupon_text);
                }
            }
        }
        remaining -= 1;
        if remaining == 0 {
            break;
        }
    }
}

/// Port of ApplyFamousCorpTweaks (0x47c2c5).
/// For the "famous" companies of the name list (140 TSLA, 423 SPCX, 621 AAPL, 954 GOOG, 620 IBM: a random
/// profit margin if the symbol matches; 1499 BKH, 1431 TOY: only with a matching symbol) tops up a low
/// cash balance and values the company at once (`ValuationMode = 3`). Other ids and companies without
/// shares are left alone.
pub fn apply_famous_corp_tweaks(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32) {
    let corp = corp_id;
    if !(g.shares_outstanding[corp] > Ext::from_f64(0.0)) {
        return;
    }
    // (symbol, margin = RND * a + b, cash below rate * lo gets + rate * add); BKH/TOY: cash only with the symbol
    let rate = g.exchange_rate;
    let margin = |g: &mut Globals, rng: &mut Rng, sym: &str, a: Ext, b: Ext| {
        if g.ent_symbol[corp] == sym {
            g.profit_margin[corp] = cast::<i16, _>(to_quad(rng.rnd() * a + b));
        }
    };
    match corp {
        140 => {
            margin(g, rng, "TSLA", Ext::from_f64(11.0), Ext::from_f64(10.0));
            if !(g.cash[corp] >= rate * Ext::from_f64(75000.0)) {
                g.cash[corp] = g.cash[corp] + rate * Ext::from_f64(50000.0);
            }
        }
        423 => {
            margin(g, rng, "SPCX", Ext::from_f64(10.0), Ext::from_f64(5.0));
            if !(g.cash[corp] >= rate * Ext::from_f64(50000.0)) {
                g.cash[corp] = g.cash[corp] + rate * Ext::from_f64(45000.0);
            }
        }
        621 => {
            margin(g, rng, "AAPL", Ext::from_f64(10.0), Ext::from_f64(15.0));
            if !(g.cash[corp] >= rate * Ext::from_f64(70000.0)) {
                g.cash[corp] = g.cash[corp] + rate * Ext::from_f64(70000.0);
            }
        }
        954 => {
            margin(g, rng, "GOOG", Ext::from_f64(6.0), Ext::from_f64(10.0));
            if !(g.cash[corp] >= rate * Ext::from_f64(35000.0)) {
                g.cash[corp] = g.cash[corp] + rate * Ext::from_f64(35000.0);
            }
        }
        1499 => {
            if g.ent_symbol[corp] == "BKH" && !(g.cash[corp] >= rate * Ext::from_f64(6000.0)) {
                g.cash[corp] = g.cash[corp] + rate * Ext::from_f64(12000.0);
            }
        }
        620 => {
            margin(g, rng, "IBM", Ext::from_f64(10.0), Ext::from_f64(8.0));
            if !(g.cash[corp] >= rate * Ext::from_f64(20000.0)) {
                g.cash[corp] = g.cash[corp] + rate * Ext::from_f64(20000.0);
            }
        }
        1431 => {
            if g.ent_symbol[corp] == "TOY" && !(g.cash[corp] >= rate * Ext::from_f64(35000.0)) {
                g.cash[corp] = g.cash[corp] + rate * Ext::from_f64(35000.0);
            }
        }
        _ => return,
    }
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, corp, 0); // mode 3 does not use TIME$
}

// ---------------------------------------------------------------------------------------------- ETFs

/// Port of InitETFs (0x648a05) with its GOSUB InitETFs_PickHoldings (0x649195).
/// Names industry 71 and sets up every fund slot 1581..1600 that is not yet in use: active, 2 shares,
/// cash `RND(200..850) * ExchangeRate` (+15000 for the bond/leveraged funds 1581..1585, plus 500000 /
/// 80000 / 20000 / 25000 / 25000 for 1581..1585), lending bank `id - 1540`, an adviser
/// ([`assign_etf_adviser`]), fee 0.2% for 1581..1585, up to 15 holdings of 5..10% for the sector/country
/// funds 1586..1600 (PickHoldings, which also tops up their cash to at least 25000 of assets), a random
/// extraordinary loss for a third of them, and an immediate valuation (`ValuationMode = 3`).
/// In a new game (`GameInProgress = 0`) also: extra cash for 1600, a bond issue for a random fund
/// 1586..1600, for 1581 (12 years, 3.75%) and, half of the time, for 1583 (8 years, 5.50%).
///
/// PickHoldings loops until 15 holdings are found; it never ends if [`pick_etf_holding_stock`] keeps
/// finding nothing (original behaviour).
pub fn init_etfs(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> bool {
    let __content = crate::content::current();
    if ui.safety_checks() {
        // The Customizer can remove all companies of a country or sector. Check distinct, publicly
        // available candidates on a copy: preflight must not alter control, scratch globals or RNG.
        let mut probe = g.clone();
        let mut probe_rng = *rng;
        let mut probe_ui = crate::ui::Scripted::default();
        for fund in 1586..=1600 {
            if g.entity_status[fund] > 0 { continue; }
            let mut eligible = 0;
            for company in 11..=1580 {
                if g.entity_status[company] != 1 { continue; }
                let mut picked = 0;
                let industry = cast::<i32, _>(g.industry_num[company]);
                sector_match(&mut probe, &mut probe_ui, &mut probe_rng, fund, 0, company, industry, &mut picked);
                if picked != 0 { eligible += 1; }
            }
            if eligible < 15 {
                let message = st(&format!("Cannot initialize ETF {fund}: it needs 15 eligible companies, but only {eligible} are available. Restore the company template or choose another saved game."));
                ui.game_error(&message, &g.app_title);
                return false;
            }
        }
    }
    g.ind_name[71] = st(__content.string(6736));
    for etf_id in 1581..=1600 {
        if g.entity_status[etf_id] > 0 {
            continue;
        }
        g.entity_status[etf_id] = 1;
        g.industry_num[etf_id] = 71;
        g.price_hist_high[(0, etf_id)] = -Ext::from_f64(1.0);
        g.shares_outstanding[etf_id] = Ext::from_f64(2.0);
        let n = rng.rnd_range(850, 200);
        g.cash[etf_id] = g.exchange_rate * cast::<Ext, _>(n);
        if etf_id < 1586 {
            g.cash[etf_id] = g.cash[etf_id] + g.exchange_rate * Ext::from_f64(15000.0);
        }
        g.lending_bank[etf_id] = cast::<i16, _>(etf_id - 1540);
        assign_etf_adviser(g, ui, rng, etf_id, 0, 0);
        if etf_id < 1586 {
            g.fund_fee_pct[etf_id] = cast::<f32, _>(0.2f32);
        }
        if etf_id > 1585 {
            if !init_etfs_pick_holdings(g, ui, rng, etf_id) { return false; }
        } else if etf_id > 1583 {
            if etf_id == 1584 || etf_id == 1585 {
                g.cash[etf_id] = g.cash[etf_id] + g.exchange_rate * Ext::from_f64(25000.0);
            }
        } else {
            if etf_id == 1581 {
                g.cash[etf_id] = g.cash[etf_id] + g.exchange_rate * Ext::from_f64(500000.0);
            }
            if etf_id == 1582 {
                g.cash[etf_id] = g.cash[etf_id] + g.exchange_rate * Ext::from_f64(80000.0);
            }
            if etf_id == 1583 {
                g.cash[etf_id] = g.cash[etf_id] + g.exchange_rate * Ext::from_f64(20000.0);
            }
        }
        if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
            let r = rng.rnd() * Ext::from_f64(2000.0);
            g.extraordinary_pl[etf_id] = to_quad(-Ext::from_f64(1.0) * r);
        }
        g.valuation_mode = 3;
        calc_stock_price(g, ui, rng, etf_id, 0); // mode 3 does not use TIME$
    }
    if g.game_in_progress == 0 {
        let r = rng.rnd() * Ext::from_f64(5000.0);
        g.cash[1600] = g.cash[1600] + g.exchange_rate * (r + Ext::from_f64(10000.0));
        let etf_id = rng.rnd_range(15, 1) + 1585;
        let mut coupon_rate = cast::<f32, _>(rng.rnd_range(19, 11));
        coupon_rate = cast::<f32, _>(s(0.25f32) * cast::<Ext, _>(coupon_rate));
        let r = rng.rnd() * Ext::from_f64(5.0);
        let maturity_year = to_long((Ext::from_f64(15.0) + cast::<Ext, _>(g.cur_year)) + r);
        let bond_text = str_trim(cast::<Ext, _>(maturity_year));
        g.bond_issue_info[etf_id] = bond_text + "@";
        let bond_text = PbStr(format(cast::<Ext, _>(coupon_rate), b"0#.00"));
        g.bond_issue_info[etf_id] = g.bond_issue_info[etf_id].clone() + &bond_text;
        let n = rng.rnd_range(11, 2);
        let amt = to_long(g.exchange_rate * (Ext::from_f64(100.0) * cast::<Ext, _>(n) + Ext::from_f64(3000.0)));
        g.bonds_issued[etf_id] = cast::<i64, _>(amt);
        g.cash[etf_id] = cast::<Ext, _>(amt) + g.cash[etf_id];
        g.bond_issue_info[1581] = str_trim(Ext::from_f64(12.0) + cast::<Ext, _>(g.cur_year)) + "@03.75";
        let n = rng.rnd_range(10, 2);
        let amt = to_long(g.exchange_rate * (Ext::from_f64(5000.0) * cast::<Ext, _>(n) + Ext::from_f64(50000.0)));
        g.bonds_issued[1581] = cast::<i64, _>(amt);
        g.cash[1581] = cast::<Ext, _>(amt) + g.cash[1581];
        if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
            g.bond_issue_info[1583] = str_trim(Ext::from_f64(8.0) + cast::<Ext, _>(g.cur_year)) + "@05.50";
            let n = rng.rnd_range(10, 1);
            let amt = to_long(g.exchange_rate * (Ext::from_f64(1000.0) * cast::<Ext, _>(n) + Ext::from_f64(5000.0)));
            g.bonds_issued[1583] = cast::<i64, _>(amt);
            g.cash[1583] = cast::<Ext, _>(amt) + g.cash[1583];
        }
    }
    true
}

/// InitETFs_PickHoldings (0x649195), GOSUB of [`init_etfs`]: fills holding slots 1..15 of fund `etf_id`
/// with stocks from [`pick_etf_holding_stock`] (5..10% each), then tops up its cash.
fn init_etfs_pick_holdings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf_id: i32) -> bool {
    let mut hold_slot = 1;
    let mut failures = 0;
    loop {
        let stock_id = pick_etf_holding_stock(g, ui, rng, etf_id, 0);
        if stock_id < 11 {
            hold_slot -= 1;
            failures += 1;
        } else if find_stock_holding(g, etf_id, stock_id) > 0 {
            hold_slot -= 1;
            failures += 1;
        } else {
            g.hold_stock_id[(hold_slot, etf_id)] = cast::<i16, _>(stock_id);
            let pct = to_quad(rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(5.0));
            g.hold_stock_pct[(hold_slot, etf_id)] = cast::<i16, _>(pct);
            g.hold_stock_id[(0, etf_id)] = g.hold_stock_id[(0, etf_id)].wrapping_add(1);
            if hold_slot == 15 {
                break;
            }
        }
        if ui.safety_checks() && failures >= 15 {
            ui.game_error(&st(&format!("Cannot finish ETF {etf_id}: no eligible holding was found after repeated bounded searches. Restore the company template or choose another saved game.")), &g.app_title);
            return false;
        }
        hold_slot += 1;
        if hold_slot > 15 {
            break;
        }
    }
    calc_entity_financials(g, ui, rng, etf_id);
    let asset_shortfall = g.exchange_rate * Ext::from_f64(25000.0) - g.total_assets * g.exchange_rate;
    if asset_shortfall > Ext::from_f64(0.0) {
        let r = rng.rnd() * Ext::from_f64(1.0);
        g.cash[etf_id] = (g.cash[etf_id] + asset_shortfall) + asset_shortfall * r;
    } else {
        g.cash[etf_id] = g.cash[etf_id] + g.exchange_rate * Ext::from_f64(10000.0);
    }
    true
}

/// Whether fund `fund_id` (1586..1600) invests in a company of `industry` / country `country`
/// (the SELECT CASE of PickEtfHoldingStock_SectorMatch 0x6494e6; fund 1593 selects by country).
fn etf_sector_match(fund_id: i32, industry: i32, country: i32) -> bool {
    let ind = industry;
    match fund_id {
        1586 => [3, 18, 55, 67, 68].contains(&ind),
        1587 => [4, 10, 24, 26, 30, 35, 38, 39, 40, 61, 64].contains(&ind),
        1588 => [7, 8, 63, 69].contains(&ind),
        1589 => [15, 17, 28, 34, 46, 57].contains(&ind),
        1590 => [4, 21, 22, 43, 56].contains(&ind),
        1591 => (41 <= ind && ind <= 48) || ind == 57,
        1592 => [11, 16, 23, 25, 52, 53].contains(&ind),
        1593 => [8, 18, 22, 26, 27, 28, 35, 40, 47, 48].contains(&country),
        1594 => [12, 13, 14, 20, 49, 53, 54, 64].contains(&ind),
        1595 => [1, 2, 37, 70].contains(&ind),
        1596 => [17, 19, 28, 31, 32, 33, 34, 58, 60, 66].contains(&ind),
        1597 => [5, 6, 59, 60, 63].contains(&ind),
        1598 => [27, 29, 36, 39, 62, 65].contains(&ind),
        1599 => [7, 9, 12, 15, 33, 50].contains(&ind),
        1600 => [6, 9, 12, 20, 33, 50, 51].contains(&ind),
        _ => false,
    }
}

/// Port of PickEtfHoldingStock (0x649382) with its GOSUB PickEtfHoldingStock_SectorMatch (0x6494e6).
/// A company for sector/country fund `etf_id` (1586..1600; 0 for other funds):
/// * `stock_id > 10`: checks that company — returns it if it is in industry 70 or matches the fund's
///   sectors, else 0 (no further checks);
/// * otherwise draws up to 2000 active companies 11..1580 until one matches the fund's sectors and is
///   at least 11% publicly held (after [`update_corp_control`], `PublicPct`) and can take a holding slot
///   of the fund (with `stock_id = 0`: not already held). Returns 0 if none was found.
pub fn pick_etf_holding_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf_id: i32, stock_id: i32) -> i32 {
    let fund_id = etf_id;
    let mut picked_id = 0;
    let mut tries = 0;
    if fund_id < 1586 {
        return 0;
    }
    if 10 < stock_id {
        let candidate_id = stock_id;
        let industry = cast::<i32, _>(g.industry_num[candidate_id]);
        if industry == 70 {
            picked_id = candidate_id;
        } else {
            sector_match(g, ui, rng, fund_id, stock_id, candidate_id, industry, &mut picked_id);
        }
        return picked_id;
    }
    while picked_id == 0 && tries < 2000 {
        let candidate_id = rng.rnd_range(1580, 11);
        if ui.safety_checks() { tries += 1; }
        if g.entity_status[candidate_id] == 1 {
            let industry = cast::<i32, _>(g.industry_num[candidate_id]);
            if !ui.safety_checks() { tries += 1; }
            sector_match(g, ui, rng, fund_id, stock_id, candidate_id, industry, &mut picked_id);
        }
    }
    picked_id
}

/// PickEtfHoldingStock_SectorMatch (0x6494e6), GOSUB of [`pick_etf_holding_stock`].
#[allow(clippy::too_many_arguments)]
fn sector_match(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    fund_id: i32,
    stock_id: i32,
    candidate_id: i32,
    industry: i32,
    picked_id: &mut i32,
) {
    // CountryIdx is only read for fund 1593
    let country = if fund_id == 1593 { g.country_idx[candidate_id] } else { 0 };
    if etf_sector_match(fund_id, industry, country) {
        *picked_id = candidate_id;
    }
    if 10 < stock_id {
        return;
    }
    if 10 < *picked_id {
        update_corp_control(g, ui, rng, *picked_id);
        if g.public_pct < 11 {
            *picked_id = 0;
        } else {
            let hold_slot = find_holding_slot(g, fund_id, *picked_id);
            if hold_slot == 0 {
                *picked_id = 0;
            } else if cast::<i32, _>(g.hold_stock_id[(hold_slot, fund_id)]) == *picked_id && stock_id == 0 {
                *picked_id = 0;
            }
        }
    }
}

/// Port of UpgradeSaveAddETFs (0x649fc9).
/// Upgrade of a loaded game: if no fund slot 1581..1600 is in use (saves before v6.30), tells the player
/// (ShowQuickMessage), creates the funds ([`init_etfs`]), re-values them and sets the cost basis of their
/// holdings to 70% of the current value. If the first used slot is 1586..1600 and an ETF (games with only
/// 15 funds), adds the five new ones ([`upgrade_save_add_extra_etfs`]).
pub fn upgrade_save_add_etfs(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> bool {
    let __content = crate::content::current();
    for etf_id in 1581..=1600 {
        if g.entity_status[etf_id] > 0 {
            if etf_id < 1586 {
                return true;
            }
            if g.industry_num[etf_id] == 71 {
                return upgrade_save_add_extra_etfs(g, ui, rng);
            }
            return true;
        }
    }
    let mut msg = st(
        __content.string(6737),
    );
    show_quick_message(g, ui, &mut msg);
    if !init_etfs(g, ui, rng) { return false; }
    for etf_id in 1581..=1600 {
        calc_stock_price(g, ui, rng, etf_id, 0); // funds are always valued in mode 3: TIME$ unused
        for hold_slot in 1..=15 {
            let stock = g.hold_stock_id[(hold_slot, etf_id)];
            if stock != 0 {
                let pct = g.hold_stock_pct[(hold_slot, etf_id)];
                g.hold_stock_basis[(hold_slot, etf_id)] = g.market_cap[cast::<i32, _>(stock)] * cast::<Ext, _>(pct) * s(0.7f32);
            }
        }
    }
    true
}

/// Port of UpgradeSaveAddExtraETFs (0x64a21d) with its GOSUB UpgradeSaveAddExtraETFs_SetSymbol (0x64a55b).
/// Upgrade of a game with only the 15 sector funds: tells the player, creates the funds 1581..1585
/// ([`init_etfs`]) and gives them their names, countries and (if not taken) symbols GOVB, BCFI, JUNK,
/// BULL, BEAR; sets `NameListDirty`.
pub fn upgrade_save_add_extra_etfs(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> bool {
    let __content = crate::content::current();
    let mut msg = st(
        __content.string(6738),
    );
    show_quick_message(g, ui, &mut msg);
    if !init_etfs(g, ui, rng) { return false; }
    for etf_id in 1581..=1585 {
        if g.entity_status[etf_id] != 1 || g.industry_num[etf_id] != 71 {
            continue;
        }
        calc_stock_price(g, ui, rng, etf_id, 0); // funds are always valued in mode 3: TIME$ unused
        let (name, country, symbol) = match etf_id {
            1581 => (__content.string(6739), 0, "GOVB"),
            1582 => (__content.string(6740), 6, "BCFI"),
            1583 => (__content.string(6741), 8, "JUNK"),
            1584 => (__content.string(2566), 0, "BULL"),
            _ => (__content.string(2568), 19, "BEAR"),
        };
        g.ent_name[etf_id] = st(name);
        g.country_idx[etf_id] = country;
        // GOSUB SetSymbol: only if no entity 11..1600 has the symbol yet
        let symbol = st(symbol);
        let mut is_unique = 1;
        for ent_idx in 11..=1600 {
            if g.ent_symbol[ent_idx] == symbol {
                is_unique = 0;
            }
        }
        if is_unique == 1 {
            g.ent_symbol[etf_id] = symbol;
        }
    }
    g.name_list_dirty = 1;
    true
}

// --------------------------------------------------------------------------------------- name tables

/// The DATA table of LoadCountryNames, read with `READ$` (PB_ReadStr 0x6b8027) from the code segment at
/// 0x47c940 (a word count of 54, then NUL-terminated strings): `"COUNTRY        | EXCHANGE CITY"`.
#[allow(non_snake_case)]
pub fn COUNTRY_DATA(__content: &crate::content::ContentPack) -> [&str; 54] { [
    __content.string(6742),
    __content.string(6743),
    __content.string(6744),
    __content.string(6745),
    __content.string(6746),
    __content.string(6747),
    __content.string(6748),
    __content.string(6749),
    __content.string(6750),
    __content.string(6751),
    __content.string(6752),
    __content.string(6753),
    __content.string(6754),
    __content.string(6755),
    __content.string(6756),
    __content.string(6757),
    __content.string(6758),
    __content.string(6759),
    __content.string(6760),
    __content.string(6761),
    __content.string(6762),
    __content.string(6763),
    __content.string(6764),
    __content.string(6765),
    __content.string(6766),
    __content.string(6767),
    __content.string(6768),
    __content.string(6769),
    __content.string(6770),
    __content.string(6771),
    __content.string(6772),
    __content.string(6773),
    __content.string(6774),
    __content.string(6775),
    __content.string(6776),
    __content.string(6777),
    __content.string(6778),
    __content.string(6779),
    __content.string(6780),
    __content.string(6781),
    __content.string(6782),
    __content.string(6783),
    __content.string(6784),
    __content.string(6785),
    __content.string(6786),
    __content.string(6787),
    __content.string(6788),
    __content.string(6789),
    __content.string(6790),
    __content.string(6791),
    __content.string(6792),
    __content.string(6793),
    __content.string(6794),
    __content.string(6795),
] }

/// Port of LoadCountryNames (0x47ceaf).
/// `CountryName(0..53)` = the trimmed first 15 characters of each [`COUNTRY_DATA`] line,
/// `CountryDisplayName(0..53)` = the trimmed text after column 16 (the exchange's city).
pub fn load_country_names(g: &mut Globals) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x47d033 (ShowRuntimeError) — no error can occur here.
    for idx in 1..55 {
        let data_line = st(COUNTRY_DATA(&__content)[cast::<usize, _>(idx - 1)]);
        let field = trim(mid(data_line.as_bytes(), 1, 15).as_bytes());
        g.country_name[idx - 1] = field;
        let name_width = 16;
        let field = right(data_line.as_bytes(), len(data_line.as_bytes()) - name_width);
        g.country_display_name[idx - 1] = trim(field.as_bytes());
    }
}

/// Port of InitMonthNames (0x4a010e).
/// `MonthName(1..12)` = "January" .. "December".
pub fn init_month_names(g: &mut Globals) {
    let __content = crate::content::current();
    let months: [&str; 12] = [
        "January", __content.string(6796), "March", "April", "May", "June", "July", "August", __content.string(6797), "October",
        __content.string(6798), __content.string(6799),
    ];
    for (k, m) in months.iter().enumerate() {
        g.month_name[cast::<i32, _>(k) + 1] = st(m);
    }
}

// ------------------------------------------------------------------------------------------- config

/// `PARSE$(line, ANY "|", n)`.
fn field(line: &PbStr, n: i32) -> PbStr {
    parse_any(line.as_bytes(), b"|", n)
}

/// `CINT`-style `x = VAL(PARSE$(line, ANY "|", n))` into a LONG (`fistp dword`).
fn field_long(line: &PbStr, n: i32) -> i32 {
    to_long(val(field(line, n).as_bytes()))
}

/// Port of LoadConfigAndCheckFiles (0x46bfff) with its GOSUB LoadConfigAndCheckFiles_Check (0x46c957).
/// Checks that SCORES.WSR, WSR101.DAT and MSCNEWS.DAT exist (`FileExists`, via `files`): a missing
/// WSR101.DAT gives its own message box; any missing file gives the "Unable to proceed" message box (style
/// 8208, title `AppTitle`) and sets `GameEndFlag = "Y"`. Then reads the first line of `cfig.wsr` if it
/// exists (else `ConfigLoaded = "N"`, `TickerSpeed = 50`): with 8, 12, 15..18, 20..23 or 25 `|`-fields the
/// preferences are loaded (`ConfigLoaded = "Y"`: currency, ticker speed, player names, and — depending on the
/// field count — players, start net worth, game length, difficulty (4 → 3), popups, earnings, autosave,
/// options, chart size, streaming list, delivery, sweep, cash warnings, fields 20/23/24), otherwise
/// `ConfigLoaded = "N"`.
pub fn load_config_and_check_files(g: &mut Globals, ui: &mut dyn Ui, files: &dyn Fn(&str) -> Option<Vec<u8>>) {
    let __content = crate::content::current();
    let mut missing_files = PbStr::new();
    // GOSUB LoadConfigAndCheckFiles_Check: filePath = DataPath + checkFile: exists = FileExists(filePath)
    let check = |name: &str, missing: &mut PbStr| -> bool {
        let found = files(name).is_some();
        if !found {
            *missing = missing.clone() + name + "  ";
        }
        found
    };
    check("SCORES.WSR", &mut missing_files);
    let exists = check("WSR101.DAT", &mut missing_files);
    if !exists {
        ui.msgbox(&st(__content.string(6800)), 8208, &g.app_title);
    }
    check("MSCNEWS.DAT", &mut missing_files);
    if !missing_files.is_empty() {
        let mut field_text = st(__content.string(6801))
            + __content.string(6802)
            + &g.exe_name
            + " ...."
            + "\r"
            + "\r"
            + __content.string(6803)
            + __content.string(6804)
            + __content.string(6805)
            + &g.exe_name
            + __content.string(6806)
            + __content.string(6807)
            + "\r"
            + "\r";
        field_text = field_text + __content.string(6808) + &missing_files;
        ui.msgbox(&field_text, 8208, &g.app_title);
        g.game_end_flag = st("Y");
        return;
    }
    // filePath = DataPath + "cfig.wsr": exists = FileExists(filePath)
    let Some(cfg) = files("cfig.wsr") else {
        g.config_loaded = st("N");
        g.ticker_speed = 50;
        return;
    };
    let mut f = InFile::new(&cfg);
    let cfg_line = f.line();
    let n = parse_count_any(cfg_line.as_bytes(), b"|");
    if !matches!(n, 8 | 12 | 15 | 16 | 17 | 18 | 20 | 21 | 22 | 23 | 25) {
        g.config_loaded = st("N");
        return; // CLOSE
    }
    g.config_loaded = st("Y");
    g.currency_code = field(&cfg_line, 1);
    g.pref_currency = g.currency_code.clone();
    g.ticker_speed = field_long(&cfg_line, 2);
    for i in 1..=5 {
        g.pref_player_names[i] = field(&cfg_line, i + 2);
    }
    if n >= 12 {
        g.num_players = field_long(&cfg_line, 8);
        g.start_net_worth_cfg = field_long(&cfg_line, 9);
        g.cfg_game_length_years = field_long(&cfg_line, 10);
        g.difficulty_level = field_long(&cfg_line, 11);
        if g.difficulty_level == 4 {
            g.difficulty_level = 3;
        }
    }
    if n >= 15 {
        g.suppress_popups = field_long(&cfg_line, 12);
        g.cur_suppress_earnings = field_long(&cfg_line, 13);
        g.cur_auto_save = field_long(&cfg_line, 14);
    }
    if n >= 16 {
        g.cur_exercise_options = field_long(&cfg_line, 15);
    }
    if n >= 17 {
        g.chart_size = field_long(&cfg_line, 16);
    }
    if n >= 18 {
        g.auto_add_stream_list = field_long(&cfg_line, 17);
    }
    if n >= 20 {
        g.cur_make_delivery = field_long(&cfg_line, 18);
        g.cur_take_delivery = field_long(&cfg_line, 19);
    }
    if n >= 21 {
        g.cfg_field20 = field_long(&cfg_line, 20);
    }
    if n >= 22 {
        g.cur_sweep_cash = field_long(&cfg_line, 21);
    }
    if n >= 23 {
        g.cur_suppress_cash_warn = field_long(&cfg_line, 22);
    }
    if n >= 24 {
        g.cfg_field23 = field_long(&cfg_line, 23);
    }
    if n >= 25 {
        g.cfg_field24 = field_long(&cfg_line, 24);
    }
}

/// Port of SaveConfig (0x46cdb0), as the game always calls it: `SaveConfig(PrefCurrency)` (the BYREF
/// currency parameter is `PrefCurrency` itself).
/// Fills in defaults (2 players, net worth 500, 2 years, difficulty 2 (also `CfgDifficultyRaw`), currency
/// "USA" when none is set, ticker speed 50, the five default player names) and returns the contents of
/// `cfig.wsr`: one `PRINT #` line of 25 `|`-terminated fields (`STR$` numbers with their leading space);
/// sets `ConfigLoaded = "Y"`. Note that field 11 is `CfgDifficultyRaw`, which LoadConfigAndCheckFiles
/// reads back into `DifficultyLevel`.
pub fn save_config(g: &mut Globals) -> Vec<u8> {
    let __content = crate::content::current();
    if g.num_players == 0 {
        g.num_players = 2;
    }
    if g.start_net_worth_cfg == 0 {
        g.start_net_worth_cfg = 500;
    }
    if g.cfg_game_length_years == 0 {
        g.cfg_game_length_years = 2;
    }
    if g.difficulty_level == 0 {
        g.difficulty_level = 2;
        g.cfg_difficulty_raw = 2;
    }
    // currencyCode = BYREF PrefCurrency
    if g.pref_currency.is_empty() && g.currency_code.is_empty() && g.pref_currency.is_empty() {
        g.currency_code = st("USA");
    }
    if g.pref_currency.is_empty() {
        g.pref_currency = g.currency_code.clone();
    }
    // IF currencyCode = "" THEN currencyCode = PrefCurrency (a no-op through the alias)
    if g.ticker_speed == 0 {
        g.ticker_speed = 50;
    }
    let default_names: [&str; 5] = [__content.string(6719), __content.string(6809), __content.string(6810), __content.string(6811), __content.string(6812)];
    for (k, name) in default_names.iter().enumerate() {
        let i = cast::<i32, _>(k) + 1;
        if g.pref_player_names[i].is_empty() {
            g.pref_player_names[i] = st(name);
        }
    }
    let num = |x: i32| PbStr::from(str_num(cast::<Ext, _>(x), 15).as_str());
    let mut line = g.pref_currency.clone() + "|" + &num(g.ticker_speed) + "|";
    for i in 1..=5 {
        line = line + &g.pref_player_names[i] + "|";
    }
    for x in [
        g.num_players,
        g.start_net_worth_cfg,
        g.cfg_game_length_years,
        g.cfg_difficulty_raw,
        g.suppress_popups,
        g.cur_suppress_earnings,
        g.cur_auto_save,
        g.cur_exercise_options,
        g.chart_size,
        g.auto_add_stream_list,
        g.cur_make_delivery,
        g.cur_take_delivery,
        g.cfg_field20,
        g.cur_sweep_cash,
        g.cur_suppress_cash_warn,
        g.cfg_field23,
        g.cfg_field24,
    ] {
        line = line + &num(x) + "|";
    }
    let mut out = OutFile::default();
    out.print(&line);
    g.config_loaded = st("Y");
    out.data
}

// ------------------------------------------------------------------------------- entity slots, records

/// Port of FindFreeEntitySlot (0x504a99).
/// For a new company `entity_id` (> 10) picks a random unused slot of the same default industry
/// (`industry_id` 1 = bank: `entity_id..85`; 2 = insurer: `entity_id..130`; else `entity_id+1..entity_id+7`;
/// ids over 1500: `entity_id+1..1580`) and, if it fits (`IndustryNum = industry_id`, `EntityStatus = 0`),
/// swaps the two slots' names, symbols and countries (`NameListDirty = 1`) so the new company gets a fresh
/// name. Returns the slot used (0 if none / `entity_id` <= 10).
pub fn find_free_entity_slot(g: &mut Globals, rng: &mut Rng, entity_id: i32, industry_id: i16) -> i32 {
    let mut result = 0;
    if 10 < entity_id {
        let b = entity_id;
        let mut candidate_id = if b > 1500 {
            rng.rnd_range(1580, b + 1)
        } else if industry_id == 1 {
            rng.rnd_range(85, b)
        } else if industry_id == 2 {
            rng.rnd_range(130, b)
        } else {
            rng.rnd_range(b + 7, b + 1)
        };
        if g.industry_num[candidate_id] != industry_id || g.entity_status[candidate_id] != 0 {
            candidate_id = 0;
        }
        if 10 < candidate_id {
            g.name_list_dirty = 1;
            let t = g.ent_name[candidate_id].clone();
            g.ent_name[candidate_id] = g.ent_name[b].clone();
            g.ent_name[b] = t;
            let t = g.ent_symbol[candidate_id].clone();
            g.ent_symbol[candidate_id] = g.ent_symbol[b].clone();
            g.ent_symbol[b] = t;
            let t = g.country_idx[candidate_id];
            g.country_idx[candidate_id] = g.country_idx[b];
            g.country_idx[b] = t;
        }
        result = candidate_id;
    }
    result
}

/// Lower bounds of the default industry bands of entity slots 86..1471 (GetDefaultIndustryForSlot): slot
/// `id` belongs to industry `1 + (number of bounds <= id)`.
#[allow(non_snake_case)]
fn INDUSTRY_SLOT_BOUNDS(__content: &crate::content::ContentPack) -> [i32; 69] {
    [__content.long(14640), __content.long(14641), __content.long(14642), __content.long(14643), __content.long(14644), __content.long(14645), __content.long(14646), __content.long(14647), __content.long(14648), __content.long(14649), __content.long(14650), __content.long(14651), __content.long(14652), __content.long(14653), __content.long(14654), __content.long(14655), __content.long(14656), __content.long(14657), __content.long(14658), __content.long(14659), __content.long(14660), __content.long(14661), __content.long(14662), __content.long(14663), __content.long(14664), __content.long(14665), __content.long(14666), __content.long(14667), __content.long(14668), __content.long(14669), __content.long(14670), __content.long(14671), __content.long(14672), __content.long(14673), __content.long(14674), __content.long(14675), __content.long(14676), __content.long(14677), __content.long(14678), __content.long(14679), __content.long(14680), __content.long(14681), __content.long(14682), __content.long(14683), __content.long(14684), __content.long(14685), __content.long(14686), __content.long(14687), __content.long(14688), __content.long(14689), __content.long(14690), __content.long(14691), __content.long(14692), __content.long(14693), __content.long(14694), __content.long(14695), __content.long(14696), __content.long(14697), __content.long(14698), __content.long(14699), __content.long(14700), __content.long(14701), __content.long(14702), __content.long(14703), __content.long(14704), __content.long(14705), __content.long(14706), __content.long(14707), __content.long(14708)]
}

/// Port of GetDefaultIndustryForSlot (0x49a929).
/// The industry an entity slot belongs to in the name list: 1 (banks 11..85), 2 (insurers 86..130),
/// 3..69 by bands, 70 (1471..1580), 71 (funds over 1580); 0 for players (<= 10).
pub fn get_default_industry_for_slot(corp_id: i32) -> i16 {
    if corp_id <= 10 {
        return 0;
    }
    if corp_id >= 1471 {
        return if corp_id <= 1580 { 70 } else { 71 };
    }
    1 + cast::<i16, _>(INDUSTRY_SLOT_BOUNDS(&crate::content::current()).iter().filter(|&&t| t <= corp_id).count())
}

/// Port of RemoveRecentOffering (0x498bc2).
/// Clears `RecentOfferings(1..10)` entries equal to `corp_id` (either sign).
pub fn remove_recent_offering(g: &mut Globals, corp_id: i32) {
    for i in 1..=10 {
        if g.recent_offerings[i].wrapping_abs() == corp_id {
            g.recent_offerings[i] = 0;
        }
    }
}

/// Port of PurgeLawsuitsForEntity (0x5412cc).
/// Deletes the open lawsuits (`LawsuitRec`, not starting with "2") in which `entity_id` is defendant or
/// plaintiff. For a slot of `RecentAntitrustFilers` equal to `entity_id` it clears
/// `RecentAntitrustFilers(entity_id)` instead of that slot (BUGS.md): for ids over 10 that is error 9 (no
/// ON ERROR here; the write goes to the runtime's dummy element and the routine carries on, as the
/// emulated original shows).
pub fn purge_lawsuits_for_entity(g: &mut Globals, entity_id: i32) {
    for slot in 1..=10 {
        if entity_id == g.recent_antitrust_filers[slot] {
            g.recent_antitrust_filers[entity_id] = 0; // (sic) entity_id, not slot
        }
        let suit_rec = g.lawsuit_rec[slot].clone();
        if suit_rec.is_empty() || mid(suit_rec.as_bytes(), 1, 1) == "2" {
            continue;
        }
        let defendant = parse_any(suit_rec.as_bytes(), b"|", 2);
        let plaintiff = parse_any(suit_rec.as_bytes(), b"|", 3);
        if val(defendant.as_bytes()) == cast::<Ext, _>(entity_id) || val(plaintiff.as_bytes()) == cast::<Ext, _>(entity_id) {
            g.lawsuit_rec[slot] = PbStr::new();
        }
    }
}

/// Port of ClearEntityRecord (0x49321d).
/// Resets entity slot `entity_id` (> 10) for reuse: default industry of the slot, all balance-sheet,
/// earnings, bank/insurer/fund fields, holdings (stocks and bonds), player advances; a player who was CEO
/// of it loses the post (news "… [runtime resource wording] …"); its lawsuits, price
/// history and option records are removed, cheat/advisory targets on it are cancelled
/// (`EventStatus(101/103) = 2`), and it leaves the recent offerings. An insurer or company that advises an
/// ETF hands the contract on ([`etf_drop_one_adviser_contract`]).
pub fn clear_entity_record(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x493daf: ShowRuntimeError and exit.
    if entity_id < 11 {
        return;
    }
    let c = entity_id;
    g.industry_num[c] = get_default_industry_for_slot(c);
    g.cash[c] = Ext::from_f64(0.0);
    g.t_bills[c] = Ext::from_f64(0.0);
    g.corp_controller[c] = 0;
    g.accrued_pl[c] = Ext::from_f64(0.0);
    g.bank_debt[c] = Ext::from_f64(0.0);
    g.div_recvd1[c] = Ext::from_f64(0.0);
    g.div_recvd2[c] = Ext::from_f64(0.0);
    g.extraordinary_pl[c] = 0;
    g.earnings_ytd[c] = Ext::from_f64(0.0);
    g.dividend_per_share[c] = Ext::from_f64(0.0);
    g.proj_qtr_earnings[c] = Ext::from_f64(0.0);
    g.market_cap[c] = Ext::from_f64(0.0);
    g.investor_sentiment[c] = 0;
    g.tax_loss_carry[c] = Ext::from_f64(0.0);
    g.profit_margin[c] = 0;
    g.bonds_issued[c] = 0;
    g.bond_issue_info[c] = PbStr::new();
    g.bond_price[c] = Cur(0);
    g.equity_income_ytd[c] = Ext::from_f64(0.0);
    g.extra_items_ytd[c] = Ext::from_f64(0.0);
    g.consol_tax_inc_ytd[c] = Ext::from_f64(0.0);
    g.tax_credits[c] = Ext::from_f64(0.0);
    g.earnings_y1[c] = Ext::from_f64(0.0);
    g.earnings_yr2_ago[c] = Ext::from_f64(0.0);
    g.earnings_yr3_ago[c] = Ext::from_f64(0.0);
    g.earnings_yr4_ago[c] = Ext::from_f64(0.0);
    g.disc_spend_pct[c] = 0;
    g.shares_outstanding[c] = Ext::from_f64(1.0);
    g.credit_frozen[c] = 0;
    g.ctrl_since_year[c] = 0;
    g.corp_prior_net_worth[c] = Ext::from_f64(0.0);
    g.contingency_reserve[c] = Ext::from_f64(0.0);
    g.reserve_adj_pending[c] = Ext::from_f64(0.0);
    g.auto_pilot[c] = 0;
    g.t_bills[c] = Ext::from_f64(0.0);
    if c < 86 {
        g.bank_demand_deposits[c] = Ext::from_f64(0.0);
        g.bad_debt_reserve[c] = Ext::from_f64(0.0);
        g.bank_consumer_loans[c] = 0;
        g.bank_mortgage_loans[c] = 0;
        g.subprime_secs[c] = 0;
        g.bank_c_ds[c] = Ext::from_f64(0.0);
        g.bank_cust_deposits[c] = Ext::from_f64(0.0);
        g.bank_largest_debtor[c] = 0;
        g.bank_alloc_off[c] = 0;
    }
    g.growth_rate[c] = 0;
    if 85 < c && c < 131 {
        g.insur_uw_profit_ytd[c] = Ext::from_f64(0.0);
        g.ins_reserves[c] = Ext::from_f64(0.0);
        etf_drop_one_adviser_contract(g, ui, rng, c, 0);
    }
    if 130 < c {
        g.business_assets[c] = Ext::from_f64(0.0);
        g.working_capital[c] = Ext::from_f64(0.0);
        g.goodwill[c] = Ext::from_f64(0.0);
        g.commod_margin_bal[c] = Ext::from_f64(0.0);
        etf_drop_one_adviser_contract(g, ui, rng, c, 0);
        g.corp_hedging_gain[c] = Ext::from_f64(0.0);
        if 1580 < c {
            g.fund_adviser[c] = 0;
            g.fund_fee_pct[c] = cast::<f32, _>(Ext::from_f32(0.0f32));
            g.fund_bonus_year[c] = 0;
        }
    }
    for i in 0..=15 {
        g.hold_stock_id[(i, c)] = 0;
        g.hold_stock_pct[(i, c)] = 0;
        g.hold_stock_basis[(i, c)] = Ext::from_f64(0.0);
        g.hold_bond_id[(i, c)] = 0;
        g.hold_bond_face[(i, c)] = 0;
        g.bond_hold_cost[(i, c)] = Ext::from_f64(0.0);
    }
    for i in 1..=5 {
        g.player_advances[(i, c)] = Ext::from_f64(0.0);
        if c == g.player_ceo_of[i] {
            g.news_msg = g.ent_name[i].clone() + __content.string(6813) + &g.ent_symbol[c];
            post_news(g, ui, i, c);
            g.player_ceo_of[i] = 0;
            g.player_ceo_since_year[i] = 0;
        }
    }
    purge_lawsuits_for_entity(g, c);
    clear_price_history(g, c);
    let mut option_count = to_long(val(g.option_recs[0].as_bytes()));
    for i in 1..=1000 {
        if option_count == 0 {
            break;
        }
        if val(parse_any(g.option_recs[i].as_bytes(), b"|", 3).as_bytes()) == cast::<Ext, _>(c) {
            g.option_recs[i] = PbStr::new();
            option_count -= 1;
        }
    }
    g.option_recs[0] = str_trim(cast::<Ext, _>(option_count));
    if cast::<i32, _>(g.cheat_eq_fund_target) == c {
        g.cheat_eq_fund_target = 0;
        g.event_status[101] = 2;
    }
    if cast::<i32, _>(g.adv_offering_corp) == c {
        g.adv_offering_corp = 0;
        g.event_status[103] = 2;
    }
    remove_recent_offering(g, c);
}

// --------------------------------------------------------------------------------------- banks, insurers

/// Port of CalcLoanSalePricePct (0x499622).
/// Price (% of face value) at which a bank loan to `borrower_id` changes hands: 88 + 2 x credit rating
/// (+1 for a profitable company rated 3..8); ratings 0/1: 35/65 minus `id MOD 17` (+5 for a profitable
/// rating-1 company). Recomputes the borrower's financials.
pub fn calc_loan_sale_price_pct(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, borrower_id: i32) -> i32 {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut bonus_pct = 0;
    calc_entity_financials(g, ui, rng, borrower_id);
    if 85 < borrower_id && g.proj_qtr_earnings[borrower_id] > Ext::from_f64(0.0) {
        bonus_pct = 1;
    }
    if 8 < g.credit_rating_num[borrower_id] {
        bonus_pct = 0;
    }
    let mut base_pct = 100 - (g.credit_rating_num[borrower_id] * -2 + 12);
    if g.credit_rating_num[borrower_id] < 2 {
        base_pct = 65 - borrower_id % 17;
        if g.credit_rating_num[borrower_id] == 0 {
            base_pct = 35 - borrower_id % 17;
        }
        if bonus_pct == 1 && g.credit_rating_num[borrower_id] == 1 {
            bonus_pct = 5;
        }
    }
    bonus_pct + base_pct
}

/// SellLoanToBank: a negative bad-debt reserve is charged to earnings and reset to `250 * ExchangeRate`.
fn fix_negative_reserve(g: &mut Globals, bank: i32) {
    if g.bad_debt_reserve[bank] < Ext::from_f64(0.0) {
        g.accrued_pl[bank] = g.bad_debt_reserve[bank] + g.accrued_pl[bank];
        g.bad_debt_reserve[bank] = g.exchange_rate * Ext::from_f64(250.0);
        g.accrued_pl[bank] = g.accrued_pl[bank] - g.exchange_rate * Ext::from_f64(250.0);
    }
}

/// Port of SellLoanToBank (0x498f42).
/// Bank `seller_bank_id` sells its loan to `borrower_id` to `buyer_bank_id` at
/// [`calc_loan_sale_price_pct`] (at most 101% when the seller is the acting entity): cash moves between the
/// banks, the discount moves between their bad-debt reserves (a negative reserve is charged to earnings and
/// reset to 250), the borrower's `LendingBank` changes. A pending `NewsMsg` is posted first; the sale's own
/// headline is left in `NewsMsg` ("… has acquired … loan from …" / "… has sold … loan to …" + " at N% of
/// face value").
pub fn sell_loan_to_bank(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, buyer_bank_id: i32, seller_bank_id: i32, borrower_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if seller_bank_id == buyer_bank_id {
        return;
    }
    let debt = g.bank_debt[borrower_id];
    let mut price_pct = calc_loan_sale_price_pct(g, ui, rng, borrower_id);
    if g.active_entity == seller_bank_id && 100 < price_pct {
        price_pct = 101;
    }
    let disc_rate = s(0.01f32) * (Ext::from_f64(100.0) - cast::<Ext, _>(price_pct));
    let discount = debt * disc_rate;
    g.cash[seller_bank_id] = (debt - discount) + g.cash[seller_bank_id];
    g.cash[buyer_bank_id] = g.cash[buyer_bank_id] - (debt - discount);
    if discount != Ext::from_f64(0.0) && !discount.is_nan() {
        g.bad_debt_reserve[seller_bank_id] = g.bad_debt_reserve[seller_bank_id] - discount;
        fix_negative_reserve(g, seller_bank_id);
        g.bad_debt_reserve[buyer_bank_id] = discount + g.bad_debt_reserve[buyer_bank_id];
        fix_negative_reserve(g, buyer_bank_id);
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.lending_bank[borrower_id] = cast::<i16, _>(buyer_bank_id);
    g.news_msg = g.ent_symbol[buyer_bank_id].clone()
        + __content.string(1823)
        + &g.ent_symbol[borrower_id]
        + __content.string(3667)
        + &g.ent_symbol[seller_bank_id];
    if g.active_entity == seller_bank_id {
        g.news_msg = g.ent_symbol[seller_bank_id].clone()
            + __content.string(3012)
            + &g.ent_symbol[borrower_id]
            + __content.string(3013)
            + &g.ent_symbol[buyer_bank_id];
    }
    g.news_msg = g.news_msg.clone() + " at " + &str_trim(cast::<Ext, _>(price_pct)) + __content.string(6814);
}

/// Port of BankHasFewBorrowers (0x4998c1).
/// 1 if `entity_id` is not an active bank (11..85), its loans total less than `5000 * ExchangeRate`, or it
/// lends to at most 5 active players/companies with a positive `BankDebt`; else 0.
pub fn bank_has_few_borrowers(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) -> i32 {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let bank_id = entity_id;
    if !(11..=85).contains(&bank_id) || g.entity_status[bank_id] != 1 {
        return 1;
    }
    calc_entity_financials(g, ui, rng, bank_id);
    if !(g.bank_loans_total >= g.exchange_rate * Ext::from_f64(5000.0)) {
        return 1;
    }
    let mut borrower_count = 0;
    for borrower_id in 1..=1600 {
        if (borrower_id <= g.num_players || 85 < borrower_id)
            && g.entity_status[borrower_id] == 1
            && cast::<i32, _>(g.lending_bank[borrower_id]) == bank_id
            && g.bank_debt[borrower_id] > Ext::from_f64(0.0)
        {
            borrower_count += 1;
        }
    }
    if borrower_count > 5 { 0 } else { 1 }
}

/// Port of CreateNewBankInFreeSlot (0x4e3408).
/// Founds a bank (`CreateBank(id, 2)` with 6000..10000 of capital in `TmpExt2`) in the first unused bank
/// slot 61..85, if any.
pub fn create_new_bank_in_free_slot(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // procTag = "BuyLoans" (BUGS.md #6); ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut new_bank_id = 0;
    let mut bank_slot = 61;
    while bank_slot <= 85 {
        if g.entity_status[bank_slot] != 1 {
            new_bank_id = bank_slot;
            bank_slot = 85;
        }
        bank_slot += 1;
    }
    if new_bank_id == 0 {
        return;
    }
    g.tmp_ext2 = rng.rnd() * Ext::from_f64(4000.0) + Ext::from_f64(6000.0);
    create_bank(g, ui, rng, new_bank_id, 2);
}

/// Port of CreateBank (0x4e3588).
/// Sets up bank `bank_id` with the capital in `TmpExt2` (cleared): slot reset ([`clear_entity_record`]),
/// active and self-controlled, startup cost 10% (max 200) charged to earnings, 2 shares, random margin and
/// spending, lending bank `id - 1`; with `setup_mode > 0` the capital is spread over cash, consumer and
/// mortgage loans and subprime securities, else it is cash and `BankAllocOff = 1`; deposits (5x CDs, 2x
/// demand deposits) are added to cash; a fresh name ([`find_free_entity_slot`]). Mode 2 also issues bonds
/// when `BaseInterestRate < 8.25` (IssueBonds) and buys up to the bank's means loans of weak borrowers of
/// other banks ([`sell_loan_to_bank`], news unless mode 0). Finally a bad-debt reserve for the loans, 85%
/// of cash into T-bills, a market cap of 1% of net worth valued at once, index divisor, `CtrlSinceYear`,
/// active-corp count and stock split check.
pub fn create_bank(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, bank_id: i32, setup_mode: i32) {
    // procTag = "CreateBank"; ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    clear_entity_record(g, ui, rng, bank_id);
    g.entity_status[bank_id] = 1;
    g.corp_controller[bank_id] = bank_id;
    let mut capital = g.tmp_ext2;
    g.tmp_ext2 = Ext::from_f64(0.0);
    let mode = setup_mode;
    let mut startup_cost = capital * s(0.1f32);
    if startup_cost > g.exchange_rate * Ext::from_f64(200.0) {
        startup_cost = g.exchange_rate * Ext::from_f64(200.0);
    }
    g.accrued_pl[bank_id] = g.accrued_pl[bank_id] - startup_cost;
    capital = capital - startup_cost;
    g.shares_outstanding[bank_id] = Ext::from_f64(2.0);
    let r = rng.rnd() * Ext::from_f64(12.0);
    g.profit_margin[bank_id] = cast::<i16, _>(to_quad(-Ext::from_f64(5.0) + r));
    g.disc_spend_pct[bank_id] = cast::<i16, _>(rng.rnd_range(6, 0) + 6);
    g.lending_bank[bank_id] = cast::<i16, _>(bank_id - 1);
    if mode > 0 {
        g.cash[bank_id] = capital * s(0.55f32);
        g.bank_consumer_loans[bank_id] = to_quad(capital * s(0.21f32));
        g.bank_mortgage_loans[bank_id] = to_quad(capital * s(0.08f32));
        g.subprime_secs[bank_id] = to_quad(capital * s(0.16f32));
    } else {
        g.cash[bank_id] = capital;
        g.bank_alloc_off[bank_id] = 1;
    }
    g.bank_c_ds[bank_id] = capital * Ext::from_f64(5.0);
    g.bank_demand_deposits[bank_id] = capital * Ext::from_f64(2.0);
    g.cash[bank_id] = (g.bank_c_ds[bank_id] + g.cash[bank_id]) + g.bank_demand_deposits[bank_id];
    find_free_entity_slot(g, rng, bank_id, 1);
    if s(8.25f32) > cast::<Ext, _>(g.base_interest_rate) && mode == 2 {
        g.tmp_ext1 = capital;
        issue_bonds(g, ui, rng, bank_id, Ext::from_f64(0.0), 0);
    }
    g.bank_cust_deposits[bank_id] = Ext::from_f64(0.0);
    if mode == 2 {
        let mut borrower_id = 86;
        while borrower_id <= 1600 {
            let b = borrower_id;
            'next: {
                if g.entity_status[b] != 1 || cast::<i32, _>(g.lending_bank[b]) == bank_id {
                    break 'next;
                }
                let ctrl = g.corp_controller[b];
                if !(0 >= ctrl || 11 <= ctrl) {
                    break 'next;
                }
                if g.cash[b] > g.bank_debt[b] * s(0.5f32) {
                    break 'next;
                }
                let old_lender = cast::<i32, _>(g.lending_bank[b]);
                if get_controller_type(g, old_lender) == 1 {
                    break 'next;
                }
                if bank_has_few_borrowers(g, ui, rng, old_lender) > 0 {
                    break 'next;
                }
                let borrower_debt = g.bank_debt[b];
                'cash_check: {
                    if !((borrower_debt - g.cash[b]) > g.cash[bank_id] * s(0.2f32)) {
                        break 'cash_check;
                    }
                    if !((borrower_debt - g.cash[b]) < g.cash[bank_id] * s(0.4f32)) {
                        break 'cash_check;
                    }
                    if !(borrower_debt > Ext::from_f64(250.0)) {
                        break 'cash_check;
                    }
                    if !g.news_msg.is_empty() {
                        post_news(g, ui, 0, 0);
                    }
                    if !(g.bank_debt[old_lender] > Ext::from_f64(0.0)) && rng.rnd() * Ext::from_f64(7.0) >= Ext::from_f64(1.0) {
                        break 'cash_check;
                    }
                    calc_entity_financials(g, ui, rng, b);
                    if g.credit_rating_num[b] < 5 {
                        break 'cash_check;
                    }
                    sell_loan_to_bank(g, ui, rng, bank_id, old_lender, b);
                    if mode == 0 {
                        g.news_msg = PbStr::new();
                    } else {
                        post_news(g, ui, 0, 0);
                    }
                    if g.cash[b] != Ext::from_f64(0.0) && !g.cash[b].is_nan() {
                        calc_entity_financials(g, ui, rng, old_lender);
                        calc_entity_financials(g, ui, rng, bank_id);
                    }
                }
                if !(g.cash[bank_id] >= Ext::from_f64(50.0)) {
                    borrower_id = 1600;
                }
            }
            borrower_id += 1;
        }
    }
    calc_entity_financials(g, ui, rng, bank_id);
    if g.tmp_bank_total_loans > Ext::from_f64(0.0) {
        let r = rng.rnd() * s(0.01f32);
        let init_reserve = g.tmp_bank_total_loans * (r + s(0.002f32));
        g.bad_debt_reserve[bank_id] = init_reserve + g.bad_debt_reserve[bank_id];
        g.accrued_pl[bank_id] = g.accrued_pl[bank_id] - init_reserve;
    }
    let total_cash = g.cash[bank_id];
    g.t_bills[bank_id] = total_cash * s(0.85f32);
    g.cash[bank_id] = total_cash * s(0.15f32);
    g.valuation_mode = 3;
    g.market_cap[bank_id] = s(0.01f32) * g.net_worth;
    calc_stock_price(g, ui, rng, bank_id, 0); // mode 3 does not use TIME$
    adjust_index_divisor(g, bank_id, 100);
    g.ctrl_since_year[bank_id] = g.cur_year;
    count_active_corps(g);
    check_stock_split(g, bank_id);
}

/// Port of CreateInsurer (0x4f1893).
/// Sets up insurer `insurer_id` with the capital in `TmpExt1` (cleared) and a bank loan of `TmpExt2`
/// (cleared): slot reset, active and self-controlled, market cap 3% of capital, startup cost 10% (max 200),
/// reserves of 1..6x the capital (capped to 2000..2200 unless `announce_mode = 1`) added to cash, a fresh
/// name ([`find_free_entity_slot`]), a lending bank in the matching country (else any active bank; loops
/// until one is found), random margin/spending/growth, 0.5 shares, the "New insurer … expanding
/// aggressively." headline (posted if `announce_mode > 0`, else left in `NewsMsg`), immediate valuation,
/// index divisor, split check, active-corp count and `CtrlSinceYear`.
pub fn create_insurer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, insurer_id: i32, announce_mode: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let announce = announce_mode;
    let c = insurer_id;
    clear_entity_record(g, ui, rng, c);
    g.entity_status[c] = 1;
    g.corp_controller[c] = c;
    g.market_cap[c] = g.tmp_ext1 * s(0.03f32);
    let mut startup_cost = g.tmp_ext1 * s(0.1f32);
    if startup_cost > g.exchange_rate * Ext::from_f64(200.0) {
        startup_cost = g.exchange_rate * Ext::from_f64(200.0);
    }
    g.accrued_pl[c] = g.accrued_pl[c] - startup_cost;
    g.tmp_ext1 = g.tmp_ext1 - startup_cost;
    g.cash[c] = g.tmp_ext1;
    let r = rng.rnd();
    g.ins_reserves[c] = (g.tmp_ext1 * r) * Ext::from_f64(5.0) + g.tmp_ext1;
    if announce_mode != 1 && g.ins_reserves[c] > g.exchange_rate * Ext::from_f64(2200.0) {
        let r = rng.rnd() * Ext::from_f64(200.0);
        g.ins_reserves[c] = g.exchange_rate * (r + Ext::from_f64(2000.0));
    }
    g.cash[c] = g.ins_reserves[c] + g.cash[c];
    g.tmp_ext1 = Ext::from_f64(0.0);
    find_free_entity_slot(g, rng, c, 2);
    let mut bank_country = g.country_idx[c];
    bank_country = match bank_country {
        22 => 2,
        48 => 8,
        49 => 25,
        14 => 45,
        20 => 12,
        9 => 5,
        21 => 27,
        47 | 35 => 17,
        23 => 50,
        x => x,
    };
    g.lending_bank[c] = 0;
    let mut bank_id = 0;
    if 0 < bank_country {
        let mut tries = 1;
        while tries <= 100 {
            bank_id = rng.rnd_range(85, 11);
            if g.entity_status[bank_id] == 1 && bank_country == g.country_idx[bank_id] {
                g.lending_bank[c] = cast::<i16, _>(bank_id);
                tries = 100;
            }
            tries += 1;
        }
    }
    while g.lending_bank[c] == 0 {
        bank_id = rng.rnd_range(85, 11);
        if g.entity_status[bank_id] == 1 {
            g.lending_bank[c] = cast::<i16, _>(bank_id);
        }
    }
    g.cash[c] = g.tmp_ext2 + g.cash[c];
    g.cash[bank_id] = g.cash[bank_id] - g.tmp_ext2;
    g.bank_debt[c] = g.tmp_ext2;
    g.tmp_ext2 = Ext::from_f64(0.0);
    let r = rng.rnd() * Ext::from_f64(10.0);
    g.profit_margin[c] = cast::<i16, _>(to_quad(-Ext::from_f64(5.0) + r));
    let r = cast::<Ext, _>(rng.rnd_range(3, 1));
    g.disc_spend_pct[c] = cast::<i16, _>(to_quad(r + (cast::<Ext, _>(g.profit_margin[c])).abs()));
    g.growth_rate[c] = cast::<i16, _>(to_quad(rng.rnd() * Ext::from_f64(8.0) + Ext::from_f64(5.0)));
    g.news_msg = st(__content.string(6815)) + &g.ent_name[c] + " (" + &g.ent_symbol[c] + __content.string(6816);
    if 0 < announce {
        post_news(g, ui, 0, 0);
    }
    g.shares_outstanding[c] = s(0.5f32);
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, c, 0); // mode 3 does not use TIME$
    adjust_index_divisor(g, c, 100);
    check_stock_split(g, c);
    count_active_corps(g);
    g.ctrl_since_year[c] = g.cur_year;
}

