//! Voting control: `UpdateCorpControl` (0x41f71b) with its GOSUB `UpdateCorpControl_CalcHolders`
//! (0x4207fb), and the helpers `SanitizeCorpGrowthSpending` (0x4a14a7) and `FindOptionPositions`
//! (0x5f0cb9).
//!
//! Control rule (CalcHolders): every holder's stake is credited to its own controller (a company held by
//! a player counts as that player's), the group with the largest combined stake above 19% (15% for
//! investment funds, industry 71) controls the company; a tie at the top means nobody does, and with no
//! controlling group the company controls itself. With more than 80% held by the public (non-funds) the
//! company is always self-controlled.

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::assign_op_pattern, clippy::collapsible_if, clippy::collapsible_else_if, clippy::if_same_then_else)]
use crate::pb::real::{Ext, cast};


use crate::logic::holdings::find_stock_holding;
use crate::logic::news::post_news;
use crate::logic::report::show_quick_message;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, mcase, parse_any, parse_count, CaseMode};
use crate::pb::numeric::{to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// The locals of UpdateCorpControl, shared with its GOSUB CalcHolders (zeroed at entry).
#[derive(Default)]
struct Frame {
    corp_id: i32,
    holder_id: i32,
    slot: i32,
    group_idx: i32,
    old_controller: i32,
    phrase_text: PbStr,
    msg_text: PbStr,
    /// `[ebp-0xb8]`: an investment fund (industry 71) has a >15% holder before any 20% holder was found.
    lost_exempt: i32,
    max_pct: i16,
    owner_id: i32,
    hold_pct: i16,
    holder_parent: i32,
    found: i32,
    adviser_id: i32,
    /// `DIM groupIds(100) AS LONG`, `DIM groupPcts(100) AS LONG` (0..100, element 0 unused).
    group_ids: Vec<i32>,
    group_pcts: Vec<i32>,
}

/// Port of UpdateCorpControl (0x41f71b).
/// Recomputes who controls `entity_ref` (`VotingController`, `CorpController()`, `WhollyOwnedBy`,
/// `Parent80Owner`, `LargestHolder(Pct)`, `PublicPct`, `PublicFloatPct`, `CorpPctNotCorpHeld()`) and, on a
/// change of control, passes the new controller on to the >50%-owned subsidiaries, adjusts `AutoPilot()`,
/// lets AI-controlled companies sanitize their growth/spending settings, and posts the news / popups.
/// Also settles options of a company that went private (CloseOptionPositions 0x5f3e5f), keeps `CtrlSinceYear()` and
/// strips an investment fund of its tax exemption when it becomes controlled (CalcHolders).
///
/// The FUNCTION result is never assigned: always 0.
///
/// Error 9 cases end the routine like its ON ERROR handler (`ShowRuntimeError "CorpControl"`, not
/// ported): `entity_ref = 0` (" [runtime resource wording] ") and ids outside 1..1600. Note: for a previous
/// controller id 6..10 the original's `IsComputerPlayer(old)` / `PlayerName(old)` (1 TO 5) would also
/// raise error 9; the port reads the arrays' dummy element instead.
pub fn update_corp_control(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_ref: i32) -> i32 {
    let __content = crate::content::current();
    if entity_ref == 0 {
        // errContext = " [runtime resource wording] ": GOSUB UpdateCorpControl_OnError (ShowRuntimeError)
        return 0;
    }
    let mut f = Frame { corp_id: entity_ref, group_ids: vec![0; 101], group_pcts: vec![0; 101], ..Default::default() };
    g.voting_controller = 0;
    g.wholly_owned_by = 0;
    g.parent80_owner = 0;
    g.largest_holder = 0;
    g.largest_holder_pct = 0;
    if f.corp_id < 1 || 1600 < f.corp_id {
        return 0; // EntSymbol(corpId): error 9 -> ON ERROR handler
    }
    g.tmp_symbol = g.ent_symbol[f.corp_id].clone();
    f.old_controller = g.corp_controller[f.corp_id];
    calc_holders(g, ui, rng, &mut f);
    g.corp_controller[f.corp_id] = g.voting_controller;
    if f.corp_id < 11 {
        return 0;
    }
    let corp_id = f.corp_id;
    let old = f.old_controller;

    if g.voting_controller != old {
        f.slot = 1;
        while f.slot <= 15 {
            f.holder_id = cast::<i32, _>(g.hold_stock_id[(f.slot, corp_id)]);
            if 10 < f.holder_id && 50 < g.hold_stock_pct[(f.slot, corp_id)] {
                g.corp_controller[f.holder_id] = g.voting_controller;
                if 10 < g.voting_controller {
                    sanitize_corp_growth_spending(g, rng, f.holder_id);
                }
                let vc = g.voting_controller;
                if 0 < vc && vc <= g.num_players {
                    if g.is_computer_player[vc] == 1 {
                        sanitize_corp_growth_spending(g, rng, f.holder_id);
                    } else if corp_id == g.player_ceo_of[vc] {
                        g.auto_pilot[corp_id] = 0;
                    } else {
                        g.auto_pilot[corp_id] = g.auto_pilot[vc];
                    }
                }
            }
            f.slot += 1;
        }
        if old < 11 && 0 < old && 10 < g.voting_controller {
            sanitize_corp_growth_spending(g, rng, corp_id);
        }
        let vc = g.voting_controller;
        if 0 < vc && vc <= g.num_players && g.is_computer_player[vc] == 1 {
            sanitize_corp_growth_spending(g, rng, corp_id);
        }
    }

    if g.voting_controller != old && g.ctrl_change_msg_mode == 0 {
        if 0 < old && old < 11 {
            g.auto_pilot[corp_id] = 0;
            let vc = g.voting_controller;
            if 0 < vc && vc <= g.num_players && g.is_computer_player[vc] == 0 {
                if corp_id != g.player_ceo_of[vc] && g.auto_pilot[vc] == 1 {
                    g.auto_pilot[corp_id] = 1;
                }
            }
            if g.industry_num[corp_id] == 1 && 6 < g.credit_rating_num[corp_id] {
                g.credit_frozen[corp_id] = 0;
            }
            f.phrase_text = s(__content.string(1862)) + &g.ent_symbol[old] + " has";
            if g.cur_player == old && !(g.action_code == "AUTO") {
                f.phrase_text = s(__content.string(1506));
            }
            f.msg_text = s("no one");
            if 0 < vc && corp_id != vc {
                f.msg_text = g.ent_name[vc].clone();
            }
            if 0 < vc && 0 < old && old < 11 && g.is_computer_player[old] == 0 {
                let skip = !(g.player_name[old] == "...OUT...") && f.msg_text == "no one" && corp_id != vc;
                if !skip && g.suppress_popups == 0 {
                    let mut m = f.phrase_text.clone()
                        + __content.string(2774)
                        + &g.ent_name[corp_id]
                        + __content.string(2775)
                        + &f.msg_text
                        + ".";
                    show_quick_message(g, ui, &mut m);
                }
            }
            g.news_msg = g.ent_symbol[old].clone() + __content.string(2776) + &g.ent_symbol[corp_id] + __content.string(2777);
            if g.player_name[old] == "...OUT..." {
                g.news_msg = PbStr::new();
            }
            if g.voting_controller == 0 && 0 < find_stock_holding(g, old, corp_id) {
                let mut m = std::mem::take(&mut g.news_msg);
                show_quick_message(g, ui, &mut m);
                g.news_msg = m;
                g.ctrl_change_msg_mode = 1;
            }
            post_news(g, ui, 0, 0);
        }
        let vc = g.voting_controller;
        if 0 < vc && vc < 11 {
            if g.is_computer_player[vc] == 0 {
                if corp_id == g.player_ceo_of[vc] {
                    g.auto_pilot[corp_id] = 0;
                } else {
                    g.auto_pilot[corp_id] = g.auto_pilot[vc];
                }
            }
            f.group_idx = 0;
            if 0 < old {
                f.group_idx = g.corp_controller[old];
            }
            if old < 1 || corp_id == old || vc == f.group_idx {
                f.phrase_text = PbStr::new();
            } else {
                f.phrase_text = s(" from ") + &g.ent_symbol[old];
            }
            f.msg_text = s(" taken");
            if f.phrase_text.is_empty() {
                f.msg_text = f.msg_text.clone() + " over";
            }
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                if f.phrase_text.is_empty() {
                    f.msg_text = s(__content.string(2778));
                } else {
                    f.msg_text = s(" seized");
                }
            }
            if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) && f.phrase_text.is_empty() {
                f.msg_text = s(__content.string(2779));
            }
            if Ext::from_f64(4.0) < rng.rnd() * Ext::from_f64(5.0) && !f.phrase_text.is_empty() {
                f.msg_text = s(__content.string(2780));
            }
            g.news_msg = s(__content.string(2781))
                + &g.ent_symbol[corp_id]
                + &f.msg_text
                + &f.phrase_text
                + " by "
                + &g.ent_symbol[vc];
            f.msg_text = g.news_msg.clone();
            f.slot = 0;
            if old == 0 || 10 < old {
                f.slot = vc;
            }
            if g.has_cpu_players == "Y" {
                if 0 < vc && vc < 11 && g.is_computer_player[vc] == 1 && (old == 0 || 10 < old) {
                    f.slot = 0;
                }
                if 0 < old && old < 11 && g.is_computer_player[old] == 1 {
                    f.slot = 0;
                }
            }
            if g.ctrl_change_msg_mode == 1 {
                f.slot = 0;
            }
            post_news(g, ui, f.slot, 0);
            if 0 < old
                && old < 11
                && g.is_computer_player[old] == 0
                && old != g.voting_controller
                && g.ctrl_change_msg_mode == 0
                && g.suppress_popups == 0
            {
                show_quick_message(g, ui, &mut f.msg_text);
            }
        }
    }
    g.ctrl_change_msg_mode = 0;

    if g.public_pct == 0 {
        find_option_positions(g, ui, 0, corp_id, 0, 0);
        if g.has_options_outstanding[corp_id] == 1 {
            // Only reached when a company with outstanding options has no public float left.
            if crate::logic::options::close_option_positions(g, ui, rng, 0, corp_id, 1, 0) == 1 {
                g.news_msg = PbStr::from(__content.string(2782)) + &g.ent_symbol[corp_id]
                    + __content.string(2783);
                post_news(g, ui, corp_id, 0);
            }
        }
    }
    if g.parent80_owner != 0 && g.entity_status[corp_id] == 1 && g.industry_num[corp_id] < 70 {
        if g.ctrl_since_year[corp_id] == 0 {
            g.ctrl_since_year[corp_id] = g.cur_year;
        } else if 30 < parse_count(g.ent_misc_data[corp_id].as_bytes(), b"|")
            && val(parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 28).as_bytes()) != cast::<Ext, _>(g.parent80_owner)
        {
            g.ctrl_since_year[corp_id] = g.cur_year;
        }
        return 0;
    }
    if g.industry_num[corp_id] == 70 {
        g.ctrl_since_year[corp_id] = 0;
    }
    0
}

/// Port of UpdateCorpControl_CalcHolders (0x4207fb), GOSUB of [`update_corp_control`].
/// Sets `VotingController` (and `CorpController(corpId)`), `PublicPct` (100 minus all positive stakes),
/// `CorpPctNotCorpHeld(corpId)` (100 minus stakes of companies), `WhollyOwnedBy`, `Parent80Owner`,
/// `LargestHolder`, `LargestHolderPct` and `PublicFloatPct`; an investment fund (industry 71) that ends up
/// controlled or with a >15% holder becomes a holding company (industry 70), fires its adviser and loses
/// its earnings history (popup + news).
///
/// The holder scan stops at the first slot holding the corp per owner, and the first pass stops early
/// once `PublicPct` reaches 0.
fn calc_holders(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let corp_id = f.corp_id;
    if corp_id < 11 {
        g.voting_controller = corp_id;
        return;
    }
    if g.entity_status[corp_id] != 1 {
        g.voting_controller = 0;
        return;
    }
    g.public_pct = 100;
    g.corp_pct_not_corp_held[corp_id] = 100;
    f.max_pct = 0;
    f.owner_id = 1;
    while f.owner_id < 1601 {
        let owner_id = f.owner_id;
        if g.entity_status[owner_id] == 1 {
            f.slot = 1;
            while f.slot < 16 {
                if cast::<i32, _>(g.hold_stock_id[(f.slot, owner_id)]) == corp_id {
                    f.hold_pct = g.hold_stock_pct[(f.slot, owner_id)];
                    if f.hold_pct == 100 {
                        g.wholly_owned_by = cast::<i16, _>(owner_id);
                    }
                    if 79 < f.hold_pct && 10 < owner_id {
                        g.parent80_owner = cast::<i16, _>(owner_id);
                    }
                    if 19 < f.hold_pct && f.max_pct <= f.hold_pct {
                        g.largest_holder = cast::<i16, _>(owner_id);
                        f.max_pct = f.hold_pct;
                    }
                    if g.industry_num[corp_id] == 71 && 15 < f.hold_pct && g.largest_holder == 0 {
                        f.lost_exempt = 1;
                    }
                    if 0 < f.hold_pct {
                        g.public_pct = g.public_pct.wrapping_sub(f.hold_pct);
                        if 10 < owner_id {
                            g.corp_pct_not_corp_held[corp_id] = g.corp_pct_not_corp_held[corp_id].wrapping_sub(f.hold_pct);
                        }
                    }
                    f.slot = 15;
                }
                f.slot += 1;
            }
            if g.public_pct <= 0 {
                f.owner_id = 1600;
            }
        }
        f.owner_id += 1;
    }
    if g.industry_num[corp_id] != 71 && 80 < g.public_pct {
        g.voting_controller = corp_id;
        return;
    }

    // ---- group the stakes by controller
    f.owner_id = 1;
    while f.owner_id <= 1600 {
        let owner_id = f.owner_id;
        if g.entity_status[owner_id] == 1 {
            f.slot = 1;
            while f.slot < 16 {
                if cast::<i32, _>(g.hold_stock_id[(f.slot, owner_id)]) == corp_id && 0 < g.hold_stock_pct[(f.slot, owner_id)] {
                    f.holder_id = owner_id;
                    f.holder_parent = 0;
                    if 10 < f.holder_id {
                        f.holder_parent = g.corp_controller[f.holder_id];
                    }
                    if 0 < f.holder_parent {
                        f.holder_id = f.holder_parent;
                    }
                    f.hold_pct = g.hold_stock_pct[(f.slot, owner_id)];
                    f.found = 0;
                    f.group_idx = 1;
                    while f.group_idx < 101 {
                        let k = cast::<usize, _>(f.group_idx);
                        if f.holder_id == f.group_ids[k] {
                            f.found = 1;
                            f.group_pcts[k] = (cast::<i32, _>(f.hold_pct)).wrapping_add(f.group_pcts[k]);
                            f.group_idx = 100;
                        }
                        f.group_idx += 1;
                    }
                    if f.found == 0 {
                        f.group_idx = 1;
                        while f.group_idx < 101 {
                            let k = cast::<usize, _>(f.group_idx);
                            if f.group_ids[k] == 0 {
                                f.group_ids[k] = f.holder_id;
                                f.group_pcts[k] = cast::<i32, _>(f.hold_pct);
                                f.group_idx = 100;
                            }
                            f.group_idx += 1;
                        }
                    }
                    f.slot = 15;
                }
                f.slot += 1;
            }
        }
        f.owner_id += 1;
    }
    f.max_pct = 19;
    if g.industry_num[corp_id] == 71 {
        f.max_pct = 15;
    }
    g.voting_controller = 0;
    for slot in 1..=100usize {
        f.hold_pct = cast::<i16, _>(f.group_pcts[slot]); // mov word: truncated to INTEGER
        if f.max_pct < f.hold_pct {
            f.max_pct = f.hold_pct;
            if 19 < f.max_pct {
                g.voting_controller = f.group_ids[slot];
            }
        } else if f.hold_pct == f.max_pct && 19 < f.hold_pct {
            g.voting_controller = 0;
        }
    }
    f.slot = 101; // FOR slot = 1 TO 100 leaves 101 in the shared local
    g.corp_controller[corp_id] = g.voting_controller;
    if g.voting_controller == 0 {
        g.voting_controller = corp_id;
        g.corp_controller[corp_id] = corp_id;
    }
    g.public_float_pct = 100i16.wrapping_sub(f.max_pct);
    if g.public_float_pct < g.public_pct {
        g.public_float_pct = g.public_pct;
    }
    if 100 < g.public_float_pct {
        g.public_float_pct = 100;
    }
    g.largest_holder_pct = f.max_pct;

    if g.industry_num[corp_id] == 71 {
        if f.lost_exempt == 1 || 15 < g.largest_holder_pct || corp_id != g.voting_controller {
            g.industry_num[corp_id] = 70;
            f.phrase_text = s(__content.string(2784));
            if 0 < g.country_idx[corp_id] {
                f.phrase_text = mcase(g.country_display_name[g.country_idx[corp_id]].as_bytes(), CaseMode::Ansi) + " ";
                if g.country_idx[corp_id] == 26 {
                    f.phrase_text = s(__content.string(2785));
                }
                f.msg_text = s(__content.string(2786));
                if g.rand_seed % 3 == 2 {
                    f.msg_text = s(__content.string(2787));
                }
                if g.rand_seed % 3 == 1 {
                    f.msg_text = s(__content.string(2788));
                }
                f.phrase_text = f.phrase_text.clone() + &f.msg_text;
            }
            f.adviser_id = g.fund_adviser[corp_id];
            if 10 < f.adviser_id {
                f.phrase_text = g.ent_name[f.adviser_id].clone();
                g.fund_adviser[corp_id] = 0;
                g.fund_fee_pct[corp_id] = cast::<f32, _>(Ext::from_f32(0.0f32));
                g.ctrl_since_year[corp_id] = 0;
            }
            f.msg_text = g.ent_symbol[corp_id].clone() + __content.string(2789);
            let mut m = f.msg_text.clone()
                + "\r\n"
                + "\r\n"
                + __content.string(2790)
                + &f.phrase_text
                + __content.string(2791);
            show_quick_message(g, ui, &mut m);
            g.news_msg = f.msg_text.clone();
            post_news(g, ui, 0, 0);
            g.earnings_y1[corp_id] = Ext::from_f64(0.0);
            g.earnings_yr2_ago[corp_id] = Ext::from_f64(0.0);
            g.earnings_yr3_ago[corp_id] = Ext::from_f64(0.0);
            g.earnings_yr4_ago[corp_id] = Ext::from_f64(0.0);
            g.earnings_ytd[corp_id] = Ext::from_f64(0.0);
            g.fund_bonus_year[corp_id] = 0;
            g.accrued_pl[corp_id] = Ext::from_f64(0.0);
            g.extraordinary_pl[corp_id] = 0;
            g.tax_loss_carry[corp_id] = Ext::from_f64(0.0);
            g.div_recvd2[corp_id] = Ext::from_f64(0.0);
            crate::logic::etf_derivs::etf_mgmt::etf_liquidate_bonds_on_disqualify(g, ui, rng, corp_id);
        }
        if g.largest_holder_pct < 20 {
            g.largest_holder_pct = 0;
        }
    }
}

/// Port of SanitizeCorpGrowthSpending (0x4a14a7).
/// For an active company: a bank (industry 2) with |GrowthRate| > 7 gets a random rate -4..4; an
/// industrial company (3..69) growing < 5% while `IndDemandGap(industry) > 9` gets 10..12; a
/// `DiscSpendPct` above a random 5..10 is reset to 3..6; a `GrowthRate` above 20 becomes
/// `ROUND(0.2 * GrowthRate + RND(1..5))`.
pub fn sanitize_corp_growth_spending(g: &mut Globals, rng: &mut Rng, corp_id: i32) {
    if !(10 < corp_id && g.entity_status[corp_id] == 1) {
        return;
    }
    let industry = g.industry_num[corp_id];
    if industry == 2 {
        if Ext::from_f64(7.0) < (cast::<Ext, _>(g.growth_rate[corp_id])).abs() {
            g.growth_rate[corp_id] = cast::<i16, _>(rng.rnd_range(9, 1) + -5);
        }
    } else if 2 < industry && industry < 70 {
        if g.growth_rate[corp_id] < 5 && Ext::from_f64(9.0) < g.ind_demand_gap[cast::<i32, _>(industry)] {
            g.growth_rate[corp_id] = cast::<i16, _>(rng.rnd_range(3, 1) + 9);
        }
    }
    if rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(5.0) < cast::<Ext, _>(g.disc_spend_pct[corp_id]) {
        g.disc_spend_pct[corp_id] = cast::<i16, _>(rng.rnd_range(4, 1) + 2);
    }
    if 20 < g.growth_rate[corp_id] {
        let x = cast::<Ext, _>(rng.rnd_range(5, 1));
        // fld dword 0.2; fimul GrowthRate; faddp; fistp qword -> low word stored
        g.growth_rate[corp_id] = cast::<i16, _>(to_quad(cast::<Ext, _>(0.2f32) * cast::<Ext, _>(g.growth_rate[corp_id]) + x));
    }
}

/// `VAL(PARSE$(OptionRecs(i), ANY "|", n))`.
fn opt_field(g: &Globals, i: i32, n: i32) -> Ext {
    val(parse_any(g.option_recs[i].as_bytes(), b"|", n).as_bytes())
}

/// Port of FindOptionPositions (0x5f0cb9).
/// Scans `OptionRecs()` (fields: 1 = underlying corp, 2 = type 1 call / 2 put, 3 = holder, 4 = contracts,
/// negative = written; "GRANT" marks employee options). Always re-counts the records (fixing
/// `OptionRecs(0)` = `STR$(count)` if it was off) and, for `corp_id > 10`, sets
/// `HasOptionsOutstanding(corp_id)`; with `holder_id = 0` it returns 0 right there.
///
/// Otherwise returns a position code for `holder_id`: on one corp (`corp_id > 10`) calls 1 / -1 (long /
/// written; 2 = both; `exclude_grants = 1` ignores granted long calls), puts 31 / -31 / 32, any 51 / -51 /
/// 52 (`opt_type` 1 / 2 / 0); over all corps (`corp_id = 0`) 11 / -11 / 12, 21 / -21 / 22, 41 / -41 / 42,
/// also flagging `HasOptionsOutstanding` of every underlying found. The scan covers records
/// 1..min(count + 100, 1000).
pub fn find_option_positions(
    g: &mut Globals,
    ui: &mut dyn Ui,
    holder_id: i32,
    corp_id: i32,
    opt_type: i32,
    exclude_grants: i32,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut rec_count = 0;
    let mut short_flag = 0;
    let mut long_flag = 0;
    let mut pos_code = 0;
    let mut scan_limit = to_long(val(g.option_recs[0].as_bytes()));
    for rec_idx in 1..=1000 {
        if !g.option_recs[rec_idx].is_empty() {
            rec_count = rec_count + 1;
        }
    }
    if scan_limit != rec_count {
        scan_limit = rec_count;
        g.option_recs[0] = PbStr::from(str_num(cast::<Ext, _>(rec_count), 15).as_str());
    }
    if scan_limit == 0 {
        if 10 < corp_id {
            g.has_options_outstanding[corp_id] = 0;
        }
        return 0;
    }
    scan_limit = scan_limit + 100;
    if 1000 < scan_limit {
        scan_limit = 1000;
    }
    if 10 < corp_id {
        g.has_options_outstanding[corp_id] = 0;
        for rec_idx in 1..=scan_limit {
            if g.option_recs[rec_idx].is_empty() {
                continue;
            }
            if opt_field(g, rec_idx, 1) == cast::<Ext, _>(corp_id) {
                g.has_options_outstanding[corp_id] = 1;
                if holder_id == 0 {
                    return 0;
                }
            }
        }
        if holder_id == 0 {
            return 0;
        }
    }
    let holder = cast::<Ext, _>(holder_id);
    if corp_id <= 10 {
        if corp_id == 0 {
            // (long, short, both) codes per optType; type filter (field 2) for 1 and 2
            let codes = match opt_type {
                1 => Some((Ext::from_f64(1.0), 11, 12)),
                2 => Some((Ext::from_f64(2.0), 21, 22)),
                0 => Some((Ext::from_f64(0.0), 41, 42)),
                _ => None,
            };
            match codes {
                Some((ty, code, both)) => {
                    for rec_idx in 1..=scan_limit {
                        if opt_type != 0 && opt_field(g, rec_idx, 2) != ty {
                            continue;
                        }
                        if opt_field(g, rec_idx, 3) == holder {
                            if Ext::from_f64(0.0) < opt_field(g, rec_idx, 4) {
                                long_flag = code;
                            }
                            if opt_field(g, rec_idx, 4) < Ext::from_f64(0.0) {
                                short_flag = -code;
                            }
                            let underlying = to_long(opt_field(g, rec_idx, 1));
                            g.has_options_outstanding[underlying] = 1;
                        }
                    }
                    pos_code = short_flag + long_flag;
                    if long_flag == code && short_flag == -code {
                        pos_code = both;
                    }
                }
                None => {
                    ui.msgbox(&s(__content.string(2792)), 0, &PbStr::new());
                }
            }
        } else {
            ui.msgbox(&s(__content.string(2793)), 0, &PbStr::new());
        }
    } else {
        let corp = cast::<Ext, _>(corp_id);
        match opt_type {
            1 => {
                for rec_idx in 1..=scan_limit {
                    if opt_field(g, rec_idx, 1) == corp
                        && opt_field(g, rec_idx, 2) == Ext::from_f64(1.0)
                        && opt_field(g, rec_idx, 3) == holder
                    {
                        if Ext::from_f64(0.0) < opt_field(g, rec_idx, 4) {
                            if exclude_grants == 0 {
                                long_flag = 1;
                            }
                            if exclude_grants == 1 && instr(1, g.option_recs[rec_idx].as_bytes(), b"GRANT") == 0 {
                                long_flag = 1;
                            }
                        }
                        if opt_field(g, rec_idx, 4) < Ext::from_f64(0.0) {
                            short_flag = -1;
                        }
                    }
                }
                pos_code = short_flag + long_flag;
                if long_flag == 1 && short_flag == -1 {
                    pos_code = 2;
                }
            }
            2 | 0 => {
                let code = if opt_type == 2 { 31 } else { 51 };
                for rec_idx in 1..=scan_limit {
                    if opt_field(g, rec_idx, 1) != corp {
                        continue;
                    }
                    if opt_type == 2 && opt_field(g, rec_idx, 2) != Ext::from_f64(2.0) {
                        continue;
                    }
                    if opt_field(g, rec_idx, 3) == holder {
                        if Ext::from_f64(0.0) < opt_field(g, rec_idx, 4) {
                            long_flag = code;
                        }
                        if opt_field(g, rec_idx, 4) < Ext::from_f64(0.0) {
                            short_flag = -code;
                        }
                    }
                }
                pos_code = short_flag + long_flag;
                if long_flag == code && short_flag == -code {
                    pos_code = code + 1;
                }
            }
            _ => {
                ui.msgbox(&s(__content.string(2794)), 0, &PbStr::new());
            }
        }
    }
    pos_code
}
