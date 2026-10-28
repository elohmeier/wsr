//! Selling business assets: `SellBusinessAssets` (0x5d8ab9) with its GOSUBs
//! `SellBusinessAssets_BlendMargin` (0x5dad74) and `SellBusinessAssets_PickRelatedBuyer` (0x5daf13).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use super::SHOW_TEXT_REPORT;
use crate::logic::report::show_text_report;
use crate::logic::corp_ai::spinoff::calc_asset_sale_price_pct;
use crate::logic::earnings::{balance_cash_t_bills, calc_projected_earnings};
use crate::logic::finance::calc_entity_financials;
use crate::logic::market::select_target_corp;
use crate::logic::sim_entity::asset_sales::gen_asset_sale_description;
use crate::logic::valuation::{calc_stock_price, calc_working_capital_ratio};

/// `BuyBusinessAssets` (0x5d2f04): may be skipped by tests (1 arg).
pub const BUY_BUSINESS_ASSETS: u32 = 0x5d2f04;

/// The locals of `SellBusinessAssets` (zeroed at entry), shared with its GOSUB blocks.
#[derive(Default)]
struct Frame {
    skip: Vec<u32>,
    time_secs: i32,
    now: crate::logic::newgame::LocalTime,
    /// `DIM menuLines(0 TO 500)` (local array of the original).
    menu_lines: Vec<PbStr>,
    line_count: i32,
    line_text: PbStr,
    msg_text: PbStr,
    suffix_text: PbStr,
    amt_text: PbStr,
    prompt_text: PbStr,
    ind_total_assets: Ext,
    old_market_cap: Ext,
    orig_assets: Ext,
    goodwill_amt: Ext,
    hold_co_buyer: i32,
    same_ind_buyer: i32,
    new_margin_wt: f32,
    margin_wt: f32,
    ind_num: i16,
    seller_id: i32,
    buyer_id: i32,
    scrap_pct: i64,
    bid_amt: i64,
    max_bid: i64,
    sale_price_pct: i64,
    answer: i32,
    done: i32,
    rand_buyer: i32,
    scan_id: i32,
    wc_release: Ext,
    proceeds: Ext,
    sale_amt: Ext,
    combined_assets: Ext,
    buyer_assets: Ext,
    gain_loss: Ext,
    premium_pct: i32,
    rnd_val: i64,
    max_len: i32,
}

impl Frame {
    fn skips(&self, addr: u32) -> bool {
        self.skip.contains(&addr)
    }
}

/// Port of SellBusinessAssets (0x5d8ab9).
/// The player's operating company `ActiveEntity` (> 130, industry < 70) sells business assets. If the
/// current player controls other possible buyers (companies of the same industry — holding companies
/// are never counted, bug 282), it may sell to one of them at cost ([`pick_related_buyer`], which runs
/// BuyBusinessAssets for the buyer). Otherwise it asks the amount (at most the assets) and looks for a
/// buyer among the active companies of the industry and the holding companies (controlled by a company,
/// not by a player; the purchase may not take them over 52% of the industry's assets; bid = 60% of their
/// liquid funds): a same-industry or holding-company buyer picked by a rotating rule, else a random
/// bidder; with none, it offers to scrap the assets at `premium + 35` % (10..50%). The sale price is
/// `100 + premium/(premium+100)` % of cost (at most `150 - 5 × DifficultyLevel`), less a 10% commission
/// (at cost for the player's own buyer); the premium part becomes the buyer's goodwill. Working capital
/// is released with the assets, a holding-company buyer enters the industry (losing its tax
/// carryovers), margins are blended; the seller's margin, investor sentiment and market value
/// (`MarketCap × 0.91` after revaluation) react; a seller left without assets becomes a holding company.
/// News, `TurnActionCount`, projected earnings and stock prices of both.
///
/// Quirks: `TotalAssets` in the sentiment test is the last value CalcEntityFinancials left (the last
/// candidate buyer scanned, bug 283); the "stock plunges" headline is never posted (bug 284).
/// `now`: the local time (`TIME$` of the valuations; the asset purchase's callees).
pub fn sell_business_assets(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &crate::logic::newgame::LocalTime) {
    sell_business_assets_skipping(g, ui, rng, now, &[]);
}

/// [`sell_business_assets`] with routines (addresses) not called, like the test harness's skips:
/// [`SHOW_TEXT_REPORT`], [`BUY_BUSINESS_ASSETS`].
#[doc(hidden)]
pub fn sell_business_assets_skipping(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &crate::logic::newgame::LocalTime, skip: &[u32]) {
    let mut f = Frame { skip: skip.to_vec(), time_secs: now.time_secs(), now: *now, menu_lines: vec![PbStr::new(); 501], ..Frame::default() };
    main(g, ui, rng, &mut f);
    sell_business_assets_exit();
}

/// Port of SellBusinessAssets_Exit (0x5db831): the routine's exit label (frame release only).
fn sell_business_assets_exit() {}

fn main(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x5db7cc -> ShowRuntimeError, exit
    let ae = g.active_entity;
    if !(11..=1600).contains(&ae) {
        // IndustryNum(ActiveEntity) of a player: error 9 -> ON ERROR handler
        crate::logic::restructure::show_runtime_error(g, ui, 9, &PbStr::new(), &PbStr::new(), 0);
        return;
    }
    f.ind_num = g.industry_num[ae];
    if !(130 < ae && g.industry_num[ae] < 70) {
        return;
    }
    f.buyer_id = 0;
    for corp_idx in 131..=1600 {
        if g.entity_status[corp_idx] != 1 {
            continue;
        }
        let ind = g.industry_num[corp_idx];
        if ind == 71 {
            continue;
        }
        if ind == f.ind_num {
            f.ind_total_assets = f.ind_total_assets + g.business_assets[corp_idx];
        }
        if g.active_entity == corp_idx {
            continue;
        }
        if ind != f.ind_num && ind != 70 {
            continue;
        }
        // (saleAmt is still 0 here: holding companies are always skipped)
        if f.sale_amt < Ext::from_f64(100.0) && ind == 70 {
            continue;
        }
        if g.cur_player == g.corp_controller[corp_idx] {
            if f.buyer_id == 0 {
                f.buyer_id = corp_idx;
            } else {
                if ind == f.ind_num {
                    f.buyer_id = -1;
                }
                if ind == 70 && f.buyer_id != -1 {
                    f.buyer_id = -2;
                }
            }
        }
    }
    if f.buyer_id != 0 {
        f.prompt_text = PbStr::new();
        f.msg_text = PbStr::from(__content.string(2218))
            + &g.active_entity_name
            + __content.string(2219)
            + __content.string(2220);
        if f.buyer_id < 131 {
            if f.buyer_id < 0 {
                f.prompt_text = PbStr::from(
                    __content.string(2221),
                );
                if f.buyer_id == -1 {
                    f.prompt_text = f.prompt_text.clone() + &g.ind_name[cast::<i32, _>(f.ind_num)];
                } else {
                    f.prompt_text = f.prompt_text.clone() + __content.string(2222);
                }
                f.prompt_text = f.prompt_text.clone() + __content.string(2223);
            }
        } else {
            f.msg_text = f.msg_text.clone() + ", " + &g.ent_name[f.buyer_id];
            f.prompt_text = PbStr::from(__content.string(2224))
                + &g.ind_name[cast::<i32, _>(g.industry_num[f.buyer_id])]
                + __content.string(2225);
        }
        f.msg_text = f.msg_text.clone()
            + "? "
            + &f.prompt_text
            + "\r"
            + "\r"
            + __content.string(2226)
            + __content.string(2227);
        f.answer = ui.msgbox(&f.msg_text, 0x2023, &__content.string(2228).into());
        if f.answer == 6 {
            pick_related_buyer(g, ui, rng, f);
            return;
        }
        if f.answer == 2 {
            return;
        }
    }
    let ae = g.active_entity;
    f.premium_pct = to_long(calc_asset_sale_price_pct(g, rng, ae));
    let p = cast::<Ext, _>(f.premium_pct);
    f.sale_price_pct = to_quad(cast::<Ext, _>(to_quad((p / (p + Ext::from_f64(100.0))) * Ext::from_f64(100.0))) + Ext::from_f64(100.0));
    f.orig_assets = g.business_assets[ae];
    let cap = Ext::from_f64(150.0) - Ext::from_f64(5.0) * cast::<Ext, _>(g.difficulty_level);
    if cap < cast::<Ext, _>(f.sale_price_pct) {
        f.sale_price_pct = to_quad(cap);
    }
    f.premium_pct = to_long(cast::<Ext, _>(f.sale_price_pct) - Ext::from_f64(100.0));
    f.amt_text = fmt_trim(g.business_assets[ae], b"* ###,.00");
    f.suffix_text = PbStr::from(" ") + &g.million_prefix_lc + "illion" + &g.currency_suffix;
    f.prompt_text = g.ent_symbol[ae].clone()
        + __content.string(2229)
        + &trim(g.currency_symbol.as_bytes())
        + &f.amt_text
        + &f.suffix_text
        + ", "
        + __content.string(2230)
        + &str_s(cast::<Ext, _>(f.sale_price_pct))
        + __content.string(2231)
        + __content.string(2232)
        + "\r"
        + "\r"
        + __content.string(2233)
        + " in "
        + &g.million_prefix_lc
        + __content.string(2234);
    f.done = 0;
    loop {
        loop {
            if f.done != 0 {
                if 10 < g.sel_entity {
                    g.sel_industry = cast::<i32, _>(g.industry_num[g.sel_entity]);
                    // (UI) SelectIndustryComboItem (0x46df33)
                }
                return;
            }
            let title = PbStr::from(__content.string(2235)) + &g.ent_symbol[g.active_entity];
            f.msg_text = ui.inputbox(&f.prompt_text, &title, &f.amt_text);
            f.sale_amt = val_removing(&f.msg_text, b"$ ,");
            if f.sale_amt <= Ext::from_f64(0.0) {
                return;
            }
            if !(g.business_assets[g.active_entity] + sf(0.1f32) < f.sale_amt) {
                break;
            }
        }
        find_buyer(g, ui, rng, f);
        if f.done == 0 && (0 < f.same_ind_buyer || 0 < f.hold_co_buyer) {
            if 0 < f.hold_co_buyer {
                f.buyer_id = f.hold_co_buyer;
            }
            if 0 < f.same_ind_buyer {
                f.buyer_id = f.same_ind_buyer;
            }
            f.done = 1;
        }
        if f.done == 0 && 130 < f.rand_buyer && f.sale_amt < cast::<Ext, _>(f.max_bid) - Ext::from_f64(1.0) {
            f.buyer_id = f.rand_buyer;
            f.done = 1;
        }
        if f.done == 0 {
            f.scrap_pct = to_quad(cast::<Ext, _>(f.premium_pct) + Ext::from_f64(35.0));
            if (cast::<Ext, _>(f.scrap_pct)) < Ext::from_f64(10.0) {
                f.scrap_pct = 10;
            }
            if Ext::from_f64(50.0) < cast::<Ext, _>(f.scrap_pct) {
                f.scrap_pct = 50;
            }
            f.prompt_text = PbStr::from(__content.string(2236)) + __content.string(2237);
            if Ext::from_f64(0.0) < cast::<Ext, _>(f.max_bid) - Ext::from_f64(1.0) {
                f.prompt_text = f.prompt_text.clone()
                    + __content.string(2238)
                    + __content.string(2239)
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(cast::<Ext, _>(f.max_bid) - Ext::from_f64(1.0), b"################,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion.";
            }
            f.prompt_text = f.prompt_text.clone()
                + "\r"
                + "\r"
                + __content.string(2240)
                + &str_s(cast::<Ext, _>(f.scrap_pct))
                + __content.string(2241);
            f.answer = ui.msgbox(&f.prompt_text, 0x2123, &__content.string(2242).into());
            if f.answer != 6 {
                return;
            }
            f.done = 1;
        }
        complete_sale(g, ui, rng, f);
    }
}

/// The buyer search of SellBusinessAssets (0x5d8ab9, the `scanId` loop): bids (60% of liquid funds) of
/// the company-controlled companies of the industry and the holding companies, the candidates
/// `randBuyer`, `sameIndBuyer`, `holdCoBuyer` and the highest bid `maxBid`.
fn find_buyer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.max_bid = 0;
    f.bid_amt = 0;
    f.scan_id = 131;
    while f.scan_id <= 1600 {
        let scan = f.scan_id;
        f.scan_id += 1;
        if g.entity_status[scan] != 1 {
            continue;
        }
        let ind = g.industry_num[scan];
        if ind == 71 {
            continue;
        }
        if ind != f.ind_num && ind != 70 {
            continue;
        }
        if g.active_entity == scan || g.corp_controller[scan] < 11 {
            continue;
        }
        if !(f.sale_amt + g.business_assets[scan] <= f.ind_total_assets * sf(0.52f32)) {
            continue;
        }
        calc_entity_financials(g, ui, rng, scan);
        f.bid_amt = to_quad(cast::<Ext, _>(to_quad(g.liquid_funds)) * sf(0.6f32));
        if (cast::<Ext, _>(f.max_bid)) < cast::<Ext, _>(f.bid_amt) {
            f.max_bid = f.bid_amt;
        }
        if !(f.sale_amt <= cast::<Ext, _>(f.bid_amt) - Ext::from_f64(1.0)) {
            continue;
        }
        if f.rand_buyer == 0 {
            f.rand_buyer = scan;
        }
        if g.industry_num[scan] == 70 {
            if Ext::from_f64(19.0) < rng.rnd() * Ext::from_f64(20.0) {
                f.rand_buyer = scan;
            }
        } else if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
            f.rand_buyer = scan;
        }
        f.answer = 5;
        let rot = g.active_entity + scan + g.cur_quarter + g.cur_year;
        if rot % 3 == 1 && g.industry_num[scan] == f.ind_num {
            f.same_ind_buyer = scan;
            continue;
        }
        if rot % 7 == 1 && g.industry_num[scan] == 70 {
            if g.tax_loss_carry[scan] < Ext::from_f64(0.0) && f.sale_amt * sf(0.25f32) < g.tax_loss_carry[scan].abs() {
                continue;
            }
            f.hold_co_buyer = scan;
        }
    }
}

/// The sale itself in SellBusinessAssets (0x5d8ab9): `saleAmt` of `ActiveEntity`'s assets go to
/// `buyerId` (`scrapPct = 0`) or are scrapped, with the money, working capital, margins, goodwill, news,
/// earnings projections and stock prices.
fn complete_sale(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let ae = g.active_entity;
    f.margin_wt = cast::<f32, _>((f.sale_amt / g.business_assets[ae]) * Ext::from_f64(10.0));
    g.business_assets[ae] = g.business_assets[ae] - f.sale_amt;
    let assets = g.business_assets[ae];
    let r = calc_working_capital_ratio(g, ae);
    f.wc_release = g.working_capital[ae] - assets * r;
    if Ext::from_f64(0.0) < f.wc_release {
        g.cash[ae] = g.cash[ae] + f.wc_release;
        g.working_capital[ae] = g.working_capital[ae] - f.wc_release;
    }
    let margin_wt = f.margin_wt;
    let buyer = f.buyer_id;
    if cast::<Ext, _>(f.scrap_pct) <= Ext::from_f64(0.0) {
        if g.industry_num[buyer] == 70 {
            g.industry_num[buyer] = f.ind_num;
            g.disc_spend_pct[buyer] = 0;
            g.growth_rate[buyer] = 0;
            if g.tax_loss_carry[buyer] < Ext::from_f64(0.0) {
                g.tax_loss_carry[buyer] = Ext::from_f64(0.0);
                g.news_msg = g.ent_symbol[buyer].clone() + __content.string(2243);
                post_news(g, ui, 0, 0);
            }
            if Ext::from_f64(0.0) < g.tax_credits[buyer] {
                g.tax_credits[buyer] = Ext::from_f64(0.0);
                g.news_msg = g.ent_symbol[buyer].clone() + __content.string(2244);
                post_news(g, ui, 0, 0);
            }
        }
        blend_margin(g, rng, f);
        g.business_assets[buyer] = g.business_assets[buyer] + f.sale_amt;
        f.goodwill_amt = f.sale_amt * sf(0.01f32) * cast::<Ext, _>(f.premium_pct);
        if g.cur_player != g.corp_controller[buyer] {
            g.goodwill[buyer] = g.goodwill[buyer] + f.goodwill_amt;
        }
        if g.goodwill[buyer] < Ext::from_f64(0.0) {
            g.extraordinary_pl[buyer] = to_quad(cast::<Ext, _>(g.extraordinary_pl[buyer]) - g.goodwill[buyer]);
            g.goodwill[buyer] = Ext::from_f64(0.0);
        }
        f.proceeds = f.sale_amt * cast::<Ext, _>(f.sale_price_pct) * sf(0.01f32) * sf(0.9f32);
        if g.cur_player == g.corp_controller[buyer] {
            f.proceeds = f.sale_amt;
        }
        g.cash[buyer] = g.cash[buyer] - f.sale_amt * cast::<Ext, _>(f.sale_price_pct) * sf(0.01f32);
        balance_cash_t_bills(g, buyer);
        if g.cash[buyer] < Ext::from_f64(0.0) {
            g.bank_debt[buyer] = g.bank_debt[buyer] + g.cash[buyer].abs();
            let bank = cast::<i32, _>(g.lending_bank[buyer]);
            g.cash[bank] = g.cash[bank] - g.cash[buyer].abs();
            g.cash[buyer] = Ext::from_f64(0.0);
        }
        let r = calc_working_capital_ratio(g, buyer);
        f.wc_release = r * f.sale_amt;
        if g.cash[buyer] < f.wc_release {
            f.wc_release = g.cash[buyer];
        }
        g.cash[buyer] = g.cash[buyer] - f.wc_release;
        g.working_capital[buyer] = g.working_capital[buyer] + f.wc_release;
    } else {
        f.rnd_val = to_quad(cast::<Ext, _>(margin_wt) + cast::<Ext, _>(g.profit_margin[ae]));
        g.profit_margin[ae] = cast::<i16, _>(f.rnd_val);
        f.proceeds = f.sale_amt * cast::<Ext, _>(f.scrap_pct) * sf(0.01f32);
    }
    g.cash[ae] = g.cash[ae] + f.proceeds;
    f.gain_loss = f.proceeds - f.sale_amt;
    g.extraordinary_pl[ae] = to_quad(cast::<Ext, _>(g.extraordinary_pl[ae]) + f.gain_loss);
    gen_asset_sale_description(g, rng, ae);
    f.amt_text = g.asset_sale_desc.clone();
    if g.business_assets[ae] < Ext::from_f64(1.0) {
        f.amt_text = __content.string(2245).into();
    }
    f.suffix_text = __content.string(2246).into();
    if Ext::from_f64(0.0) < f.gain_loss {
        f.suffix_text = __content.string(2247).into();
    }
    f.prompt_text = f.suffix_text.clone()
        + " of "
        + &g.currency_symbol
        + &fmt_trim(f.gain_loss.abs(), b"################,")
        + " "
        + &g.million_prefix_lc
        + "il."
        + &g.currency_suffix;
    if f.gain_loss.abs() < f.sale_amt * sf(0.03f32) || f.gain_loss.abs() < Ext::from_f64(2.0) {
        f.prompt_text = __content.string(2248).into();
    }
    if cast::<Ext, _>(f.scrap_pct) == Ext::from_f64(0.0) && 10 < buyer && g.cur_player == g.corp_controller[buyer] {
        f.prompt_text = __content.string(2249).into();
    }
    g.news_msg = g.ent_symbol[ae].clone() + " sells " + &f.amt_text + &f.prompt_text;
    if Ext::from_f64(0.0) < cast::<Ext, _>(f.scrap_pct) {
        g.news_msg = replace(g.news_msg.as_bytes(), b"sells", b"scraps");
    }
    f.max_len = 60;
    if len(g.news_msg.as_bytes()) < f.max_len && cast::<Ext, _>(f.scrap_pct) == Ext::from_f64(0.0) {
        g.news_msg = g.news_msg.clone() + " to " + &g.ent_symbol[buyer];
    }
    let news = g.news_msg.clone();
    ui.msgbox(&news, 0x2000, &__content.string(2250).into());
    post_news(g, ui, 0, 0);
    if cast::<Ext, _>(f.scrap_pct) == Ext::from_f64(0.0) {
        f.suffix_text = " from ".into();
        if g.business_assets[ae] < Ext::from_f64(1.0) {
            f.suffix_text = " of ".into();
        }
        g.news_msg = g.ent_symbol[buyer].clone() + __content.string(2062) + &f.amt_text + &f.suffix_text + &g.active_entity_name;
        f.max_len = 68;
        if len(g.news_msg.as_bytes()) < f.max_len {
            post_news(g, ui, 0, 0);
        } else {
            g.news_msg = PbStr::new();
        }
        calc_projected_earnings(g, ui, rng, buyer);
        g.valuation_mode = 2;
        calc_stock_price(g, ui, rng, buyer, f.time_secs);
    }
    g.turn_action_count += 1;
    if g.business_assets[ae] < Ext::from_f64(1.0) {
        g.business_assets[ae] = Ext::from_f64(0.0);
        if g.industry_num[ae] == 37 && 10 < buyer && g.industry_num[buyer] == 37 {
            crate::logic::restructure::transfer_etf_contracts(g, ui, buyer, ae);
        }
        g.industry_num[ae] = 70;
        g.profit_margin[ae] = 0;
        g.disc_spend_pct[ae] = 0;
        g.growth_rate[ae] = 0;
        g.cash[ae] = g.business_assets[ae] + g.cash[ae] + g.working_capital[ae];
        g.working_capital[ae] = Ext::from_f64(0.0);
        g.business_assets[ae] = Ext::from_f64(0.0);
    }
    calc_projected_earnings(g, ui, rng, ae);
    if g.business_assets[ae] < f.orig_assets * sf(0.85f32) || g.business_assets[ae] < g.total_assets * sf(0.3f32) {
        if g.total_assets * sf(0.4f32) < f.orig_assets {
            if -10 < g.investor_sentiment[ae] {
                g.investor_sentiment[ae] -= 10;
            }
            if g.business_assets[ae] < f.orig_assets * sf(0.5f32) {
                if g.investor_sentiment[ae] < -14 {
                    g.investor_sentiment[ae] -= 5;
                } else {
                    let x = rng.rnd_range(10, 1);
                    g.investor_sentiment[ae] = g.investor_sentiment[ae] - (x + 5);
                }
                if Ext::from_f64(10.0) < g.business_assets[ae] {
                    g.profit_margin[ae] = g.profit_margin[ae].wrapping_add(3);
                }
            }
        }
        if Ext::from_f64(100.0) < g.business_assets[ae] && g.business_assets[ae] < Ext::from_f64(10000.0) {
            g.profit_margin[ae] = g.profit_margin[ae].wrapping_add(1);
        }
    }
    f.old_market_cap = g.market_cap[ae];
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, ae, f.time_secs);
    g.market_cap[ae] = g.market_cap[ae] * sf(0.91f32);
    if g.market_cap[ae] < f.old_market_cap * sf(0.9f32) {
        g.news_msg = g.ent_symbol[ae].clone() + __content.string(2251);
    }
}

/// Port of SellBusinessAssets_BlendMargin (0x5dad74), GOSUB of [`sell_business_assets`].
/// The buyer's new `ProfitMargin`: the asset-weighted blend of its margin and the seller's margin
/// - 1..+1 (random) for the acquired assets net of the premium (`goodwillAmt`); a buyer without assets
///   (< 1) takes the seller's margin - 1.
fn blend_margin(g: &mut Globals, rng: &mut Rng, f: &mut Frame) {
    let buyer = f.buyer_id;
    let ae = g.active_entity;
    f.buyer_assets = g.business_assets[buyer];
    if f.buyer_assets < Ext::from_f64(1.0) {
        g.profit_margin[buyer] = g.profit_margin[ae].wrapping_sub(1);
        return;
    }
    f.goodwill_amt = f.sale_amt * sf(0.01f32) * cast::<Ext, _>(f.premium_pct);
    let net = f.sale_amt - f.goodwill_amt;
    f.combined_assets = f.buyer_assets + net;
    f.margin_wt = cast::<f32, _>((f.buyer_assets / f.combined_assets) * Ext::from_f64(100.0) * cast::<Ext, _>(g.profit_margin[buyer]));
    let x = rng.rnd_range(3, 1);
    f.bid_amt = to_quad(cast::<Ext, _>(x) - Ext::from_f64(2.0));
    f.bid_amt = to_quad(cast::<Ext, _>(f.bid_amt) + cast::<Ext, _>(g.profit_margin[ae]));
    f.new_margin_wt = cast::<f32, _>(cast::<Ext, _>(f.bid_amt) * (net / f.combined_assets) * Ext::from_f64(100.0));
    f.rnd_val = to_quad((cast::<Ext, _>(f.new_margin_wt) + cast::<Ext, _>(f.margin_wt)) / Ext::from_f64(100.0));
    g.profit_margin[buyer] = cast::<i16, _>(f.rnd_val);
}

/// Port of SellBusinessAssets_PickRelatedBuyer (0x5daf13), GOSUB of [`sell_business_assets`].
/// With several possible buyers the player controls (`buyerId` < 131), lists them (same industry or
/// holding companies) as a `GetCorp` pick list. A picked eligible company (same industry or holding
/// company, controlled by the player, not the seller) becomes the selected company (`SelectTargetCorp`,
/// which makes it `ActiveEntity`) and buys the assets: BuyBusinessAssets with the old `ActiveEntity`
/// as seller.
fn pick_related_buyer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    if f.buyer_id < 131 {
        f.menu_lines[1] =
            PbStr::from(__content.string(2252)) + &g.ent_symbol[g.active_entity];
        f.menu_lines[2] = string_fill(55, b"-");
        f.menu_lines[3] = __content.string(2253).into();
        f.menu_lines[4] = "--------------------------- ---------------------------".into();
        f.scan_id = 4;
        for list_corp_id in 131..=1600 {
            if g.active_entity == list_corp_id
                || g.entity_status[list_corp_id] != 1
                || g.cur_player != g.corp_controller[list_corp_id]
            {
                continue;
            }
            let ind = g.industry_num[list_corp_id];
            if ind != f.ind_num && ind != 70 {
                continue;
            }
            f.scan_id += 1;
            f.prompt_text = PbStr::from("  ") + &g.ent_name[list_corp_id] + &string_fill(26, b" ");
            f.line_text = mid(f.prompt_text.as_bytes(), 1, 28);
            f.prompt_text = PbStr::from("  ") + &g.ind_name[cast::<i32, _>(ind)] + &string_fill(25, b" ");
            f.prompt_text = mid(f.prompt_text.as_bytes(), 1, 27);
            f.line_text = f.line_text.clone() + &f.prompt_text;
            f.line_text = f.line_text.clone() + &string_fill(125, b" ");
            f.prompt_text = PbStr::from("000") + &str_trim(cast::<Ext, _>(list_corp_id));
            f.prompt_text = right(f.prompt_text.as_bytes(), 4);
            f.menu_lines[cast::<usize, _>(f.scan_id)] = f.line_text.clone() + "@C" + &f.prompt_text;
        }
        f.menu_lines[cast::<usize, _>(f.scan_id + 1)] = f.menu_lines[4].clone();
        f.scan_id += 1;
        f.menu_lines[cast::<usize, _>(f.scan_id + 3)] = "@END".into();
        f.line_count = f.scan_id + 5;
        f.scan_id = 1;
        while f.scan_id <= f.line_count {
            g.text_lines[f.scan_id] = f.menu_lines[cast::<usize, _>(f.scan_id)].clone();
            f.scan_id += 1;
        }
        g.entity_pick_mode = "GetCorp".into();
        g.text_box_title =
            PbStr::from(__content.string(2254)) + &g.active_entity_name + " ===";
        g.text_box_mode = PbStr::new();
        g.text_has_links = 1;
        if !f.skips(SHOW_TEXT_REPORT) {
            show_text_report(g, ui, rng);
        }
        if g.target_corp < 131 {
            g.tmp_symbol = PbStr::new();
            f.buyer_id = 0;
        } else {
            f.buyer_id = g.target_corp;
        }
        let ae = g.active_entity;
        calc_entity_financials(g, ui, rng, ae);
    }
    let buyer = f.buyer_id;
    if 10 < buyer {
        if g.industry_num[buyer] != f.ind_num && g.industry_num[buyer] != 70 {
            let mut msg = PbStr::from("ERROR: ")
                + &g.ent_name[buyer]
                + __content.string(2255)
                + "in the "
                + &g.ind_name[cast::<i32, _>(f.ind_num)]
                + __content.string(2256)
                + __content.string(2257)
                + &g.active_entity_name;
            show_quick_message(g, ui, &mut msg);
            return;
        }
        if 10 < buyer && g.cur_player != g.corp_controller[buyer] {
            let mut msg = PbStr::from("ERROR: ") + &g.ent_name[buyer] + __content.string(2258);
            show_quick_message(g, ui, &mut msg);
            return;
        }
        if g.active_entity == buyer {
            let mut msg = PbStr::from("ERROR: ") + &g.ent_name[buyer] + __content.string(2033);
            show_quick_message(g, ui, &mut msg);
            return;
        }
        f.seller_id = g.active_entity;
        g.tmp_symbol = g.ent_symbol[buyer].clone();
        g.target_corp = buyer;
        select_target_corp(g, ui, rng, f.time_secs);
        let mut msg = g.ent_name[buyer].clone()
            + __content.string(2259)
            + &g.ent_name[f.seller_id]
            + __content.string(2260);
        show_quick_message(g, ui, &mut msg);
        if !f.skips(BUY_BUSINESS_ASSETS) {
            let skip = f.skip.clone();
            super::buy::buy_business_assets_skipping(g, ui, rng, f.seller_id, &f.now.clone(), &skip);
        }
    }
}
