//! Buying business assets: `BuyBusinessAssets` (0x5d2f04) with its GOSUBs, `SelectIndustryOfSelEntity`
//! (0x5d7e6a).
//!
//! The GOSUB blocks (`BuyBusinessAssets_BlendMargin` 0x5d795b, `_AntitrustCheck` 0x5d7b88,
//! `AssetBuyWatchList_Add/Remove/Contains` 0x5d7ea1/0x5d7fb7/0x5d801b, `_ListSellers` 0x5d8073) run on
//! the routine's frame: they are private fns over [`Frame`] (the routine's locals, zeroed at entry).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use super::SHOW_TEXT_REPORT;
use crate::logic::corp_ai::spinoff::calc_asset_sale_price_pct;
use crate::logic::earnings::{balance_cash_t_bills, calc_projected_earnings};
use crate::logic::finance::calc_entity_financials;
use crate::logic::report::show_text_report;
use crate::logic::report::number_to_words;
use crate::logic::reports_lists::pb_array_sort;
use crate::logic::restructure::transfer_etf_contracts;
use crate::logic::trading::borrow_to_cover_purchase;
use crate::logic::sim_entity::asset_sales::{calc_industry_unmet_demand, gen_asset_sale_description, notify_discount_offer};
use crate::logic::valuation::{calc_industry_avg_net_margin, calc_industry_market_share, calc_working_capital_ratio};
use crate::pb::string::compare;

/// `ShowAssetSaleOffers` (0x666a27): may be skipped by tests.
pub const SHOW_ASSET_SALE_OFFERS: u32 = 0x666a27;

/// The locals of `BuyBusinessAssets` (zeroed at entry), shared with its GOSUB blocks.
#[derive(Default)]
struct Frame {
    skip: Vec<u32>,
    /// The local time (for the offers list's stock purchase).
    now: crate::logic::newgame::LocalTime,
    /// `DIM menuLines(0 TO 500)` (local array of the original).
    menu_lines: Vec<PbStr>,
    line_text: PbStr,
    goodwill_msg: PbStr,
    own_ind_revenue: i64,
    mkt_share_pct: i32,
    on_watch_list: i32,
    offer_checked: i32,
    seller_assets_before: Ext,
    tax_loss_flags: i32,
    neg_goodwill: Ext,
    loop_idx: i32,
    saved_sel_ind: i32,
    seller_gain: Ext,
    goodwill_amt: i64,
    want_new: i32,
    board_capped: PbStr,
    input_text: PbStr,
    default_text: PbStr,
    max_amt_text: PbStr,
    tmp_text: PbStr,
    msg_text: PbStr,
    new_margin_wt: f32,
    old_margin_wt: f32,
    ind_num: i16,
    buyer_ind: i16,
    seller_corp: i32,
    new_asset_limit: i32,
    own_ind_assets: i64,
    total_ind_assets: i64,
    share_pct: i64,
    answer: i32,
    ok_flag: i32,
    max_new_assets: i64,
    only_seller: i32,
    seller_count: i32,
    line_idx: i32,
    demand_factor: Ext,
    max_spend: Ext,
    liquid_cash: Ext,
    amount: Ext,
    combined_assets: Ext,
    buyer_assets: Ext,
    max_buyable: Ext,
    avg_margin: f32,
    premium_pct: i32,
    rnd_val: i64,
    tmp_val: i32,
}

impl Frame {
    fn skips(&self, addr: u32) -> bool {
        self.skip.contains(&addr)
    }
}

/// How a GOSUB block left: back to the caller, or out of the whole SUB (`EXIT SUB` / the ON ERROR
/// handler).
#[derive(PartialEq, Eq)]
enum Flow {
    Return,
    ExitSub,
}

/// The ON ERROR handler of BuyBusinessAssets (0x5d8a0a) after an out-of-range subscript (error 9):
/// `ShowRuntimeError(ERR, …)` (0x46ea48) shows the error as a quick message; then the SUB is
/// left. Only the case the port detects (see [`list_sellers`]) is modelled; an error 9 inside a callee
/// without its own handler (e.g. `IndDemand(0)` in CalcIndustryUnmetDemand) does not reach this handler.
///
/// Not modelled: when the pick list's closing UpdateMarketDisplay makes a player the `ActiveEntity`
/// (the selected company was `ActiveEntity` and the player lost its voting control), the original's
/// next `…(ActiveEntity)` access raises error 9 and ends the SUB here; the port goes on with the
/// arrays' dummy elements.
fn runtime_error_9(g: &mut Globals, ui: &mut dyn Ui) {
    crate::logic::restructure::show_runtime_error(g, ui, 9, &PbStr::new(), &PbStr::new(), 0);
}

/// `ShowTextReport` (a pick list; the report follows the clicked link) unless the caller skips it
/// (tests).
fn text_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &Frame) {
    if !f.skips(SHOW_TEXT_REPORT) {
        show_text_report(g, ui, rng);
    }
}

/// `ShowAssetSaleOffers` (the list of discounted offers) unless the caller skips it (tests).
fn asset_sale_offers(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &Frame) {
    if !f.skips(SHOW_ASSET_SALE_OFFERS) {
        super::offers::show_asset_sale_offers(g, ui, rng, &f.now.clone());
    }
}

/// Port of BuyBusinessAssets (0x5d2f04).
/// The player's company `ActiveEntity` (an active operating corp [runtime resource wording] ; banks,
/// insurers and funds are refused) buys business assets: with `seller_id` = 0 either NEW assets (up to a
/// limit set by "the Board": the industry's unmet demand, a demand share for high-margin industries,
/// `(85..115) × ExchangeRate`, twice the total assets of small companies, nothing for depressed
/// industries; repeated large purchases put the company on `AssetBuyWatchList`) for cash with a 7% tax
/// credit, or EXISTING assets of another company of the industry (a holding company first picks the
/// industry and loses its tax carryovers). The seller is the selected / last selected company of the
/// industry (confirmed, else remembered in `DeclinedSellerCorp`) or picked from a list; with `seller_id`
/// ≥ 131 that company is the seller. Sellers not controlled by the current player sell at most 30%
/// (40% if that is under 100) of their assets, refuse when small, when [runtime resource wording]  or
/// company (3 years in 4), when highly profitable, or when the player's [runtime resource wording] more
/// than 20% of the industry (antitrust ruling at 55%). The price includes a premium (or discount) over
/// book value booked as goodwill (negative goodwill: extraordinary income); the seller pays a 10%
/// commission. Working capital moves with the assets; a seller left without assets becomes a holding
/// company (industry 70). News, `TurnActionCount`, projected earnings.
///
/// Quirk (bug 280): with `seller_id` ≥ 131 the local industry `indNum` is never set (0), so the market
/// share is computed for industry 0. `now`: the local time (for the offers list's `BuyStockMenu`).
pub fn buy_business_assets(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, seller_id: i32, now: &crate::logic::newgame::LocalTime) {
    buy_business_assets_skipping(g, ui, rng, seller_id, now, &[]);
}

/// [`buy_business_assets`] with routines (addresses) not called, like the test harness's skips:
/// [`SHOW_TEXT_REPORT`], [`SHOW_ASSET_SALE_OFFERS`].
#[doc(hidden)]
pub fn buy_business_assets_skipping(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    seller_id: i32,
    now: &crate::logic::newgame::LocalTime,
    skip: &[u32],
) {
    let mut f = Frame { skip: skip.to_vec(), now: *now, menu_lines: vec![PbStr::new(); 501], ..Frame::default() };
    main(g, ui, rng, &mut f, seller_id);
    buy_business_assets_exit();
}

/// Port of BuyBusinessAssets_Exit (0x5d8a6f): the routine's exit label (frame release only).
fn buy_business_assets_exit() {}

fn main(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame, seller_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x5d8a0a -> ShowRuntimeError, exit (modelled where detected: runtime_error_9)
    if g.active_entity < 11 {
        return;
    }
    if g.entity_status[g.active_entity] != 1 {
        return;
    }
    f.buyer_ind = g.industry_num[g.active_entity];
    if f.buyer_ind < 3 || f.buyer_ind == 71 {
        f.tmp_text = "BANKS".into();
        if f.buyer_ind == 2 {
            f.tmp_text = "INSURERS".into();
        }
        if f.buyer_ind == 71 {
            f.tmp_text = g.ind_name[71].clone() + "S";
        }
        f.msg_text = f.tmp_text.clone()
            + __content.string(1920);
        ui.msgbox(&f.msg_text, 0x2010, &__content.string(1921).into());
        return;
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    if g.liquid_funds < Ext::from_f64(0.0) {
        g.liquid_funds = Ext::from_f64(0.0);
    }
    if g.liquid_funds < g.exchange_rate * Ext::from_f64(100.0) {
        f.msg_text = PbStr::new();
        if f.buyer_ind == 70 {
            f.msg_text = PbStr::from(__content.string(1922))
                + &fmt_trim(g.exchange_rate * Ext::from_f64(100.0), b"* ###,")
                + " "
                + &g.million_prefix_lc
                + __content.string(1923);
        } else if Ext::from_f64(0.0) < g.liquid_funds {
            if g.liquid_funds <= Ext::from_f64(5.0) {
                f.msg_text = PbStr::from(__content.string(1924))
                    + &trim(g.currency_symbol.as_bytes())
                    + "5 "
                    + &g.million_prefix_lc
                    + __content.string(1925);
            }
        } else {
            f.msg_text = __content.string(1926).into();
        }
        if !f.msg_text.is_empty() {
            ui.msgbox(&(PbStr::from(__content.string(1927)) + &f.msg_text), 0x2040, &__content.string(1928).into());
            return;
        }
    }
    if f.buyer_ind == 70 {
        let ae = g.active_entity;
        if !(-Ext::from_f64(25.0) * g.exchange_rate <= g.tax_loss_carry[ae] && g.tax_credits[ae] <= g.exchange_rate * Ext::from_f64(10.0)) {
            f.msg_text = g.ent_name[ae].clone()
                + __content.string(1929)
                + __content.string(1930)
                + "\r"
                + "\r"
                + __content.string(1931)
                + "\r"
                + "\r";
            if g.tax_loss_carry[ae] < -Ext::from_f64(1.0) {
                f.msg_text = f.msg_text.clone()
                    + __content.string(1932)
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(g.tax_loss_carry[ae].abs(), b"* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ".";
            }
            if Ext::from_f64(1.0) < g.tax_credits[ae] {
                f.msg_text = f.msg_text.clone()
                    + "\r"
                    + __content.string(1933)
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(g.tax_credits[ae], b"* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ".";
            }
            let title = PbStr::from(__content.string(1934)) + &g.ent_symbol[ae] + __content.string(1935);
            f.answer = ui.msgbox(&f.msg_text, 0x2223, &title);
            if f.answer != 6 {
                return;
            }
        }
    }
    if seller_id < 131 {
        f.answer = 5;
        // `true` when the code jumps straight to the seller loop (LAB_005d5179)
        let mut to_seller_loop = false;
        if f.buyer_ind == 70 {
            f.msg_text = PbStr::from(__content.string(1936))
                + &g.active_entity_name
                + __content.string(1937)
                + __content.string(1938);
            f.answer = ui.msgbox(&f.msg_text, 0x2023, &__content.string(1939).into());
            if f.answer == 2 {
                return;
            }
            if f.answer == 6 {
                f.want_new = 1;
            }
            f.menu_lines[3] = __content.string(1940).into();
            f.menu_lines[4] = string_fill(70, b"-");
            f.line_idx = 4;
            f.loop_idx = 3;
            while f.loop_idx <= 69 {
                f.line_idx += 1;
                f.line_text = g.ind_name[f.loop_idx].clone() + &string_fill(150, b" ");
                f.msg_text = PbStr::from("000") + &str_trim(cast::<Ext, _>(f.loop_idx));
                f.msg_text = right(f.msg_text.as_bytes(), 4);
                f.menu_lines[cast::<usize, _>(f.line_idx)] = f.line_text.clone() + "@I" + &f.msg_text;
                f.loop_idx += 1;
            }
            // ARRAY SORT menuLines(5) FOR 67
            pb_array_sort::<PbStr, ()>(&mut f.menu_lines[5..72], None, false, &|a, b| {
                compare(a.as_bytes(), b.as_bytes()) == std::cmp::Ordering::Less
            });
            f.menu_lines[cast::<usize, _>(f.line_idx + 3)] = "@END".into();
            for line_idx in 1..=90 {
                g.text_lines[line_idx] = f.menu_lines[cast::<usize, _>(line_idx)].clone();
            }
            f.line_idx = 91;
            g.entity_pick_mode = "GetInd".into();
            g.text_box_title =
                PbStr::from(__content.string(1941)) + &g.active_entity_name + " ===";
            g.text_box_mode = PbStr::new();
            f.saved_sel_ind = g.sel_industry;
            g.sel_industry = 0;
            text_report(g, ui, rng, f);
            if g.sel_industry == 70 {
                select_industry_of_sel_entity(g);
                return;
            }
            f.ind_num = cast::<i16, _>(g.sel_industry);
            let ae = g.active_entity;
            calc_entity_financials(g, ui, rng, ae);
            g.tmp_ext1 = cast::<Ext, _>(g.cur_player);
            f.mkt_share_pct = calc_industry_market_share(g, f.ind_num);
            if f.answer == 7 {
                to_seller_loop = true;
            }
        } else {
            f.ind_num = f.buyer_ind;
        }
        if !to_seller_loop {
            match new_assets_board(g, ui, rng, f) {
                NewAssets::Exit => return,
                NewAssets::SellerLoop => to_seller_loop = true,
                NewAssets::Continue => {}
            }
        }
        if !to_seller_loop {
            // LAB_005d4882
            if f.buyer_ind < 70 {
                f.tmp_val = 1;
                if notify_discount_offer(g, ui, g.cur_player, 2, 0, f.ind_num) == f.tmp_val {
                    asset_sale_offers(g, ui, rng, f);
                    return;
                }
                f.offer_checked = 1;
            }
            if f.answer == 6 {
                new_assets_purchase(g, ui, rng, f);
                select_industry_of_sel_entity(g);
                return;
            }
        }
        // LAB_005d5179: pick the seller
        loop {
            if 2 < f.buyer_ind && f.buyer_ind < 70 && f.offer_checked == 0 {
                f.tmp_val = 1;
                if notify_discount_offer(g, ui, g.cur_player, 2, 0, f.ind_num) == f.tmp_val {
                    asset_sale_offers(g, ui, rng, f);
                    return;
                }
            }
            f.ok_flag = 0;
            let sel = g.sel_entity;
            if sel < 11
                || g.industry_num[sel] != f.ind_num
                || sel == g.active_entity
                || g.cur_player == g.corp_controller[sel]
            {
                // LAB_005d5290
                let last = g.last_sel_entity;
                if 10 < last && g.industry_num[last] == f.ind_num && last != g.active_entity {
                    f.ok_flag = last;
                }
            } else {
                f.ok_flag = sel;
            }
            if 10 < f.ok_flag && f.ok_flag == g.declined_seller_corp {
                f.ok_flag = 0;
            }
            if 0 < f.ok_flag {
                g.target_corp = f.ok_flag;
                g.tmp_symbol = g.ent_symbol[f.ok_flag].clone();
                f.msg_text = PbStr::from(__content.string(1936))
                    + &g.tmp_symbol
                    + " ("
                    + &g.ent_name[f.ok_flag]
                    + __content.string(1942)
                    + &g.active_entity_name
                    + "?";
                f.new_asset_limit = ui.msgbox(&f.msg_text, 0x2024, &__content.string(1943).into());
                if f.new_asset_limit != 6 {
                    f.ok_flag = 0;
                }
                if f.ok_flag == 0 {
                    g.declined_seller_corp = g.target_corp;
                }
            }
            if f.ok_flag == 0 {
                g.target_corp = 0;
                g.tmp_symbol = PbStr::new();
                if list_sellers(g, ui, rng, f) == Flow::ExitSub {
                    runtime_error_9(g, ui);
                    return;
                }
            }
            if g.tmp_symbol.is_empty() || g.target_corp < 11 {
                return;
            }
            if g.industry_num[g.target_corp] == f.ind_num {
                // LAB_005d56f6
                f.seller_corp = g.target_corp;
                g.target_corp = 0;
                break;
            }
            f.msg_text = PbStr::from("ERROR: ")
                + &g.ent_name[g.target_corp]
                + __content.string(1944)
                + &g.ind_name[cast::<i32, _>(f.ind_num)]
                + __content.string(1945)
                + __content.string(1946)
                + &g.active_entity_name
                + __content.string(1947)
                + &g.ind_name[cast::<i32, _>(f.ind_num)]
                + __content.string(1547)
                + "\r"
                + "\r";
            if f.buyer_ind != 70 {
                f.msg_text = replace(f.msg_text.as_bytes(), b"from a", __content.bytes(1948));
            }
            f.msg_text = f.msg_text.clone() + __content.string(1949);
            let title = PbStr::from(__content.string(1950))
                + &g.ind_name[cast::<i32, _>(g.industry_num[g.target_corp])]
                + __content.string(1951);
            f.new_asset_limit = ui.msgbox(&f.msg_text, 0x2013, &title);
            g.target_corp = 0;
            if f.new_asset_limit == 7 || f.new_asset_limit == 2 {
                return;
            }
        }
    } else {
        f.seller_corp = seller_id;
        f.max_buyable = g.business_assets[seller_id];
        g.tmp_symbol = g.ent_symbol[seller_id].clone();
    }
    // LAB_005d570c
    existing_assets_purchase(g, ui, rng, f, seller_id);
}

/// Outcome of the Board's limits on new assets.
enum NewAssets {
    Continue,
    SellerLoop,
    Exit,
}

/// The new-asset limits of BuyBusinessAssets (0x5d2f04, from the unmet demand to the "Old or New
/// Assets?" question): `maxNewAssets`, `newAssetLimit`, the watch list and the Board's messages.
fn new_assets_board(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) -> NewAssets {
    let __content = crate::content::current();
    // (indNum = 0 when a holding company picked no industry: IndDemand(0) raises error 9 inside
    // CalcIndustryUnmetDemand, which has no handler; the caller does not notice and goes on with 0)
    f.max_new_assets = calc_industry_unmet_demand(g, f.ind_num);
    f.avg_margin = calc_industry_avg_net_margin(g, rng, f.ind_num);
    if Ext::from_f32(17.0f32) < f.avg_margin {
        f.demand_factor = (cast::<Ext, _>(f.avg_margin) - Ext::from_f64(17.0)) * sf(0.008f32);
        if f.demand_factor < sf(0.01f32) {
            f.demand_factor = sf(0.01f32);
        }
        let ind_demand = cast::<Ext, _>(g.ind_demand[cast::<i32, _>(f.ind_num)]);
        if (cast::<Ext, _>(f.max_new_assets)) < ind_demand * f.demand_factor {
            f.max_new_assets = to_quad(ind_demand * f.demand_factor);
        }
    }
    let ex = g.exchange_rate;
    f.new_asset_limit = to_long((rng.rnd() * Ext::from_f64(30.0) + Ext::from_f64(85.0)) * ex);
    if f.buyer_ind == 70 && (cast::<Ext, _>(f.new_asset_limit)) < g.exchange_rate * Ext::from_f64(100.0) {
        f.new_asset_limit = to_long(g.exchange_rate * Ext::from_f64(99.0));
    }
    if (cast::<Ext, _>(f.max_new_assets)) < g.exchange_rate * Ext::from_f64(85.0) && (cast::<Ext, _>(f.max_new_assets)) < g.liquid_funds {
        asset_buy_watch_list_contains(g, f);
    }
    if f.on_watch_list == 1 {
        let ind = g.ind_name[cast::<i32, _>(f.ind_num)].clone();
        f.msg_text = PbStr::from(__content.string(1952))
            + &g.active_entity_name
            + __content.string(1953)
            + __content.string(1954)
            + &ind
            + __content.string(1955)
            + __content.string(1956)
            + __content.string(1957)
            + "\r"
            + "\r"
            + __content.string(1958)
            + &ind
            + __content.string(1959)
            + &g.active_entity_name
            + __content.string(1960)
            + __content.string(1961);
        ui.msgbox(&f.msg_text, 0x2040, &__content.string(1962).into());
        f.answer = 7;
        return NewAssets::SellerLoop;
    }
    if f.buyer_ind == 70 {
        g.profit_margin[g.active_entity] = 0;
    }
    if g.total_assets <= g.exchange_rate * Ext::from_f64(1500.0) {
        f.max_new_assets = to_quad(g.total_assets * Ext::from_f64(2.0));
        if (cast::<Ext, _>(f.max_new_assets)) * sf(0.5f32) < g.liquid_funds {
            f.board_capped = "Y".into();
        }
    }
    if Ext::from_f32(9.0f32) < f.avg_margin && g.exchange_rate * Ext::from_f64(1500.0) < g.total_assets && f.on_watch_list == 0 {
        if (cast::<Ext, _>(f.max_new_assets)) < g.exchange_rate * Ext::from_f64(150.0) && Ext::from_f32(22.0f32) < f.avg_margin {
            f.max_new_assets = to_quad(cast::<Ext, _>(f.max_new_assets) * Ext::from_f64(3.0));
            let ind_demand = cast::<Ext, _>(g.ind_demand[cast::<i32, _>(f.ind_num)]);
            if (cast::<Ext, _>(f.max_new_assets)) < ind_demand * sf(0.025f32) {
                f.max_new_assets = to_quad(ind_demand * sf(0.025f32));
            }
        }
        if f.answer != 7 {
            if (cast::<Ext, _>(f.new_asset_limit)) < cast::<Ext, _>(f.max_new_assets) {
                if (cast::<Ext, _>(f.max_new_assets)) < g.liquid_funds * sf(0.98f32) && f.answer != 7 {
                    f.msg_text = PbStr::from(__content.string(1963))
                        + &g.active_entity_name
                        + __content.string(1964)
                        + "up to "
                        + &trim(g.currency_symbol.as_bytes())
                        + &fmt_trim(cast::<Ext, _>(f.max_new_assets) + Ext::from_f64(1.0), b"* ###,")
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix
                        + __content.string(1965)
                        + __content.string(1966)
                        + __content.string(1967)
                        + &g.ind_name[cast::<i32, _>(f.ind_num)]
                        + __content.string(1968)
                        + "\r"
                        + "\r"
                        + __content.string(1969)
                        + __content.string(1970);
                    ui.msgbox(&f.msg_text, 0x2040, &__content.string(1971).into());
                    f.board_capped = "Y".into();
                }
            } else {
                f.max_new_assets = to_quad(cast::<Ext, _>(f.new_asset_limit));
                if (cast::<Ext, _>(f.max_new_assets)) < g.liquid_funds {
                    f.msg_text = PbStr::from(__content.string(1972))
                        + &g.active_entity_name
                        + __content.string(1973)
                        + __content.string(1974)
                        + &g.ind_name[cast::<i32, _>(f.ind_num)]
                        + __content.string(1975);
                    if Ext::from_f32(12.0f32) <= f.avg_margin {
                        if sf(14.5f32) <= cast::<Ext, _>(f.avg_margin) {
                            f.msg_text = f.msg_text.clone()
                                + __content.string(1976);
                        } else {
                            f.msg_text = f.msg_text.clone() + __content.string(1977);
                        }
                    } else {
                        f.msg_text = f.msg_text.clone() + __content.string(1978);
                    }
                    f.msg_text = f.msg_text.clone()
                        + __content.string(1979)
                        + __content.string(1980)
                        + __content.string(1981);
                    if sf(14.5f32) <= cast::<Ext, _>(f.avg_margin) {
                        f.msg_text =
                            f.msg_text.clone() + __content.string(1982);
                    } else {
                        f.msg_text =
                            f.msg_text.clone() + __content.string(1983);
                    }
                    f.msg_text = f.msg_text.clone()
                        + "."
                        + "\r"
                        + "\r"
                        + __content.string(1984)
                        + "of "
                        + &trim(g.currency_symbol.as_bytes())
                        + &fmt_trim(cast::<Ext, _>(f.max_new_assets) + Ext::from_f64(1.0), b"* ###,")
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix
                        + " at "
                        + __content.string(1985);
                    if f.buyer_ind < 70 && 7 < g.growth_rate[g.active_entity] {
                        f.msg_text = f.msg_text.clone()
                            + __content.string(1986)
                            + "("
                            + &trim(str_num(cast::<Ext, _>(g.growth_rate[g.active_entity]), 7).as_bytes())
                            + __content.string(1987);
                    }
                    f.msg_text = f.msg_text.clone()
                        + "."
                        + "\r"
                        + "\r"
                        + __content.string(1988)
                        + &g.active_entity_name
                        + " from "
                        + __content.string(1989)
                        + "\r"
                        + "\r"
                        + __content.string(1990);
                    let title = g.ent_name[g.active_entity].clone() + __content.string(1991);
                    ui.msgbox(&f.msg_text, 0x2040, &title);
                    f.board_capped = "Y".into();
                }
            }
        }
    }
    if cast::<Ext, _>(f.max_new_assets) <= Ext::from_f64(0.0) {
        if Ext::from_f32(9.0f32) < f.avg_margin && Ext::from_f32(10.0f32) < calc_industry_avg_net_margin(g, rng, f.ind_num) {
            return NewAssets::Continue; // LAB_005d4882
        }
        if f.on_watch_list != 1 {
            f.msg_text = PbStr::from("The ")
                + &g.ind_name[cast::<i32, _>(f.ind_num)]
                + __content.string(1992)
                + __content.string(1993);
            if Ext::from_f32(10.0f32) < calc_industry_avg_net_margin(g, rng, f.ind_num) {
                f.msg_text = PbStr::from(
                    __content.string(1994),
                ) + __content.string(1995)
                    + &g.ind_name[cast::<i32, _>(f.ind_num)]
                    + __content.string(1996);
            }
            f.msg_text = f.msg_text.clone()
                + "\r"
                + "\r"
                + __content.string(1997)
                + &g.active_entity_name
                + __content.string(1998)
                + __content.string(1999)
                + __content.string(2000)
                + &g.active_entity_name
                + __content.string(2001)
                + __content.string(2002)
                + "\r"
                + "\r"
                + __content.string(1990);
            ui.msgbox(&f.msg_text, 0x2040, &__content.string(1962).into());
            f.answer = 7;
        }
    } else if f.want_new == 1 {
        f.answer = 6;
    } else {
        f.msg_text = PbStr::from(__content.string(1936))
            + &g.active_entity_name
            + __content.string(1937)
            + __content.string(1938);
        f.answer = ui.msgbox(&f.msg_text, 0x2023, &__content.string(1939).into());
        if f.answer == 2 {
            return NewAssets::Exit;
        }
    }
    NewAssets::Continue
}

/// The purchase of NEW assets in BuyBusinessAssets (0x5d2f04): asks the amount (up to the Board's limit
/// and the funds), pays cash (borrowing the shortfall), adds the assets with a 7% tax credit; a holding
/// company enters industry `indNum` with a random margin and loses its tax carryovers.
fn new_assets_purchase(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let ae = g.active_entity;
    f.msg_text = PbStr::from(__content.string(2003))
        + &g.ent_symbol[ae]
        + __content.string(2004)
        + &g.ind_name[cast::<i32, _>(f.ind_num)]
        + __content.string(2005);
    f.max_spend = g.liquid_funds - Ext::from_f64(1.0);
    if cast::<Ext, _>(f.max_new_assets) + Ext::from_f64(1.0) < f.max_spend {
        f.max_spend = cast::<Ext, _>(f.max_new_assets) + Ext::from_f64(1.0);
    }
    f.tmp_text = fmt_trim(f.max_spend, b"* ###,.00") + " " + &g.million_prefix_lc + "illion.";
    f.ok_flag = 0;
    while f.ok_flag == 0 {
        f.input_text = ui.inputbox(&f.msg_text, &__content.string(2006).into(), &f.tmp_text);
        f.amount = val_removing(&f.input_text, b"$ ,");
        if f.amount <= Ext::from_f64(0.0) {
            break;
        }
        if !(f.amount <= f.max_spend + sf(0.5f32)) {
            continue;
        }
        if !(Ext::from_f64(100.0) <= f.amount || f.buyer_ind != 70) {
            let mut msg = PbStr::from(__content.string(2007))
                + __content.string(2008)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(g.exchange_rate * Ext::from_f64(100.0), b"* ###.00")
                + " "
                + &g.million_prefix_lc
                + __content.string(2009)
                + __content.string(2010);
            show_quick_message(g, ui, &mut msg);
            continue;
        }
        let ae = g.active_entity;
        f.liquid_cash = g.cash[ae] + g.t_bills[ae];
        if f.liquid_cash < f.amount {
            let mut need = f.amount - f.liquid_cash;
            let answer = borrow_to_cover_purchase(g, ui, &mut need, Ext::from_f64(0.0), 0);
            f.tmp_val = 7;
            if answer == 7 {
                break;
            }
        }
        g.cash[ae] = g.cash[ae] - f.amount;
        if g.cash[ae] < Ext::from_f64(0.0) {
            balance_cash_t_bills(g, ae);
        }
        if f.buyer_ind == 70 {
            f.rnd_val = to_quad(rng.rnd() * Ext::from_f64(13.0) - Ext::from_f64(5.0));
            g.profit_margin[ae] = cast::<i16, _>(f.rnd_val);
            g.industry_num[ae] = f.ind_num;
            if g.tax_loss_carry[ae] < Ext::from_f64(0.0) {
                if g.tax_loss_carry[ae] < -Ext::from_f64(25.0) * g.exchange_rate {
                    f.tax_loss_flags = 1;
                }
                g.tax_loss_carry[ae] = Ext::from_f64(0.0);
            }
            if g.exchange_rate * Ext::from_f64(10.0) < g.tax_credits[ae] {
                f.tax_loss_flags += 2;
            }
            g.tax_credits[ae] = Ext::from_f64(0.0);
        } else {
            blend_margin(g, rng, f);
        }
        if !(f.board_capped == "Y") || f.amount <= cast::<Ext, _>(f.max_new_assets) * sf(0.1f32) {
            asset_buy_watch_list_remove(g);
        } else {
            asset_buy_watch_list_add(g, f);
        }
        g.business_assets[ae] = g.business_assets[ae] + f.amount;
        g.news_msg = g.ent_symbol[ae].clone()
            + __content.string(2011)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(f.amount, b"* ###,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + __content.string(2012);
        let news = g.news_msg.clone();
        ui.msgbox(&news, 0x2000, &__content.string(2013).into());
        g.tax_credits[ae] = g.tax_credits[ae] + f.amount * sf(0.07f32);
        if Ext::from_f64(1.0) <= f.amount {
            post_news(g, ui, 0, 0);
        }
        if f.tax_loss_flags == 1 || f.tax_loss_flags == 3 {
            g.news_msg = g.ent_symbol[ae].clone() + __content.string(2014);
        }
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        if 1 < f.tax_loss_flags {
            g.news_msg = g.ent_symbol[ae].clone() + __content.string(2015);
        }
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        f.ok_flag = 1;
        g.turn_action_count += 1;
        calc_projected_earnings(g, ui, rng, ae);
    }
}

/// The purchase of EXISTING assets from `sellerCorp` in BuyBusinessAssets (0x5d2f04, from
/// LAB_005d570c): the seller's terms, the amount, goodwill, the payment and the transfer.
fn existing_assets_purchase(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame, seller_id: i32) {
    let __content = crate::content::current();
    let seller = f.seller_corp;
    f.ok_flag = 1;
    f.msg_text = PbStr::new();
    if f.buyer_ind == 70 {
        if !(130 < seller && g.industry_num[seller] < 70) {
            f.msg_text = PbStr::from(__content.string(2016))
                + &g.ent_name[seller]
                + __content.string(1546)
                + &g.ind_name[cast::<i32, _>(g.industry_num[seller])]
                + __content.string(2017);
            f.ok_flag = 0;
        }
    } else if g.industry_num[seller] != f.buyer_ind {
        f.msg_text = PbStr::from(__content.string(2018))
            + &g.ent_name[seller]
            + __content.string(2019)
            + __content.string(2020)
            + &g.active_entity_name
            + __content.string(2021)
            + __content.string(2022)
            + &g.ind_name[cast::<i32, _>(f.buyer_ind)]
            + __content.string(2023)
            + __content.string(2024);
        f.ok_flag = 0;
    }
    if f.ok_flag == 0 {
        show_quick_message(g, ui, &mut f.msg_text);
        return;
    }
    if g.business_assets[seller] < g.exchange_rate * Ext::from_f64(250.0) && g.cur_player != g.corp_controller[seller] {
        f.ok_flag = 0;
    }
    if f.ok_flag == 1 {
        f.max_buyable = g.business_assets[seller] * sf(0.3f32);
        if f.max_buyable < Ext::from_f64(100.0) {
            f.max_buyable = g.business_assets[seller] * sf(0.4f32);
        }
        if g.cur_player == g.corp_controller[seller] {
            f.max_buyable = g.business_assets[seller];
        }
    }
    g.voting_controller = g.corp_controller[seller];
    if g.voting_controller < 11 || seller == g.voting_controller {
        if 0 < g.voting_controller && g.voting_controller < 11 && g.cur_player != g.voting_controller {
            g.voting_controller = -1;
            f.ok_flag = 0;
        }
    } else if (seller + g.cur_year) % 4 != 1 {
        g.voting_controller = -1;
        f.ok_flag = 0;
    }
    if Ext::from_f64(30.0) < calc_asset_sale_price_pct(g, rng, seller)
        && (g.cur_year + seller) % 5 != 2
        && g.cur_player != g.corp_controller[seller]
    {
        g.voting_controller = -2;
    }
    if seller == g.active_entity {
        f.ok_flag = 0;
    }
    g.tmp_ext1 = cast::<Ext, _>(g.cur_player);
    f.mkt_share_pct = calc_industry_market_share(g, f.ind_num);
    if 20 < f.mkt_share_pct && g.cur_player != g.corp_controller[seller] {
        f.ok_flag = 0;
    }
    if f.ok_flag != 0 && Ext::from_f64(0.0) < g.liquid_funds && Ext::from_f64(0.0) < f.max_buyable && g.voting_controller != -2 {
        if f.buyer_ind == 70 && seller_id == 0 {
            f.tmp_val = 1;
            let seller_ind = g.industry_num[seller];
            if notify_discount_offer(g, ui, g.cur_player, 2, 0, seller_ind) == f.tmp_val {
                asset_sale_offers(g, ui, rng, f);
                return;
            }
        }
        if g.cur_player == g.corp_controller[seller] {
            f.max_buyable = g.business_assets[seller];
        }
        if f.mkt_share_pct < 25 && g.cur_player != g.corp_controller[seller] {
            calc_industry_unmet_demand(g, f.ind_num);
            f.own_ind_revenue = to_quad(g.ind_total_assets * sf(0.01f32) * cast::<Ext, _>(f.mkt_share_pct));
            if (cast::<Ext, _>(f.own_ind_revenue)) < g.business_assets[g.active_entity] {
                f.own_ind_revenue = to_quad(g.business_assets[g.active_entity]);
            }
            f.max_new_assets = to_quad(g.ind_total_assets * sf(0.25f32) - cast::<Ext, _>(f.own_ind_revenue));
            if (cast::<Ext, _>(f.max_new_assets)) < f.max_buyable {
                f.max_buyable = cast::<Ext, _>(f.max_new_assets) + Ext::from_f64(1.0);
            }
        }
        f.tmp_text = fmt_trim(g.liquid_funds, b"############.00");
        f.max_amt_text = fmt_trim(f.max_buyable, b"* ###,");
        let ae = g.active_entity;
        f.msg_text = PbStr::from(__content.string(2025))
            + &g.million_prefix_lc
            + __content.string(2026)
            + &g.tmp_symbol
            + " does "
            + &g.ent_symbol[ae]
            + __content.string(2027)
            + "of "
            + &trim(g.currency_symbol.as_bytes())
            + &f.max_amt_text
            + " "
            + &g.million_prefix_lc
            + __content.string(2028)
            + &g.ent_symbol[ae]
            + __content.string(2029)
            + &f.tmp_text
            + " "
            + &g.million_prefix_lc
            + "illion.";
        f.default_text = f.tmp_text.clone();
        if f.max_buyable < g.liquid_funds {
            f.default_text = f.max_amt_text.clone();
        }
        f.ok_flag = 0;
        while f.ok_flag == 0 {
            f.input_text = ui.inputbox(&f.msg_text, &__content.string(2030).into(), &f.default_text);
            f.amount = val_removing(&f.input_text, b"$ ,");
            if f.amount <= Ext::from_f64(0.0) {
                return;
            }
            if f.amount <= f.max_buyable + Ext::from_f64(1.0) {
                if g.liquid_funds + Ext::from_f64(1.0) < f.amount {
                    f.input_text = g.active_entity_name.clone()
                        + __content.string(2031)
                        + &f.tmp_text
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix
                        + ".";
                } else if Ext::from_f64(100.0) <= f.amount || f.buyer_ind != 70 {
                    if transfer_existing_assets(g, ui, rng, f) == Flow::ExitSub {
                        return;
                    }
                } else {
                    let mut msg = PbStr::from(__content.string(2007))
                        + __content.string(2008)
                        + &trim(g.currency_symbol.as_bytes())
                        + "100 "
                        + &g.million_prefix_lc
                        + __content.string(2009)
                        + __content.string(2010);
                    show_quick_message(g, ui, &mut msg);
                    continue;
                }
            } else {
                f.input_text = PbStr::from(__content.string(2032))
                    + &f.max_amt_text
                    + " "
                    + &g.million_prefix_lc
                    + "illion.";
                if g.cur_player == g.corp_controller[f.seller_corp] {
                    f.input_text = replace(f.input_text.as_bytes(), b"willing", b"able");
                }
            }
            // LAB_005d791a
            if !f.input_text.is_empty() {
                show_quick_message(g, ui, &mut f.input_text);
            }
        }
        // LAB_005d7951
        select_industry_of_sel_entity(g);
        return;
    }
    if seller == g.active_entity {
        f.msg_text = g.active_entity_name.clone() + __content.string(2033);
    } else {
        'msg: {
            if 20 < f.mkt_share_pct && g.cur_player != g.corp_controller[seller] {
                f.msg_text = PbStr::from(__content.string(2034))
                    + &g.ent_name[seller]
                    + __content.string(2035)
                    + &g.ind_name[cast::<i32, _>(f.ind_num)]
                    + __content.string(2036)
                    + __content.string(2037)
                    + "of"
                    + &number_to_words(f.mkt_share_pct, 0, 1)
                    + __content.string(2038)
                    + __content.string(2039);
                if f.mkt_share_pct < rng.rnd_range(5, 0) + 45 {
                    f.msg_text = f.msg_text.clone() + __content.string(2040);
                } else if 50 < f.mkt_share_pct {
                    f.msg_text = f.msg_text.clone() + __content.string(2041);
                }
                break 'msg;
            }
            if Ext::from_f64(0.0) < g.liquid_funds {
                if g.voting_controller == -1 && f.msg_text.is_empty() {
                    f.msg_text = PbStr::from(__content.string(2042))
                        + &g.ent_name[seller]
                        + __content.string(1121)
                        + &g.ent_name[g.corp_controller[seller]]
                        + __content.string(2043);
                    break 'msg;
                }
                if Ext::from_f64(0.0) < f.max_buyable || f.ok_flag != 1 {
                    let seller_ind = g.industry_num[seller];
                    if 2 < seller_ind && seller_ind != 70 && (seller_ind == f.buyer_ind || f.buyer_ind == 70) {
                        if f.msg_text.is_empty()
                            && g.business_assets[seller] < g.exchange_rate * Ext::from_f64(250.0)
                            && g.cur_player != g.corp_controller[seller]
                            && f.ok_flag == 0
                        {
                            f.msg_text = g.ent_name[seller].clone()
                                + __content.string(2044);
                            break 'msg;
                        }
                        if g.voting_controller == -2 {
                            f.msg_text = g.ent_name[seller].clone()
                                + __content.string(2045);
                        }
                    }
                } else {
                    f.msg_text = g.ent_name[seller].clone() + __content.string(2046);
                }
            } else {
                f.msg_text =
                    g.active_entity_name.clone() + __content.string(2047);
            }
        }
    }
    // LAB_005d6193
    show_quick_message(g, ui, &mut f.msg_text);
}

/// The completed purchase of `amount` of `sellerCorp`'s business assets (BuyBusinessAssets 0x5d2f04,
/// inside the amount loop): antitrust check, premium/goodwill, payment and transfer, news. Sets
/// `okFlag = 1`; `ExitSub` when the antitrust ruling or the goodwill question ends the routine.
fn transfer_existing_assets(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) -> Flow {
    let __content = crate::content::current();
    if antitrust_check(g, ui, f) == Flow::ExitSub {
        return Flow::ExitSub;
    }
    let seller = f.seller_corp;
    f.premium_pct = 0;
    f.goodwill_amt = 0;
    if g.cur_player != g.corp_controller[seller] {
        f.premium_pct = to_long(calc_asset_sale_price_pct(g, rng, seller));
        if !(f.mkt_share_pct < 11 && g.exchange_rate * Ext::from_f64(1000.0) <= g.business_assets[seller]) {
            f.tmp_val = f.premium_pct + 20;
            f.premium_pct = f.tmp_val + (g.cur_quarter + g.cur_year) % 11;
            if g.business_assets[seller] < g.exchange_rate * Ext::from_f64(600.0) {
                f.premium_pct += 10;
            }
        }
    }
    // LAB_005d6951
    if f.premium_pct != 0 {
        let p = cast::<Ext, _>(f.premium_pct);
        f.goodwill_amt = to_quad((p / (p + Ext::from_f64(100.0))) * Ext::from_f64(100.0));
        f.goodwill_msg = g.active_entity_name.clone() + __content.string(2048);
        if f.premium_pct < 0 {
            f.goodwill_msg = f.goodwill_msg.clone()
                + __content.string(2049)
                + &str_s(p.abs())
                + "% "
                + __content.string(2050)
                + "to"
                + &str_s((cast::<Ext, _>(f.goodwill_amt)).abs())
                + __content.string(2051)
                + __content.string(2052);
            if g.goodwill[g.active_entity] <= Ext::from_f64(0.0) {
                f.goodwill_msg = f.goodwill_msg.clone() + ".";
            } else {
                f.goodwill_msg = f.goodwill_msg.clone() + __content.string(2053);
            }
        } else {
            f.goodwill_msg = f.goodwill_msg.clone()
                + __content.string(2054)
                + &str_s(p)
                + "% "
                + __content.string(2055)
                + &str_s(cast::<Ext, _>(f.goodwill_amt))
                + __content.string(2056)
                + &str_s(p)
                + "/"
                + &str_trim(Ext::from_f64(100.0) + p)
                + __content.string(2057)
                + __content.string(2058);
        }
        f.goodwill_msg =
            f.goodwill_msg.clone() + "\r" + "\r" + __content.string(2059);
        f.answer = ui.msgbox(&f.goodwill_msg, 0x2223, &__content.string(2060).into());
        if f.answer != 6 {
            return Flow::ExitSub;
        }
        f.goodwill_amt = to_quad(cast::<Ext, _>(f.goodwill_amt) * sf(0.01f32) * f.amount);
    }
    let ae = g.active_entity;
    f.liquid_cash = g.cash[ae] + g.t_bills[ae];
    if f.liquid_cash < f.amount {
        f.neg_goodwill = cast::<Ext, _>(f.goodwill_amt);
        if Ext::from_f64(0.0) < f.neg_goodwill {
            f.neg_goodwill = Ext::from_f64(0.0);
        }
        let mut need = f.amount - f.liquid_cash;
        let answer = borrow_to_cover_purchase(g, ui, &mut need, f.neg_goodwill, 0);
        f.tmp_val = 7;
        if answer == 7 {
            return Flow::ExitSub;
        }
    }
    g.cash[ae] = g.cash[ae] - f.amount;
    if g.cash[ae] < Ext::from_f64(0.0) {
        balance_cash_t_bills(g, ae);
    }
    g.cash[seller] = g.cash[seller] + f.amount;
    f.seller_gain = cast::<Ext, _>(f.goodwill_amt);
    if g.cur_player != g.corp_controller[seller] {
        g.cash[seller] = g.cash[seller] - f.amount * sf(0.1f32);
        f.seller_gain = f.seller_gain - f.amount * sf(0.1f32);
    }
    g.extraordinary_pl[seller] = to_quad(cast::<Ext, _>(g.extraordinary_pl[seller]) + f.seller_gain);
    g.goodwill[ae] = g.goodwill[ae] + cast::<Ext, _>(f.goodwill_amt);
    if g.goodwill[ae] < Ext::from_f64(0.0) {
        g.extraordinary_pl[ae] = to_quad(cast::<Ext, _>(g.extraordinary_pl[ae]) - g.goodwill[ae]);
        g.goodwill[ae] = Ext::from_f64(0.0);
    }
    if f.buyer_ind == 70 {
        g.business_assets[ae] = Ext::from_f64(0.0);
        g.profit_margin[ae] = g.profit_margin[seller].wrapping_sub(cast::<i16, _>(g.difficulty_level));
        g.industry_num[ae] = g.industry_num[seller];
        if g.tax_loss_carry[ae] < -Ext::from_f64(25.0) * g.exchange_rate {
            f.tax_loss_flags = 1;
        }
        g.tax_loss_carry[ae] = Ext::from_f64(0.0);
        if g.exchange_rate * Ext::from_f64(10.0) < g.tax_credits[ae] {
            f.tax_loss_flags += 2;
        }
        g.tax_credits[ae] = Ext::from_f64(0.0);
    } else {
        blend_margin(g, rng, f);
    }
    let net = f.amount - cast::<Ext, _>(f.goodwill_amt);
    g.business_assets[ae] = g.business_assets[ae] + net;
    let r = calc_working_capital_ratio(g, ae);
    g.working_capital[ae] = g.working_capital[ae] + net * r;
    let r = calc_working_capital_ratio(g, ae);
    g.cash[ae] = g.cash[ae] - net * r;
    f.seller_assets_before = g.business_assets[seller];
    g.business_assets[seller] = g.business_assets[seller] - net;
    let assets = g.business_assets[seller];
    let r = calc_working_capital_ratio(g, seller);
    f.max_spend = g.working_capital[seller] - assets * r;
    if Ext::from_f64(0.0) < f.max_spend {
        g.cash[seller] = g.cash[seller] + f.max_spend;
        g.working_capital[seller] = g.working_capital[seller] - f.max_spend;
    }
    if g.business_assets[seller] < Ext::from_f64(1.0) {
        if g.business_assets[seller] < Ext::from_f64(0.0) {
            g.business_assets[seller] = Ext::from_f64(0.0);
        }
        if g.industry_num[seller] == 37 {
            let ae = g.active_entity;
            transfer_etf_contracts(g, ui, ae, seller);
        }
        g.industry_num[seller] = 70;
        g.profit_margin[seller] = 0;
        g.disc_spend_pct[seller] = 0;
        g.growth_rate[seller] = 0;
        g.cash[seller] = g.business_assets[seller] + g.cash[seller] + g.working_capital[seller];
        g.working_capital[seller] = Ext::from_f64(0.0);
        g.business_assets[seller] = Ext::from_f64(0.0);
    }
    if g.business_assets[seller] < f.seller_assets_before * sf(0.85f32) && 30 < g.investor_sentiment[seller] {
        g.investor_sentiment[seller] -= 10;
    }
    f.ok_flag = 1;
    gen_asset_sale_description(g, rng, ae);
    f.tmp_text = g.asset_sale_desc.clone() + " from ";
    if g.business_assets[seller] == Ext::from_f64(0.0) {
        f.tmp_text = __content.string(2061).into();
    }
    g.news_msg = g.ent_symbol[ae].clone() + __content.string(2062) + &f.tmp_text + &g.ent_symbol[seller];
    f.msg_text = fmt_trim(f.amount, b"* ###,");
    if Ext::from_f64(1.0) < f.amount {
        g.news_msg =
            g.news_msg.clone() + " for " + &g.currency_symbol + &f.msg_text + " " + &g.million_prefix_lc + "il.";
    }
    f.input_text = PbStr::new();
    let news = g.news_msg.clone();
    ui.msgbox(&news, 0x2000, &__content.string(2013).into());
    post_news(g, ui, 0, 0);
    if f.tax_loss_flags == 1 {
        g.news_msg = g.ent_symbol[ae].clone() + __content.string(2014);
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    if 1 < f.tax_loss_flags {
        g.news_msg = g.ent_symbol[ae].clone() + __content.string(2063);
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.turn_action_count += 1;
    calc_projected_earnings(g, ui, rng, ae);
    calc_projected_earnings(g, ui, rng, seller);
    Flow::Return
}

/// Port of BuyBusinessAssets_BlendMargin (0x5d795b), GOSUB of [`buy_business_assets`].
/// The buyer's new `ProfitMargin`: the asset-weighted blend of its margin and the acquired assets'
/// margin (the seller's + 2 - `DifficultyLevel`; for new assets a random -5..8). A buyer without assets
/// (< 1) takes the seller's margin (- 1 if both have the same controller, else + 2 - difficulty).
fn blend_margin(g: &mut Globals, rng: &mut Rng, f: &mut Frame) {
    let ae = g.active_entity;
    f.buyer_assets = g.business_assets[ae];
    if f.buyer_assets < Ext::from_f64(1.0) && 10 < f.seller_corp {
        if g.corp_controller[ae] == g.corp_controller[f.seller_corp] {
            g.profit_margin[ae] = g.profit_margin[f.seller_corp].wrapping_sub(1);
        } else {
            f.tmp_val = g.difficulty_level;
            g.profit_margin[ae] = (g.profit_margin[f.seller_corp].wrapping_add(2)).wrapping_sub(cast::<i16, _>(f.tmp_val));
        }
        return;
    }
    let net = f.amount - cast::<Ext, _>(f.goodwill_amt);
    f.combined_assets = f.buyer_assets + net;
    f.old_margin_wt = cast::<f32, _>((f.buyer_assets / f.combined_assets) * Ext::from_f64(100.0) * cast::<Ext, _>(g.profit_margin[ae]));
    f.own_ind_assets = to_quad(rng.rnd() * Ext::from_f64(13.0) - Ext::from_f64(5.0));
    if 10 < f.seller_corp {
        f.own_ind_assets = to_quad((Ext::from_f64(2.0) + cast::<Ext, _>(g.profit_margin[f.seller_corp])) - cast::<Ext, _>(g.difficulty_level));
    }
    f.new_margin_wt = cast::<f32, _>(cast::<Ext, _>(f.own_ind_assets) * (net / f.combined_assets) * Ext::from_f64(100.0));
    f.rnd_val = to_quad((cast::<Ext, _>(f.new_margin_wt) + cast::<Ext, _>(f.old_margin_wt)) / Ext::from_f64(100.0));
    g.profit_margin[ae] = cast::<i16, _>(f.rnd_val);
}

/// Port of BuyBusinessAssets_AntitrustCheck (0x5d7b88), GOSUB of [`buy_business_assets`].
/// Unless the antitrust cheat is on or the seller is the current player's: if the player's companies
/// own assets in the industry (`indNum` = the seller's for a holding company) and would own 55% or more
/// of it after buying `amount`, shows the agency's ruling and leaves BuyBusinessAssets (`ExitSub`).
///
/// Quirk (bug 285): unreachable in practice — sellers outside the player's control refuse above a 20%
/// share, and below 25% the purchase is capped at `25% of the industry - own assets`.
fn antitrust_check(g: &mut Globals, ui: &mut dyn Ui, f: &mut Frame) -> Flow {
    let __content = crate::content::current();
    if g.antitrust_cheat_used == 1 {
        return Flow::Return;
    }
    if g.cur_player == g.corp_controller[f.seller_corp] {
        return Flow::Return;
    }
    if f.buyer_ind == 70 {
        f.ind_num = g.industry_num[f.seller_corp];
    } else {
        f.ind_num = f.buyer_ind;
    }
    f.total_ind_assets = 0;
    f.own_ind_assets = 0;
    f.line_idx = 131;
    while f.line_idx <= 1600 {
        let c = f.line_idx;
        if g.entity_status[c] == 1 && g.industry_num[c] == f.ind_num {
            f.total_ind_assets = to_quad(cast::<Ext, _>(f.total_ind_assets) + g.business_assets[c]);
            if g.cur_player == g.corp_controller[c] {
                f.own_ind_assets = to_quad(cast::<Ext, _>(f.own_ind_assets) + g.business_assets[c]);
            }
        }
        f.line_idx += 1;
    }
    if cast::<Ext, _>(f.own_ind_assets) == Ext::from_f64(0.0) {
        return Flow::Return;
    }
    f.own_ind_assets = to_quad(cast::<Ext, _>(f.own_ind_assets) + f.amount);
    if (cast::<Ext, _>(f.own_ind_assets) / cast::<Ext, _>(f.total_ind_assets)) < sf(0.55f32) {
        return Flow::Return;
    }
    f.amount = cast::<Ext, _>(f.own_ind_assets) / cast::<Ext, _>(f.total_ind_assets);
    f.share_pct = to_quad(f.amount * Ext::from_f64(100.0));
    f.msg_text = PbStr::from(__content.string(2064))
        + __content.string(2065)
        + __content.string(2066)
        + &g.cur_player_name
        + __content.string(2067)
        + &str_s(cast::<Ext, _>(f.share_pct))
        + __content.string(2068)
        + &g.ind_name[cast::<i32, _>(f.buyer_ind)]
        + __content.string(2069)
        + __content.string(2070);
    ui.msgbox(&f.msg_text, 0x1010, &__content.string(2071).into());
    Flow::ExitSub
}

/// Port of SelectIndustryOfSelEntity (0x5d7e6a).
/// For a selected company (`SelEntity > 10`): `SelIndustry` = its industry (the industry combo box,
/// SelectIndustryComboItem 0x46df33, is UI).
pub fn select_industry_of_sel_entity(g: &mut Globals) {
    if 10 < g.sel_entity {
        g.sel_industry = cast::<i32, _>(g.industry_num[g.sel_entity]);
        // (UI) SelectIndustryComboItem (0x46df33): selects the combo item, EnableMainControls
    }
}

/// Port of AssetBuyWatchList_Add (0x5d7ea1), GOSUB of [`buy_business_assets`].
/// Puts `ActiveEntity` on `AssetBuyWatchList(1..50)` (first free entry; a full list overwrites entry 1).
fn asset_buy_watch_list_add(g: &mut Globals, f: &mut Frame) {
    f.on_watch_list = 0;
    for watch_idx in 1..=50 {
        if g.active_entity == g.asset_buy_watch_list[watch_idx] {
            f.on_watch_list = 1;
        }
    }
    if f.on_watch_list == 0 {
        for watch_idx in 1..=50 {
            if g.asset_buy_watch_list[watch_idx] == 0 {
                g.asset_buy_watch_list[watch_idx] = g.active_entity;
                return;
            }
            if watch_idx == 50 && 0 < g.asset_buy_watch_list[50] {
                g.asset_buy_watch_list[1] = g.active_entity;
            }
        }
    }
}

/// Port of AssetBuyWatchList_Remove (0x5d7fb7), GOSUB of [`buy_business_assets`].
/// Removes `ActiveEntity` from `AssetBuyWatchList(1..50)`.
fn asset_buy_watch_list_remove(g: &mut Globals) {
    for watch_idx in 1..=50 {
        if g.active_entity == g.asset_buy_watch_list[watch_idx] {
            g.asset_buy_watch_list[watch_idx] = 0;
        }
    }
}

/// Port of AssetBuyWatchList_Contains (0x5d801b), GOSUB of [`buy_business_assets`].
/// `onWatchList` = 1 if `ActiveEntity` is on `AssetBuyWatchList(1..50)`, else 0.
fn asset_buy_watch_list_contains(g: &mut Globals, f: &mut Frame) {
    f.on_watch_list = 0;
    for watch_idx in 1..=50 {
        if g.active_entity == g.asset_buy_watch_list[watch_idx] {
            f.on_watch_list = 1;
        }
    }
}

/// Port of BuyBusinessAssets_ListSellers (0x5d8073), GOSUB of [`buy_business_assets`].
/// Lists the other active companies of industry `indNum` (name, assets, controller) as a `GetCorp` pick
/// list; the picked company (`TargetCorp` ≥ 131) gives `TmpSymbol`, else `TmpSymbol = ""`. With no other
/// company, or exactly one, only a message is shown.
///
/// Quirk (bug 281): for the single possible seller it sets `SelIndustry` (not `TargetCorp`) to the
/// company's id, so the caller ends without a purchase.
///
/// Returns `ExitSub` when the original hits error 9 (a listed company with `CorpController` outside
/// 1..1600: `EntSymbol(CorpController)`), which ends BuyBusinessAssets through its ON ERROR handler.
fn list_sellers(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) -> Flow {
    let __content = crate::content::current();
    for x in f.menu_lines.iter_mut() {
        *x = PbStr::new();
    }
    f.seller_count = 0;
    f.only_seller = 0;
    for corp_idx in 131..=1600 {
        if g.active_entity != corp_idx && g.entity_status[corp_idx] == 1 && g.industry_num[corp_idx] == f.ind_num {
            f.seller_count += 1;
            f.only_seller = corp_idx;
        }
    }
    if f.seller_count < 2 {
        if f.seller_count == 0 {
            let mut msg = PbStr::from(__content.string(2072))
                + &g.ind_name[cast::<i32, _>(f.ind_num)]
                + __content.string(2073)
                + &g.active_entity_name;
            show_quick_message(g, ui, &mut msg);
            return Flow::Return;
        }
        if 130 < f.only_seller && f.seller_count == 1 {
            g.sel_industry = f.only_seller;
            g.tmp_symbol = g.ent_symbol[f.only_seller].clone();
            f.msg_text = PbStr::from(__content.string(2074))
                + &g.ind_name[cast::<i32, _>(f.ind_num)]
                + __content.string(2075)
                + &g.active_entity_name
                + __content.string(2076)
                + __content.string(2077)
                + &g.ent_name[f.only_seller];
            show_quick_message(g, ui, &mut f.msg_text);
            return Flow::Return;
        }
    }
    f.menu_lines[1] =
        PbStr::from(__content.string(2078)) + &g.active_entity_name + __content.string(2079);
    f.menu_lines[2] = string_fill(len(g.active_entity_name.as_bytes()) + 58, b"-");
    f.menu_lines[3] = PbStr::from(__content.string(2080))
        + &g.million_prefix_uc
        + __content.string(2081);
    f.menu_lines[4] = "------------------------- --------------------------- -------------".into();
    f.line_idx = 4;
    for loop_idx in 131..=1600 {
        f.loop_idx = loop_idx;
        if g.active_entity == loop_idx || g.entity_status[loop_idx] != 1 || g.industry_num[loop_idx] != f.ind_num {
            continue;
        }
        f.line_idx += 1;
        f.msg_text = g.ent_name[loop_idx].clone() + &string_fill(26, b" ");
        f.msg_text = mid(f.msg_text.as_bytes(), 1, 26);
        f.line_text = PbStr::from("  ") + &f.msg_text;
        f.msg_text = string_fill(30, b" ")
            + &fmt_trim(g.business_assets[loop_idx], b"##############,.00")
            + &string_fill(10, b" ");
        f.msg_text = right(f.msg_text.as_bytes(), 27);
        f.line_text = f.line_text.clone() + &f.msg_text;
        f.msg_text = __content.string(2082).into();
        let ctrl = g.corp_controller[loop_idx];
        if g.cur_player != ctrl {
            if loop_idx == ctrl {
                f.msg_text = __content.string(2083).into();
            } else if 0 < ctrl && ctrl < 6 {
                f.msg_text = PbStr::from("PLAYER") + &str_s(cast::<Ext, _>(ctrl));
            } else {
                if !(1..=1600).contains(&ctrl) {
                    return Flow::ExitSub; // EntSymbol(CorpController): error 9
                }
                f.msg_text = PbStr::from("        ") + &g.ent_symbol[ctrl];
                f.msg_text = right(f.msg_text.as_bytes(), 8);
            }
        }
        f.line_text = f.line_text.clone() + &f.msg_text + &string_fill(125, b" ");
        f.msg_text = PbStr::from("000") + &str_trim(cast::<Ext, _>(loop_idx));
        f.msg_text = right(f.msg_text.as_bytes(), 4);
        f.menu_lines[cast::<usize, _>(f.line_idx)] = f.line_text.clone() + "@C" + &f.msg_text;
    }
    f.loop_idx = 1601;
    f.menu_lines[cast::<usize, _>(f.line_idx + 1)] = f.menu_lines[4].clone();
    f.line_idx += 1;
    f.menu_lines[cast::<usize, _>(f.line_idx + 3)] = "@END".into();
    let line_count = f.seller_count + 13;
    f.line_idx = 1;
    while f.line_idx <= line_count {
        g.text_lines[f.line_idx] = f.menu_lines[cast::<usize, _>(f.line_idx)].clone();
        f.line_idx += 1;
    }
    g.entity_pick_mode = "GetCorp".into();
    g.text_box_title =
        PbStr::from(__content.string(2084)) + &g.active_entity_name + " ===";
    g.text_box_mode = "80".into();
    g.text_has_links = 1;
    text_report(g, ui, rng, f);
    if g.target_corp < 131 {
        g.tmp_symbol = PbStr::new();
    } else {
        g.tmp_symbol = g.ent_symbol[g.target_corp].clone();
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    Flow::Return
}
