//! Player actions: rumors, lawsuits (harassing, antitrust), becoming / resigning as CEO, renaming a
//! company, buying an ETF advisory contract and setting the ETF advisory fee.

#![allow(clippy::manual_clamp)]
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::ai_player::mgmt::elect_self_ceo;
use crate::logic::control::update_corp_control;
use crate::logic::market::{refresh_selection_labels, select_target_corp, toggle_streaming_quote};
use crate::logic::earnings::{balance_cash_t_bills, get_funds_advised_by};
use crate::logic::finance::calc_entity_financials;
use crate::logic::report::{number_to_words, show_quick_message};
use crate::logic::trading::select::{in_box_symbol, lookup_symbol};
use crate::logic::valuation::{calc_industry_market_share, price_pct_change_months, update_price_history_tick};
use crate::pb::string::{asc, mcase, remove_any, retain_any, ucase, CaseMode};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `ShowQuickMessage(text)` on a temporary.
fn quick(g: &mut Globals, ui: &mut dyn Ui, text: PbStr) {
    let mut t = text;
    show_quick_message(g, ui, &mut t);
}

// --------------------------------------------------------------------------------------- SpreadRumors

/// Port of SpreadRumors (0x5aac8f).
/// The current player spreads rumors about a company picked in the symbol dialog (InBox 3). Not in
/// the game's last year; no effect on ETFs (industry 71); a controlled target needs confirmation. With
/// notoriety under 10: a random rumor headline, the cap drops to 85..97%, sentiment -20 (if above
/// -40), profit margin -5 (industries < 70), notoriety +1 (+2 below 6), `RumorVictimId` /
/// `LibelSuitDefendant` set, a progress report and, with notoriety above 6, a libel suit notice and
/// headline; then the price history tick. Notoriety 10+: refused (notoriety +1).
pub fn spread_rumors(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if g.game_end_year - 1 == g.cur_year {
        quick(g, ui, s(__content.string(3776)));
        return;
    }
    g.tmp_symbol = PbStr::new();
    g.action_code = PbStr::new();
    g.input_dlg_label = PbStr::new();
    g.input_dlg_prompt = s(__content.string(3777))
        + __content.string(3778)
        + __content.string(3779)
        + "\r\n"
        + "\r\n"
        + __content.string(3780)
        + "symbol."
        + "\r\n"
        + "\r\n";
    g.input_dlg_title = s(__content.string(3781)) + &g.active_entity_name + ")";
    g.lookup_filter = PbStr::new();
    in_box_symbol(g, ui, rng);
    if g.tmp_symbol.is_empty() {
        return;
    }
    let target_id = g.target_corp;
    if !(10 < g.target_corp) {
        return;
    }
    let cur = g.cur_player;
    if g.industry_num[g.target_corp] == 71 {
        quick(g, ui,
            s(__content.string(3782))
                + __content.string(3783)
                + __content.string(3784),
        );
        return;
    }
    if cur == g.corp_controller[target_id] {
        let t = s(__content.string(3785))
            + __content.string(3786);
        let title = g.app_title.clone();
        if ui.msgbox(&t, 8227, &title) != 6 {
            return;
        }
    }
    if g.player_notoriety[g.cur_player] < 10 {
        g.news_msg = g.tmp_symbol.clone() + __content.string(1810);
        if Ext::from_f64(200.0) < g.earnings_ytd[target_id] {
            g.news_msg = g.tmp_symbol.clone() + __content.string(1811);
        }
        if g.cash[target_id] <= Ext::from_f64(0.0) {
            g.news_msg = g.tmp_symbol.clone() + __content.string(3787);
        }
        if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
            g.news_msg = s(__content.string(1813)) + &g.tmp_symbol;
        }
        if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
            g.news_msg = s(__content.string(1814)) + &g.tmp_symbol + __content.string(1815);
        }
        post_news(g, ui, 0, 0);
        let factor = rng.rnd() * sf(0.12f32) + sf(0.85f32);
        g.market_cap[target_id] = g.market_cap[target_id] * factor;
        if -40 < g.investor_sentiment[target_id] {
            g.investor_sentiment[target_id] = g.investor_sentiment[target_id] - 20;
        }
        g.turn_action_count += 1;
        let p = g.cur_player;
        g.player_notoriety[p] += 1;
        if g.player_notoriety[p] < 6 {
            g.player_notoriety[p] += 1;
        }
        g.rumor_victim_id = target_id;
        if g.industry_num[target_id] < 70 {
            g.profit_margin[target_id] = g.profit_margin[target_id] - 5;
        }
        g.libel_suit_defendant = cast::<i16, _>(g.cur_player);
        let msg_text = s(__content.string(3788))
            + __content.string(3789)
            + __content.string(3790)
            + &g.ent_name[target_id]
            + ".";
        ui.msgbox(&msg_text, 8256, &s(__content.string(3791)));
        if 6 < g.player_notoriety[g.cur_player] {
            let v = g.rumor_victim_id;
            g.news_msg = g.ent_symbol[v].clone() + __content.string(3792) + &g.cur_player_name;
            let msg_text = ucase(g.news_msg.as_bytes(), CaseMode::Ansi)
                + __content.string(3793)
                + "ABOUT "
                + &g.ent_name[v]
                + ".";
            ui.msgbox(&msg_text, 8240, &s(__content.string(3794)));
            post_news(g, ui, 0, 0);
        }
        update_price_history_tick(g, ui, rng, target_id);
    } else {
        quick(g, ui,
            s(__content.string(3795))
                + __content.string(3796)
                + __content.string(3797)
                + __content.string(3798),
        );
        g.player_notoriety[g.cur_player] += 1;
    }
}

// ---------------------------------------------------------------------------------- BecomeOrResignCeo

/// Port of BecomeOrResignCeo (0x59ca81).
/// Menu "Become / resign as CEO". A CEO post at a company no longer active is cleared first. With a
/// corporation as active entity the current player is elected its CEO ([`elect_self_ceo`], which also
/// handles resignations); the menu caption update is UI only. With the player himself active: if he
/// controls exactly one company (by vote among his holdings, then by `CorpController`) and is not
/// already its CEO (or has it selected) he is elected there; otherwise a message explains why not
/// (nothing controlled / already CEO / no control of the selected company).
pub fn become_or_resign_ceo(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x59d1f2 (ShowRuntimeError) — error path not ported
    let mut ctrl_count = 0;
    let mut ctrl_corp_id = 0;
    let cp = g.cur_player;
    if 10 < g.player_ceo_of[cp] && g.entity_status[g.player_ceo_of[cp]] != 1 {
        g.player_ceo_of[cp] = 0;
        g.player_ceo_since_year[cp] = 0;
    }
    if g.active_entity < 11 {
        for i in 1..=15 {
            if g.hold_stock_id[(i, g.cur_player)] == 0 {
                continue;
            }
            let corp_id = cast::<i32, _>(g.hold_stock_id[(i, g.cur_player)]);
            if g.entity_status[corp_id] == 1 && 0 < g.hold_stock_pct[(i, g.cur_player)] {
                update_corp_control(g, ui, rng, corp_id);
                if g.cur_player == g.voting_controller {
                    ctrl_count += 1;
                    ctrl_corp_id = corp_id;
                }
            }
        }
        if ctrl_count == 1 {
            for i in 11..=1600 {
                if g.entity_status[i] == 1 && ctrl_corp_id != i && g.cur_player == g.corp_controller[i] {
                    ctrl_count += 1;
                }
            }
        }
        if ctrl_count == 1 && g.player_ceo_of[g.cur_player] == ctrl_corp_id && g.sel_entity != ctrl_corp_id {
            ctrl_count = 0;
        }
        let sel = g.sel_entity;
        if ctrl_count == 1 {
            g.tmp_ext1 = cast::<Ext, _>(ctrl_corp_id);
            elect_self_ceo(g, ui, rng);
        } else if g.cur_player == g.sel_entity {
            let mut msg_text = s(__content.string(3799))
                + __content.string(3800);
            let ceo_corp_id = g.player_ceo_of[g.cur_player];
            if ceo_corp_id < 11 {
                if ctrl_count == 0 {
                    msg_text = msg_text + "\r" + "\r" + __content.string(3801);
                }
            } else {
                msg_text = msg_text + "\r" + "\r" + __content.string(1619) + &g.ent_name[ceo_corp_id] + ".";
                if g.cur_player != g.corp_controller[ceo_corp_id] {
                    msg_text = msg_text
                        + "\r"
                        + "\r"
                        + __content.string(3802)
                        + __content.string(3803)
                        + __content.string(3804);
                }
            }
            let title = g.app_title.clone();
            ui.msgbox(&msg_text, 8208, &title);
        } else if 10 < g.sel_entity {
            if sel == g.player_ceo_of[g.cur_player] {
                let mut msg_text = s(__content.string(3805)) + &g.sel_corp_name + "." + "\r" + "\r";
                if g.cur_player != g.corp_controller[g.sel_entity] {
                    msg_text = msg_text
                        + __content.string(3806)
                        + __content.string(3807);
                }
                let title = g.app_title.clone();
                ui.msgbox(&msg_text, 8256, &title);
            } else {
                let t = s(__content.string(3808))
                    + __content.string(3809)
                    + &g.ent_name[g.sel_entity]
                    + __content.string(3810)
                    + __content.string(3811)
                    + "\r"
                    + "\r"
                    + __content.string(3812)
                    + __content.string(3813);
                ui.msgbox(&t, 8208, &s(__content.string(3814)));
            }
        }
    } else {
        g.tmp_ext1 = cast::<Ext, _>(g.active_entity);
        elect_self_ceo(g, ui, rng);
        // (UI) CONTROL SET TEXT hDlgTransMenu, 462, "Resig&n As CEO of " + EntSymbol(PlayerCeoOf(CurPlayer))
    }
}

// ---------------------------------------------------------------------------------- ChangeCompanyName

/// Port of ChangeCompanyName (0x4715f3).
/// Renames the active company (not after the game's end; a player can't be renamed). If the company
/// is not also the selected entity the player first confirms (YES/NO/CANCEL; NO skips to the symbol
/// question). The new name (INPUTBOX, upper case, trimmed; under 3 characters cancels; under 5 gets
/// " CORPORATION"; cut to 25; only `A-Z0-9,.'&/-` and blanks kept) must not be another company's
/// name (asked again). A renamed company on the player's watch list is re-added to the streaming
/// quotes and selected (`SelectTargetCorp`). Then the stock symbol may be changed (1..4 letters, not
/// reserved, not taken; errors ask again; an empty answer ends the routine). Afterwards the selection
/// labels are refreshed and `NameListDirty` set (unless nothing changed). Returns 1 if the name was set.
pub fn change_company_name(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) -> i32 {
    let __content = crate::content::current();
    let mut result = 0;
    let i = g.active_entity;
    if g.game_end_flag == "Y" {
        return result;
    }
    let old_name = g.active_entity_name.clone();
    let old_symbol = g.ent_symbol[i].clone();
    let sel_at_start = g.sel_entity;
    let mut reply = 6;
    let mut answer = PbStr::new();
    let mut symbol_status = 0;
    let mut on_watch_list = 0;
    if g.active_entity != g.sel_entity || g.active_entity < 11 {
        let mut msg_text = s(__content.string(3815)) + "\r" + "\r";
        if g.active_entity < 11 {
            show_quick_message(g, ui, &mut msg_text);
            return result;
        }
        msg_text = msg_text + __content.string(3313) + &g.active_entity_name + __content.string(3816);
        reply = ui.msgbox(&msg_text, 8227, &s(__content.string(3817)));
        if reply == 2 {
            return result;
        }
    }
    if reply != 7 {
        loop {
            let msg_text = s(__content.string(3818))
                + &old_name
                + __content.string(3819)
                + __content.string(3820);
            answer = ui.inputbox(&msg_text, &(s(__content.string(3821)) + &g.ent_symbol[i]), &old_name);
            answer = ucase(answer.as_bytes(), CaseMode::Ansi);
            answer = trim(answer.as_bytes());
            if len(answer.as_bytes()) < 3 {
                return result;
            }
            if len(answer.as_bytes()) < 5 {
                answer = answer + __content.string(3822);
            }
            if 25 < len(answer.as_bytes()) {
                answer = mid(answer.as_bytes(), 1, 25);
            }
            answer = retain_any(answer.as_bytes(), __content.bytes(3823));
            let mut taken = false;
            for idx in 11..=1600 {
                if answer == g.ent_name[idx] && g.active_entity != idx {
                    quick(g, ui, s(__content.string(3824)));
                    taken = true;
                    break;
                }
            }
            if !taken {
                break;
            }
        }
        g.ent_name[i] = answer.clone();
        g.active_entity_name = answer.clone();
        result = 1;
        for watch_idx in 1..=15 {
            if i == g.watch_list[(watch_idx, g.cur_player)] {
                on_watch_list = 1;
            }
        }
        if on_watch_list == 1 {
            toggle_streaming_quote(g, ui, rng, 0, 1);
            if g.auto_add_stream_list == 1 {
                g.last_auto_added_quote = 0;
                toggle_streaming_quote(g, ui, rng, 1, 1);
            }
        }
        g.sel_entity = g.cur_player;
        g.sel_corp_name = g.cur_player_name.clone();
        g.active_entity = g.cur_player;
        refresh_selection_labels(g);
        g.sel_corp_name = PbStr::new();
        g.target_corp = i;
        select_target_corp(g, ui, rng, time_secs);
    }
    let mut msg_text = old_name.clone() + __content.string(3512) + &answer + "\r" + "\r";
    if g.active_entity_name == old_name {
        msg_text = PbStr::new();
    }
    msg_text = msg_text + __content.string(3825);
    loop {
        if symbol_status != 2 {
            let title = g.app_title.clone();
            reply = ui.msgbox(&msg_text, 8484, &title);
        }
        symbol_status = 0;
        if reply == 6 {
            answer = ui.inputbox(&s(__content.string(3826)), &s(__content.string(3827)), &g.ent_symbol[i].clone());
            if answer.is_empty() {
                return result;
            }
            answer = ucase(answer.as_bytes(), CaseMode::Ansi);
            answer = trim(answer.as_bytes());
            let n = len(answer.as_bytes());
            let mut idx = 1;
            while idx <= n {
                let ch = mid(answer.as_bytes(), idx, 1);
                let c = asc(ch.as_bytes());
                if c < 65 || 90 < c {
                    symbol_status = 2;
                    quick(g, ui, s(__content.string(3828)));
                    idx = n;
                }
                idx += 1;
            }
            if symbol_status == 2 {
                continue;
            }
            let reserved = [b"ETF".as_slice(), b"CEO", b"SEC", b"IRS", b"EPA", b"FTC", b"LBO", b"IPO", b"FDIC"];
            if reserved.iter().any(|r| answer.as_bytes() == *r) {
                let t = s(__content.string(3829)) + &answer + __content.string(3830);
                ui.msgbox(&t, 8208, &s(__content.string(3831)));
                symbol_status = 2;
            } else if 0 < len(answer.as_bytes()) && len(answer.as_bytes()) < 5 {
                for idx in 1..=1600 {
                    if answer == g.ent_symbol[idx] {
                        symbol_status = idx;
                    }
                    if i == symbol_status {
                        symbol_status = 0;
                    }
                }
                if symbol_status < 1 {
                    if answer != g.ent_symbol[i] {
                        symbol_status = -1;
                    }
                    g.ent_symbol[i] = answer.clone();
                } else {
                    quick(g, ui, s(__content.string(3832)) + &g.ent_name[symbol_status]);
                    symbol_status = 2;
                }
            } else {
                quick(g, ui, s(__content.string(3833)));
                symbol_status = 2;
            }
        }
        if !(0 < symbol_status) {
            break;
        }
    }
    if -1 < symbol_status && old_name == g.ent_name[i] {
        return result;
    }
    g.sel_corp_name = PbStr::new();
    if g.active_entity != sel_at_start {
        g.sel_entity = sel_at_start;
    }
    refresh_selection_labels(g);
    g.name_list_dirty = 1;
    if g.last_sel_entity == i && g.ent_symbol[i] != old_symbol {
        // (UI) CONTROL SET TEXT hDlgMain, 173, "Se&lect " + EntSymbol(i)
    }
    result
}

// ----------------------------------------------------------------------------------- HarassingLawsuit

/// Port of HarassingLawsuit (0x59e9e5).
/// The active entity (liquid funds of at least 100) files a nuisance suit against a company picked in
/// the symbol dialog (not itself; suing a controlled company needs confirmation). Refused when a suit
/// by it against that target is pending, by the court when no lawsuit slot is free or the target is
/// already a defendant (or when the target sued it maliciously). The player enters the fees (100..500,
/// at most liquid funds + 0.01; asked again; <= 0 cancels): the defendant pays them less 10% per law
/// firm grade the player is ahead. Notoriety +2; the suit (`"2|plaintiff|defendant|term|"`, term =
/// notoriety + 4 - edge, at least 3) goes into the last free `LawsuitRec` slot; both pay (extraordinary
/// P&L), a cash shortfall of the plaintiff is borrowed, a corporate plaintiff's cap drops by 1% of the
/// fees; headline and notice.
pub fn harassing_lawsuit(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x59fc2b (ShowRuntimeError) — error path not ported
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    if !(Ext::from_f64(100.0) <= g.liquid_funds) {
        quick(g, ui,
            g.active_entity_name.clone()
                + __content.string(3834)
                + &g.million_prefix_lc
                + __content.string(3835)
                + __content.string(3836),
        );
        return;
    }
    g.tmp_symbol = PbStr::new();
    g.action_code = PbStr::new();
    g.input_dlg_label = PbStr::new();
    g.input_dlg_prompt = s(__content.string(3837))
        + &g.ent_symbol[g.active_entity]
        + __content.string(3838)
        + "\r\n"
        + "\r\n"
        + __content.string(3839)
        + "\r\n"
        + "\r\n";
    g.input_dlg_title = s(__content.string(3840)) + &g.active_entity_name;
    g.lookup_filter = PbStr::new();
    in_box_symbol(g, ui, rng);
    let i = g.target_corp;
    if g.tmp_symbol.is_empty() || !(10 < g.target_corp) {
        return;
    }
    if g.active_entity == g.target_corp {
        quick(g, ui, g.active_entity_name.clone() + __content.string(3841));
        return;
    }
    if g.cur_player == g.corp_controller[g.target_corp] {
        let t = s(__content.string(3842)) + &g.ent_name[i] + __content.string(3843) + __content.string(3844);
        if ui.msgbox(&t, 8484, &s(__content.string(3845))) == 7 {
            return;
        }
    }
    let mut allowed = true;
    let mut suit_status = 1;
    for slot in 1..=10 {
        let msg_text = g.lawsuit_rec[slot].clone();
        if msg_text.is_empty() && suit_status == 1 {
            suit_status = 0;
        }
        if mid(msg_text.as_bytes(), 1, 1) == "2" {
            let plaintiff = val(parse_any(msg_text.as_bytes(), b"|", 2).as_bytes());
            let defendant = val(parse_any(msg_text.as_bytes(), b"|", 3).as_bytes());
            if to_quad(plaintiff) == cast::<i64, _>(g.active_entity) && i == to_long(defendant) {
                allowed = false;
            }
            if i == to_long(defendant) {
                suit_status = -1;
            }
        }
    }
    if g.active_entity == g.malicious_suit_defendant && i == g.malicious_suit_plaintiff {
        allowed = false;
    }
    if !(allowed && suit_status == 0) {
        let mut msg_text = g.active_entity_name.clone() + __content.string(3846) + &g.ent_symbol[i];
        if suit_status == 1 {
            msg_text = s(__content.string(3847));
        }
        if suit_status == -1 && allowed {
            msg_text = s(__content.string(3848)) + __content.string(3849) + &g.ent_name[i] + ".";
        }
        ui.msgbox(&msg_text, 8256, &s(__content.string(3850)));
        return;
    }
    let mut msg_text = s(__content.string(3851))
        + &g.million_prefix_lc
        + __content.string(3852)
        + &g.active_entity_name
        + " want "
        + __content.string(3853)
        + __content.string(3854)
        + "\r\n"
        + "\r\n"
        + __content.string(3855);
    let mut aux_text = s("500") + " " + &g.million_prefix_uc + ". +";
    if g.liquid_funds < Ext::from_f64(500.0) {
        aux_text = fmt_trim(g.liquid_funds, b"###,.00") + " " + &g.million_prefix_uc + ".";
    }
    msg_text = msg_text + &aux_text + ")";
    let mut fees: Ext;
    loop {
        let input_text = ui.inputbox(&msg_text, &s(__content.string(3856)), &aux_text);
        fees = val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes());
        if !(fees > Ext::from_f64(0.0)) {
            return;
        }
        if !(fees < Ext::from_f64(100.0) || Ext::from_f64(500.0) < fees || g.liquid_funds + sf(0.01f32) < fees) {
            break;
        }
    }
    let mut def_law_firm: i64 = 0;
    let ctl = g.corp_controller[i];
    if 0 < ctl && ctl < 11 {
        def_law_firm = cast::<i64, _>(g.player_law_firm[ctl]);
    }
    if def_law_firm == 0 {
        def_law_firm = 2;
    }
    let law_firm = g.player_law_firm[g.cur_player];
    let law_firm_edge = to_long(cast::<Ext, _>(cast::<i64, _>(law_firm) - def_law_firm));
    let def_fees = fees - cast::<Ext, _>(to_quad(sf(0.1f32) * cast::<Ext, _>(law_firm_edge) * fees));
    let p = g.cur_player;
    g.player_notoriety[p] += 2;
    let mut suit_term = (cast::<i32, _>(g.player_notoriety[p]) + 4) - law_firm_edge;
    if suit_term < 3 {
        suit_term = 3;
    }
    let mut filed = false;
    let rec = s("2|") + &str_s(cast::<Ext, _>(g.active_entity)) + "|" + &str_s(cast::<Ext, _>(i)) + "|" + &str_s(cast::<Ext, _>(suit_term)) + "|";
    let mut slot = 10;
    loop {
        if g.lawsuit_rec[slot].is_empty() {
            g.lawsuit_rec[slot] = rec.clone();
            slot = 1;
            filed = true;
        }
        slot -= 1;
        if !(0 < slot) {
            break;
        }
    }
    if !filed {
        let t = g.active_entity_name.clone()
            + __content.string(3857)
            + __content.string(3858)
            + __content.string(3849)
            + &g.ent_name[i]
            + ".";
        ui.msgbox(&t, 8256, &s(__content.string(3859)));
        g.player_notoriety[p] -= 2;
        return;
    }
    let ae = g.active_entity;
    g.cash[ae] = g.cash[ae] - fees;
    if 10 < ae {
        g.extraordinary_pl[ae] = to_quad(cast::<Ext, _>(g.extraordinary_pl[ae]) - fees);
    }
    balance_cash_t_bills(g, ae);
    if g.cash[ae] < Ext::from_f64(0.0) {
        let c = g.cash[ae];
        g.bank_debt[ae] = g.bank_debt[ae] + c.abs();
        g.cash[ae] = Ext::from_f64(0.0);
        let bank_id = cast::<i32, _>(g.lending_bank[ae]);
        g.cash[bank_id] = g.cash[bank_id] - c.abs();
    }
    g.cash[i] = g.cash[i] - def_fees;
    g.extraordinary_pl[i] = to_quad(cast::<Ext, _>(g.extraordinary_pl[i]) - def_fees);
    g.news_msg = g.tmp_symbol.clone() + __content.string(3860) + &g.ent_symbol[ae];
    if 10 < ae {
        g.market_cap[ae] = g.market_cap[ae] - fees * sf(0.01f32);
        if g.market_cap[ae] < sf(0.5f32) {
            g.market_cap[ae] = sf(0.5f32);
        }
        let k = g.rand_seed % 5;
        if k == 1 {
            g.news_msg = g.ent_symbol[ae].clone() + __content.string(3861) + &g.tmp_symbol;
        }
        if k == 2 {
            g.news_msg = g.tmp_symbol.clone() + __content.string(3862) + &g.ent_symbol[ae];
        }
        if k == 3 {
            g.news_msg = s(__content.string(3863)) + &g.ent_symbol[ae] + __content.string(3864) + &g.tmp_symbol;
        }
        if k == 4 {
            g.news_msg = g.ent_symbol[ae].clone() + __content.string(3865) + &g.tmp_symbol + __content.string(3866);
        }
    }
    post_news(g, ui, 0, 0);
    let t = s(__content.string(3867))
        + &g.active_entity_name
        + " vs. "
        + &g.ent_name[i]
        + "."
        + "\r"
        + "\r"
        + __content.string(3868)
        + &g.currency_symbol
        + &fmt_trim(def_fees, b"####")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + ".";
    ui.msgbox(&t, 8256, &s(__content.string(3850)));
    g.turn_action_count += 1;
}

// ----------------------------------------------------------------------------------- FileAntitrustSuit

/// Port of FileAntitrustSuit (0x588f0f).
/// The active company sues a competitor of its industry (picked in the symbol dialog, not controlled
/// by the player) for antitrust damages. Dismissed when [runtime resource wording]  are off, or when the
/// company sued recently (`AntitrustPlaintiff`, `RecentAntitrustFilers`, a pending `"1 |…"` suit it
/// filed). Needs liquid funds of 10; the damages (INPUTBOX, >= 100, at most the defendant's net worth,
/// `ExchangeRate x 250000` and the funds / fee rate of the player's law firm; <= 0 cancels, too much asks
/// again) and the minimum settlement are entered; fee estimate box, headline, one turn action. The suit
/// (`"1 | defendant| plaintiff| market share| damages| settlement|"`) goes into the first free
/// `LawsuitRec` slot, the defendant reserves 6% of the after-tax damages (`EquityIncomeYTD`,
/// `ContingencyReserve`); with no free slot the oldest record is dropped (headline) and the new one
/// appended. `TmpExt1` = the defendant's controller (or the defendant).
pub fn file_antitrust_suit(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x58a170 (ShowRuntimeError) — error path not ported
    if !(10 < g.active_entity && g.active_entity < 1601) {
        return;
    }
    if g.private_antitrust_off == 1 {
        let t = s(__content.string(3869))
            + __content.string(3870)
            + __content.string(3871)
            + __content.string(3872);
        ui.msgbox(&t, 8256, &s(__content.string(3873)));
        return;
    }
    let mut allowed = g.active_entity != g.antitrust_plaintiff;
    for d in 1..=10 {
        if g.active_entity == g.recent_antitrust_filers[d] {
            allowed = false;
        }
        let rec = g.lawsuit_rec[d].clone();
        if mid(rec.as_bytes(), 1, 1) == "1" {
            let field = parse_any(rec.as_bytes(), b"|", 3);
            if val(field.as_bytes()) == cast::<Ext, _>(g.active_entity) {
                allowed = false;
            }
        }
    }
    if !allowed {
        let t = s(__content.string(3874))
            + &g.active_entity_name
            + __content.string(3875)
            + __content.string(3876);
        ui.msgbox(&t, 8256, &s(__content.string(3873)));
        return;
    }
    let industry_id = g.industry_num[g.active_entity];
    g.tmp_symbol = PbStr::new();
    g.action_code = s("ANTITRUST");
    g.input_dlg_label = s(__content.string(3877));
    g.input_dlg_title = s(__content.string(3878)) + &g.active_entity_name;
    g.input_dlg_prompt = s(__content.string(3879))
        + &mcase(g.ind_name[cast::<i32, _>(industry_id)].as_bytes(), CaseMode::Ansi)
        + __content.string(3880)
        + &g.active_entity_name
        + __content.string(3881)
        + "\r\n"
        + "\r\n"
        + __content.string(3839)
        + "\r\n"
        + "\r\n";
    g.lookup_filter = PbStr::new();
    in_box_symbol(g, ui, rng);
    let dlg_result = g.symbol_dlg_text.clone();
    if dlg_result == "Cancel" {
        return;
    }
    lookup_symbol(g, ui, rng, &dlg_result);
    if g.tmp_symbol.is_empty() || !(10 < g.target_corp) {
        return;
    }
    let defendant_id = g.target_corp;
    update_corp_control(g, ui, rng, defendant_id);
    if !(g.industry_num[defendant_id] == industry_id && g.cur_player != g.corp_controller[defendant_id]) {
        let mut msg_text = g.ent_name[defendant_id].clone() + __content.string(3882) + &g.active_entity_name + "\r" + "\r";
        if g.cur_player == g.corp_controller[defendant_id] {
            msg_text = s(__content.string(3883)) + &g.ent_name[defendant_id] + "." + "\r" + "\r";
        }
        msg_text = msg_text + __content.string(3884);
        ui.msgbox(&msg_text, 8208, &s(__content.string(3885)));
        return;
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    if !(Ext::from_f64(10.0) <= g.liquid_funds) {
        let mut msg_text = g.active_entity_name.clone()
            + __content.string(3886)
            + &g.million_prefix_lc
            + __content.string(3835)
            + __content.string(3887)
            + "\r"
            + "\r";
        let mut input_text = s(__content.string(3888));
        if g.player_law_firm[g.cur_player] == 1 {
            input_text = s(__content.string(3889));
        }
        msg_text = msg_text + &input_text;
        ui.msgbox(&msg_text, 8256, &s(__content.string(1928)));
        return;
    }
    let mut fee_rate = sf(0.06f32);
    if g.player_law_firm[g.cur_player] == 1 {
        fee_rate = sf(0.035f32);
    } else if g.player_law_firm[g.cur_player] == 3 {
        fee_rate = sf(0.085f32);
    }
    let max_affordable = to_quad(g.liquid_funds / fee_rate);
    calc_entity_financials(g, ui, rng, defendant_id);
    let mut damages_limit = to_quad(g.net_worth);
    if g.exchange_rate * Ext::from_f64(250000.0) < cast::<Ext, _>(damages_limit) {
        damages_limit = to_quad(g.exchange_rate * Ext::from_f64(250000.0));
    }
    let mut capped_limit = damages_limit;
    if damages_limit < 100 {
        capped_limit = 100;
    }
    damages_limit = if capped_limit <= max_affordable { capped_limit } else { max_affordable };
    let cur_sym = trim(g.currency_symbol.as_bytes());
    let msg_text = s(__content.string(3890))
        + &g.active_entity_name
        + __content.string(3891)
        + __content.string(3892)
        + &cur_sym
        + &fmt_trim(cast::<Ext, _>(damages_limit), b"* ###,")
        + " "
        + &g.million_prefix_lc
        + __content.string(3893)
        + __content.string(3894)
        + &cur_sym
        + "100 "
        + &g.million_prefix_uc
        + __content.string(3895);
    let mut damages_sought: i64;
    loop {
        let input_text = ui.inputbox(&msg_text, &s(__content.string(3896)), &fmt_trim(cast::<Ext, _>(damages_limit), b"#########,"));
        damages_sought = to_quad(val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes()));
        if damages_sought <= 0 {
            return;
        }
        if damages_limit >= damages_sought {
            break;
        }
    }
    if damages_sought < 100 {
        damages_sought = 100;
    }
    g.tmp_ext1 = cast::<Ext, _>(defendant_id);
    if 0 < g.corp_controller[defendant_id] {
        g.tmp_ext1 = cast::<Ext, _>(g.corp_controller[defendant_id]);
    }
    let i = defendant_id;
    let plaintiff = g.active_entity;
    let market_share = calc_industry_market_share(g, industry_id);
    let law_firm = g.player_law_firm[g.cur_player];
    let est_fees = to_quad((cast::<Ext, _>(law_firm) * sf(0.025f32) + sf(0.01f32)) * cast::<Ext, _>(damages_sought));
    let t = s(__content.string(3897))
        + "\r"
        + "\r"
        + __content.string(3898)
        + &g.currency_symbol
        + &str_s(cast::<Ext, _>(est_fees))
        + " "
        + &g.million_prefix_lc
        + __content.string(3899);
    ui.msgbox(&t, 8256, &s(__content.string(3900)));
    g.news_msg = g.ent_symbol[g.active_entity].clone() + __content.string(3901) + &g.ent_symbol[defendant_id];
    post_news(g, ui, 0, 0);
    g.turn_action_count += 1;
    let msg_text = s(__content.string(3902))
        + &g.currency_symbol
        + &fmt_trim(cast::<Ext, _>(damages_sought), b"* ###,")
        + " "
        + &g.million_prefix_lc
        + __content.string(3903)
        + __content.string(3904);
    let mut settle: i64;
    loop {
        let input_text = ui.inputbox(&msg_text, &s(__content.string(3905)), &PbStr::new());
        settle = to_quad(val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes()));
        if settle <= 0 {
            settle = damages_sought;
        }
        if !(damages_sought + 1 < settle) {
            break;
        }
    }
    if settle < to_quad(cast::<Ext, _>(damages_sought) * sf(0.9f32)) {
        quick(g, ui, s(__content.string(3906)));
    }
    let rec = s("1 |")
        + &str_s(cast::<Ext, _>(i))
        + "|"
        + &str_s(cast::<Ext, _>(plaintiff))
        + "|"
        + &str_s(cast::<Ext, _>(market_share))
        + "|"
        + &str_s(cast::<Ext, _>(damages_sought))
        + "|"
        + &str_s(cast::<Ext, _>(settle))
        + "|";
    for d in 1..=10 {
        if g.lawsuit_rec[d].is_empty() {
            g.lawsuit_rec[d] = rec;
            let reserve = cast::<Ext, _>(100 - g.corp_tax_rate) * sf(0.01f32) * sf(0.06f32) * cast::<Ext, _>(damages_sought);
            g.equity_income_ytd[i] = g.equity_income_ytd[i] - reserve;
            g.contingency_reserve[i] = g.contingency_reserve[i] + reserve;
            return;
        }
    }
    let f2 = val(parse_any(g.lawsuit_rec[1].as_bytes(), b"|", 2).as_bytes());
    let f3 = val(parse_any(g.lawsuit_rec[1].as_bytes(), b"|", 3).as_bytes());
    let field_text = if mid(g.lawsuit_rec[1].as_bytes(), 1, 1) == "1" { s(__content.string(3907)) } else { PbStr::new() };
    g.news_msg = g.ent_symbol[to_long(f2)].clone()
        + __content.string(3908)
        + &field_text
        + __content.string(3909)
        + &g.ent_symbol[to_long(f3)];
    post_news(g, ui, 0, 0);
    for d in 1..=9 {
        g.lawsuit_rec[d] = g.lawsuit_rec[d + 1].clone();
    }
    g.lawsuit_rec[10] = rec;
}

// ----------------------------------------------------------------------------------- SetEtfAdvisoryFee

/// `STR$` of a SINGLE (7 digits).
fn str_single(x: f32) -> PbStr {
    PbStr::from(str_num(cast::<Ext, _>(x), 7).as_str())
}

/// Port of SetEtfAdvisoryFee_MsgTooNew (0x5c2d0b), GOSUB of `SetEtfAdvisoryFee`: the "only became the
/// advisor in …" refusal text for ETF `etf_id`.
fn set_etf_advisory_fee_msg_too_new(g: &Globals, etf_id: i32) -> PbStr {
    let __content = crate::content::current();
    g.active_entity_name.clone()
        + __content.string(3910)
        + &g.ent_name[etf_id]
        + " in"
        + &str_s(cast::<Ext, _>(g.ctrl_since_year[etf_id]))
        + ". "
        + __content.string(3911)
        + &g.ent_name[etf_id]
        + __content.string(3912)
        + &g.active_entity_name
        + __content.string(3913)
        + __content.string(3914)
        + &str_s(cast::<Ext, _>(3 + g.ctrl_since_year[etf_id]))
        + __content.string(3915)
        + __content.string(3916)
        + &g.active_entity_name
        + __content.string(3917)
        + __content.string(3918)
}

/// Port of SetEtfAdvisoryFee_Exit (0x5c2e75): the routine's exit label (frame release only).
fn set_etf_advisory_fee_exit() {}

/// Port of SetEtfAdvisoryFee (0x5c1dae) with its GOSUB SetEtfAdvisoryFee_MsgTooNew (0x5c2d0b).
/// The active insurer / securities broker resets [runtime resource wording] of every ETF (1581..1600) it
/// advises (an inactive or non-fund record loses its adviser). Bond and index funds (< 1586) stay at
/// 0.2% (notice). An equity fund may raise its fee (max 1%) only after three years as adviser and if
/// it beat the stock index by 2 points over 24 months, else only lower it (the board's refusal or the
/// "too new" notice). The new rate (INPUTBOX, tenths of a percent, 0.2..max; <= 0 cancels everything;
/// the same rate: "No change made...") may need a warning about losing the bonus period
/// (`FundBonusYear` cleared). CANCEL in a box ends the loop. One turn action if a rate changed.
pub fn set_etf_advisory_fee(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let mut changed = 0;
    if g.active_entity < 86 || g.entity_status[g.active_entity] != 1 {
        set_etf_advisory_fee_exit();
        return;
    }
    if g.industry_num[g.active_entity] != 2 && g.industry_num[g.active_entity] != 37 {
        set_etf_advisory_fee_exit();
        return;
    }
    if get_funds_advised_by(g, g.active_entity) == 0 {
        quick(g, ui,
            g.active_entity_name.clone()
                + __content.string(3919)
                + __content.string(3920),
        );
        set_etf_advisory_fee_exit();
        return;
    }
    'etfs: for etf_id in 1581..=1600 {
        if g.active_entity != g.fund_adviser[etf_id] {
            continue;
        }
        if !(g.entity_status[etf_id] == 1 && g.industry_num[etf_id] == 71) {
            g.fund_adviser[etf_id] = 0;
            continue;
        }
        let mut cur_fee: f32 = cast::<f32, _>(g.fund_fee_pct[etf_id]);
        let mut max_fee: f32 = cast::<f32, _>(Ext::from_f32(1.0f32));
        let index_gain = price_pct_change_months(g, 0, 24);
        let etf_gain = price_pct_change_months(g, etf_id, 24);
        let outperformance = etf_gain - index_gain;
        if outperformance < 2 {
            max_fee = cur_fee;
        }
        if etf_id < 1586 {
            let mut label_text = s("index");
            if etf_id < 1584 {
                label_text = s("bond");
            }
            let t = s(__content.string(3921))
                + __content.string(3922)
                + __content.string(3923)
                + &label_text
                + __content.string(3924)
                + &g.ent_name[etf_id]
                + __content.string(3925)
                + &label_text
                + __content.string(3926)
                + __content.string(3927)
                + "\r"
                + "\r"
                + __content.string(3928)
                + &label_text
                + __content.string(3929)
                + __content.string(3930);
            ui.msgbox(&t, 8208, &s(__content.string(3931)));
            continue;
        }
        if !(0.21f32 <= cur_fee) && !(g.ctrl_since_year[etf_id] <= g.cur_year - 3) {
            let t = set_etf_advisory_fee_msg_too_new(g, etf_id);
            ui.msgbox(&t, 8208, &s(__content.string(3931)));
            continue;
        }
        if !(0.21f32 <= cur_fee) && !(0.21f32 <= max_fee) {
            let mut msg_text = s(__content.string(3911))
                + &g.ent_name[etf_id]
                + __content.string(3912)
                + &g.active_entity_name
                + __content.string(3913)
                + __content.string(3932)
                + __content.string(3933);
            if index_gain != 0 && etf_gain != 0 {
                msg_text = msg_text
                    + __content.string(3934)
                    + &number_to_words(etf_gain, 0, 1)
                    + __content.string(3935)
                    + __content.string(3936)
                    + &number_to_words(index_gain, 0, 1)
                    + ".";
                if etf_gain == index_gain {
                    msg_text = replace(msg_text.as_bytes(), b"rose", __content.bytes(3937));
                }
                if index_gain < 0 {
                    msg_text = replace(msg_text.as_bytes(), b"rose", b"fell");
                }
                if etf_gain < 0 {
                    msg_text = replace(msg_text.as_bytes(), b"grew", __content.bytes(1113));
                }
            }
            msg_text = msg_text
                + "\r\n"
                + "\r\n"
                + __content.string(3938)
                + __content.string(3939);
            show_quick_message(g, ui, &mut msg_text);
            continue;
        }
        if 0.2f32 > cur_fee {
            cur_fee = 0.2f32;
        } else if Ext::from_f32(1.0f32) < cur_fee {
            cur_fee = 1.0f32;
        } // (not `clamp`: keeps the original's NaN behaviour)
        let mut max_fee_text = trim(str_single(max_fee).as_bytes());
        if max_fee <= 0.99f32 {
            max_fee_text = s("0") + &max_fee_text;
        } else {
            max_fee_text = s(" 1.0");
        }
        let mut msg_text = g.active_entity_name.clone()
            + __content.string(3940)
            + &g.ent_name[etf_id]
            + ". "
            + __content.string(3941)
            + &str_single(cur_fee)
            + __content.string(3942)
            + __content.string(3943)
            + &g.ent_symbol[g.active_entity]
            + " "
            + __content.string(3944)
            + "\r"
            + "\r";
        if outperformance < 2 {
            msg_text = s(__content.string(3945))
                + &g.ent_name[etf_id]
                + __content.string(3946)
                + __content.string(3947)
                + "\r"
                + "\r";
        }
        msg_text = msg_text
            + __content.string(3948)
            + "\r"
            + "\r"
            + __content.string(3949)
            + &max_fee_text
            + "%)";
        if max_fee == cur_fee {
            msg_text = replace(msg_text.as_bytes(), __content.bytes(3950), __content.bytes(3951));
        }
        let title = s(__content.string(3952)) + &g.ent_name[etf_id] + " (" + &g.ent_symbol[etf_id] + ")";
        let btn = ui.msgbox(&msg_text, 8227, &title);
        if btn == 2 {
            break;
        }
        if btn != 6 {
            continue;
        }
        let mut new_fee: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
        let msg_text = s(__content.string(3953))
            + &max_fee_text
            + __content.string(3954)
            + &max_fee_text;
        loop {
            let answer = ui.inputbox(&msg_text, &(s(__content.string(3955)) + &g.ent_symbol[etf_id]), &s(" 0.2%"));
            let entered = val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes());
            if !(Ext::from_f64(0.0) < entered) {
                quick(g, ui, s(__content.string(3956)));
                break 'etfs;
            }
            if !(sf(0.19f32) > entered) && sf(1.01f32) >= entered && cast::<Ext, _>(max_fee) * sf(1.01f32) >= entered {
                let tenths = to_long(Ext::from_f64(10.0) * entered);
                new_fee = cast::<f32, _>(cast::<Ext, _>(tenths) / Ext::from_f64(10.0));
                if cur_fee == new_fee {
                    quick(g, ui, s(__content.string(3490)));
                    break;
                }
            }
            if !(0.2f32 > new_fee || Ext::from_f32(1.0f32) < new_fee) {
                break;
            }
        }
        if !(new_fee <= cur_fee) && !(g.cur_year - 3 >= g.ctrl_since_year[etf_id]) {
            let t = set_etf_advisory_fee_msg_too_new(g, etf_id);
            ui.msgbox(&t, 8208, &s(__content.string(3931)));
            continue;
        }
        if cur_fee < 0.21f32 && 0.29f32 < new_fee {
            let mut label_text = s("next");
            if 0 < g.fund_bonus_year[etf_id] {
                label_text = s("current");
            }
            let mut warn_text = s(__content.string(3957))
                + &g.active_entity_name
                + " from "
                + __content.string(3958)
                + &g.ent_symbol[etf_id]
                + __content.string(3959)
                + __content.string(3960)
                + &label_text
                + __content.string(3961)
                + "\r"
                + "\r"
                + __content.string(3962);
            if label_text == "next" {
                warn_text = replace(warn_text.as_bytes(), b"will", b"may");
            }
            let btn = ui.msgbox(&warn_text, 8227, &s(__content.string(3963)));
            if btn == 2 {
                break;
            }
            if btn == 7 {
                continue;
            }
            g.fund_bonus_year[etf_id] = 0;
        }
        if cur_fee != new_fee {
            changed = 1;
            let mut label_text = s("0") + &trim(str_single(new_fee).as_bytes());
            if new_fee == Ext::from_f32(1.0f32) {
                label_text = s("1.0");
            }
            quick(g, ui,
                s(__content.string(3964)) + &g.ent_name[etf_id] + __content.string(3965) + &label_text + __content.string(3966),
            );
            g.fund_fee_pct[etf_id] = cast::<f32, _>(new_fee);
        }
    }
    if changed == 1 {
        g.turn_action_count += 1;
    }
    set_etf_advisory_fee_exit();
}

// -------------------------------------------------------------------------------- SeekBecomeEtfAdviser

/// Port of SeekBecomeEtfAdviser (0x6501b3).
/// The active insurer (industry 2) or securities broker (37) buys the advisory contract of an ETF
/// (picked in the symbol dialog; asked again for a non-ETF or an ETF whose adviser another player
/// controls) for 10x the annual base fee (5x for bond / index funds < 1586) of the fund's net worth
/// (fee rate at least 0.2%, stored back). Refused without the liquid funds; an insurer may not lose
/// more than half its net worth (expensed); a broker capitalises it as goodwill (risk warning if it
/// exceeds 3x the business assets or the net worth). The seller (a corp adviser) receives the price
/// as extraordinary income; a cash shortfall is borrowed from `LenderId`. Sets `FundAdviser`,
/// `CtrlSinceYear`, clears `FundBonusYear`, one turn action, confirmation notices; an equity fund with
/// a fee above 0.2% offers [`set_etf_advisory_fee`].
pub fn seek_become_etf_adviser(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x651d70 (ShowRuntimeError) — error path not ported
    let adviser_id = g.active_entity;
    if g.entity_status[g.active_entity] != 1 {
        return;
    }
    let mut msg_text = PbStr::new();
    let mut word_text = s(__content.string(3967));
    if adviser_id < 86 || 1600 < adviser_id {
        msg_text = word_text.clone();
    }
    if g.industry_num[adviser_id] != 2 && g.industry_num[adviser_id] != 37 {
        msg_text = word_text.clone();
    }
    if !msg_text.is_empty() {
        show_quick_message(g, ui, &mut msg_text);
        return;
    }
    word_text = s(__content.string(3968));
    if g.industry_num[adviser_id] == 37 {
        word_text = s(__content.string(3969));
    }
    let msg_text = s("As an ")
        + &word_text
        + __content.string(3970)
        + __content.string(3971)
        + &g.active_entity_name
        + __content.string(3972)
        + __content.string(3973)
        + __content.string(3974)
        + "\r"
        + "\r"
        + "Once "
        + &g.ent_symbol[adviser_id]
        + __content.string(3975)
        + __content.string(3976)
        + __content.string(3977)
        + &g.active_entity_name
        + __content.string(3978)
        + __content.string(3979)
        + "\r"
        + "\r"
        + __content.string(3980)
        + __content.string(3981)
        + __content.string(3982)
        + __content.string(3983)
        + __content.string(3984)
        + "\r"
        + "\r"
        + __content.string(3985)
        + &g.active_entity_name
        + __content.string(3986)
        + __content.string(3987);
    if ui.msgbox(&msg_text, 8227, &s(__content.string(3988))) != 6 {
        return;
    }
    loop {
        let msg_text = s(__content.string(3989))
            + __content.string(3990)
            + &g.active_entity_name
            + __content.string(3991)
            + __content.string(3992)
            + "\r\n"
            + "\r\n";
        g.input_dlg_label = PbStr::new();
        g.input_dlg_prompt = msg_text;
        g.input_dlg_title = s(__content.string(3993)) + &g.active_entity_name;
        g.lookup_filter = PbStr::new();
        in_box_symbol(g, ui, rng);
        if g.tmp_symbol.is_empty() {
            return;
        }
        let picked = g.target_corp;
        let mut etf_id = g.target_corp;
        g.target_corp = 0;
        if 1580 < picked && picked < 1601 && g.industry_num[picked] != 71 {
            etf_id = 0;
        }
        if !(1580 < etf_id && etf_id < 1601) {
            quick(g, ui,
                s(__content.string(3994))
                    + __content.string(3995)
                    + &g.active_entity_name
                    + __content.string(3996),
            );
            continue;
        }
        if 10 < g.fund_adviser[etf_id] {
            let cur = g.fund_adviser[etf_id];
            let ctl = g.corp_controller[cur];
            if 0 < ctl && ctl < 11 && g.cur_player != ctl {
                quick(g, ui,
                    g.ent_name[cur].clone()
                        + __content.string(3997)
                        + &g.ent_name[etf_id]
                        + ". "
                        + &g.ent_name[cur]
                        + __content.string(3998)
                        + &g.ent_symbol[ctl]
                        + __content.string(3999)
                        + __content.string(4000),
                );
                continue;
            }
        }
        let cur_adviser = g.fund_adviser[etf_id];
        let mut fee_pct: f32 = cast::<f32, _>(g.fund_fee_pct[etf_id]);
        if 0.2f32 > fee_pct {
            fee_pct = 0.2f32;
            g.fund_fee_pct[etf_id] = cast::<f32, _>(Ext::from_f32(0.2f32));
        }
        calc_entity_financials(g, ui, rng, etf_id);
        let mut contract_cost = g.net_worth * cast::<Ext, _>(fee_pct) * sf(0.1f32);
        if etf_id < 1586 {
            contract_cost = contract_cost * sf(0.5f32);
        }
        calc_entity_financials(g, ui, rng, adviser_id);
        if g.liquid_funds < contract_cost {
            quick(g, ui,
                g.active_entity_name.clone()
                    + __content.string(4001)
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(contract_cost, b"* ###.##")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix,
            );
            return;
        }
        let mut word_text = PbStr::new();
        let mut adviser_name_text = PbStr::new();
        if 85 < cur_adviser {
            adviser_name_text = s(", ") + &g.ent_name[cur_adviser];
            word_text = adviser_name_text.clone() + ",";
        }
        let mut msg_text = s(__content.string(4002))
            + &word_text
            + " is: "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(contract_cost, b"* ###,.##")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "."
            + "\r"
            + "\r"
            + __content.string(4003);
        let mut warn_text = PbStr::new();
        if g.industry_num[adviser_id] == 2 {
            msg_text = msg_text + __content.string(4004);
            if g.net_worth * sf(0.5f32) < contract_cost {
                msg_text = msg_text
                    + "\r"
                    + "\r"
                    + __content.string(4005)
                    + &g.active_entity_name
                    + " ";
                let mut wt = s(__content.string(4006));
                if g.net_worth < contract_cost {
                    wt = s(__content.string(4007));
                }
                msg_text = msg_text
                    + &wt
                    + __content.string(4008)
                    + __content.string(4009)
                    + "\r"
                    + "\r"
                    + __content.string(4010);
                ui.msgbox(&msg_text, 8208, &s(__content.string(4011)));
                return;
            }
        } else {
            msg_text = msg_text
                + __content.string(4012);
            if g.business_assets[adviser_id] * Ext::from_f64(3.0) < contract_cost + g.goodwill[adviser_id] || g.net_worth < contract_cost {
                warn_text = s(__content.string(4013))
                    + __content.string(4014)
                    + __content.string(4015)
                    + &g.ent_name[adviser_id]
                    + "."
                    + "\r"
                    + "\r"
                    + __content.string(4016)
                    + &g.ent_name[adviser_id]
                    + __content.string(4017);
                warn_text = msg_text.clone() + "\r" + "\r" + &warn_text;
                if ui.msgbox(&warn_text, 8227, &s(__content.string(4018))) != 6 {
                    return;
                }
            }
        }
        if warn_text.is_empty() {
            msg_text = msg_text + "\r" + "\r" + __content.string(3089) + &g.active_entity_name + __content.string(4019);
            if 85 < cur_adviser {
                msg_text = msg_text + &adviser_name_text;
            }
            msg_text = msg_text + "?";
            let title = s(__content.string(4020)) + &g.ent_symbol[etf_id] + __content.string(4021);
            if ui.msgbox(&msg_text, 8227, &title) != 6 {
                return;
            }
        }
        if 85 < cur_adviser {
            g.cash[cur_adviser] = g.cash[cur_adviser] + contract_cost;
            g.extraordinary_pl[cur_adviser] = to_quad(cast::<Ext, _>(g.extraordinary_pl[cur_adviser]) + contract_cost);
        }
        let shortfall = contract_cost - (g.cash[adviser_id] + g.t_bills[adviser_id]);
        if Ext::from_f64(0.0) < shortfall {
            g.cash[adviser_id] = g.cash[adviser_id] + shortfall;
            g.bank_debt[adviser_id] = g.bank_debt[adviser_id] + shortfall;
            let lender = g.lender_id;
            g.cash[lender] = g.cash[lender] - shortfall;
        }
        if g.industry_num[adviser_id] == 37 {
            g.goodwill[adviser_id] = g.goodwill[adviser_id] + contract_cost;
        } else {
            g.accrued_pl[adviser_id] = g.accrued_pl[adviser_id] - contract_cost;
        }
        g.cash[adviser_id] = g.cash[adviser_id] - contract_cost;
        g.fund_adviser[etf_id] = adviser_id;
        g.ctrl_since_year[etf_id] = g.cur_year;
        if 85 < cur_adviser {
            g.news_msg = g.ent_symbol[adviser_id].clone()
                + __content.string(2062)
                + &g.ent_symbol[etf_id]
                + __content.string(4022)
                + &g.ent_symbol[cur_adviser];
            post_news(g, ui, 0, 0);
        }
        g.fund_bonus_year[etf_id] = 0;
        // (UI) DIALOG END hDlgTransMenu, 0
        g.turn_action_count += 1;
        let mut msg_text = s(__content.string(3176))
            + "\r"
            + "\r"
            + &g.active_entity_name
            + __content.string(4023)
            + &g.ent_name[etf_id]
            + __content.string(4024);
        if etf_id < 1586 {
            let mut wt = s("index");
            if etf_id < 1584 {
                wt = s("bond");
            }
            msg_text = msg_text
                + __content.string(4025)
                + &g.ent_name[etf_id]
                + " is a "
                + &wt
                + __content.string(4026)
                + __content.string(4027)
                + &g.active_entity_name
                + __content.string(4028)
                + __content.string(4029)
                + __content.string(4030);
            let title = s(__content.string(4031)) + &ucase(wt.as_bytes(), CaseMode::Ansi) + __content.string(4032);
            ui.msgbox(&msg_text, 8256, &title);
            return;
        }
        msg_text = msg_text
            + __content.string(4033)
            + &g.ent_symbol[adviser_id]
            + " will "
            + __content.string(4034)
            + __content.string(4035)
            + &g.ent_symbol[adviser_id]
            + __content.string(4036)
            + __content.string(4037)
            + __content.string(4038)
            + "\r"
            + "\r"
            + __content.string(4039)
            + __content.string(4040)
            + __content.string(4041)
            + &g.ent_symbol[adviser_id]
            + __content.string(4042)
            + __content.string(4043)
            + "\r"
            + "\r";
        msg_text = msg_text
            + &g.active_entity_name
            + __content.string(4044)
            + __content.string(4045)
            + __content.string(4046)
            + __content.string(4047)
            + &g.ent_symbol[etf_id]
            + __content.string(4048)
            + __content.string(4049)
            + &g.ent_symbol[adviser_id]
            + " "
            + __content.string(4050)
            + "\r"
            + "\r";
        let period_year;
        let wt;
        if adviser_id % 2 == 1 {
            wt = s("odd");
            period_year = if g.cur_year % 2 == 1 { g.cur_year + 1 } else { g.cur_year + 2 };
        } else {
            wt = s("even");
            period_year = if g.cur_year % 2 == 0 { g.cur_year + 1 } else { g.cur_year + 2 };
        }
        ui.msgbox(&msg_text, 8256, &s("CONFIRMATION"));
        let mut msg_text = s(__content.string(4051))
            + &g.ent_name[etf_id]
            + __content.string(4052)
            + __content.string(4053)
            + &str_s(cast::<Ext, _>(period_year))
            + __content.string(4054)
            + __content.string(4055)
            + __content.string(4056)
            + &str_s(cast::<Ext, _>(period_year))
            + " and"
            + &str_s(cast::<Ext, _>(1 + period_year))
            + __content.string(4057)
            + __content.string(4058)
            + &wt
            + __content.string(4059)
            + "\r"
            + "\r";
        if g.fund_fee_pct[etf_id] <= 0.2f32 {
            msg_text = msg_text + __content.string(4060) + &g.ent_name[etf_id] + __content.string(4061);
            ui.msgbox(&msg_text, 8256, &s(__content.string(4062)));
        } else {
            msg_text = msg_text
                + __content.string(4063)
                + &g.ent_name[etf_id]
                + __content.string(4064)
                + __content.string(4065)
                + &str_s(cast::<Ext, _>(period_year))
                + "), "
                + __content.string(4066)
                + &g.active_entity_name
                + __content.string(4067)
                + "\r"
                + "\r"
                + __content.string(4068);
            if ui.msgbox(&msg_text, 8227, &s(__content.string(4069))) == 6 {
                set_etf_advisory_fee(g, ui);
            }
        }
        return;
    }
}
