//! Management changes: `FireManagement` (0x4844cb) with its GOSUB `FireMgmt_DismissPlayerCeo`,
//! `ElectSelfCeo` (0x4835d4), `CheckCeoOptionForfeiture` (0x5f1dae) and the AiComputerPlayerTurn steps
//! that use them (`AiPlayer_FireCeos`, `AiPlayer_TuneControlledCorps`).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_ceo_pay;
use crate::logic::finance::get_controller_type;
use crate::logic::report::show_quick_message;
use crate::logic::sim_entity::quarter_start::revoke_ex_ceo_options;
use crate::pb::string::{CaseMode, lcase, ucase};

/// `RIGHT$("000" + RTRIM$(LTRIM$(STR$(id))), 4)`: the 4-digit id used in the management-change lists.
fn id4(id: i32) -> PbStr {
    right((PbStr::from("000") + &trim(str_s(cast::<Ext, _>(id)).as_bytes())).as_bytes(), 4)
}

// ------------------------------------------------------------------------------------- AI steps

/// Port of AiPlayer_TuneControlledCorps (0x5cc888), GOSUB of AiComputerPlayerTurn ("9000").
/// Once per quarter (`AiDivCheckQuarter`): raises the dividend of `AiTargetCorp` by 0..20% when it pays
/// out less than 35% of last year's earnings and is not player-controlled; for every operating company
/// the player controls: more discretionary spending for a loss maker (and up to 3 margin boosts of
/// `DifficultyLevel` when the player is far behind its rival), growth for slow insurers, growth cuts
/// for weakly rated fast growers, and picks the first company with a margin below -6% not on the
/// management-change lists for `FireManagement`.
pub(crate) fn tune_controlled_corps(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.trace_tag = PbStr::from("9000");
    if g.cur_quarter == g.ai_div_check_quarter {
        return;
    }
    g.ai_div_check_quarter = g.cur_quarter;
    f.trade_count = 0;
    f.ent_id = 11;
    loop {
        let e = f.ent_id;
        if g.entity_status[e] == 1 {
            if g.ai_target_corp == e
                && Ext::from_f64(100.0) * g.dividend_per_share[e] * g.shares_outstanding[e] < g.earnings_y1[e] * sf(0.35f32)
                && (g.corp_controller[e] == 0 || g.num_players < g.corp_controller[e])
            {
                g.dividend_per_share[e] = (sf(0.2f32) * rng.rnd() + Ext::from_f64(1.0)) * g.dividend_per_share[e];
            }
            if f.player_id == g.corp_controller[e] && g.industry_num[e] <= 69 {
                if g.profit_margin[e] < -3 && g.disc_spend_pct[e] < 4 {
                    f.rnd_val = to_quad(Ext::from_f64(10.0) * rng.rnd() + cast::<Ext, _>(g.disc_spend_pct[e]));
                    g.disc_spend_pct[e] = cast::<i16, _>(f.rnd_val);
                    if Ext::from_f64(6.0) * rng.rnd() > Ext::from_f64(5.0) && f.trade_count < 3 && f.own_net_worth < f.rival_net_worth * sf(0.05f32) {
                        g.profit_margin[e] = cast::<i16, _>(g.difficulty_level + cast::<i32, _>(g.profit_margin[e]));
                        f.trade_count += 1;
                    }
                }
                if g.industry_num[e] != 1 {
                    if g.industry_num[e] == 2 && g.growth_rate[e] < 3 {
                        f.rnd_val = to_quad(Ext::from_f64(5.0) * rng.rnd() + cast::<Ext, _>(g.growth_rate[e]));
                        g.growth_rate[e] = cast::<i16, _>(f.rnd_val);
                    }
                    if 30 < g.growth_rate[e] && g.credit_rating_num[e] < 7 {
                        f.rnd_val = to_quad(sf(0.4f32) * cast::<Ext, _>(g.growth_rate[e]));
                        g.growth_rate[e] = cast::<i16, _>(f.rnd_val);
                    }
                    if g.profit_margin[e] < -6 && f.fire_mgmt_corp == 0 {
                        f.pending_msg = id4(e);
                        if instr(1, g.new_mgmt_list.as_bytes(), f.pending_msg.as_bytes()) == 0
                            && instr(1, g.recent_mgmt_change_list.as_bytes(), f.pending_msg.as_bytes()) == 0
                        {
                            f.fire_mgmt_corp = e;
                        }
                    }
                }
            }
        }
        f.ent_id += 1;
        if 1600 < f.ent_id {
            if 10 < f.fire_mgmt_corp {
                fire_management(g, ui, rng, f.fire_mgmt_corp);
            }
            return;
        }
    }
}

/// Port of AiPlayer_FireCeos (0x5cd0cd), GOSUB of AiComputerPlayerTurn ("10000").
/// Fires the other players from CEO posts at companies this player controls (news each); then finds
/// the controlled company with the highest CEO pay (salary + bonus, CalcCeoPay) and, unless the
/// player's current post pays more than 110% of that, elects the player CEO there (ElectSelfCeo).
pub(crate) fn fire_ceos(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.trace_tag = PbStr::from("10000");
    let n = g.num_players;
    f.ent_id = 1;
    while f.ent_id <= n {
        f.pick_id = g.player_ceo_of[f.ent_id];
        if f.pick_id != 0 && f.player_id == g.corp_controller[f.pick_id] && f.ent_id != f.player_id {
            g.news_msg = g.player_name[f.player_id].clone()
                + " fires "
                + &g.player_name[f.ent_id]
                + __content.string(1829)
                + &g.ent_symbol[f.pick_id];
            g.player_ceo_of[f.ent_id] = 0;
            g.player_ceo_since_year[f.ent_id] = 0;
            post_news(g, ui, f.player_id, f.ent_id);
        }
        f.ent_id += 1;
    }
    f.x_c8 = Ext::from_f64(0.0);
    f.best_corp_id = 0;
    f.own_ceo_corp = g.player_ceo_of[f.player_id];
    f.ent_id = 11;
    while f.ent_id <= 1600 {
        if g.entity_status[f.ent_id] == 1 && f.player_id == g.corp_controller[f.ent_id] {
            f.amount = calc_ceo_pay(g, ui, rng, f.ent_id, 0);
            f.amount = g.ceo_bonus_multiple * f.amount + f.amount;
            if f.x_c8 < f.amount {
                f.x_c8 = f.amount;
                f.best_corp_id = f.ent_id;
            }
        }
        f.ent_id += 1;
    }
    f.amount = Ext::from_f64(0.0);
    if 10 < f.own_ceo_corp {
        f.amount = calc_ceo_pay(g, ui, rng, f.own_ceo_corp, 0);
        f.amount = g.ceo_bonus_multiple * f.amount + f.amount;
    }
    if f.amount > f.x_c8 * sf(1.1f32) {
        return;
    }
    if f.own_ceo_corp != f.best_corp_id && f.best_corp_id != 0 {
        g.tmp_ext1 = cast::<Ext, _>(f.best_corp_id);
        elect_self_ceo(g, ui, rng);
    }
}

// ------------------------------------------------------------------------------------ FireManagement

/// The locals of FireManagement shared with its GOSUB FireMgmt_DismissPlayerCeo.
#[derive(Default)]
struct FireFrame {
    /// -0xc0: copy of the `corpId` argument.
    corp_id: i32,
    /// -0xb0: INSTR position in the lists; loop variable of the GOSUB.
    list_pos: i32,
    /// -0xac: the managers' golden parachute (QUAD).
    severance: i64,
}

/// Port of FireManagement (0x4844cb).
/// The controlling raider (`CurPlayer`, or a computer player with `ActionCode = "AUTO"`) fires the
/// management of `corp_id`: a human is asked to confirm the golden-parachute severance
/// (`-0.4 * MarketCap * min(margin, -1) * ExchangeRate`, a random 250..305 above 300, half for banks,
/// +50% for a company on the management-change lists), which the company pays. Players who were CEO
/// there are dismissed (FireMgmt_DismissPlayerCeo). A bank (or recently re-managed company < 86) takes a
/// bad-debt write-off of 40% of its reserve (at least 500); a company re-managed recently loses 5 margin
/// points (" [runtime resource wording] "); otherwise headlines about the replaced team. The id is
/// added to `NewMgmtList` (CheckMgmtChange later improves the margin). Holding companies (industry 70)
/// only dismiss player CEOs.
///
/// The ON ERROR handler (0x485720) only shows ShowRuntimeError (not ported).
pub fn fire_management(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32) {
    let __content = crate::content::current();
    let mut f = FireFrame::default();
    if corp_id < 11 {
        return fire_management_exit();
    }
    if g.entity_status[corp_id] != 1 {
        return fire_management_exit();
    }
    f.corp_id = corp_id;
    let c = corp_id;
    g.tmp_symbol = g.ent_symbol[c].clone();
    let corp_name = g.ent_name[c].clone();
    let mut adj_text: PbStr;
    let mut msg_text: PbStr;
    if g.industry_num[c] == 70 {
        if !(g.action_code == "AUTO") {
            let mut s = PbStr::from(__content.string(1830))
                + __content.string(1831);
            show_quick_message(g, ui, &mut s);
        }
        dismiss_player_ceo(g, ui, &mut f);
        return fire_management_exit();
    }
    let id_str = id4(c);
    f.list_pos = instr(1, g.recent_mgmt_change_list.as_bytes(), id_str.as_bytes());
    if f.list_pos == 16 {
        f.list_pos = 0;
    }
    if f.list_pos == 0 {
        f.list_pos = instr(1, g.new_mgmt_list.as_bytes(), id_str.as_bytes());
    }
    let mut recent_change_id = if f.list_pos == 0 { 0 } else { c };
    let mut margin: i64 = cast::<i64, _>(g.profit_margin[c]);
    if margin >= 0 {
        margin = -1;
    }
    f.severance = to_quad(cast::<Ext, _>(margin) * g.market_cap[c] * -sf(0.4f32));
    f.severance = to_quad(cast::<Ext, _>(f.severance) * g.exchange_rate);
    if f.severance > 300 {
        f.severance = cast::<i64, _>(rng.rnd_range(305, 250));
    }
    if c < 86 {
        f.severance = to_quad(cast::<Ext, _>(f.severance) * Ext::from_f64(0.5));
    }
    if 0 < recent_change_id {
        f.severance = to_quad(cast::<Ext, _>(f.severance) * Ext::from_f64(1.5));
    }
    if f.severance < 1 {
        f.severance = 0;
    }
    if !(g.action_code == "AUTO" || f.severance <= 0) {
        msg_text = PbStr::from(__content.string(1832))
            + &corp_name
            + "?"
            + "\r"
            + "\r"
            + __content.string(1833)
            + __content.string(1834)
            + __content.string(1835)
            + __content.string(1836)
            + &g.currency_symbol
            + &fmt_trim(cast::<Ext, _>(f.severance), b"###")
            + " "
            + &g.million_prefix_lc
            + __content.string(1837);
        if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
            msg_text = replace(msg_text.as_bytes(), __content.bytes(1838), __content.bytes(1839));
        }
        if instr(1, g.new_mgmt_list.as_bytes(), id_str.as_bytes()) >= 1
            && instr(1, g.recent_mgmt_change_list.as_bytes(), id_str.as_bytes()) == 0
        {
            msg_text = msg_text
                + "\r"
                + "\r"
                + __content.string(1840)
                + __content.string(1841)
                + __content.string(1842)
                + __content.string(1843)
                + "\r"
                + "\r";
        } else if recent_change_id < 1 {
            msg_text = msg_text + "\r" + "\r";
        } else {
            msg_text = msg_text
                + __content.string(1844)
                + "\r"
                + "\r"
                + __content.string(1845)
                + __content.string(1846)
                + __content.string(1847)
                + &corp_name
                + __content.string(1848);
            adj_text = PbStr::from(__content.string(1849));
            if g.rand_seed % 3 == 1 {
                adj_text = PbStr::from(__content.string(1850));
            }
            if g.rand_seed % 3 == 2 {
                adj_text = PbStr::from(__content.string(1851));
            }
            msg_text = msg_text + "of " + &adj_text + __content.string(1852) + "\r" + "\r";
            if g.rand_seed % 7 < 5 {
                if 2 < g.rand_seed % 7 {
                    msg_text = replace(msg_text.as_bytes(), __content.bytes(1853), __content.bytes(1854));
                }
            } else {
                msg_text = replace(msg_text.as_bytes(), __content.bytes(1853), b"loser");
            }
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                msg_text = replace(msg_text.as_bytes(), b"turkey", __content.bytes(1855));
            }
        }
        msg_text = msg_text + __content.string(1856);
        if ui.msgbox(&msg_text, 0x2124, &g.app_title) == 7 {
            return fire_management_exit();
        }
        g.cash[c] = g.cash[c] - cast::<Ext, _>(f.severance);
        g.accrued_pl[c] = g.accrued_pl[c] - cast::<Ext, _>(f.severance);
        if g.rand_seed % 10 == 1 {
            recent_change_id = 0;
        }
    }
    dismiss_player_ceo(g, ui, &mut f);
    if recent_change_id < 86 {
        if c < 86 {
            let mut write_off = g.bad_debt_reserve[c] * sf(0.4f32);
            if write_off < Ext::from_f64(500.0) {
                write_off = Ext::from_f64(500.0);
            }
            g.bad_debt_reserve[c] = g.bad_debt_reserve[c] + write_off;
            g.extraordinary_pl[c] = to_quad(cast::<Ext, _>(g.extraordinary_pl[c]) - write_off);
            g.news_msg = PbStr::from(__content.string(1857)) + &g.tmp_symbol + __content.string(1858);
            post_news(g, ui, 0, 0);
        }
    } else {
        while len(g.new_mgmt_list.as_bytes()) >= len(g.recent_mgmt_change_list.as_bytes()) {
            g.recent_mgmt_change_list = g.recent_mgmt_change_list.clone() + "QQQQ/";
        }
        g.news_msg = PbStr::from(__content.string(1859)) + &g.cur_player_name + " at " + &g.tmp_symbol;
        g.profit_margin[c] -= 5;
    }
    if g.news_msg.is_empty() {
        if 2 < g.rand_seed % 5 {
            g.news_msg = PbStr::from(__content.string(1860)) + &g.tmp_symbol + __content.string(1861);
            post_news(g, ui, 0, 0);
        }
        g.news_msg =
            PbStr::from(__content.string(1862)) + &g.cur_player_name + __content.string(1863) + &g.tmp_symbol + __content.string(1864);
        if f.severance == 0 && len(g.news_msg.as_bytes()) < 58 {
            g.news_msg = replace(g.news_msg.as_bytes(), b"team", __content.bytes(1865));
        }
        if g.rand_seed % 3 == 1 && c != g.player_ceo_of[g.cur_player] {
            g.news_msg = g.cur_player_name.clone() + __content.string(1866) + &g.tmp_symbol + __content.string(1867);
        }
        post_news(g, ui, 0, 0);
    }
    adj_text = PbStr::from(__content.string(1868));
    msg_text = PbStr::from(" fired!");
    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(3.0) {
        msg_text = PbStr::from(__content.string(1869));
        adj_text = PbStr::from(__content.string(1870));
    }
    if g.action_code == "AUTO" {
        adj_text = PbStr::from(__content.string(1871)) + &corp_name + &msg_text;
    }
    show_quick_message(g, ui, &mut adj_text);
    if 0 < instr(1, g.new_mgmt_list.as_bytes(), id_str.as_bytes()) {
        return fire_management_exit();
    }
    if len(g.new_mgmt_list.as_bytes()) < 231 {
        g.new_mgmt_list = g.new_mgmt_list.clone() + &id_str + "/";
    }
    if len(g.new_mgmt_list.as_bytes()) == 5 {
        g.new_mgmt_list = PbStr::from(__content.string(1872)) + &g.new_mgmt_list + &id_str + "/";
    }
    fire_management_exit()
}

/// Port of FireMgmt_DismissPlayerCeo (0x4854b4), GOSUB of FireManagement.
/// Every other player who is CEO of the company loses the post (a human's autopilot setting passes to
/// the company), gets half the severance (news "Former … CEO … fired"), and loses the unvested options
/// (RevokeExCeoOptions). Counts one turn action.
fn dismiss_player_ceo(g: &mut Globals, ui: &mut dyn Ui, f: &mut FireFrame) {
    let __content = crate::content::current();
    let n = g.num_players;
    f.list_pos = 1;
    while f.list_pos <= n {
        let p = f.list_pos;
        if f.corp_id == g.player_ceo_of[p] && g.cur_player != p {
            g.player_ceo_of[p] = 0;
            g.player_ceo_since_year[p] = 0;
            if g.is_computer_player[p] == 0 {
                g.auto_pilot[f.corp_id] = g.auto_pilot[p];
            }
            g.news_msg = PbStr::from("Former ") + &g.ent_symbol[f.corp_id] + " CEO " + &g.ent_name[p] + " fired";
            if 0 < f.severance {
                g.news_msg = g.news_msg.clone() + __content.string(1873);
            }
            post_news(g, ui, f.corp_id, p);
            g.cash[p] = cast::<Ext, _>(f.severance) * sf(0.5f32) + g.cash[p];
            g.accrued_pl[p] = cast::<Ext, _>(f.severance) * sf(0.5f32) + g.accrued_pl[p];
            revoke_ex_ceo_options(g, ui);
        }
        f.list_pos += 1;
    }
    g.turn_action_count += 1;
}

/// Port of FireManagement_Exit (0x485787): the routine's exit label (frame release only).
fn fire_management_exit() {}

// -------------------------------------------------------------------------------------- ElectSelfCeo

/// Port of ElectSelfCeo (0x4835d4).
/// Makes `CurPlayer` CEO of the company `TmpExt1` (cleared). Changing posts first asks
/// CheckCeoOptionForfeiture (unvested option grants) and, for a human with seniority, confirms the loss
/// of seniority (a computer player never leaves a post held since before last year). Sets
/// `PlayerCeoOf`, clears the company's autopilot, hands the old company's autopilot back to its
/// controlling player, starts the tenure (`PlayerCeoSinceYear`, one turn action), fires other players
/// who were CEO there (a human's run: with a year's pay from the company), posts the election news,
/// shows the notice (salary and bonus from CalcCeoPay) for a human, and revokes options of ex-CEOs.
/// `ActionCode` is cleared at the end.
///
/// The ON ERROR handler (0x484427) only shows ShowRuntimeError (not ported).
pub fn elect_self_ceo(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let corp_id = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    let corp_name = g.ent_name[corp_id].clone();
    let mut is_auto = 0;
    if g.action_code == "AUTO" {
        is_auto = 1;
    }
    let cp = g.cur_player;
    let mut old_ceo_corp_id = g.player_ceo_of[cp];
    if 10 < old_ceo_corp_id && g.entity_status[old_ceo_corp_id] != 1 {
        old_ceo_corp_id = 0;
        g.player_ceo_of[cp] = 0;
        g.player_ceo_since_year[cp] = 0;
    }
    let mut re_prefix = PbStr::from(" ");
    if corp_id == old_ceo_corp_id {
        re_prefix = PbStr::from(" RE-");
    } else {
        let cp = g.cur_player;
        if check_ceo_option_forfeiture(g, ui, cp, old_ceo_corp_id, corp_id) == 1 {
            return;
        }
        let cp = g.cur_player;
        if 0 < g.player_ceo_since_year[cp] && g.player_ceo_since_year[cp] < g.cur_year {
            if g.is_computer_player[cp] == 0 {
                let o = old_ceo_corp_id;
                let mut msg_text = PbStr::from(__content.string(1874))
                    + &g.ent_name[o]
                    + __content.string(1875)
                    + __content.string(1876)
                    + &g.ent_name[o]
                    + __content.string(1877)
                    + __content.string(1878)
                    + "\r"
                    + "\r";
                if g.cur_player != g.corp_controller[o] {
                    msg_text = msg_text
                        + __content.string(1879)
                        + __content.string(1880)
                        + &g.ent_name[o]
                        + __content.string(1881)
                        + __content.string(1882)
                        + "\r"
                        + "\r";
                }
                msg_text = msg_text
                    + __content.string(1883)
                    + __content.string(1884)
                    + &corp_name
                    + "?";
                let title = PbStr::from(__content.string(1885)) + &g.ent_symbol[o];
                if ui.msgbox(&msg_text, 8484, &title) != 6 {
                    return;
                }
            } else if g.player_ceo_since_year[cp] < g.cur_year - 1 {
                return;
            }
        }
    }
    let cp = g.cur_player;
    g.player_ceo_of[cp] = corp_id;
    g.auto_pilot[corp_id] = 0;
    crate::logic::corp_finance::lending::set_auto_pilot_menu_text(g, corp_id, &PbStr::from("ON"));
    if g.player_ceo_of[cp] != old_ceo_corp_id {
        if 10 < old_ceo_corp_id && get_controller_type(g, old_ceo_corp_id) == 1 {
            let ctl = g.corp_controller[old_ceo_corp_id];
            g.auto_pilot[old_ceo_corp_id] = g.auto_pilot[ctl];
        }
        g.player_ceo_since_year[cp] = g.cur_year;
        g.turn_action_count += 1;
    }
    let mut notice_text = ucase(g.cur_player_name.as_bytes(), CaseMode::Ansi)
        + &re_prefix
        + __content.string(1886)
        + &corp_name
        + "\r"
        + "\r";
    let base_pay = calc_ceo_pay(g, ui, rng, corp_id, 0);
    notice_text = notice_text
        + __content.string(1887)
        + &g.currency_symbol
        + &fmt_trim(base_pay, b"* ###,.00")
        + " "
        + &g.million_prefix_uc
        + "ILLION."
        + "\r"
        + "\r";
    notice_text = notice_text
        + __content.string(1888)
        + &g.currency_symbol
        + &fmt_trim(base_pay * g.ceo_bonus_multiple, b"* ###,.00")
        + " "
        + &g.million_prefix_uc
        + "ILLION."
        + "\r"
        + "\r";
    update_corp_control(g, ui, rng, corp_id);
    if 0 < g.public_pct {
        notice_text = notice_text + __content.string(1889) + &g.ent_symbol[corp_id] + __content.string(1890);
        if g.cur_year == g.player_ceo_since_year[g.cur_player] {
            notice_text = notice_text + "," + "\r" + __content.string(1891) + "\r";
        } else {
            notice_text = notice_text + "\r";
        }
    }
    notice_text = notice_text + "\r";
    let mut msg_text = PbStr::new();
    let n = g.num_players;
    let mut player_id = 1;
    while player_id <= n {
        if g.cur_player != player_id && corp_id == g.player_ceo_of[player_id] {
            msg_text = ucase(g.player_name[player_id].as_bytes(), CaseMode::Ansi) + " ";
            g.player_ceo_of[player_id] = 0;
            g.player_ceo_since_year[player_id] = 0;
            if is_auto == 0 {
                g.news_msg = g.player_name[player_id].clone()
                    + __content.string(1892)
                    + &g.ent_symbol[corp_id]
                    + __content.string(1893);
                post_news(g, ui, corp_id, 0);
                g.cash[player_id] = base_pay + g.cash[player_id];
                g.accrued_pl[player_id] = base_pay + g.accrued_pl[player_id];
                g.cash[corp_id] = g.cash[corp_id] - base_pay;
                g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - base_pay;
            }
        }
        player_id += 1;
    }
    if corp_id == old_ceo_corp_id && msg_text.is_empty() {
        g.turn_action_count -= 1;
    }
    let mut step_down_corp_name = corp_name.clone();
    if 0 < old_ceo_corp_id && msg_text.is_empty() {
        step_down_corp_name = g.ent_name[old_ceo_corp_id].clone();
    }
    if (0 < old_ceo_corp_id || !msg_text.is_empty()) && corp_id != old_ceo_corp_id {
        notice_text = notice_text
            + &msg_text
            + &ucase(g.cur_player_name.as_bytes(), CaseMode::Ansi)
            + __content.string(1894)
            + &step_down_corp_name
            + ".";
    }
    if corp_id != old_ceo_corp_id {
        g.news_msg = g.cur_player_name.clone()
            + &lcase(re_prefix.as_bytes(), CaseMode::Ansi)
            + __content.string(1895)
            + &g.ent_name[corp_id]
            + " ("
            + &g.ent_symbol[corp_id]
            + ")...";
        post_news(g, ui, 0, 0);
    }
    if is_auto == 0 {
        ui.msgbox(&notice_text, 8256, &PbStr::from(__content.string(1896)));
        // (UI) CONTROL SET TEXT GetDlgItem(hDlgTransMenu, 461), "&Elect Me As CEO"
    }
    revoke_ex_ceo_options(g, ui);
    g.action_code = PbStr::new();
}

/// Port of CheckCeoOptionForfeiture (0x5f1dae).
/// Before `player_id` gives up the CEO post at `corp_id` (to become CEO of `new_ceo_corp`, 0 = none):
/// if the player holds unvested executive options (`OptionRecs` with `GRANT`) on `corp_id`, a computer
/// player refuses (1) and a human is asked to confirm the forfeiture (1 unless answered YES). Returns 0
/// when the change may go ahead; 1 also for `player_id` 0 or above `NumPlayers`.
///
/// The ON ERROR handler (0x5f2334) only shows ShowRuntimeError (not ported).
pub fn check_ceo_option_forfeiture(g: &mut Globals, ui: &mut dyn Ui, player_id: i32, corp_id: i32, new_ceo_corp: i32) -> i32 {
    let __content = crate::content::current();
    if player_id == 0 || g.num_players < player_id {
        return 1;
    }
    let mut new_corp = new_ceo_corp;
    let mut rec_idx = 1;
    while rec_idx <= 1000 {
        let rec = g.option_recs[rec_idx].clone();
        if !rec.is_empty()
            && val(parse_any(rec.as_bytes(), b"|", 1).as_bytes()) == cast::<Ext, _>(corp_id)
            && val(parse_any(rec.as_bytes(), b"|", 3).as_bytes()) == cast::<Ext, _>(player_id)
        {
            let mut msg_text = parse_any(rec.as_bytes(), b"|", 8);
            if instr(1, msg_text.as_bytes(), b"GRANT") != 0 {
                if g.is_computer_player[player_id] == 1 {
                    return 1;
                }
                msg_text = PbStr::new();
                if 0 < new_corp {
                    msg_text = PbStr::from(__content.string(1897))
                        + &g.ent_name[new_corp]
                        + __content.string(1898)
                        + &g.ent_name[corp_id]
                        + "."
                        + "\r"
                        + "\r";
                }
                msg_text = msg_text
                    + __content.string(1899)
                    + &g.ent_name[corp_id]
                    + " ("
                    + &g.ent_symbol[corp_id]
                    + __content.string(1900)
                    + __content.string(1901)
                    + &g.ent_symbol[corp_id]
                    + __content.string(1902);
                new_corp = g.corp_controller[corp_id];
                if g.cur_player == new_corp {
                    msg_text = msg_text + __content.string(1903);
                } else {
                    msg_text = msg_text + __content.string(1904);
                }
                msg_text = msg_text
                    + "\r"
                    + "\r"
                    + __content.string(1905)
                    + &g.ent_name[corp_id]
                    + "?";
                let title = PbStr::from(__content.string(1906)) + &g.ent_name[corp_id] + __content.string(1907);
                if ui.msgbox(&msg_text, 8227, &title) != 6 {
                    return 1;
                }
                break;
            }
        }
        rec_idx += 1;
    }
    0
}
