//! Start-up companies: `StartupCompany` (0x5af020).
//!
//! The two pick lists of the routine (industry, country of incorporation) are `ShowTextReport` reports
//! in `EntityPickMode = "GetInd"` whose lines carry `@I<n>` links; a clicked link sets `SelIndustry = n`
//! (FollowReportLink), applied through [`show_text_report`].
//!
//! Error handling: the ON ERROR handler (0x5b0e9c) calls ShowRuntimeError (0x46ea48) and leaves the
//! routine. The one reachable error is error 9 on `CountryIdx(newCorpId)` in the final "Startup Results"
//! message when no company was created (`newCorpId` 0, BUGS.md #290); the port shows the handler's
//! popup ([`runtime_error9`]) and leaves at the same point.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::SHOW_TEXT_REPORT;
use crate::logic::earnings::{balance_cash_t_bills, calc_projected_earnings};
use crate::logic::finance::calc_entity_financials;
use crate::logic::market::{select_target_corp, update_market_display};
use crate::logic::newgame_helpers::{create_bank, create_insurer, find_free_entity_slot};
use crate::logic::report::show_text_report;
use crate::logic::reports_lists::pb_array_sort;
use crate::logic::sim_entity::asset_sales::start_new_corp_or_expand;
use crate::logic::valuation::{adjust_index_divisor, calc_stock_price, check_stock_split};
use crate::pb::string::{compare, mcase, ucase, CaseMode};

/// `ShowTextReport` of a "GetInd" pick list with the pick-list part of FollowReportLink for a clicked
/// `@I<n>` line (`SelIndustry = n`, at most 71) — [`show_text_report`]. Skipped entirely when `skip`
/// lists [`SHOW_TEXT_REPORT`].
fn pick_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, skip: &[u32]) {
    if skip.contains(&SHOW_TEXT_REPORT) {
        return;
    }
    show_text_report(g, ui, rng);
}

/// The ON ERROR handler (0x5b0e9c) for error 9: `ShowRuntimeError(ERR, "FrontStartup", "  INDUSTRY # " +
/// STR$(i), 0)` (0x46ea48), which shows its message with ShowQuickMessage. `i` is the routine's shared
/// loop variable.
fn runtime_error9(g: &mut Globals, ui: &mut dyn Ui, i: i32) {
    let __content = crate::content::current();
    let diag = PbStr::from(__content.string(2261)) + &str_s(cast::<Ext, _>(i));
    crate::logic::restructure::show_runtime_error(g, ui, 9, &PbStr::from(__content.string(2262)), &diag, 0);
}

/// Port of StartupCompany (0x5af020).
/// The player (or `ActiveEntity`, a company the player controls) founds a new 100%-owned company:
/// at most one start-up per player and year (`PlayerStartupCount < 2`), liquid funds of at least 100
/// (x `ExchangeRate`), a free stock-holding slot (not for banks/insurers). After a YES/NO confirmation
/// the player picks the industry (report, `SelIndustry`), enters the capital (INPUTBOX; at least 999,
/// or 99 for industries > 2, x `ExchangeRate`; a cash shortfall is borrowed via BorrowToCoverPurchase).
/// Banks (61..85) / insurers (121..130) are created in a free slot ([`create_bank`] / [`create_insurer`]),
/// a holding company (70) in a dormant slot 1471..1580 (else a random one 131..1580), an operating
/// company with [`start_new_corp_or_expand`] (mode 1). The new company is owned by the acting entity
/// (holding slot, basis = capital), controlled by `CurPlayer`, with a start-up cost of 10% (max 200,
/// 100 for corps, 25 for shells, x `ExchangeRate`); the capital is paid from cash (bank debt for any
/// remaining deficit). Optional rename (ChangeCompanyName) and country of incorporation (report),
/// news, "Startup Results" message.
pub fn startup_company(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    startup_company_skipping(g, ui, rng, time_secs, &[]);
}

/// [`startup_company`] that does not call the routines in `skip` (only [`SHOW_TEXT_REPORT`] is honoured:
/// the two pick-list reports are left out, as when the differential tests skip them in the original).
pub fn startup_company_skipping(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, skip: &[u32]) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x5b0e9c (ShowRuntimeError, exit)
    let mut pick_list: Vec<PbStr> = vec![PbStr::new(); 91]; // DIM pickList(0 TO 90)
    let mut slot: i16 = 0;
    let mut msg_text: PbStr;
    let mut new_corp_id: i32 = 0;
    let mut found: i32 = 0;
    let mut cost_rate: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
    let mut startup_cost: i64 = 0;
    let s = PbStr::from;

    if g.player_startup_count[g.cur_player] >= 2 {
        msg_text = s(__content.string(2263))
            + __content.string(2264);
        show_quick_message(g, ui, &mut msg_text);
        return;
    }
    let parent_symbol = g.ent_symbol[g.active_entity].clone();
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    let avail_funds = g.liquid_funds;
    if !(g.exchange_rate * Ext::from_f64(100.0) <= g.liquid_funds) {
        msg_text = s("You do");
        if 10 < g.active_entity {
            msg_text = g.active_entity_name.clone() + " does";
        }
        let mut m = msg_text + __content.string(2265);
        show_quick_message(g, ui, &mut m);
        return;
    }
    let mut i: i32 = 1; // local_dc, shared by the loops below (shown by the error handler)
    loop {
        if g.hold_stock_id[(i, g.active_entity)] == 0 {
            slot = cast::<i16, _>(i);
            i = 15;
        }
        i += 1;
        if i >= 16 {
            break;
        }
    }
    msg_text = PbStr::new();
    if 10 < g.active_entity && g.active_entity < 131 {
        slot = -1;
        msg_text = s(__content.string(2266));
        if 85 < g.active_entity {
            msg_text = replace(msg_text.as_bytes(), b"Banks", __content.bytes(2267));
        }
    }
    if slot < 1 {
        if slot == 0 {
            msg_text = g.active_entity_name.clone()
                + __content.string(2268)
                + __content.string(2269)
                + __content.string(2270)
                + __content.string(2271);
        }
        show_quick_message(g, ui, &mut msg_text);
        return;
    }
    let er = g.exchange_rate;
    let ae_name_uc = ucase(g.active_entity_name.as_bytes(), CaseMode::Ansi);
    msg_text = ae_name_uc.clone()
        + __content.string(2272)
        + __content.string(2273)
        + &ae_name_uc
        + "."
        + "\r"
        + "\r"
        + __content.string(2274)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(er * Ext::from_f64(100.0), b"* ############,.00")
        + " "
        + &g.million_prefix_lc
        + __content.string(2275)
        + __content.string(2276)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(er * Ext::from_f64(1000.0), b"* ############,.00")
        + " "
        + &g.million_prefix_lc
        + __content.string(2277)
        + __content.string(2278)
        + "\r"
        + "\r"
        + __content.string(2279)
        + __content.string(2280)
        + __content.string(2281)
        + __content.string(2282)
        + __content.string(2283)
        + __content.string(2284)
        + __content.string(2285)
        + "\r"
        + "\r"
        + __content.string(2286);
    let answer = ui.msgbox(&msg_text, 0x2024, &s(__content.string(2287)));
    if answer == 7 {
        return;
    }
    // ---- industry pick list
    pick_list[3] = s(__content.string(2288));
    pick_list[4] = string_fill(70, b"-");
    let mut line_no: i32 = 4;
    i = 1;
    while i <= 70 {
        line_no += 1;
        let min_cap_str = g.ind_name[i].clone() + &string_fill(75, b" ");
        let id = right((s("000") + &str_trim(cast::<Ext, _>(i))).as_bytes(), 4);
        pick_list[cast::<usize, _>(line_no)] = min_cap_str + "@I" + &id;
        i += 1;
    }
    // ARRAY SORT pickList(5) FOR 70
    pb_array_sort::<PbStr, ()>(&mut pick_list[5..75], None, false, &|a, b| {
        compare(a.as_bytes(), b.as_bytes()) == std::cmp::Ordering::Less
    });
    pick_list[cast::<usize, _>(line_no + 3)] = s("@END");
    for line_no in 1..=90 {
        g.text_lines[line_no] = pick_list[cast::<usize, _>(line_no)].clone();
    }
    g.entity_pick_mode = s("GetInd");
    g.text_box_title = s(__content.string(2289)) + &ae_name_uc + " ===";
    if g.active_entity < 11 {
        g.text_box_title = replace(g.text_box_title.as_bytes(), b"NEW SUB", __content.bytes(2290));
    }
    let saved_sel_industry = g.sel_industry;
    g.sel_industry = 0;
    pick_report(g, ui, rng, skip);
    if g.sel_industry == 0 {
        g.sel_industry = saved_sel_industry;
        return;
    }
    let mut min_cap_str = fmt_trim(er * Ext::from_f64(100.0), b"* ############.00");
    let mut industry: i16 = cast::<i16, _>(g.sel_industry);
    if industry < 3 {
        min_cap_str = fmt_trim(er * Ext::from_f64(1000.0), b"* ###,.00");
    }
    msg_text = s(__content.string(2291))
        + &trim(g.currency_symbol.as_bytes())
        + &min_cap_str
        + " "
        + &g.million_prefix_lc
        + __content.string(2292)
        + __content.string(2293)
        + &g.ind_name[cast::<i32, _>(industry)]
        + __content.string(2294)
        + __content.string(2295)
        + &g.active_entity_name
        + __content.string(2296)
        + __content.string(2297);
    let mut capital: Ext;
    loop {
        let input_text = ui.inputbox(
            &msg_text,
            &s(__content.string(2298)),
            &fmt_trim(avail_funds, b"#################"),
        );
        capital = val_removing(&input_text, b"$ ,%");
        if capital <= Ext::from_f64(0.0) {
            return;
        }
        if !(avail_funds + Ext::from_f64(1.0) <= capital) {
            break;
        }
    }
    if !(er * Ext::from_f64(999.0) <= capital || (er * Ext::from_f64(99.0) <= capital && 2 < g.sel_industry)) {
        let m = g.active_entity_name.clone()
            + __content.string(2299)
            + &trim(g.currency_symbol.as_bytes())
            + &min_cap_str
            + " "
            + &g.million_prefix_lc
            + "illion "
            + __content.string(2300);
        ui.msgbox(&m, 0x2010, &s(__content.string(2301)));
        return;
    }
    if g.cash[g.active_entity] + g.t_bills[g.active_entity] + Ext::from_f64(1.0) < capital {
        let mut shortfall = capital - (g.cash[g.active_entity] + g.t_bills[g.active_entity]);
        let ae = g.active_entity;
        calc_entity_financials(g, ui, rng, ae);
        let borrow_result = crate::logic::trading::borrow_to_cover_purchase(g, ui, &mut shortfall, Ext::from_f64(0.0), 0);
        if borrow_result == 7 {
            return;
        }
    }
    let industry_key = cast::<Ext, _>(industry);
    if industry_key == Ext::from_f64(1.0) || industry_key == Ext::from_f64(2.0) {
        let (range_start, range_end) = if industry == 2 { (121, 130) } else { (61, 85) };
        let last_id = range_end;
        i = range_start;
        while i <= last_id {
            if g.entity_status[i] != 1 {
                found = 1;
                new_corp_id = i;
                i = range_end;
            }
            i += 1;
        }
        if found != 0 {
            if industry == 1 {
                g.tmp_ext2 = capital;
                create_bank(g, ui, rng, new_corp_id, 0);
            } else {
                g.tmp_ext1 = capital;
                g.tmp_ext2 = capital * sf(0.28f32);
                create_insurer(g, ui, rng, new_corp_id, 0);
            }
            cost_rate = 0.1f32;
        }
    } else if industry_key == Ext::from_f64(70.0) {
        i = 1471;
        loop {
            if g.entity_status[i] != 1 {
                found = 1;
                new_corp_id = i;
                i = 1600;
            }
            i += 1;
            if i >= 1581 {
                break;
            }
        }
        if found == 0 {
            i = 1;
            loop {
                let rand_id = rng.rnd_range(1580, 131);
                if g.entity_status[rand_id] != 1 {
                    found = 1;
                    new_corp_id = rand_id;
                    i = 2000;
                }
                i += 1;
                if i >= 2001 {
                    break;
                }
            }
        }
        if found == 0 {
            let mut m = s(__content.string(2302))
                + __content.string(2303)
                + __content.string(2304)
                + "\r\n"
                + "\r\n"
                + __content.string(2305);
            show_quick_message(g, ui, &mut m);
            return;
        }
        let nc = new_corp_id;
        g.entity_status[nc] = 1;
        cost_rate = 0.1f32;
        startup_cost = to_quad(capital * sf(0.1f32));
        if er * Ext::from_f64(25.0) < cast::<Ext, _>(startup_cost) {
            startup_cost = to_quad(er * Ext::from_f64(25.0));
        }
        g.cash[nc] = capital - cast::<Ext, _>(startup_cost);
        g.accrued_pl[nc] = g.accrued_pl[nc] - cast::<Ext, _>(startup_cost);
        g.ent_misc_data[nc] = PbStr::new();
        g.industry_num[nc] = 70;
        g.market_cap[nc] = capital * sf(0.01f32);
        while g.lending_bank[nc] <= 10 {
            let bank = rng.rnd_range(85, 11);
            if g.entity_status[bank] == 1 {
                g.lending_bank[nc] = cast::<i16, _>(bank);
            }
        }
        g.shares_outstanding[nc] = Ext::from_f64(1.0);
        g.tmp_ext3 = capital;
        adjust_index_divisor(g, nc, 0);
        g.valuation_mode = 3;
        calc_stock_price(g, ui, rng, nc, time_secs);
        check_stock_split(g, nc);
        if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
            let _free_slot = find_free_entity_slot(g, rng, nc, industry);
        }
    } else {
        g.tmp_ext1 = capital;
        new_corp_id = start_new_corp_or_expand(g, ui, rng, industry, 1, time_secs);
        g.tmp_ext1 = Ext::from_f64(0.0);
        if 0 < new_corp_id {
            found = 1;
        }
        cost_rate = 0.1f32;
    }
    if found == 1 {
        let nc = new_corp_id;
        if capital < Ext::from_f64(2000.0) {
            g.shares_outstanding[nc] = sf(0.5f32);
        }
        if startup_cost == 0 {
            startup_cost = to_quad(capital * cast::<Ext, _>(cost_rate));
            if er * Ext::from_f64(200.0) < cast::<Ext, _>(startup_cost) {
                startup_cost = to_quad(er * Ext::from_f64(200.0));
            }
            if er * Ext::from_f64(100.0) < cast::<Ext, _>(startup_cost) && 130 < nc {
                startup_cost = 100;
            }
        }
        let ae = g.active_entity;
        g.hold_stock_id[(cast::<i32, _>(slot), ae)] = cast::<i16, _>(nc);
        g.hold_stock_pct[(cast::<i32, _>(slot), ae)] = 100;
        g.corp_controller[nc] = g.cur_player;
        g.auto_pilot[nc] = g.auto_pilot[g.cur_player];
        g.hold_stock_basis[(cast::<i32, _>(slot), ae)] = capital;
        g.market_cap[nc] = capital * sf(0.01f32);
        g.turn_action_count += 1;
        g.player_startup_count[g.cur_player] += 1;
        g.cash[ae] = g.cash[ae] - capital;
        balance_cash_t_bills(g, ae);
        if g.cash[ae] < Ext::from_f64(0.0) {
            let shortfall = -g.cash[ae]; // Cash * -1
            g.cash[ae] = Ext::from_f64(0.0);
            let bank = cast::<i32, _>(g.lending_bank[ae]);
            g.cash[bank] = g.cash[bank] - shortfall;
            g.bank_debt[ae] = g.bank_debt[ae] + shortfall;
        }
        g.clock_state = PbStr::new();
        g.ticker_on = 0;
        let prompt = s(__content.string(2306))
            + &g.ent_name[nc]
            + " ("
            + &g.ent_symbol[nc]
            + ")."
            + "\r"
            + "\r"
            + __content.string(2307);
        if ui.msgbox(&prompt, 0x2124, &s(__content.string(2308))) == 6 {
            g.target_corp = nc;
            select_target_corp(g, ui, rng, time_secs);
            crate::logic::corp_finance::misc::change_company_name(g, ui, rng, time_secs);
            update_market_display(g, ui, rng, time_secs);
            g.news_msg = g.ent_symbol[nc].clone()
                + " ("
                + &g.ent_name[nc]
                + __content.string(2309)
                + &mcase(g.ind_name[cast::<i32, _>(industry)].as_bytes(), CaseMode::Ansi)
                + __content.string(1547);
        }
        if industry == 1 {
            g.news_msg = parent_symbol.clone()
                + __content.string(2310)
                + &g.ent_symbol[nc]
                + " ("
                + &g.ent_name[nc]
                + ").";
        }
        post_news(g, ui, 0, 0);
        let prompt = s("\r")
            + "\r"
            + __content.string(2311)
            + &g.currency_symbol
            + &fmt_trim(cast::<Ext, _>(startup_cost), b"################")
            + " "
            + &g.million_prefix_lc
            + "illion.";
        msg_text = s(__content.string(2312)) + &g.ent_name[nc] + "." + &prompt;
        let prompt = s(__content.string(2313))
            + &g.ent_name[nc]
            + __content.string(2314)
            + __content.string(2315)
            + &g.country_name[g.country_idx[nc]]
            + __content.string(2316)
            + __content.string(2317);
        if ui.msgbox(&prompt, 0x2124, &s(__content.string(2318))) == 6 {
            industry = cast::<i16, _>(g.sel_industry);
            for line_no in 0..=53 {
                let id = right((s("000") + &str_trim(cast::<Ext, _>(1 + line_no))).as_bytes(), 4);
                pick_list[cast::<usize, _>(line_no)] = g.country_name[line_no].clone() + &string_fill(80, b" ") + "@I" + &id;
            }
            pick_list[54] = PbStr::new();
            pick_list[55] = PbStr::new();
            pick_list[56] = PbStr::new();
            pick_list[57] = s("@END");
            for line_no in 1..=90 {
                g.text_lines[line_no] = pick_list[cast::<usize, _>(line_no - 1)].clone();
            }
            g.entity_pick_mode = s("GetInd");
            g.text_box_title =
                s(__content.string(2319)) + &g.ent_name[nc] + " ===";
            g.sel_industry = 0;
            g.text_has_links = 1;
            pick_report(g, ui, rng, skip);
            if 0 < g.sel_industry && g.sel_industry < 55 {
                if g.sel_industry - 1 != g.country_idx[nc] {
                    g.sel_industry -= 1;
                    g.country_idx[nc] = g.sel_industry;
                    g.name_list_dirty = 1;
                }
            }
            g.sel_industry = cast::<i32, _>(industry);
        }
        if 10 < nc {
            if rng.rnd() * Ext::from_f64(5.0) < Ext::from_f64(1.0) {
                let margin = to_quad(rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(5.0));
                g.profit_margin[nc] = cast::<i16, _>(margin);
            }
            calc_projected_earnings(g, ui, rng, nc);
        }
    } else {
        msg_text = s(__content.string(2320))
            + &g.ind_name[cast::<i32, _>(industry)]
            + " "
            + __content.string(2321);
        g.sel_industry = saved_sel_industry;
    }
    if !(11..=1600).contains(&new_corp_id) {
        // CountryIdx(newCorpId): error 9 -> ON ERROR handler (BUGS.md #290)
        let _ = &msg_text; // (" [runtime resource wording] ..." is never shown)
        runtime_error9(g, ui, i);
        return;
    }
    msg_text = msg_text
        + "\r"
        + "\r"
        + __content.string(2322)
        + &g.country_name[g.country_idx[new_corp_id]]
        + ".";
    ui.msgbox(&msg_text, 0x2040, &s(__content.string(2323)));
}
