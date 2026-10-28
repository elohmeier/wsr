//! The asset/stock sale offer list: `ShowAssetSaleOffers` (0x666a27) and `OfferAssetsForSale` (0x6699ec)
//! with their GOSUBs.
//!
//! Offer records (`AssetSaleOffers(1..100)`, count in element 0) are `"seller|stock|pct|S|qtr|year"` for
//! a stock holding or `"seller|CA|amount|S|qtr|year"` for business assets.
//!
//! Picking from a report: the report dialog follows a clicked `@C<id>` line through `FollowReportLink`
//! (0x46c9f4), which sets `TargetCorp`.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::{find_holding_slot, find_stock_holding};
use crate::logic::report::{number_to_words, show_text_report};
use crate::logic::reports_corp::check_cross_ownership;
use crate::logic::reports_lists::pb_array_sort;
use crate::logic::reports_lists::show_portfolio_listing;
use crate::logic::sim_entity::asset_sales::{
    add_remove_asset_sale_offer, execute_business_asset_purchase, gen_asset_sale_description,
    reduce_asset_sale_offer,
};
use crate::logic::sim_entity::quarter_start::{expire_asset_sale_offers, pack_holding_arrays};
use crate::logic::valuation::calc_analyst_rating;
use crate::pb::string::{compare, ucase, CaseMode};

/// `ShowTextReport` (0x41dfaa).
const SHOW_TEXT_REPORT: u32 = super::SHOW_TEXT_REPORT;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `PARSE$(text, ANY "|", n)`.
fn field(text: &PbStr, n: i32) -> PbStr {
    parse_any(text.as_bytes(), b"|", n)
}

/// `VAL(PARSE$(text, ANY "|", n))`.
fn fval(text: &PbStr, n: i32) -> Ext {
    val(field(text, n).as_bytes())
}

/// `RIGHT$("000" + RTRIM$(LTRIM$(STR$(n))), 4)`.
fn id4(n: i32) -> PbStr {
    right((s("000") + &str_trim(cast::<Ext, _>(n))).as_bytes(), 4)
}

// ------------------------------------------------------------------------------------ ShowAssetSaleOffers

/// The locals of ShowAssetSaleOffers shared with its GOSUBs.
struct SaoFrame {
    lines: Vec<PbStr>,
    sort_keys: Vec<PbStr>,
    line_no: i32,
    offer_idx: i32,
    offer_text: PbStr,
    expiry_str: PbStr,
}

impl SaoFrame {
    /// Port of ShowAssetSaleOffers_FmtExpiry (0x6691b0).
    /// `expiryStr = " " + month + " " + year + " "` from fields 5 (quarter: "JAN."/"APR."/"JUL."/"OCT.")
    /// and 6 of the current offer. Another quarter keeps the previous `expiryStr` as the "month".
    fn fmt_expiry(&mut self) {
        let expiry_qtr = to_long(fval(&self.offer_text, 5));
        match expiry_qtr {
            1 => self.expiry_str = s("JAN."),
            2 => self.expiry_str = s("APR."),
            3 => self.expiry_str = s("JUL."),
            4 => self.expiry_str = s("OCT."),
            _ => {}
        }
        self.expiry_str =
            s(" ") + &self.expiry_str + " " + &trim(field(&self.offer_text, 6).as_bytes()) + " ";
    }

    /// Port of ShowAssetSaleOffers_AppendId (0x669322).
    /// Appends 60 blanks and the link code `"@C" + 4-digit offer index` to the current line.
    fn append_id(&mut self) {
        let link_tag = s("@C") + &id4(self.offer_idx);
        let l = cast::<usize, _>(self.line_no);
        self.lines[l] = self.lines[l].clone() + &string_fill(60, b" ") + &link_tag;
    }
}

/// `ARRAY SORT keys(first) FOR count [, TAGARRAY tags()]` of a local `DIM (0 TO 200) AS STRING`
/// (ascending, binary comparison; count 0 sorts to the end of the array).
fn sort_strings(keys: &mut [PbStr], tags: Option<&mut [PbStr]>, first: usize, count: usize) {
    let n = if count == 0 { keys.len() - first } else { count };
    let less = |a: &PbStr, b: &PbStr| compare(a.as_bytes(), b.as_bytes()) == std::cmp::Ordering::Less;
    match tags {
        Some(t) => pb_array_sort(&mut keys[first..first + n], Some(&mut t[first..first + n]), false, &less),
        None => pb_array_sort::<PbStr, ()>(&mut keys[first..first + n], None, false, &less),
    }
}

/// Port of ShowAssetSaleOffers (0x666a27) with its GOSUBs ShowAssetSaleOffers_FmtExpiry (0x6691b0) and
/// ShowAssetSaleOffers_AppendId (0x669322); ShowAssetSaleOffers_Exit (0x6694a6) is the routine's exit.
///
/// After `ExpireAssetSaleOffers`, lists the pending offers (stocks offered at 95% of market value, sorted
/// by line text, with rating / "N/T"; business assets at 95% of cost, sorted by industry name) as a pick
/// list (`@C<offer index>`). Stock offers larger than what the seller still holds are reduced
/// (`ReduceAssetSaleOffer`), asset offers larger than the seller's (truncated) business assets too. For
/// the picked offer: an own offer (of `ActiveEntity`, or of a company the current player controls) may be
/// cancelled; a stock offer goes to `BuyStockMenu` (with `TmpExt1` = offer index) and
/// `CheckCrossOwnership`; a business-asset offer is bought by `ActiveEntity` (a corporation; a holding
/// company enters the industry and loses its tax carryovers, else only the same industry) for an entered
/// amount (`ExecuteBusinessAssetPurchase`), with confirmation and news. An invalid offer is withdrawn.
///
/// Quirk: a seller holding the offered stock short (`FindStockHolding` < 0) makes the listing read
/// `HoldStockPct(-slot, …)`: error 9, the ON ERROR handler (ShowRuntimeError, not ported) ends the
/// routine before the list is shown; the port returns there. `now`: the local time (for `BuyStockMenu`).
pub fn show_asset_sale_offers(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &crate::logic::newgame::LocalTime) {
    show_asset_sale_offers_skipping(g, ui, rng, now, &[]);
}

/// [`show_asset_sale_offers`] without calling the routines in `skip` (tests: `ShowTextReport` 0x41dfaa).
pub fn show_asset_sale_offers_skipping(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &crate::logic::newgame::LocalTime, skip: &[u32]) {
    let __content = crate::content::current();
    expire_asset_sale_offers(g, ui);
    if val(g.asset_sale_offers[0].as_bytes()) == Ext::from_f64(0.0) {
        let mut m = s(__content.string(2085));
        show_quick_message(g, ui, &mut m);
        return;
    }
    let mut fr = SaoFrame {
        lines: vec![PbStr::new(); 201],
        sort_keys: vec![PbStr::new(); 201],
        line_no: 0,
        offer_idx: 0,
        offer_text: PbStr::new(),
        expiry_str: PbStr::new(),
    };
    let mut has_asset_offers = 0;
    let mut has_stock_offers = 0;
    for offer_idx in 1..=100 {
        if g.asset_sale_offers[offer_idx].is_empty() {
            continue;
        }
        fr.offer_text = g.asset_sale_offers[offer_idx].clone();
        if field(&fr.offer_text, 2) == "CA" {
            has_asset_offers = 1;
        } else {
            has_stock_offers = 1;
        }
    }
    if has_stock_offers == 1 {
        fr.lines[2] = s(__content.string(2086));
        fr.lines[3] = s("         ------------------------------------------------------");
    }
    fr.lines[4] = s(__content.string(2087));
    fr.line_no = 5;
    if has_stock_offers != 0 {
        fr.lines[6] = s(__content.string(2088));
        fr.lines[7] = s(__content.string(2089));
        fr.lines[8] = s("------------------------- ---- ------- --------- ------------------------- ------");
        fr.line_no = 8;
        fr.offer_idx = 1;
        while fr.offer_idx <= 100 {
            let offer_idx = fr.offer_idx;
            'next: {
                if g.asset_sale_offers[offer_idx].is_empty() {
                    break 'next;
                }
                fr.offer_text = g.asset_sale_offers[offer_idx].clone();
                if field(&fr.offer_text, 2) == "CA" {
                    break 'next;
                }
                let corp_id = to_long(fval(&fr.offer_text, 2));
                if corp_id < 0 || corp_id > 1600 {
                    return; // EntityStatus(): error 9 -> ON ERROR handler
                }
                if g.entity_status[corp_id] != 1 {
                    break 'next;
                }
                let mut line_text = g.ent_name[corp_id].clone() + &string_fill(26, b" ");
                line_text = mid(line_text.as_bytes(), 1, 26);
                let sym_field = g.ent_symbol[corp_id].clone() + "    ";
                line_text = line_text + &mid(sym_field.as_bytes(), 1, 5);
                let mut pct_offered = to_long(fval(&fr.offer_text, 3));
                let seller_id = to_long(fval(&fr.offer_text, 1));
                if seller_id < 0 || seller_id > 1600 {
                    return;
                }
                if g.entity_status[seller_id] != 1 {
                    break 'next;
                }
                let hold_slot = find_stock_holding(g, seller_id, corp_id);
                if hold_slot >= 1
                    && g.hold_stock_pct[(hold_slot, seller_id)] >= 1
                    && pct_offered > cast::<i32, _>(g.hold_stock_pct[(hold_slot, seller_id)])
                {
                    let pct_excess = pct_offered - cast::<i32, _>(g.hold_stock_pct[(hold_slot, seller_id)]);
                    reduce_asset_sale_offer(g, ui, offer_idx, pct_excess, 0);
                    fr.offer_text = g.asset_sale_offers[offer_idx].clone();
                    pct_offered = cast::<i32, _>(g.hold_stock_pct[(hold_slot, seller_id)]);
                } else {
                    if hold_slot == 0 {
                        break 'next;
                    }
                    if hold_slot < 0 {
                        return; // HoldStockPct(-slot, …): error 9 -> ON ERROR handler
                    }
                    if g.hold_stock_pct[(hold_slot, seller_id)] <= 0 {
                        break 'next;
                    }
                }
                let mut fld = s("   ") + &str_trim(cast::<Ext, _>(pct_offered)) + "%";
                fld = right(fld.as_bytes(), 5) + "  ";
                line_text = line_text + &fld;
                fr.fmt_expiry();
                line_text = line_text + &fr.expiry_str;
                let industry_id = g.industry_num[corp_id];
                fld = g.ind_name[cast::<i32, _>(industry_id)].clone() + &string_fill(25, b" ");
                fld = mid(fld.as_bytes(), 1, 26);
                line_text = line_text + &fld;
                let rating = calc_analyst_rating(g, ui, rng, corp_id);
                update_corp_control(g, ui, rng, corp_id);
                if g.public_pct == 0 {
                    line_text = line_text + "  N/T";
                } else {
                    line_text = line_text + "  " + &str_s(cast::<Ext, _>(rating));
                }
                fr.line_no += 1;
                fr.lines[cast::<usize, _>(fr.line_no)] = line_text;
                fr.sort_keys[cast::<usize, _>(fr.line_no)] = g.ind_name[cast::<i32, _>(industry_id)].clone();
                fr.append_id();
            }
            fr.offer_idx += 1;
        }
        // ARRAY SORT lines(9) FOR lineNo - 8
        sort_strings(&mut fr.lines, None, 9, cast::<usize, _>(fr.line_no - 8));
        fr.line_no += 1;
        fr.lines[cast::<usize, _>(fr.line_no)] =
            s("------------------------- ---- ------- --------- ------------------------- ------");
        fr.line_no += 2;
        fr.lines[cast::<usize, _>(fr.line_no)] =
            s(__content.string(2090));
        fr.line_no += 1;
    }
    if has_asset_offers != 0 {
        let l = cast::<usize, _>(fr.line_no);
        fr.lines[l + 1] = s(__content.string(2091));
        fr.lines[l + 2] = s("         -------------------------------------------------------");
        fr.line_no += 3;
        let mut currency_pad = s("   ");
        if len(trim(g.currency_symbol.as_bytes()).as_bytes()) < 3 {
            currency_pad = g.currency_symbol.clone() + &currency_pad;
            currency_pad = mid(currency_pad.as_bytes(), 1, 3);
            if right(currency_pad.as_bytes(), 2) == "  " {
                currency_pad = s(" ") + &mid(currency_pad.as_bytes(), 1, 2);
            }
        }
        let l = cast::<usize, _>(fr.line_no);
        fr.lines[l + 1] = s("                               ") + &currency_pad + &g.million_prefix_uc + "ILLIONS";
        fr.lines[l + 2] = s(__content.string(2092));
        fr.lines[l + 3] = s(__content.string(2093));
        fr.lines[l + 4] = s("------------------------- ---- ------------- ----------- -------------------------");
        let assets_start_line = fr.line_no + 5;
        fr.line_no += 4;
        fr.offer_idx = 1;
        while fr.offer_idx <= 100 {
            let offer_idx = fr.offer_idx;
            'next: {
                if g.asset_sale_offers[offer_idx].is_empty() {
                    break 'next;
                }
                fr.offer_text = g.asset_sale_offers[offer_idx].clone();
                if !(fval(&fr.offer_text, 2) <= Ext::from_f64(0.0)) {
                    break 'next;
                }
                let corp_id = to_long(fval(&fr.offer_text, 1));
                if corp_id < 0 || corp_id > 1600 {
                    return; // EntityStatus(): error 9 -> ON ERROR handler
                }
                if g.entity_status[corp_id] != 1 || corp_id <= 130 || g.industry_num[corp_id] >= 70 {
                    break 'next;
                }
                let mut line_text = g.ent_name[corp_id].clone() + &string_fill(26, b" ");
                line_text = mid(line_text.as_bytes(), 1, 26);
                let sym_field = g.ent_symbol[corp_id].clone() + "    ";
                line_text = line_text + &mid(sym_field.as_bytes(), 1, 5);
                if g.business_assets[corp_id] == Ext::from_f64(0.0) {
                    break 'next;
                }
                let mut offer_amt = to_quad(fval(&fr.offer_text, 3));
                if fix(g.business_assets[corp_id]) < cast::<Ext, _>(offer_amt) {
                    let excess_amt = cast::<Ext, _>(offer_amt) - fix(g.business_assets[corp_id]);
                    if excess_amt > Ext::from_f64(0.5) {
                        let reduce_amt = to_quad(excess_amt);
                        reduce_asset_sale_offer(g, ui, offer_idx, 0, reduce_amt);
                        offer_amt = to_quad(fix(g.business_assets[corp_id]));
                    }
                }
                let mut fld = string_fill(13, b" ") + &fmt_trim(cast::<Ext, _>(offer_amt), b"#############");
                fld = right(fld.as_bytes(), 13);
                line_text = line_text + &fld;
                fr.fmt_expiry();
                line_text = line_text + &fr.expiry_str;
                let industry_id = g.industry_num[corp_id];
                line_text = line_text + "  " + &g.ind_name[cast::<i32, _>(industry_id)];
                fr.line_no += 1;
                fr.lines[cast::<usize, _>(fr.line_no)] = line_text;
                fr.sort_keys[cast::<usize, _>(fr.line_no)] = g.ind_name[cast::<i32, _>(industry_id)].clone();
                fr.append_id();
            }
            fr.offer_idx += 1;
        }
        // ARRAY SORT sortKeys(assetsStartLine) FOR lineNo - (assetsStartLine - 1), TAGARRAY lines()
        let count = cast::<usize, _>(fr.line_no - (assets_start_line - 1));
        sort_strings(&mut fr.sort_keys, Some(&mut fr.lines), cast::<usize, _>(assets_start_line), count);
        fr.line_no += 1;
        fr.lines[cast::<usize, _>(fr.line_no)] =
            s("------------------------- ---- ------------- ----------- -------------------------");
    }
    let line_count = fr.line_no;
    for i in 1..=line_count {
        g.text_lines[i] = fr.lines[cast::<usize, _>(i)].clone();
    }
    g.text_lines[fr.line_no + 2] = string_fill(18, b" ") + __content.string(2094);
    g.text_lines[fr.line_no + 5] = s("@END");
    g.text_box_title = s("     ") + __content.string(2095);
    g.entity_pick_mode = s("GetCorp");
    g.list_item_noun = s("offer");
    g.text_box_mode = s("74");
    g.target_corp = 0;
    g.text_has_links = 1;
    if !skip.contains(&SHOW_TEXT_REPORT) {
        show_text_report(g, ui, rng);
    }
    if g.game_end_flag == "Y" {
        return;
    }
    if g.target_corp < 1 || g.target_corp > 100 {
        return;
    }
    let sel_offer = g.target_corp;
    g.target_corp = 0;
    let offer_text = g.asset_sale_offers[sel_offer].clone();
    if offer_text.is_empty() {
        return;
    }
    let seller_id = to_long(fval(&offer_text, 1));
    if g.active_entity == seller_id {
        let mut who = s("You");
        if g.active_entity > 10 {
            who = g.active_entity_name.clone();
        }
        let mut what = s(__content.string(2096));
        if field(&offer_text, 2) == "CA" {
            what = s(__content.string(2097));
        }
        let mut whose = s("your");
        if g.active_entity > 10 {
            whose = s("it");
        }
        let line_text = who + __content.string(2098) + &what + __content.string(2099) + &whose + "self."
            + "\r" + "\r" + __content.string(2100);
        let answer = ui.msgbox(&line_text, 0x2124, &s(__content.string(2101)));
        if answer == 6 {
            add_remove_asset_sale_offer(g, &PbStr::new(), sel_offer);
        }
        return;
    }
    if seller_id < 0 || seller_id > 1600 {
        return; // CorpController(): error 9 -> ON ERROR handler
    }
    if g.cur_player == g.corp_controller[seller_id] {
        let mut what = s(__content.string(2096));
        if field(&offer_text, 2) == "CA" {
            what = s(__content.string(2097));
        }
        let line_text = s(__content.string(2102)) + __content.string(2098) + &what + __content.string(2103) + "\r" + "\r"
            + __content.string(2100);
        let answer = ui.msgbox(&line_text, 0x2124, &s(__content.string(2104)));
        if answer == 6 {
            add_remove_asset_sale_offer(g, &PbStr::new(), sel_offer);
            return;
        }
    }
    if fval(&offer_text, 2) > Ext::from_f64(0.0) {
        g.tmp_ext1 = cast::<Ext, _>(sel_offer);
        if !skip.contains(&0x58d471) {
            crate::logic::trading::buy_menu::buy_stock_menu(g, ui, rng, now);
        }
        let stock_corp_id = to_long(fval(&offer_text, 2));
        check_cross_ownership(g, ui, rng, stock_corp_id, 1);
        return;
    }
    if seller_id > 130 && g.industry_num[seller_id] < 70 {
        let active = g.active_entity;
        if active < 131 {
            let mut line_text = s(__content.string(2105))
                + "\r" + "\r"
                + __content.string(2106)
                + __content.string(2107)
                + __content.string(2108) + "\r" + "\r" + __content.string(2109);
            if active > 10 {
                line_text = remove(line_text.as_bytes(), b" in W$R");
            }
            if active > 10 && active < 86 {
                line_text = replace(line_text.as_bytes(), b"Player", b"Bank");
            }
            if active > 85 {
                line_text = replace(line_text.as_bytes(), b"Player", b"Insurer");
            }
            if active < 11 {
                line_text = line_text + __content.string(2110) + &g.ind_name[cast::<i32, _>(g.industry_num[seller_id])]
                    + __content.string(2111)
                    + __content.string(2112);
            } else {
                line_text = line_text + &g.active_entity_name
                    + __content.string(2113)
                    + __content.string(2114);
                if active > 85 {
                    line_text = line_text + __content.string(2115) + &g.active_entity_name
                        + __content.string(2116) + __content.string(2117);
                }
                line_text = line_text + ". ";
            }
            line_text = line_text
                + __content.string(2118);
            ui.msgbox(&line_text, 0x2010, &s(__content.string(2119)));
            return;
        }
        if g.industry_num[active] != 70 && g.industry_num[active] != g.industry_num[seller_id] {
            let industry_id = g.industry_num[active];
            let mut line_text = g.ent_symbol[active].clone() + __content.string(2120)
                + &g.ind_name[cast::<i32, _>(industry_id)] + __content.string(2121) + __content.string(2122)
                + &g.ind_name[cast::<i32, _>(g.industry_num[seller_id])] + __content.string(1975)
                + __content.string(2123);
            show_quick_message(g, ui, &mut line_text);
            return;
        }
        let mut tax_loss_flags = 0;
        if g.industry_num[active] == 70 {
            let mut nol_text = PbStr::new();
            let mut credit_text = PbStr::new();
            let mut lose_text = PbStr::new();
            if g.tax_loss_carry[active] < -Ext::from_f64(25.0) {
                nol_text = s(__content.string(2124));
                tax_loss_flags = 1;
            }
            if g.tax_credits[active] > Ext::from_f64(10.0) {
                credit_text = s(__content.string(2125));
                tax_loss_flags += 2;
            }
            if tax_loss_flags > 0 {
                if tax_loss_flags == 1 {
                    lose_text = nol_text.clone();
                }
                if tax_loss_flags == 2 {
                    lose_text = credit_text.clone();
                }
                if tax_loss_flags == 3 {
                    lose_text = nol_text + " and " + &credit_text;
                }
                let line_text = g.active_entity_name.clone() + __content.string(2126)
                    + __content.string(2127) + &g.ind_name[cast::<i32, _>(g.industry_num[seller_id])]
                    + __content.string(2128) + &lose_text + "." + "\r" + "\r"
                    + __content.string(2129);
                let answer = ui.msgbox(&line_text, 0x2023, &s(__content.string(2130)));
                if answer != 6 {
                    return;
                }
            }
        }
        calc_entity_financials(g, ui, rng, active);
        let mut max_affordable = to_quad(fix(g.liquid_funds * sf(1.045f32)));
        let offer_text = g.asset_sale_offers[sel_offer].clone();
        let offer_amt = to_quad(fval(&offer_text, 3));
        if (cast::<Ext, _>(offer_amt)) < cast::<Ext, _>(max_affordable) {
            max_affordable = offer_amt;
        }
        let mut amount_to_buy;
        loop {
            let mut line_text = g.ent_name[seller_id].clone() + __content.string(2131)
                + &trim(g.currency_symbol.as_bytes()) + &fmt_trim(cast::<Ext, _>(offer_amt), b"* ###,") + " "
                + &g.million_prefix_lc + "illion " + __content.string(2132)
                + &g.active_entity_name + __content.string(2133);
            let mut default_amt = offer_amt;
            if (cast::<Ext, _>(max_affordable)) < cast::<Ext, _>(offer_amt) {
                line_text = line_text + "(" + &g.ent_symbol[g.active_entity] + __content.string(2134)
                    + &str_s(cast::<Ext, _>(max_affordable)) + " " + &g.million_prefix_uc + "il.)";
                default_amt = max_affordable;
            }
            let title = g.ent_symbol[g.active_entity].clone() + __content.string(2135);
            let input_text = ui.inputbox(&line_text, &title, &str_s(cast::<Ext, _>(default_amt)));
            amount_to_buy = to_quad(val_removing(&input_text, b"$ ,%"));
            if cast::<Ext, _>(amount_to_buy) <= Ext::from_f64(0.0) {
                return;
            }
            if (cast::<Ext, _>(max_affordable)) < cast::<Ext, _>(amount_to_buy) && fix(g.liquid_funds) < cast::<Ext, _>(amount_to_buy) {
                let mut m = g.active_entity_name.clone() + __content.string(2136)
                    + &trim(g.currency_symbol.as_bytes()) + &fmt_trim(cast::<Ext, _>(max_affordable), b"* ###,") + " "
                    + &g.million_prefix_lc + "illion.";
                show_quick_message(g, ui, &mut m);
                amount_to_buy = to_quad(cast::<Ext, _>(offer_amt) + Ext::from_f64(1.0));
            }
            if !((cast::<Ext, _>(offer_amt)) < cast::<Ext, _>(amount_to_buy)) {
                break;
            }
        }
        execute_business_asset_purchase(g, ui, g.active_entity, sel_offer, amount_to_buy);
        let mut line_text = s(__content.string(2137)) + "\r" + "\r" + __content.string(2138)
            + &g.ent_name[seller_id] + "\r" + __content.string(2139) + &g.active_entity_name + "\r" + "\r"
            + __content.string(2140) + __content.string(2141)
            + &trim(g.currency_symbol.as_bytes()) + &fmt_trim(cast::<Ext, _>(amount_to_buy), b"* ###,") + " "
            + &g.million_prefix_lc + "illion" + &g.currency_suffix;
        if g.cur_player != g.corp_controller[seller_id] {
            line_text = line_text + __content.string(2142);
        }
        line_text = line_text + ".";
        ui.msgbox(&line_text, 0x2040, &s(__content.string(2013)));
        g.news_msg = g.ent_symbol[g.active_entity].clone() + __content.string(2143) + &g.ent_symbol[seller_id];
        let mut news_entity = 0;
        if tax_loss_flags > 0 {
            g.news_msg = g.news_msg.clone() + __content.string(2144);
            news_entity = g.active_entity;
        }
        post_news(g, ui, news_entity, 0);
        g.turn_action_count += 1;
        return;
    }
    let mut m = s(__content.string(2145));
    show_quick_message(g, ui, &mut m);
    add_remove_asset_sale_offer(g, &PbStr::new(), sel_offer);
}

// ------------------------------------------------------------------------------------- OfferAssetsForSale

/// The locals of OfferAssetsForSale shared with its GOSUBs.
struct OafFrame {
    offer_type: i32,
    corp_id: i32,
    exp_qtr: i32,
    exp_year: i32,
    phrase_text: PbStr,
    msg_text: PbStr,
    expiry_date: PbStr,
    answer: i32,
}

impl OafFrame {
    /// Port of OfferAssetsForSale_FmtDate (0x66ae50).
    /// `expiryDate = "MM/01/" + year` for the expiry quarter (another quarter keeps the old text).
    fn fmt_date(&mut self) {
        match self.exp_qtr {
            1 => self.expiry_date = s("01/01"),
            2 => self.expiry_date = s("04/01"),
            3 => self.expiry_date = s("07/01"),
            4 => self.expiry_date = s("10/01"),
            _ => {}
        }
        self.expiry_date = self.expiry_date.clone() + "/" + &str_trim(cast::<Ext, _>(self.exp_year));
    }

    /// Port of OfferAssetsForSale_CheckDup (0x66af48).
    /// Looks for a pending offer of `ActiveEntity` of the same kind (the same stock, or business assets)
    /// and asks whether to cancel it: YES removes it and returns `true` (continue); NO jumps to
    /// OfferAssetsForSale_Exit (0x66b1f4), i.e. ends OfferAssetsForSale (`false`).
    fn check_dup(&mut self, g: &mut Globals, ui: &mut dyn Ui) -> bool {
    let __content = crate::content::current();
        for offer_idx in 1..=100 {
            if g.asset_sale_offers[offer_idx].is_empty() {
                continue;
            }
            let existing_offer = g.asset_sale_offers[offer_idx].clone();
            let offer_seller_id = to_long(fval(&existing_offer, 1));
            if g.active_entity != offer_seller_id {
                continue;
            }
            if self.offer_type == 1 {
                if field(&existing_offer, 2) == "CA" || fval(&existing_offer, 2) != cast::<Ext, _>(self.corp_id) {
                    continue;
                }
                self.phrase_text = s(__content.string(2146)) + &g.ent_symbol[self.corp_id];
            } else {
                if !(field(&existing_offer, 2) == "CA") {
                    continue;
                }
                self.phrase_text = s(__content.string(2147));
            }
            self.msg_text = g.active_entity_name.clone() + __content.string(2148) + &self.phrase_text
                + __content.string(2149) + __content.string(2150) + "\r" + "\r"
                + __content.string(2151);
            self.answer = ui.msgbox(&self.msg_text, 0x2024, &s(__content.string(2152)));
            if self.answer == 6 {
                add_remove_asset_sale_offer(g, &PbStr::new(), offer_idx);
                return true;
            }
            return false; // OfferAssetsForSale_Exit (0x66b1f4)
        }
        true
    }
}

/// Port of OfferAssetsForSale (0x6699ec) with its GOSUBs OfferAssetsForSale_FmtDate (0x66ae50) and
/// OfferAssetsForSale_CheckDup (0x66af48); OfferAssetsForSale_Exit (0x66b1f4) is the routine's exit.
///
/// `ActiveEntity` lists an offer for private sale at a 5% discount (at most 100 live offers;
/// `PackHoldingArrays` first): `offer_type = 1` a percentage of one of its stock holdings (the only one,
/// or picked from its portfolio listing), `2` an amount [runtime resource wording] at least 10). A pending
/// offer of the same kind may be cancelled first. The offer expires at the end of this or (YES) the next
/// quarter; after the terms are confirmed the record `"seller|stock|pct|S|qtr|year"` /
/// `"seller|CA|amount|S|qtr|year"` is added (`AddRemoveAssetSaleOffer`) and the news posted.
///
/// Quirks (BUGS.md 295..297): the stock prompt says "<name> owns" for players too (`0 < sellerId`
/// instead of `10 < sellerId`); with exactly one long holding, slot 1 is taken even when it holds a
/// short position (the long one being in another slot); a negative asset amount is accepted.
pub fn offer_assets_for_sale(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, offer_type: i32) {
    let __content = crate::content::current();
    let seller_id = g.active_entity;
    pack_holding_arrays(g, ui);
    if val(g.asset_sale_offers[0].as_bytes()) > Ext::from_f64(99.0) {
        let mut m = s(__content.string(2153))
            + __content.string(2154) + "\r\n" + "\r\n"
            + __content.string(2155);
        show_quick_message(g, ui, &mut m);
        return;
    }
    let mut fr = OafFrame {
        offer_type,
        corp_id: 0,
        exp_qtr: 0,
        exp_year: 0,
        phrase_text: PbStr::new(),
        msg_text: PbStr::new(),
        expiry_date: PbStr::new(),
        answer: 0,
    };
    let mut stock_count = 0;
    for slot in 1..=15 {
        if g.hold_stock_id[(slot, seller_id)] != 0 && g.hold_stock_pct[(slot, seller_id)] > 0 {
            stock_count += 1;
        }
    }
    let mut offer_text = PbStr::new();
    let mut pct_to_offer = 0;
    let mut assets_owned: i64 = 0;
    let mut assets_to_offer: i64 = 0;
    if offer_type == 1 {
        if stock_count == 0 {
            let mut phrase = s("You do");
            if seller_id > 10 {
                phrase = g.active_entity_name.clone() + " does";
            }
            let msg = phrase + __content.string(2156);
            ui.msgbox(&msg, 0x2010, &s(__content.string(2157)));
            return;
        }
        let pct;
        if stock_count == 1 {
            fr.corp_id = cast::<i32, _>(g.hold_stock_id[(1, seller_id)]);
            pct = g.hold_stock_pct[(1, seller_id)];
        } else {
            let mut phrase = s("You own");
            if seller_id > 10 {
                phrase = g.active_entity_name.clone() + " owns";
            }
            let msg = phrase + &number_to_words(stock_count, 0, 0) + __content.string(2158) + "\r" + "\r"
                + __content.string(2159)
                + __content.string(2160) + "\r" + "\r"
                + __content.string(2161);
            fr.answer = ui.msgbox(&msg, 0x2023, &s(__content.string(2162)));
            if fr.answer != 6 {
                return;
            }
            g.tmp_symbol = PbStr::new();
            g.entity_pick_mode = s("GetCorp");
            g.target_corp = 0;
            g.tmp_ext1 = -Ext::from_f64(1.0);
            show_portfolio_listing(g, ui, rng, seller_id, 0, 0);
            if g.target_corp == 0 {
                g.action_code = PbStr::new();
                return;
            }
            fr.corp_id = g.target_corp;
            g.target_corp = 0;
            let hold_slot = find_holding_slot(g, seller_id, fr.corp_id);
            pct = g.hold_stock_pct[(hold_slot, seller_id)];
        }
        let pct_owned = cast::<i32, _>(pct);
        if !fr.check_dup(g, ui) {
            return;
        }
        let mut phrase = s("You own");
        if seller_id > 0 {
            phrase = g.active_entity_name.clone() + " owns";
        }
        fr.msg_text = phrase + &str_s(cast::<Ext, _>(pct_owned)) + __content.string(2163) + &g.ent_name[fr.corp_id]
            + __content.string(2164) + "of " + &g.ent_symbol[fr.corp_id]
            + __content.string(2165);
        let mut input_ok = 0;
        while input_ok == 0 {
            let title = g.ent_symbol[g.active_entity].clone() + __content.string(2166);
            let input_text = ui.inputbox(&fr.msg_text, &title, &(str_trim(cast::<Ext, _>(pct_owned)) + "%"));
            let v = val_removing(&input_text, b"$ ,%");
            if v <= cast::<Ext, _>(pct_owned) {
                if v < Ext::from_f64(1.0) {
                    return;
                }
                input_ok = 1;
                pct_to_offer = to_long(v);
            } else {
                fr.msg_text = s(__content.string(2167)) + &g.active_entity_name + __content.string(2168) + &str_s(cast::<Ext, _>(pct_owned))
                    + "% OF " + &g.ent_symbol[fr.corp_id] + "!";
                show_quick_message(g, ui, &mut fr.msg_text);
            }
        }
        offer_text = str_trim(cast::<Ext, _>(seller_id)) + "|" + &str_trim(cast::<Ext, _>(fr.corp_id)) + "|"
            + &str_trim(cast::<Ext, _>(pct_to_offer)) + "|" + "S" + "|";
    } else if offer_type == 2 {
        if !fr.check_dup(g, ui) {
            return;
        }
        assets_owned = to_quad(fix(g.business_assets[g.active_entity]));
        if (cast::<Ext, _>(assets_owned)) < Ext::from_f64(10.0) {
            let mut m = s(__content.string(2169)) + &trim(g.currency_symbol.as_bytes())
                + "10 " + &g.million_prefix_lc + "illion." + "\r\n" + "\r\n"
                + __content.string(2170);
            show_quick_message(g, ui, &mut m);
            return;
        }
        loop {
            fr.msg_text = g.active_entity_name.clone() + " owns " + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(cast::<Ext, _>(assets_owned), b"* ######,") + " " + &g.million_prefix_lc + "illion"
                + &g.currency_suffix
                + __content.string(2171)
                + __content.string(2172) + &g.ent_symbol[seller_id] + __content.string(2173);
            let title = g.ent_symbol[g.active_entity].clone() + __content.string(2174);
            let input_text = ui.inputbox(&fr.msg_text, &title, &PbStr::new());
            assets_to_offer = to_quad(val_removing(&input_text, b"$ ,%"));
            if cast::<Ext, _>(assets_to_offer) == Ext::from_f64(0.0) {
                return;
            }
            if !((cast::<Ext, _>(assets_owned)) < cast::<Ext, _>(assets_to_offer)) {
                break;
            }
        }
        offer_text = str_trim(cast::<Ext, _>(seller_id)) + "|CA|" + &str_trim(cast::<Ext, _>(assets_to_offer)) + "|" + "S" + "|";
    }
    fr.phrase_text = str_s(cast::<Ext, _>(pct_to_offer)) + __content.string(2175);
    if offer_type == 2 {
        fr.phrase_text = s(__content.string(2176)) + " of " + &g.ent_symbol[seller_id];
    }
    fr.msg_text = s(__content.string(2177)) + &fr.phrase_text + __content.string(2178)
        + __content.string(2179)
        + __content.string(2180) + "\r" + "\r"
        + __content.string(2181) + __content.string(2182);
    fr.answer = ui.msgbox(&fr.msg_text, 0x2123, &s(__content.string(2183)));
    if fr.answer == 2 {
        return;
    }
    fr.exp_qtr = g.cur_quarter + 1;
    fr.exp_year = g.cur_year;
    if fr.exp_qtr > 4 {
        fr.exp_qtr = 1;
        fr.exp_year = g.cur_year + 1;
    }
    if fr.answer == 6 {
        fr.exp_qtr += 1;
        if fr.exp_qtr > 4 {
            fr.exp_qtr = 1;
            fr.exp_year = g.cur_year + 1;
        }
    }
    fr.phrase_text = s("STOCK");
    if offer_type == 2 {
        fr.phrase_text = s(__content.string(2184)) + "\r";
    }
    fr.msg_text = s(__content.string(2185)) + &fr.phrase_text + " BY "
        + &ucase(g.active_entity_name.as_bytes(), CaseMode::Ansi) + ":" + "\r" + "\r";
    if offer_type == 1 {
        fr.phrase_text = s(__content.string(2186)) + &str_s(cast::<Ext, _>(pct_to_offer)) + __content.string(2163)
            + &g.ent_name[fr.corp_id] + ", " + __content.string(2187)
            + __content.string(2188) + "\r" + "\r";
    } else {
        fr.phrase_text = s(__content.string(2189)) + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cast::<Ext, _>(assets_to_offer), b"* ###,") + " " + &g.million_prefix_lc
            + __content.string(2190) + __content.string(2191) + &g.ent_symbol[seller_id]
            + __content.string(2192) + __content.string(2193) + "\r" + "\r";
    }
    fr.phrase_text = fr.phrase_text.clone() + __content.string(2194)
        + __content.string(2195) + __content.string(2196) + "\r" + "\r";
    if seller_id < 11 || offer_type == 2 {
        fr.phrase_text = replace(fr.phrase_text.as_bytes(), __content.bytes(2197), __content.bytes(2198));
    }
    if offer_type == 1 {
        fr.phrase_text = fr.phrase_text.clone() + __content.string(2199) + &g.ent_symbol[fr.corp_id]
            + __content.string(2200) + __content.string(2201)
            + __content.string(2202) + &g.ent_name[fr.corp_id] + "." + "\r" + "\r";
    }
    fr.fmt_date();
    fr.msg_text = fr.msg_text.clone() + &fr.phrase_text + __content.string(2203)
        + __content.string(2204) + "\r" + "\r" + __content.string(2205)
        + &fr.expiry_date + __content.string(2206) + "\r" + "\r" + __content.string(2207);
    fr.answer = ui.msgbox(&fr.msg_text, 0x2023, &s(__content.string(2208)));
    if fr.answer != 6 {
        return;
    }
    offer_text = offer_text + &str_trim(cast::<Ext, _>(fr.exp_qtr)) + "|" + &str_trim(cast::<Ext, _>(fr.exp_year));
    add_remove_asset_sale_offer(g, &offer_text, 0);
    fr.msg_text = s(__content.string(2209)) + "\r" + "\r"
        + __content.string(2210)
        + __content.string(2211)
        + __content.string(2212) + "posted." + "\r" + "\r"
        + __content.string(2213);
    ui.msgbox(&fr.msg_text, 0x2040, &s(__content.string(2214)));
    g.news_msg = g.ent_symbol[seller_id].clone() + __content.string(2215);
    fr.phrase_text = s(" a") + &str_s(cast::<Ext, _>(pct_to_offer)) + __content.string(2216) + &g.ent_symbol[fr.corp_id];
    if offer_type == 2 {
        fr.phrase_text = s(__content.string(2217));
        if (cast::<Ext, _>(assets_to_offer)) < g.exchange_rate * Ext::from_f64(15.0) || (cast::<Ext, _>(assets_to_offer)) < cast::<Ext, _>(assets_owned) * sf(0.05f32) {
            fr.phrase_text = replace(fr.phrase_text.as_bytes(), b"all", b"some");
        } else if (cast::<Ext, _>(assets_to_offer)) < cast::<Ext, _>(assets_owned) {
            gen_asset_sale_description(g, rng, g.active_entity);
            fr.phrase_text = s(" its ") + &g.asset_sale_desc;
        }
    }
    g.news_msg = g.news_msg.clone() + &fr.phrase_text;
    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
        g.news_msg = replace(g.news_msg.as_bytes(), b"puts up", b"offers");
    }
    post_news(g, ui, 0, 0);
}
