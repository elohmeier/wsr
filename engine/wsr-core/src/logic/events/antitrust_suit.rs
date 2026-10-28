//! The pending private antitrust suit (`AntitrustDefendant` / `AntitrustPlaintiff` /
//! `AntitrustMarketShare` / `AntitrustDamages` / `AntitrustSettleAmt`): the periodic tick
//! (`AntitrustSuitTick`), the trial driver (`AntitrustSuitTrial`) and the court case itself
//! (`AntitrustCaseRuling`: dismissals, plaintiff funding, settlement offers, verdict and damages).
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::earnings::{calc_projected_earnings, sell_t_bills_to_cash};
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::market::update_market_display;
use crate::logic::report::show_quick_message;
use crate::logic::valuation::{calc_stock_price, update_price_history_tick};
use crate::pb::string::{CaseMode, lcase, mcase, remove_any};

/// `RTRIM$(LTRIM$(CurrencySymbol)) + RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
fn money(g: &Globals, x: Ext, mask: &[u8]) -> PbStr {
    trim(g.currency_symbol.as_bytes()) + &fmt_trim(x, mask)
}

/// Port of AntitrustSuitTick (0x540401).
/// Runs while a private antitrust suit is pending (defendant, plaintiff and market share set, private
/// suits not switched off). The plaintiff type is 1 when a computer player controls the plaintiff, 0 when
/// a human manages it personally, else 2; for types 1/2 the suit is withdrawn when the market share is
/// below `43 + 8 * RND` (news, pop-up for the current player's companies, the plaintiff joins
/// `RecentAntitrustFilers`). If the parties are still active and the defendant's controller is no other
/// human player (`GetControllerType` of the *controller id*, 2 counts as none), the trial runs
/// ([`antitrust_suit_trial`], plaintiff type in `TmpExt1`); otherwise nothing happens this tick. Without
/// a pending suit (or once it is withdrawn / a party is gone) the suit globals are cleared.
///
/// After a withdrawal it reads `EntityStatus(0)` (error 9 without ON ERROR: dummy element) — harmless
/// (BUGS.md #143).
pub fn antitrust_suit_tick(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let mut def_controller;
    if g.antitrust_defendant != 0
        && g.antitrust_plaintiff != 0
        && g.antitrust_market_share != 0
        && g.private_antitrust_off != 1
    {
        let mut plaintiff_type = 2;
        if get_controller_type(g, g.antitrust_plaintiff) == 2 {
            plaintiff_type = 1;
        } else if 0 < get_managing_player(g, g.antitrust_plaintiff) {
            plaintiff_type = 0;
        }
        if 0 < plaintiff_type && (cast::<Ext, _>(g.antitrust_market_share)) < rng.rnd() * Ext::from_f64(8.0) + Ext::from_f64(43.0) {
            g.news_msg = st(__content.string(5054))
                + &g.ent_symbol[g.antitrust_plaintiff]
                + " vs. "
                + &g.ent_symbol[g.antitrust_defendant]
                + __content.string(5055);
            if g.cur_player == g.corp_controller[g.antitrust_plaintiff]
                || g.cur_player == g.corp_controller[g.antitrust_defendant]
            {
                let mut msg = std::mem::take(&mut g.news_msg);
                show_quick_message(g, ui, &mut msg);
                g.news_msg = msg;
                post_news(g, ui, 0, 0);
            } else {
                post_news(g, ui, g.antitrust_defendant, g.antitrust_plaintiff);
            }
            // (UI) PauseTicker(2)
            g.antitrust_defendant = 0;
            let mut stored = false;
            let mut slot = 1;
            while slot < 11 {
                if g.recent_antitrust_filers[slot] == 0 {
                    g.recent_antitrust_filers[slot] = g.antitrust_plaintiff;
                    stored = true;
                    slot = 10;
                }
                slot += 1;
            }
            if !stored {
                for slot in 1..=9 {
                    g.recent_antitrust_filers[slot] = g.recent_antitrust_filers[slot + 1];
                }
                g.recent_antitrust_filers[10] = g.antitrust_plaintiff;
            }
        }
        if g.entity_status[g.antitrust_defendant] != 1 || g.entity_status[g.antitrust_plaintiff] != 1 {
            g.antitrust_defendant = 0;
        }
        if g.antitrust_defendant != 0 {
            def_controller = g.corp_controller[g.antitrust_defendant];
            if get_controller_type(g, def_controller) == 2 {
                def_controller = 0;
            }
            if def_controller < 1 || 10 < def_controller || g.cur_player == def_controller {
                g.tmp_ext1 = cast::<Ext, _>(plaintiff_type);
                antitrust_suit_trial(g, ui, rng, time_secs);
            }
            return;
        }
    }
    g.antitrust_damages = 0;
    g.antitrust_settle_amt = 0;
    g.antitrust_plaintiff = 0;
    g.antitrust_market_share = 0;
    g.antitrust_defendant = 0;
}

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

/// Port of AntitrustSuitTrial (0x540077).
/// Takes the plaintiff type from `TmpExt1` (and clears it). Unless private suits are off: if a party is
/// no longer an active operating corp (id < 131 or status <> 1) the suit is dropped; otherwise the case
/// runs ([`antitrust_case_ruling`]). On `"OK"` the news is posted (unless it says the plaintiff
/// "drops" the suit — then the ruling already handled it), both parties' earnings are re-projected and
/// the suit globals are cleared; on `"Delay"` the "suit pending" news is posted and the defendant's cap
/// falls 5%. Both parties are then re-valued (`ValuationMode = 3`) and the market display refreshed.
/// Reproduces BUGS.md #141 (a stale `NewsMsg` is posted after early dismissals) and #142 (the "drops"
/// headline is never posted).
pub fn antitrust_suit_trial(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    // ON ERROR GOTO -> ShowRuntimeError("AntiTrust ..." ) — error path not reached.
    let plaintiff_type = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    if g.private_antitrust_off == 1 {
        return;
    }
    let mut defendant_id = g.antitrust_defendant;
    if 0 < g.antitrust_defendant && g.entity_status[g.antitrust_defendant] != 1 {
        defendant_id = 0;
    }
    let mut plaintiff_id = g.antitrust_plaintiff;
    if 0 < g.antitrust_plaintiff && g.entity_status[g.antitrust_plaintiff] != 1 {
        plaintiff_id = 0;
    }
    if defendant_id < 131 || plaintiff_id < 131 {
        g.antitrust_defendant = 0;
        g.antitrust_plaintiff = 0;
        return;
    }
    let ruling = antitrust_case_ruling(g, ui, rng, plaintiff_type);
    // (UI) PauseTicker
    if ruling == "OK" {
        if instr(1, g.news_msg.as_bytes(), b"drops") == 0 {
            post_news(g, ui, g.antitrust_plaintiff, g.antitrust_defendant);
            calc_projected_earnings(g, ui, rng, g.antitrust_defendant);
            calc_projected_earnings(g, ui, rng, g.antitrust_plaintiff);
        }
        g.antitrust_defendant = 0;
        g.antitrust_plaintiff = 0;
        g.antitrust_market_share = 0;
        g.antitrust_damages = 0;
        g.antitrust_settle_amt = 0;
    } else if ruling == "Delay" && 130 < g.antitrust_defendant {
        post_news(g, ui, g.antitrust_plaintiff, g.antitrust_defendant);
        let d = g.antitrust_defendant;
        g.market_cap[d] = g.market_cap[d] * sf(0.95f32);
        update_price_history_tick(g, ui, rng, d);
    }
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, defendant_id, time_secs);
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, plaintiff_id, time_secs);
    update_market_display(g, ui, rng, time_secs);
}

/// Port of Antitrust_RecordFiler (0x5468db), GOSUB of AntitrustCaseRuling.
/// Shifts `RecentAntitrustFilers` down one slot and stores the plaintiff in slot 10.
fn record_filer(g: &mut Globals) {
    for slot in 1..=9 {
        g.recent_antitrust_filers[slot] = g.recent_antitrust_filers[slot + 1];
    }
    g.recent_antitrust_filers[10] = g.antitrust_plaintiff;
}

/// Port of AntitrustCaseRuling (0x541713), with its error label Antitrust_ErrorHandler (0x546952).
/// The court case of the pending private antitrust suit. `mode` is the plaintiff type of
/// [`antitrust_suit_tick`] (0 = a human manages the plaintiff, 1 = computer-player plaintiff, 2 = other);
/// returns `"OK"` (case closed), `"Delay"` (the defendant belongs to another, non-current human player:
/// "suit pending" news in `NewsMsg`) or `""` (error label).
///
/// * Dismissed (OK, no costs) when the plaintiff is in `RecentAntitrustFilers`, or when both parties
///   have the same `CorpController`, 0 included (BUGS.md #140; pop-up and "withdrawn" news only in
///   mode 0).
/// * Damages: 75% of the defendant's net worth (a prior claim is kept when `mode = 0`), above
///   `250000 * ExchangeRate` 60% of net worth + contingency reserve (at least that floor, at most the
///   prior claim), at least 100. The plaintiff's legal costs are `(2.5% * law firm + 1%)` of the claim,
///   paid from cash / T-bills / its credit line (or, above 71% share in modes 1/2, a loan from its bank
///   for a 10% fee); the claim shrinks 5% steps until affordable, else the suit is dropped
///   ("drops" news, mode > 0).
/// * Settlement: a human defendant (current player) is asked (accountant's memo once, lawyers' advice
///   by market share and law firms; demand adjusted by the player's past settle/refuse history in
///   `PlayerAntitrustSettlesPaid/Refused`); a human plaintiff (mode 0) names its price (INPUTBOX); else
///   the defendant settles with probability 2/3 below 66% share.
/// * Trial: verdict for the plaintiff (judgment ~ claim x share x 1.3%), the defendant (countersuit
///   damages) or neither, by law firms, share and `RND`; cash, accrued / extraordinary P&L, contingency
///   reserves and market caps of both parties are updated; the news is posted and the plaintiff joins
///   `RecentAntitrustFilers`.
pub fn antitrust_case_ruling(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, mode: i32) -> PbStr {
    let __content = crate::content::current();
    // DIM PlayerAntitrustSettlesPaid(1 TO 5), PlayerAntitrustSettlesRefused(1 TO 5): the globals are
    // already dimensioned (the DIM keeps their contents).
    if g.antitrust_defendant < 131 || g.antitrust_plaintiff < 131 {
        return error_handler(g, ui);
    }
    let defd = g.antitrust_defendant;
    let plt = g.antitrust_plaintiff;
    let def_name = g.ent_name[defd].clone();
    let plaint_name = g.ent_name[plt].clone();
    calc_entity_financials(g, ui, rng, defd);
    let def_net_worth = g.net_worth;
    calc_entity_financials(g, ui, rng, plt);
    let plaint_net_worth = g.net_worth;
    let mut credit_line = g.unused_credit_line;
    let mut ind_name = g.ind_name[cast::<i32, _>(g.industry_num[defd])].clone();
    if 69 < g.industry_num[defd] {
        ind_name = st("NULL");
    }
    let mut plaint_law_firm = rng.rnd_range(3, 1);
    let mut def_law_firm = rng.rnd_range(3, 1);
    g.voting_controller = g.corp_controller[plt];
    if 0 < g.voting_controller && g.voting_controller <= g.num_players {
        let vc = g.voting_controller;
        if g.is_computer_player[vc] == 0 {
            if 0 < g.player_law_firm[vc] {
                plaint_law_firm = g.player_law_firm[vc];
            }
        } else {
            g.player_law_firm[vc] = 3;
            plaint_law_firm = g.player_law_firm[vc];
        }
    }
    g.voting_controller = g.corp_controller[defd];
    if 0 < g.voting_controller && g.voting_controller <= g.num_players {
        let vc = g.voting_controller;
        if g.is_computer_player[vc] == 0 {
            if 0 < g.player_law_firm[vc] {
                def_law_firm = g.player_law_firm[vc];
            }
        } else {
            g.player_law_firm[vc] = 2;
            def_law_firm = g.player_law_firm[vc];
        }
    }
    let fee_accrued_pct = cast::<i16, _>(rng.rnd_range(59, 41));
    let mut eligible = 1;
    for slot in 1..=10 {
        if g.antitrust_plaintiff == g.recent_antitrust_filers[slot] {
            eligible = 0;
        }
    }
    if eligible == 0 {
        if mode == 0 {
            ui.msgbox(
                &(st(__content.string(5056))
                    + &def_name
                    + __content.string(5057)
                    + &plaint_name
                    + __content.string(3875)
                    + __content.string(3876)),
                8256,
                &st(__content.string(5058)),
            );
        }
        return st("OK");
    }
    g.voting_controller = g.corp_controller[defd];
    if g.corp_controller[plt] == g.voting_controller {
        if mode == 0 {
            ui.msgbox(
                &(plaint_name.clone()
                    + __content.string(5059)
                    + &def_name
                    + __content.string(5060)
                    + __content.string(5061)
                    + "\r"
                    + "\r"
                    + __content.string(5062)
                    + __content.string(5063)),
                8256,
                &st(__content.string(3850)),
            );
            g.news_msg = g.ent_symbol[plt].clone()
                + __content.string(5064)
                + &g.ent_symbol[defd]
                + __content.string(5065);
            post_news(g, ui, 0, 0);
        }
        return st("OK");
    }
    let vc = g.voting_controller;
    if 0 < vc && vc < 11 {
        if g.is_computer_player[vc] == 1 {
        } else if g.cur_player != vc && 0 < mode {
            g.news_msg = st(__content.string(5066))
                + &g.ent_symbol[defd]
                + " by "
                + &g.ent_symbol[plt];
            return st("Delay");
        }
    }
    if 0 < mode {
        g.antitrust_damages = 0;
        g.antitrust_settle_amt = 0;
    }
    let mut prior_damages: i64 = 0;
    if g.antitrust_damages == 0 {
        g.antitrust_damages = to_quad(def_net_worth * sf(0.75f32));
    } else {
        prior_damages = g.antitrust_damages;
    }
    if g.exchange_rate * Ext::from_f64(250000.0) < cast::<Ext, _>(g.antitrust_damages) {
        g.antitrust_damages = to_quad((def_net_worth + g.contingency_reserve[defd]) * sf(0.6f32));
        if (cast::<Ext, _>(g.antitrust_damages)) < g.exchange_rate * Ext::from_f64(250000.0) {
            g.antitrust_damages = to_quad(g.exchange_rate * Ext::from_f64(250000.0));
        }
        if 0 < prior_damages && prior_damages < g.antitrust_damages {
            g.antitrust_damages = prior_damages;
        }
    }
    if g.antitrust_damages < 100 {
        g.antitrust_damages = 100;
    }
    let mut plaint_funds = credit_line + g.cash[plt] + g.t_bills[plt];
    if g.antitrust_market_share < 72 && plaint_net_worth * sf(0.9f32) < plaint_funds {
        plaint_funds = plaint_net_worth * sf(0.9f32);
    }
    if plaint_funds < g.exchange_rate * Ext::from_f64(10.0) {
        plaint_funds = g.exchange_rate * Ext::from_f64(10.0);
    }
    let mut plaint_legal_cost =
        (cast::<Ext, _>(plaint_law_firm) * sf(0.025f32) + sf(0.01f32)) * cast::<Ext, _>(g.antitrust_damages);
    let mut plaint_fees_so_far = plaint_legal_cost * cast::<Ext, _>(fee_accrued_pct) * sf(0.01f32);
    if 75 < g.antitrust_market_share
        && 0 < mode
        && plaint_funds < plaint_legal_cost
        && 2 < g.credit_rating_num[plt]
        && 10 < g.corp_controller[g.lender_id]
    {
        plaint_funds = plaint_legal_cost + sf(0.01f32);
    }
    if g.exchange_rate * Ext::from_f64(50.0) < plaint_funds {
        while plaint_funds < plaint_legal_cost {
            g.antitrust_damages = to_quad(cast::<Ext, _>(g.antitrust_damages) * sf(0.95f32));
            plaint_legal_cost =
                (cast::<Ext, _>(plaint_law_firm) * sf(0.025f32) + sf(0.01f32)) * cast::<Ext, _>(g.antitrust_damages);
            plaint_fees_so_far = plaint_legal_cost * cast::<Ext, _>(fee_accrued_pct) * sf(0.01f32);
        }
    }
    if plaint_funds < plaint_legal_cost {
        if mode < 1 {
            ui.msgbox(
                &(g.ent_symbol[plt].clone()
                    + __content.string(5067)
                    + "\r"
                    + "\r"
                    + __content.string(5068)
                    + &g.ent_symbol[defd]
                    + __content.string(5069)),
                8256,
                &st(__content.string(5070)),
            );
        } else {
            g.news_msg = g.ent_symbol[plt].clone()
                + __content.string(5071)
                + &def_name
                + " ("
                + &g.ent_symbol[defd]
                + ")";
        }
        record_filer(g);
        return st("OK");
    }
    let mut lender_msg = PbStr::new();
    let mut lender_loan: Ext = Ext::from_f64(0.0);
    let mut shortfall = plaint_legal_cost - g.cash[plt];
    if Ext::from_f64(0.0) < shortfall {
        sell_t_bills_to_cash(g, plt, shortfall);
    }
    shortfall = plaint_legal_cost - g.cash[plt];
    if Ext::from_f64(0.0) < shortfall && Ext::from_f64(0.0) < credit_line {
        if shortfall <= credit_line {
            credit_line = credit_line - shortfall;
        } else {
            shortfall = credit_line;
            credit_line = Ext::from_f64(0.0);
        }
        let lender = g.lender_id;
        g.cash[plt] = g.cash[plt] + shortfall;
        g.cash[lender] = g.cash[lender] - shortfall;
        g.bank_debt[plt] = g.bank_debt[plt] + shortfall;
    }
    shortfall = plaint_legal_cost - g.cash[plt];
    'funding: {
        if sf(0.01f32) < shortfall && credit_line <= Ext::from_f64(0.0) && 71 < g.antitrust_market_share && 0 < mode {
            let lender = g.lender_id;
            if g.corp_controller[defd] == g.corp_controller[lender]
                && get_controller_type(g, defd) == 1
                && 0 < get_managing_player(g, lender)
            {
                break 'funding;
            }
            lender_loan = shortfall;
            g.cash[plt] = g.cash[plt] + lender_loan;
            g.cash[lender] = g.cash[lender] - lender_loan;
            g.bank_debt[plt] = g.bank_debt[plt] + lender_loan;
            shortfall = Ext::from_f64(0.0);
            lender_msg = g.ent_name[lender].clone()
                + " ("
                + &g.ent_symbol[lender]
                + __content.string(5072)
                + &g.ent_symbol[plt]
                + __content.string(5073);
            if g.corp_controller[defd] == g.corp_controller[lender] && get_controller_type(g, defd) == 1 {
                lender_msg = PbStr::new();
            }
        }
    }
    if shortfall <= Ext::from_f64(0.0) {
        g.cash[plt] = g.cash[plt] - plaint_legal_cost;
    } else {
        let lender = g.lender_id;
        g.cash[plt] = Ext::from_f64(0.0);
        g.cash[lender] = g.cash[lender] - shortfall;
        g.bank_debt[plt] = g.bank_debt[plt] + shortfall;
    }
    if g.antitrust_market_share < 1 {
        g.antitrust_market_share = 1;
    }
    let market_share = g.antitrust_market_share;
    let mut odds_base: i32 = 5;
    if plaint_law_firm == 2 {
        odds_base = 13;
    }
    if plaint_law_firm == 3 {
        odds_base = 22;
    }
    if def_law_firm == 1 {
        odds_base = to_long(cast::<Ext, _>(odds_base) * sf(1.15f32));
    }
    if def_law_firm == 3 {
        odds_base = to_long(cast::<Ext, _>(odds_base) * sf(0.85f32));
    }
    let mut offer_amt: i64 = cast::<i64, _>(rng.rnd_range(odds_base, 1));
    if mode == 1 {
        g.antitrust_market_share = g.antitrust_market_share + g.difficulty_level * 3;
    }
    if mode == 0 && 30 < g.antitrust_market_share {
        g.antitrust_market_share = g.antitrust_market_share - 3;
    }
    if g.antitrust_market_share < 1 {
        g.antitrust_market_share = 1;
    }
    if 50 < g.antitrust_market_share {
        offer_amt = to_quad(cast::<Ext, _>(offer_amt) + (cast::<Ext, _>(g.antitrust_market_share) - Ext::from_f64(40.0)) * sf(0.3f32));
    }
    let mut settle_amt = to_quad(cast::<Ext, _>(offer_amt) * sf(0.01f32) * cast::<Ext, _>(g.antitrust_damages));
    if 64 < g.antitrust_market_share && (cast::<Ext, _>(settle_amt)) < cast::<Ext, _>(g.antitrust_damages) * sf(0.35f32) {
        settle_amt = to_quad(cast::<Ext, _>(settle_amt) * Ext::from_f64(2.0));
    }
    let mut def_player: i64 = cast::<i64, _>(g.corp_controller[defd]);
    if def_player != cast::<i64, _>(g.cur_player) {
        def_player = 0;
    }
    if def_player == cast::<i64, _>(g.cur_player) && g.is_computer_player[cast::<i32, _>(def_player)] == 1 {
        def_player = 0;
    }
    let damages_text = money(g, cast::<Ext, _>(g.antitrust_damages), b"######,")
        + " "
        + &g.million_prefix_uc
        + "."
        + &g.currency_suffix;
    let mut def_legal_cost: Ext = Ext::from_f64(0.0);
    let mut def_fees_so_far: Ext = Ext::from_f64(0.0);
    let answer: i32;
    'decided: {
        if def_player <= 0 {
            let mut plaint_player: i64 = cast::<i64, _>(g.corp_controller[plt]);
            if plaint_player <= cast::<i64, _>(g.num_players) {
                if 0 < plaint_player && g.is_computer_player[cast::<i32, _>(plaint_player)] == 1 {
                    plaint_player = 0;
                }
            } else {
                plaint_player = 0;
            }
            if plaint_player == cast::<i64, _>(g.cur_player) && mode != 1 {
                offer_amt = 0;
                if 0 < g.antitrust_settle_amt {
                    offer_amt = g.antitrust_settle_amt;
                    if g.antitrust_market_share < 43 {
                        let damages = cast::<Ext, _>(g.antitrust_damages);
                        let r = rng.rnd();
                        if (cast::<Ext, _>(g.antitrust_settle_amt)) < (r * sf(0.1f32) + sf(0.2f32)) * damages {
                            offer_amt = to_quad(cast::<Ext, _>(settle_amt) + Ext::from_f64(1.0));
                        }
                    }
                }
                if offer_amt <= 0 {
                    let text = st(__content.string(5074))
                        + __content.string(5075)
                        + __content.string(5076)
                        + &plaint_name
                        + " ("
                        + &g.ent_symbol[plt]
                        + "), "
                        + __content.string(3849)
                        + &def_name
                        + " ("
                        + &g.ent_symbol[defd]
                        + ")."
                        + "\r"
                        + "\r"
                        + __content.string(5077)
                        + &plaint_name
                        + __content.string(5078)
                        + __content.string(5079)
                        + &mcase(ind_name.as_bytes(), CaseMode::Ansi)
                        + " "
                        + __content.string(5080)
                        + __content.string(5081)
                        + &str_s(cast::<Ext, _>(g.antitrust_market_share))
                        + __content.string(5082)
                        + "\r"
                        + "\r"
                        + __content.string(5083)
                        + __content.string(5084)
                        + "\r"
                        + "\r"
                        + __content.string(5085)
                        + &plaint_name
                        + __content.string(5086)
                        + __content.string(5087)
                        + &money(g, plaint_fees_so_far, b"######,")
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix
                        + __content.string(5088)
                        + &g.ent_symbol[defd]
                        + __content.string(5089)
                        + &plaint_name
                        + __content.string(5090)
                        + __content.string(5091)
                        + __content.string(5092)
                        + &plaint_name
                        + __content.string(5093)
                        + "\r"
                        + "\r"
                        + __content.string(5094);
                    ui.msgbox(&text, 8256, &st(__content.string(5095)));
                }
                while offer_amt <= 0 {
                    let msg_text = st(__content.string(5096))
                        + &plaint_name
                        + __content.string(5097)
                        + __content.string(5098)
                        + &mcase(damages_text.as_bytes(), CaseMode::Ansi)
                        + ". "
                        + __content.string(5099)
                        + &g.ent_symbol[plt]
                        + __content.string(5100)
                        + &g.ent_symbol[defd]
                        + __content.string(5101)
                        + &g.ent_symbol[plt]
                        + __content.string(5102)
                        + &money(g, plaint_fees_so_far, b"######,")
                        + " "
                        + &g.million_prefix_uc
                        + "."
                        + &g.currency_suffix
                        + ")";
                    let amount_text =
                        ui.inputbox(&msg_text, &st(__content.string(5103)), &PbStr::new());
                    offer_amt = to_quad(val(remove_any(amount_text.as_bytes(), b"$ ,%").as_bytes()));
                }
                let msg_text;
                let a;
                if offer_amt <= settle_amt {
                    msg_text = st(__content.string(5104))
                        + &def_name
                        + ";"
                        + "\r"
                        + "\r"
                        + __content.string(5105)
                        + &plaint_name
                        + __content.string(5106)
                        + &money(g, cast::<Ext, _>(offer_amt), b"#######,")
                        + " "
                        + &g.million_prefix_uc
                        + ".";
                    a = 6;
                    settle_amt = offer_amt;
                } else {
                    msg_text = plaint_name.clone()
                        + __content.string(5107)
                        + &def_name
                        + "."
                        + "\r"
                        + "\r"
                        + __content.string(5108)
                        + &def_name
                        + __content.string(5109);
                    a = 7;
                }
                ui.msgbox(&msg_text, 8256, &st(__content.string(3850)));
                answer = a;
            } else {
                // LAB_005449ea: the computer decides for the defendant.
                if g.antitrust_market_share < 66 && Ext::from_f64(1.0) <= rng.rnd() * Ext::from_f64(3.0) {
                    answer = 6;
                    break 'decided;
                }
                answer = 7;
            }
        } else {
            let p = cast::<i32, _>(def_player);
            let paid = |g: &Globals| g.player_antitrust_settles_paid[p];
            let refused = |g: &Globals| g.player_antitrust_settles_refused[p];
            if paid(g) < 4 || paid(g) <= refused(g) * 3 {
                // LAB_00542cf1
                if 3 < refused(g) && paid(g) * 3 < refused(g) {
                    if refused(g) < 7 {
                        settle_amt = to_quad(cast::<Ext, _>(settle_amt) * sf(0.8f32));
                    } else {
                        settle_amt = to_quad(cast::<Ext, _>(settle_amt) * sf(0.6f32));
                    }
                    if cast::<Ext, _>(g.antitrust_damages) * sf(0.1f32) < cast::<Ext, _>(settle_amt) && paid(g) * 8 < refused(g) {
                        settle_amt = to_quad(cast::<Ext, _>(g.antitrust_damages) * sf(0.035f32));
                    }
                }
            } else {
                if paid(g) < 7 {
                    settle_amt = to_quad(cast::<Ext, _>(settle_amt) * sf(1.25f32));
                } else {
                    settle_amt = to_quad(cast::<Ext, _>(settle_amt) * sf(1.5f32));
                }
                if (cast::<Ext, _>(settle_amt)) < cast::<Ext, _>(g.antitrust_damages) * sf(0.2f32) && refused(g) * 8 < paid(g) {
                    let damages = cast::<Ext, _>(g.antitrust_damages);
                    let r = rng.rnd();
                    settle_amt = to_quad((r * sf(0.1f32) + sf(0.15f32)) * damages);
                }
            }
            ui.msgbox(
                &(st(__content.string(5110))
                    + &def_name
                    + " ("
                    + &g.ent_symbol[defd]
                    + __content.string(5111)
                    + __content.string(5112)
                    + "\r"
                    + "\r"
                    + __content.string(5094)),
                8208,
                &(st(__content.string(5113)) + &def_name),
            );
            if 0 < g.antitrust_settle_amt && g.antitrust_settle_amt <= g.antitrust_damages {
                let plaint_ctrl: i64 = cast::<i64, _>(g.corp_controller[plt]);
                if 1 < plaint_ctrl && plaint_ctrl < 11 && g.is_computer_player[cast::<i32, _>(plaint_ctrl)] == 0 {
                    settle_amt = g.antitrust_settle_amt;
                }
            }
            let mut msg_text;
            if g.antitrust_memo_shown == 0 {
                msg_text = def_name.clone()
                    + __content.string(5114)
                    + &plaint_name
                    + " "
                    + "("
                    + &g.ent_symbol[plt]
                    + __content.string(5115)
                    + "\r"
                    + "\r"
                    + __content.string(5116)
                    + &def_name
                    + " pays "
                    + &g.ent_symbol[plt]
                    + __content.string(5117)
                    + __content.string(5118)
                    + __content.string(5119)
                    + &def_name;
                if Ext::from_f64(5.0) < g.contingency_reserve[defd] {
                    msg_text = msg_text
                        + __content.string(5120);
                }
                msg_text = msg_text + "." + "\r" + "\r";
                msg_text = msg_text
                    + __content.string(5121)
                    + &g.ent_symbol[plt]
                    + __content.string(5122)
                    + &def_name
                    + __content.string(5123)
                    + &def_name
                    + __content.string(5124)
                    + __content.string(5125)
                    + &def_name
                    + __content.string(5093)
                    + "\r"
                    + "\r"
                    + __content.string(5126);
                ui.msgbox(&msg_text, 8256, &(st(__content.string(5127)) + &def_name + " <<<"));
                g.antitrust_memo_shown = 1;
            }
            if g.antitrust_memo_shown == 1 {
                msg_text = plaint_name.clone()
                    + __content.string(5128)
                    + &def_name
                    + " for "
                    + &damages_text
                    + __content.string(5129);
            } else {
                msg_text = plaint_name.clone() + __content.string(5130);
            }
            msg_text = msg_text
                + __content.string(5131)
                + &money(g, cast::<Ext, _>(settle_amt), b"######,")
                + " "
                + &g.million_prefix_lc
                + "llion"
                + &g.currency_suffix
                + __content.string(5132)
                + &def_name
                + "."
                + "\r"
                + "\r"
                + __content.string(5133)
                + __content.string(5134)
                + &str_s(cast::<Ext, _>(market_share))
                + "% "
                + __content.string(5135);
            if ind_name == "NULL" {
                msg_text = msg_text + ".";
            } else {
                msg_text = msg_text + __content.string(1110) + &ind_name + __content.string(1547);
            }
            msg_text = msg_text + "\r" + "\r";
            let mut advice = PbStr::new();
            let ms = market_share;
            if 65 < ms {
                advice = st(__content.string(5136));
            }
            if ms < 66 {
                advice = st(__content.string(5137));
            }
            if ms < 63 {
                advice = st(__content.string(5138));
            }
            if ms < 60 {
                advice = st(__content.string(5139));
                if 57 < ms && plaint_law_firm == 3 {
                    advice = advice
                        + __content.string(5140);
                }
            }
            if ms < 58 {
                advice = st(__content.string(5141));
            }
            if ms < 54 {
                advice = replace(advice.as_bytes(), __content.bytes(5142), __content.bytes(5143));
            }
            if ms < 52 {
                advice = st(__content.string(5144));
            }
            if ms < 50 {
                advice = st(__content.string(5145));
            }
            if ms < 48 {
                advice = st(__content.string(5146));
            }
            if ms < 40 {
                advice = st(__content.string(5147));
            }
            if ms < 35 {
                advice = st(__content.string(5148));
            }
            if ms < 30 {
                advice = st(__content.string(5149))
                    + __content.string(5150);
            }
            if !(ms < 56 || plaint_law_firm != 3) && instr(1, advice.as_bytes(), __content.bytes(5151)) == 0 {
                advice = advice
                    + " "
                    + &plaint_name
                    + __content.string(5152);
                if ms < 58 {
                    advice = advice + __content.string(1387);
                }
                advice = advice + ".";
            } else if ms < 60 || plaint_law_firm != 2 {
                if ms < 56 && plaint_law_firm == 1 {
                    advice = advice
                        + __content.string(5153);
                    if g.rand_seed % 5 < 3 {
                        advice = replace(advice.as_bytes(), __content.bytes(5154), __content.bytes(5155));
                    }
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        advice = replace(advice.as_bytes(), b"cut-", b"second-");
                    }
                } else if ms < 54 && plaint_law_firm == 2 {
                    advice = advice
                        + __content.string(5156)
                        + __content.string(5157);
                }
            } else {
                advice = advice
                    + __content.string(5158)
                    + __content.string(5159);
            }
            def_legal_cost = (cast::<Ext, _>(def_law_firm) * sf(0.025f32) + sf(0.01f32)) * cast::<Ext, _>(g.antitrust_damages);
            def_fees_so_far = def_legal_cost * cast::<Ext, _>(fee_accrued_pct) * sf(0.01f32);
            advice = advice + "\r" + "\r";
            let mln = lcase(g.million_prefix_uc.as_bytes(), CaseMode::Ansi);
            if !advice.is_empty() && cast::<Ext, _>(settle_amt) * sf(0.1f32) < g.contingency_reserve[defd] {
                advice = advice
                    + __content.string(5160)
                    + &def_name
                    + __content.string(5161)
                    + "of "
                    + &money(g, g.contingency_reserve[defd], b"######,")
                    + " "
                    + &mln
                    + "illion"
                    + &g.currency_suffix
                    + ". ";
            }
            advice = advice
                + &def_name
                + __content.string(5162)
                + &money(g, def_fees_so_far, b"######,")
                + " "
                + &mln
                + "illion"
                + &g.currency_suffix
                + ", "
                + __content.string(5163)
                + &money(g, def_legal_cost - def_fees_so_far, b"######,")
                + " "
                + &mln
                + "illion"
                + &g.currency_suffix
                + " in "
                + __content.string(5164);
            if !advice.is_empty() {
                msg_text = msg_text + __content.string(5165) + &advice + "\r" + "\r";
            }
            msg_text = msg_text
                + __content.string(5166)
                + &def_name
                + __content.string(5167)
                + __content.string(5168)
                + &money(g, cast::<Ext, _>(settle_amt), b"######,")
                + " "
                + &g.million_prefix_uc
                + "ILLION"
                + &g.currency_suffix
                + " "
                + __content.string(5169);
            answer = ui.msgbox(&msg_text, 8228, &st(__content.string(5170)));
            if answer == 6 {
                let text = st(__content.string(5171))
                    + "\r"
                    + "\r"
                    + &def_name
                    + __content.string(5172)
                    + &plaint_name
                    + __content.string(5173)
                    + &money(g, cast::<Ext, _>(settle_amt), b"######,")
                    + " "
                    + &mln
                    + "illion"
                    + &g.currency_suffix
                    + ", "
                    + __content.string(5174)
                    + &plaint_name
                    + __content.string(5175);
                ui.msgbox(&text, 8256, &st(__content.string(5176)));
                if market_share < 58 {
                    g.player_antitrust_settles_paid[p] += 1;
                }
            } else if market_share < 58 {
                g.player_antitrust_settles_refused[p] += 1;
            }
        }
    }
    // LAB_00544a2b
    if answer == 6 {
        def_legal_cost = def_fees_so_far;
    }
    let tax_keep = |g: &Globals| (Ext::from_f64(100.0) - cast::<Ext, _>(g.corp_tax_rate)) * sf(0.01f32);
    g.cash[defd] = g.cash[defd] - def_legal_cost;
    g.accrued_pl[defd] = g.accrued_pl[defd] - def_legal_cost;
    if Ext::from_f64(0.0) < g.contingency_reserve[defd] {
        let mut reserve_offset = tax_keep(g) * def_legal_cost;
        if g.contingency_reserve[defd] < reserve_offset {
            reserve_offset = g.contingency_reserve[defd];
        }
        if Ext::from_f64(0.0) < reserve_offset {
            g.contingency_reserve[defd] = g.contingency_reserve[defd] - reserve_offset;
            g.equity_income_ytd[defd] = g.equity_income_ytd[defd] + reserve_offset;
        }
    }
    let lender = g.lender_id;
    let mut judgment: i64 = 0;
    let mut amount_text = PbStr::new();
    if answer == 6 {
        let settle = cast::<Ext, _>(settle_amt);
        g.cash[defd] = g.cash[defd] - settle;
        g.accrued_pl[defd] = g.accrued_pl[defd] - settle;
        if Ext::from_f64(0.0) < g.contingency_reserve[defd] {
            let mut reserve_use = tax_keep(g) * settle;
            if g.contingency_reserve[defd] < reserve_use {
                reserve_use = g.contingency_reserve[defd];
            }
            g.contingency_reserve[defd] = g.contingency_reserve[defd] - reserve_use;
            if Ext::from_f64(0.0) < reserve_use {
                g.equity_income_ytd[defd] = g.equity_income_ytd[defd] + reserve_use;
            }
        }
        g.cash[plt] = g.cash[plt] + settle;
        g.accrued_pl[plt] = g.accrued_pl[plt] + settle;
        g.accrued_pl[plt] = g.accrued_pl[plt] - plaint_fees_so_far;
        g.cash[plt] = g.cash[plt] + (plaint_legal_cost - plaint_fees_so_far);
        if 0 < mode && g.exchange_rate * Ext::from_f64(1000.0) < settle - plaint_fees_so_far && g.rand_seed % 3 < 2 {
            let reserve_add = (settle - plaint_fees_so_far) * sf(0.25f32) * sf(0.01f32) * cast::<Ext, _>(g.corp_tax_rate);
            g.equity_income_ytd[plt] = g.equity_income_ytd[plt] - reserve_add;
            g.contingency_reserve[plt] = g.contingency_reserve[plt] + reserve_add;
        }
        g.news_msg = g.ent_symbol[defd].clone()
            + __content.string(5177)
            + &g.ent_symbol[plt]
            + " "
            + &money(g, settle, b"* ###,")
            + " "
            + &g.million_prefix_uc
            + "."
            + &g.currency_suffix;
        if lender_loan <= Ext::from_f64(0.0) {
            if Ext::from_f64(0.0) < shortfall && shortfall < g.cash[plt] && shortfall <= g.bank_debt[plt] {
                g.cash[plt] = g.cash[plt] - shortfall;
                g.cash[lender] = g.cash[lender] + shortfall;
                g.bank_debt[plt] = g.bank_debt[plt] - shortfall;
            }
        } else {
            repay_lender_loan(g, ui, &mut lender_loan, &mut lender_msg, false);
        }
        g.market_cap[plt] = g.market_cap[plt] + settle * sf(0.01f32);
        g.market_cap[defd] = g.market_cap[defd] * sf(0.93f32);
    } else {
        odds_base = 25;
        if plaint_law_firm == 2 {
            odds_base = 12;
        }
        if plaint_law_firm == 3 {
            odds_base = 8;
        }
        if def_law_firm == 1 {
            odds_base = to_long(cast::<Ext, _>(odds_base) * sf(0.85f32));
        }
        if def_law_firm == 3 {
            odds_base = to_long(cast::<Ext, _>(odds_base) * sf(1.15f32));
        }
        let ms = g.antitrust_market_share;
        let odds_span = to_quad(cast::<Ext, _>(odds_base) / (cast::<Ext, _>(ms) * sf(0.01f32)));
        let odds_roll = to_quad(rng.rnd() * cast::<Ext, _>(odds_span) + Ext::from_f64(1.0));
        let mut verdict: i64 = if odds_roll == 4 || 65 < ms { 1 } else { 0 };
        if odds_roll < 12 && 59 < ms {
            verdict = 1;
        }
        if odds_roll < 10 && 56 < ms {
            verdict = 1;
        }
        if odds_roll < 8 && 53 < ms {
            verdict = 1;
        }
        if odds_roll < 4 && 49 < ms {
            verdict = 1;
        }
        if plaint_law_firm == 1 && ms < 50 {
            if odds_roll < 7 {
                verdict = 2;
            }
            if ms < 35 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
                verdict = 2;
            }
            if ms < 24 {
                verdict = 2;
            }
        }
        if ms < 38 {
            if g.rand_seed % 3 == 2 && plaint_law_firm < 3 {
                verdict = 2;
            } else if rng.rnd() * Ext::from_f64(10.0) < Ext::from_f64(1.0) && plaint_law_firm == 3 && ms < 33 {
                verdict = 2;
            }
        }
        if ms < 18 {
            verdict = 2;
        }
        if 0 < verdict {
            if verdict == 1 {
                judgment = to_quad(cast::<Ext, _>(g.antitrust_damages) * cast::<Ext, _>(ms) * sf(0.013f32));
                if plaint_law_firm == 1 {
                    judgment = to_quad(cast::<Ext, _>(judgment) * sf(0.57f32));
                }
                if plaint_law_firm == 2 {
                    judgment = to_quad(cast::<Ext, _>(judgment) * sf(0.83f32));
                }
                if def_law_firm == 3 {
                    judgment = to_quad(cast::<Ext, _>(judgment) * sf(0.75f32));
                }
                if def_law_firm == 1 {
                    judgment = to_quad(cast::<Ext, _>(judgment) * sf(1.2f32));
                }
                if Ext::from_f64(0.0) < lender_loan {
                    repay_lender_loan(g, ui, &mut lender_loan, &mut lender_msg, true);
                }
            } else {
                judgment = to_quad(
                    cast::<Ext, _>(to_quad(cast::<Ext, _>(g.antitrust_damages) * (Ext::from_f64(101.0) - cast::<Ext, _>(ms)) * sf(0.01f32)))
                        * cast::<Ext, _>(def_law_firm)
                        * sf(0.25f32),
                );
                if plaint_net_worth < cast::<Ext, _>(judgment) {
                    judgment = to_quad(plaint_net_worth);
                    if judgment <= 0 {
                        judgment = to_quad(g.exchange_rate * Ext::from_f64(15.0));
                    }
                }
            }
            let j = cast::<Ext, _>(judgment);
            let r = rng.rnd();
            judgment = to_quad(j * r * sf(0.5f32) + j * sf(0.6f32));
            amount_text = money(g, cast::<Ext, _>(judgment), b"######,")
                + " "
                + &g.million_prefix_uc
                + "ILLION"
                + &g.currency_suffix;
        }
        let mut msg_text;
        if verdict == 1 {
            msg_text = st(__content.string(5178))
                + &plaint_name
                + __content.string(5179)
                + "\r"
                + "\r"
                + __content.string(5180)
                + &g.ent_symbol[plt]
                + __content.string(5181)
                + &amount_text
                + __content.string(5182)
                + &def_name
                + " ("
                + &g.ent_symbol[defd]
                + ")."
                + "\r"
                + "\r"
                + __content.string(5183)
                + __content.string(5184)
                + &def_name
                + " "
                + __content.string(5185)
                + __content.string(5186)
                + &mcase(ind_name.as_bytes(), CaseMode::Ansi)
                + __content.string(4980)
                + "with a"
                + &str_s(cast::<Ext, _>(market_share))
                + __content.string(5187);
            if g.rand_seed % 2 == 1 {
                msg_text = replace(msg_text.as_bytes(), __content.bytes(5188), __content.bytes(5189));
            }
            g.news_msg = g.ent_symbol[plt].clone()
                + " wins "
                + &amount_text
                + __content.string(5190)
                + &g.ent_symbol[defd];
            g.market_cap[plt] = g.market_cap[plt] + cast::<Ext, _>(judgment) * sf(0.01f32);
            g.market_cap[defd] = g.market_cap[defd] * sf(0.78f32);
        } else if verdict == 2 {
            msg_text = st(__content.string(5191))
                + __content.string(5192)
                + &def_name
                + " "
                + __content.string(5193)
                + &amount_text
                + __content.string(5194)
                + &plaint_name
                + "."
                + "\r"
                + "\r";
            if plaint_law_firm == 1 {
                msg_text = msg_text
                    + __content.string(5195)
                    + __content.string(5196);
            }
            g.news_msg = g.ent_symbol[defd].clone()
                + __content.string(5197)
                + &amount_text
                + __content.string(5198)
                + &g.ent_symbol[plt];
            g.market_cap[plt] = g.market_cap[plt] * sf(0.78f32);
            g.market_cap[defd] = g.market_cap[defd] + cast::<Ext, _>(judgment) * sf(0.01f32);
        } else {
            msg_text = st(__content.string(5199))
                + __content.string(5200)
                + &plaint_name
                + __content.string(5201)
                + &def_name
                + ".";
            g.news_msg = g.ent_symbol[plt].clone()
                + __content.string(5202)
                + &g.ent_symbol[defd];
            g.market_cap[plt] = g.market_cap[plt] * sf(0.9f32);
            g.market_cap[defd] = g.market_cap[defd] * sf(0.87f32);
        }
        if mode == 0 || g.cur_player == g.corp_controller[defd] || g.cur_player == g.corp_controller[plt] {
            ui.msgbox(&msg_text, 8256, &st(__content.string(5203)));
        }
        g.accrued_pl[plt] = g.accrued_pl[plt] - plaint_legal_cost;
        if 0 < verdict {
            if verdict == 2 {
                judgment = -judgment;
            }
            let j = cast::<Ext, _>(judgment);
            g.cash[defd] = g.cash[defd] - j;
            g.extraordinary_pl[defd] = to_quad(cast::<Ext, _>(g.extraordinary_pl[defd]) - j);
            if verdict == 1 && Ext::from_f64(0.0) < g.contingency_reserve[defd] {
                let mut reserve_use = tax_keep(g) * j;
                if g.contingency_reserve[defd] < reserve_use {
                    reserve_use = g.contingency_reserve[defd];
                }
                g.contingency_reserve[defd] = g.contingency_reserve[defd] - reserve_use;
                if Ext::from_f64(0.0) < reserve_use {
                    g.extra_items_ytd[defd] = g.extra_items_ytd[defd] + reserve_use;
                }
            }
            g.cash[plt] = g.cash[plt] + j;
            g.extraordinary_pl[plt] = to_quad((cast::<Ext, _>(g.extraordinary_pl[plt]) + j) - plaint_legal_cost);
            if verdict == 1 {
                g.accrued_pl[plt] = g.accrued_pl[plt] + plaint_legal_cost;
            } else {
                g.extraordinary_pl[plt] = to_quad(cast::<Ext, _>(g.extraordinary_pl[plt]) + plaint_legal_cost);
                g.extraordinary_pl[defd] = to_quad(cast::<Ext, _>(g.extraordinary_pl[defd]) - def_legal_cost);
                g.accrued_pl[defd] = g.accrued_pl[defd] + def_legal_cost;
            }
        }
    }
    if mode < 1 {
        post_news(g, ui, 0, 0);
    } else {
        post_news(g, ui, g.antitrust_plaintiff, g.antitrust_defendant);
    }
    record_filer(g);
    st("OK")
}

/// The plaintiff pays its bank's 10% fee on a suit loan (`lenderLoan`; to accrued P&L after a
/// settlement, `trial` = to extraordinary P&L after a won trial), cashes in its T-bills and repays what
/// it can of the loan; the lender's message pops up then. (Inline twice in AntitrustCaseRuling.)
fn repay_lender_loan(g: &mut Globals, ui: &mut dyn Ui, lender_loan: &mut Ext, lender_msg: &mut PbStr, trial: bool) {
    let plt = g.antitrust_plaintiff;
    let lender = g.lender_id;
    *lender_loan = *lender_loan * sf(0.1f32);
    g.cash[plt] = g.cash[plt] - *lender_loan;
    if trial {
        g.extraordinary_pl[plt] = to_quad(cast::<Ext, _>(g.extraordinary_pl[plt]) - *lender_loan);
    } else {
        g.accrued_pl[plt] = g.accrued_pl[plt] - *lender_loan;
    }
    g.cash[lender] = g.cash[lender] + *lender_loan;
    g.accrued_pl[lender] = g.accrued_pl[lender] + *lender_loan;
    *lender_loan = *lender_loan * Ext::from_f64(10.0);
    if Ext::from_f64(0.0) < g.t_bills[plt] {
        g.cash[plt] = g.cash[plt] + g.t_bills[plt];
        g.t_bills[plt] = Ext::from_f64(0.0);
    }
    if Ext::from_f64(0.0) < g.cash[plt] {
        if g.cash[plt] < *lender_loan {
            *lender_loan = g.cash[plt];
        }
        if *lender_loan <= g.cash[plt] {
            g.cash[plt] = g.cash[plt] - *lender_loan;
            g.cash[lender] = g.cash[lender] + *lender_loan;
            g.bank_debt[plt] = g.bank_debt[plt] - *lender_loan;
        }
        if !lender_msg.is_empty() {
            show_quick_message(g, ui, lender_msg);
        }
    }
}

/// Antitrust_ErrorHandler (0x546952): the ON ERROR label of AntitrustCaseRuling (also used when a party
/// id is below 131 at entry): `MSGBOX " [runtime resource wording] " + STR$(ERR) + ...` and return the
/// (empty) result.
fn error_handler(g: &mut Globals, ui: &mut dyn Ui) -> PbStr {
    let __content = crate::content::current();
    ui.msgbox(
        &(st(__content.string(5204)) + &str_s(Ext::from_f64(0.0)) + __content.string(5205) + &str_s(cast::<Ext, _>(g.antitrust_defendant))),
        0,
        &PbStr::new(),
    );
    PbStr::new()
}
