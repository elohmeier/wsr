//! Antitrust: the private-suit queue (`LawsuitRec` records `"1 |defendant|plaintiff|share|damages|settle|"`)
//! with its plaintiff search and damages claim, the AI's suit filings, the government's enforcement
//! against a player's monopoly and the antitrust review of acquisitions.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::holdings::find_stock_holding;
use crate::logic::sim_entity::bonds::issue_bonds;
use crate::logic::news::is_in_recent_headlines;
use crate::logic::report::{number_to_words, show_quick_message, show_text_report};
use crate::logic::reports_corp::{divest_stock, update_antitrust_reserves};
use crate::logic::reports_lists::update_control_chain;
use crate::logic::sim_entity::asset_sales::start_new_corp_or_expand;
use crate::logic::valuation::{
    calc_industry_market_share, calc_industry_outlook, calc_stock_price,
};
use crate::pb::string::{CaseMode, string_fill, ucase};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `RTRIM$(LTRIM$(STR$(x)))`.
fn str_trim(x: Ext) -> PbStr {
    trim(str_s(x).as_bytes())
}

/// True if suit record `rec` is a pending private antitrust suit (`"1…"`) filed by `plaintiff` (field 3).
fn suit_filed_by(rec: &PbStr, plaintiff: i32) -> bool {
    mid(rec.as_bytes(), 1, 1) == "1"
        && val(parse_any(rec.as_bytes(), b"|", 3).as_bytes()) == cast::<Ext, _>(plaintiff)
}

/// Port of CalcAntitrustDamages (0x5414a0).
/// The damages claim of a private antitrust suit of `plaintiff_id` against `defendant_id`: 75% of the
/// defendant's net worth plus contingency reserve (60% above `250000 * ExchangeRate`, but not below that),
/// at least 100, then scaled by `0.7 + 0.3 * RND`. While the plaintiff's liquid funds (capped at 90% of
/// its net worth below a 72% share, at least `10 * ExchangeRate`) exceed `50 * ExchangeRate` and the
/// share is below 75%, the claim shrinks by 5% steps until 6% of it (the legal costs, also shrinking)
/// is affordable. 0 when [runtime resource wording]  are off. Leaves `NetWorth`/`LiquidFunds` of the plaintiff.
pub fn calc_antitrust_damages(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    plaintiff_id: i32,
    defendant_id: i32,
    market_share: i32,
) -> i64 {
    if g.private_antitrust_off == 1 {
        return 0;
    }
    calc_entity_financials(g, ui, rng, defendant_id);
    let mut damages = to_quad((g.net_worth + g.contingency_reserve[defendant_id]) * sf(0.75f32));
    if g.exchange_rate * Ext::from_f64(250000.0) < cast::<Ext, _>(damages) {
        damages = to_quad((g.net_worth + g.contingency_reserve[defendant_id]) * sf(0.6f32));
        if (cast::<Ext, _>(damages)) < g.exchange_rate * Ext::from_f64(250000.0) {
            damages = to_quad(g.exchange_rate * Ext::from_f64(250000.0));
        }
    }
    if damages < 100 {
        damages = 100;
    }
    if 100 < damages {
        let r = rng.rnd();
        damages = to_quad(cast::<Ext, _>(damages) * sf(0.7f32) + r * sf(0.3f32) * cast::<Ext, _>(damages));
    }
    calc_entity_financials(g, ui, rng, plaintiff_id);
    if market_share < 72 && g.net_worth * sf(0.9f32) < g.liquid_funds {
        g.liquid_funds = g.net_worth * sf(0.9f32);
    }
    if g.liquid_funds < g.exchange_rate * Ext::from_f64(10.0) {
        g.liquid_funds = g.exchange_rate * Ext::from_f64(10.0);
    }
    let mut legal_cost = cast::<Ext, _>(damages) * sf(0.06f32);
    if g.exchange_rate * Ext::from_f64(50.0) < g.liquid_funds && market_share < 75 {
        while g.liquid_funds < legal_cost {
            legal_cost = legal_cost * sf(0.95f32);
            damages = to_quad(sf(0.95f32) * cast::<Ext, _>(damages));
        }
    }
    damages
}

/// Port of FindAntitrustPlaintiff (0x53eee8).
/// Looks for a competitor of `defendant_id` in `industry` (3..69) to sue it: an active corp without a
/// managing player (outside `"AUTO"` mode only with 20% chance each), not the defendant or the current
/// suit's defendant, not under the defendant's controller, not the current plaintiff, not among
/// `RecentAntitrustFilers` and without a queued suit. Its liquid funds (capped at 90% of net worth below
/// a 72% share; if under 10: half its cash when that exceeds `10 * ExchangeRate` and the current suit's
/// share is over 57, capped at `300 * ExchangeRate`) decide: the first candidate over `35 * ExchangeRate`
/// is kept tentatively (for good over `300 * ExchangeRate`), replaced by one over `1000 * ExchangeRate`;
/// a kept candidate controlled by a corp (`GetControllerType` 2) ends the search. Returns the plaintiff
/// or 0.
pub fn find_antitrust_plaintiff(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    industry: i16,
    defendant_id: i32,
    market_share: i32,
) -> i32 {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut candidate_id = 0;
    let mut plaintiff_id = 0;
    if 130 < defendant_id && 2 < industry && industry < 70 && g.private_antitrust_off != 1 {
        let mut corp_id = 131;
        loop {
            'next: {
                if g.entity_status[corp_id] != 1 {
                    break 'next;
                }
                if g.industry_num[corp_id] != industry {
                    break 'next;
                }
                if get_managing_player(g, corp_id) > 0 {
                    break 'next;
                }
                if g.action_code != "AUTO" && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(5.0) {
                    break 'next;
                }
                if defendant_id == corp_id || g.antitrust_defendant == corp_id {
                    break 'next;
                }
                if 0 < g.corp_controller[corp_id]
                    && g.corp_controller[defendant_id] == g.corp_controller[corp_id]
                {
                    break 'next;
                }
                let mut eligible = 1;
                if g.antitrust_plaintiff == corp_id {
                    break 'next;
                }
                for slot in 1..=10 {
                    if corp_id == g.recent_antitrust_filers[slot] {
                        eligible = 0;
                    }
                    let suit_rec = g.lawsuit_rec[slot].clone();
                    if suit_filed_by(&suit_rec, corp_id) {
                        eligible = 0;
                    }
                }
                if eligible == 0 {
                    break 'next;
                }
                calc_entity_financials(g, ui, rng, corp_id);
                let funds = g.liquid_funds;
                if market_share < 72 && g.net_worth * sf(0.9f32) < funds {
                    g.liquid_funds = g.net_worth * sf(0.9f32);
                }
                if g.liquid_funds < Ext::from_f64(10.0) {
                    g.liquid_funds = Ext::from_f64(10.0);
                    if g.exchange_rate * Ext::from_f64(10.0) < g.cash[corp_id] * Ext::from_f64(0.5)
                        && 57 < g.antitrust_market_share
                    {
                        g.liquid_funds = g.cash[corp_id] * Ext::from_f64(0.5);
                    }
                    if g.exchange_rate * Ext::from_f64(300.0) < g.liquid_funds {
                        g.liquid_funds = g.exchange_rate * Ext::from_f64(300.0);
                    }
                }
                if 0 < candidate_id || g.liquid_funds <= g.exchange_rate * Ext::from_f64(35.0) {
                    if 0 < candidate_id && g.exchange_rate * Ext::from_f64(1000.0) <= g.liquid_funds {
                        candidate_id = corp_id;
                    }
                } else {
                    candidate_id = -corp_id;
                    if g.exchange_rate * Ext::from_f64(300.0) < g.liquid_funds {
                        candidate_id = corp_id;
                    }
                }
                if (cast::<Ext, _>(corp_id)).abs() == cast::<Ext, _>(candidate_id)
                    && get_controller_type(g, corp_id) == 2
                {
                    candidate_id = to_long((cast::<Ext, _>(corp_id)).abs());
                    corp_id = 1600;
                }
            }
            corp_id += 1;
            if corp_id >= 1601 {
                break;
            }
        }
        plaintiff_id = to_long((cast::<Ext, _>(candidate_id)).abs());
    }
    plaintiff_id
}

/// Port of ProcessAntitrustSuitQueue (0x53f52a).
/// Every 200 ticks (`SimCoCount MOD 200 = 199`, unless private suits are off): with no active suit, the
/// first queued `LawsuitRec` becomes the active suit (`AntitrustDefendant`/`Plaintiff`/`MarketShare`/
/// `Damages`/`SettleAmt`), the queue shifts and the filing is posted (between companies of one
/// controller the suit is cancelled again). Then, unless three suits are queued, scans an industry (the
/// defendant's of a suit without plaintiff, else the next of `AntitrustScanIndustry`): if the leader's
/// share reaches `50 + 12 * RND`, a plaintiff with an earnings history ([`find_antitrust_plaintiff`]) that
/// has no suit pending gets a new record queued with its damages claim, and the defendant's antitrust
/// reserves are updated.
pub fn process_antitrust_suit_queue(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if g.private_antitrust_off == 1 || g.sim_co_count % 200 != 199 {
        return;
    }
    if 10 < g.antitrust_defendant && 10 < g.antitrust_plaintiff && g.antitrust_market_share == 0 {
        g.antitrust_market_share = 1;
    }
    let suit_rec = g.lawsuit_rec[1].clone();
    if g.antitrust_plaintiff == 0 && mid(suit_rec.as_bytes(), 1, 1) == "1" {
        let field = |n: i32| val(parse_any(suit_rec.as_bytes(), b"|", n).as_bytes());
        g.antitrust_defendant = to_long(field(2));
        g.antitrust_plaintiff = to_long(field(3));
        g.antitrust_market_share = to_long(field(4));
        g.antitrust_damages = to_quad(field(5));
        g.antitrust_settle_amt = to_quad(field(6));
        for slot in 1..=9 {
            g.lawsuit_rec[slot] = g.lawsuit_rec[slot + 1].clone();
        }
        g.lawsuit_rec[10] = PbStr::new();
        let mut msg_part = s(__content.string(4960));
        if g.corp_controller[g.antitrust_plaintiff] == g.corp_controller[g.antitrust_defendant] {
            msg_part = replace(msg_part.as_bytes(), b"files", __content.bytes(4961));
            msg_part = msg_part + __content.string(4962);
        } else if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) && 10 < g.corp_controller[g.antitrust_plaintiff] {
            // keep " [runtime resource wording] "
        } else {
            msg_part = s(__content.string(4963));
            if g.rand_seed % 2 == 1 {
                msg_part = crate::pb::string::remove(msg_part.as_bytes(), __content.bytes(4964));
            }
        }
        g.news_msg = g.ent_symbol[g.antitrust_plaintiff].clone()
            + &msg_part
            + &g.ent_symbol[g.antitrust_defendant];
        let (d, p) = (g.antitrust_defendant, g.antitrust_plaintiff);
        post_news(g, ui, d, p);
        if g.corp_controller[g.antitrust_plaintiff] == g.corp_controller[g.antitrust_defendant] {
            g.antitrust_defendant = 0;
            g.antitrust_plaintiff = 0;
            g.antitrust_market_share = 0;
            g.antitrust_damages = 0;
            g.antitrust_settle_amt = 0;
        }
    }
    let mut industry: i16 = 0;
    if 10 < g.antitrust_defendant && g.antitrust_plaintiff == 0 {
        industry = g.industry_num[g.antitrust_defendant];
        g.antitrust_defendant = 0;
    }
    if industry == 0 {
        g.antitrust_scan_industry += 1;
        if 69 < g.antitrust_scan_industry {
            g.antitrust_scan_industry = 3;
        }
        if g.antitrust_scan_industry < 3 {
            g.antitrust_scan_industry = rng.rnd_range(69, 3);
        }
        industry = cast::<i16, _>(g.antitrust_scan_industry);
    }
    if !g.lawsuit_rec[3].is_empty() || industry == 0 {
        return;
    }
    let leader_id = calc_industry_outlook(g, rng, industry);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if leader_id == 0 {
        return;
    }
    let leader_controller = g.corp_controller[leader_id];
    g.tmp_ext1 = cast::<Ext, _>(leader_controller);
    let market_share = calc_industry_market_share(g, industry);
    if !(rng.rnd() * Ext::from_f64(12.0) + Ext::from_f64(50.0) <= cast::<Ext, _>(market_share)) {
        return;
    }
    let defendant_id = leader_id;
    let mut plaintiff_id =
        find_antitrust_plaintiff(g, ui, rng, industry, defendant_id, market_share);
    if plaintiff_id == 0 {
        return;
    }
    if 0 < g.corp_controller[defendant_id]
        && 0 < g.corp_controller[plaintiff_id]
        && g.corp_controller[plaintiff_id] == g.corp_controller[defendant_id]
    {
        plaintiff_id = 0;
    }
    if 130 < plaintiff_id
        && g.earnings_yr3_ago[plaintiff_id] == Ext::from_f64(0.0)
        && g.earnings_yr2_ago[plaintiff_id] == Ext::from_f64(0.0)
    {
        plaintiff_id = 0;
    }
    if plaintiff_id == 0 {
        return;
    }
    let mut eligible = cast::<i32, _>(plaintiff_id != g.antitrust_plaintiff);
    for slot in 1..=10 {
        if plaintiff_id == g.recent_antitrust_filers[slot] {
            eligible = 0;
        }
        let rec = g.lawsuit_rec[slot].clone();
        if suit_filed_by(&rec, plaintiff_id) {
            eligible = 0;
        }
    }
    if eligible == 0 {
        return;
    }
    let mut msg_part = s("1 |")
        + &str_s(cast::<Ext, _>(defendant_id))
        + "|"
        + &str_s(cast::<Ext, _>(plaintiff_id))
        + "|"
        + &str_s(cast::<Ext, _>(market_share))
        + "|";
    let damages = calc_antitrust_damages(g, ui, rng, plaintiff_id, defendant_id, market_share);
    msg_part = msg_part + &str_trim(cast::<Ext, _>(damages)) + "|0|";
    for slot in 1..=10 {
        if g.lawsuit_rec[slot].is_empty() {
            g.lawsuit_rec[slot] = msg_part;
            update_antitrust_reserves(g, ui, rng, defendant_id);
            break;
        }
    }
}

/// Port of AiFileAntitrustSuits (0x5407f6).
/// The AI looks for a monopolist to sue (when private suits are on, the queue has room — the 5th slot
/// only matters outside `"AUTO"` mode — and slot 10 is free): from a random industry 3..68 up to 69,
/// the leader not controlled by a corp (`GetControllerType` 2) with the largest share above
/// `48 + RND * 8` (5 in `"AUTO"`); outside `"AUTO"` an industry is skipped with 2/3 chance once a target
/// exists, and a target controlled by a player ends the scan with 20% chance. With 1/3 chance a sister
/// company (same controller and industry, market cap 15+) becomes the defendant. The plaintiff comes from
/// [`find_antitrust_plaintiff`], else (unmet demand over 5, share over 55, not the crowded industry) a
/// newly started company; one without an earnings history only issues bonds (net worth over
/// `350 * ExchangeRate`) and does not sue. A plaintiff without a pending suit gets a record queued and a
/// headline. Returns the plaintiff or 0. In `"AUTO"` mode the current player's own monopoly is spared.
pub fn ai_file_antitrust_suits(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    time_secs: i32,
) -> i32 {
    let __content = crate::content::current();
    let mut result = 0;
    if g.private_antitrust_off == 1 {
        return result;
    }
    if !g.lawsuit_rec[5].is_empty() && g.action_code != "AUTO" {
        return result;
    }
    if !g.lawsuit_rec[10].is_empty() {
        return result;
    }
    let mut best_share: Ext = Ext::from_f64(0.0);
    let mut target_id = 0;
    let mut market_share;
    let start_ind = rng.rnd_range(68, 3);
    let mut industry = cast::<i16, _>(start_ind);
    'scan: while industry < 70 {
        'next: {
            let mut leader_controller = 0;
            let leader_id = calc_industry_outlook(g, rng, industry);
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
            if 0 < leader_id {
                leader_controller = g.corp_controller[leader_id];
            }
            if get_controller_type(g, leader_controller) == 2 {
                break 'next;
            }
            g.tmp_ext1 = cast::<Ext, _>(leader_controller);
            market_share = calc_industry_market_share(g, industry);
            let mut share_slack = to_long(rng.rnd() * Ext::from_f64(8.0));
            if g.action_code == "AUTO" {
                share_slack = 5;
            } else if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) && 10 < target_id {
                break 'next;
            }
            if share_slack + 48 < market_share && best_share < cast::<Ext, _>(market_share) {
                best_share = cast::<Ext, _>(market_share);
                target_id = leader_id;
                if get_controller_type(g, target_id) == 1 && rng.rnd() * Ext::from_f64(5.0) < Ext::from_f64(1.0) {
                    break 'scan;
                }
            }
        }
        industry += 1;
    }
    market_share = to_long(best_share);
    if target_id == 0 {
        return result;
    }
    if g.action_code == "AUTO" && g.cur_player == g.corp_controller[target_id] {
        return 0;
    }
    let mut defendant_id = target_id;
    if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
        for idx in 131..=1600 {
            if g.entity_status[idx] == 1
                && g.industry_num[idx] == g.industry_num[target_id]
                && g.corp_controller[defendant_id] == g.corp_controller[idx]
                && defendant_id != idx
                && rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(2.0)
                && Ext::from_f64(15.0) <= g.market_cap[idx]
            {
                defendant_id = idx;
                break;
            }
        }
    }
    let industry = g.industry_num[defendant_id];
    let mut plaintiff_id =
        find_antitrust_plaintiff(g, ui, rng, industry, defendant_id, market_share);
    if plaintiff_id == 0
        && Ext::from_f64(5.0) < g.ind_demand_gap[cast::<i32, _>(industry)]
        && Ext::from_f64(55.0) < best_share
        && cast::<i32, _>(industry) != g.crowded_industry
    {
        g.tmp_ext1 = Ext::from_f64(0.0);
        plaintiff_id = start_new_corp_or_expand(g, ui, rng, industry, 0, time_secs);
    }
    if 130 < plaintiff_id
        && g.earnings_yr3_ago[plaintiff_id] == Ext::from_f64(0.0)
        && g.earnings_yr2_ago[plaintiff_id] == Ext::from_f64(0.0)
    {
        if g.exchange_rate * Ext::from_f64(350.0) < g.net_worth && g.bonds_issued[plaintiff_id] == 0 {
            g.tmp_ext1 = Ext::from_f64(0.0);
            issue_bonds(g, ui, rng, plaintiff_id, Ext::from_f64(0.0), 0);
        }
        plaintiff_id = 0;
    }
    if plaintiff_id == 0 {
        return 0;
    }
    let mut eligible = cast::<i32, _>(plaintiff_id != g.antitrust_plaintiff);
    if g.lawsuit_rec[10].is_empty() {
        for idx in 1..=10 {
            if plaintiff_id == g.recent_antitrust_filers[idx] {
                eligible = 0;
            }
            let rec = g.lawsuit_rec[idx].clone();
            if suit_filed_by(&rec, plaintiff_id) {
                eligible = 0;
            }
        }
    }
    if eligible == 0 {
        return 0;
    }
    let mut suit_rec = s("1 |")
        + &str_s(cast::<Ext, _>(defendant_id))
        + "|"
        + &str_s(cast::<Ext, _>(plaintiff_id))
        + "|"
        + &str_s(cast::<Ext, _>(market_share))
        + "|";
    let damages = calc_antitrust_damages(g, ui, rng, plaintiff_id, defendant_id, market_share);
    suit_rec = suit_rec + &str_trim(cast::<Ext, _>(damages)) + "|0|";
    g.news_msg = g.ent_symbol[defendant_id].clone()
        + __content.string(4965)
        + &g.ent_symbol[plaintiff_id];
    if !g.lawsuit_rec[1].is_empty() {
        g.news_msg = g.ent_symbol[plaintiff_id].clone()
            + __content.string(4966)
            + &g.ent_symbol[defendant_id];
        let k = g.rand_seed % 5;
        if k == 1 {
            g.news_msg = replace(g.news_msg.as_bytes(), b"says it", __content.bytes(4967));
        }
        if k == 2 {
            g.news_msg = replace(g.news_msg.as_bytes(), b"plan", b"intend");
        }
        if k == 3 {
            g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(4968), b"will");
        }
        if k == 4 {
            g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(4969), __content.bytes(4970));
        }
    }
    post_news(g, ui, plaintiff_id, defendant_id);
    let mut idx = 1;
    while idx < 11 {
        if g.lawsuit_rec[idx].is_empty() {
            g.lawsuit_rec[idx] = suit_rec.clone();
            update_antitrust_reserves(g, ui, rng, defendant_id);
            idx = 10;
        }
        idx += 1;
    }
    result = plaintiff_id;
    result
}

/// Port of CheckPrivateAntitrustSuit (0x6823ff).
/// After a change of `corp_id` (control updated first; nothing for the current player's companies):
/// if its business assets still reach 48% of the active corps' assets in `industry`, the share field of
/// every queued suit against it is rewritten with the new percentage; otherwise those suits are dropped.
pub fn check_private_antitrust_suit(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    industry: i16,
) {
    let corp = corp_id;
    update_corp_control(g, ui, rng, corp);
    if g.cur_player == g.corp_controller[corp] {
        return;
    }
    let mut industry_assets: Ext = Ext::from_f64(0.0);
    for i in 131..=1600 {
        if g.entity_status[i] == 1 && g.industry_num[i] == industry {
            industry_assets = industry_assets + g.business_assets[i];
        }
    }
    let defendant_of = |rec: &PbStr| {
        mid(rec.as_bytes(), 1, 1) == "1"
            && val(parse_any(rec.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(corp)
    };
    if industry_assets * sf(0.48f32) <= g.business_assets[corp] {
        let share_pct = to_long(g.business_assets[corp] / industry_assets * Ext::from_f64(100.0));
        for i in 1..=10 {
            let suit_rec = g.lawsuit_rec[i].clone();
            if !suit_rec.is_empty() && defendant_of(&suit_rec) {
                let f = |n: i32| parse_any(suit_rec.as_bytes(), b"|", n);
                let mut new_rec = s("1 |") + &f(2) + "|" + &f(3) + "|";
                new_rec = new_rec + &str_trim(cast::<Ext, _>(share_pct)) + "|" + &f(5) + "|" + &f(6) + "|";
                g.lawsuit_rec[i] = new_rec;
            }
        }
    } else {
        for i in 1..=10 {
            let suit_rec = g.lawsuit_rec[i].clone();
            if !suit_rec.is_empty() && defendant_of(&suit_rec) {
                g.lawsuit_rec[i] = PbStr::new();
            }
        }
    }
}

/// Locals of `GovtAntitrustEnforcement` shared with its GOSUB.
struct GovtFrame {
    corp_id: i32,
    market_share: i32,
    industry: i16,
    target_corp: i32,
    agency_name: PbStr,
}

/// Port of GovtAntitrustEnforcement (0x68114c) with its GOSUB GovtAntitrust_IssueWarning (0x68212b)
/// and exit block GovtAntitrustEnforcement_Exit (0x6823d9).
/// Checks industry `SimCoCount MOD 500` (3..69) for a monopoly of the current player (share over
/// `65 + 10 * RND`; otherwise nothing happens — the lower thresholds by `PlayerAntitrustLevel` all exit
/// too). At most once per `CurYear + CurQuarter` key (re-checks with 1/7 chance), with chances to let it
/// go for weak leaders, single companies below `70 - level` and on the easy level / early in the
/// quarter. The first offence brings the official warning (level 1). Later ones fine the player's
/// leader (or the largest company) — 20% of its business assets, at least 7 times its market cap, 1.5..1.75
/// times from level 4, +10% for the same company as last time — booked to cash and equity income;
/// with several companies the stake of the holder (the controlling largest holder, the player, or a
/// player-controlled corp) is divested, the growth rate raised to 20..32 and the private suits
/// re-checked. Raises the level and posts the news.
pub fn govt_antitrust_enforcement(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    if g.govt_antitrust_off == 1 {
        return;
    }
    if g.cur_quarter + g.cur_year == g.antitrust_act_key && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(7.0) {
        return;
    }
    let industry = cast::<i16, _>(g.sim_co_count % 500);
    if industry < 3 || 69 < industry {
        return;
    }
    g.tmp_ext1 = cast::<Ext, _>(g.cur_player);
    let market_share = calc_industry_market_share(g, industry);
    if cast::<Ext, _>(market_share) <= rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(65.0) {
        let lvl = |g: &Globals| g.player_antitrust_level[g.cur_player];
        if (cast::<Ext, _>(market_share)) < rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(65.0) && lvl(g) < 2 {
            return;
        }
        if (cast::<Ext, _>(market_share)) < rng.rnd() * Ext::from_f64(6.0) + Ext::from_f64(59.0) && lvl(g) < 3 {
            return;
        }
        if (cast::<Ext, _>(market_share)) < rng.rnd() * Ext::from_f64(6.0) + Ext::from_f64(53.0) && lvl(g) < 5 {
            return;
        }
        return;
    }
    let mut target_corp = calc_industry_outlook(g, rng, industry);
    if g.cur_player != g.corp_controller[target_corp] {
        target_corp = 0;
    }
    let outlook_ext = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if (cast::<Ext, _>(outlook_ext)) < rng.rnd() * Ext::from_f64(2.0) + Ext::from_f64(2.0) && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
        return;
    }
    let mut ctrl_count = 0;
    for corp_id in 131..=1600 {
        if g.entity_status[corp_id] == 1
            && g.industry_num[corp_id] == industry
            && g.cur_player == g.corp_controller[corp_id]
        {
            ctrl_count += 1;
        }
    }
    if ctrl_count == 0 {
        return;
    }
    if ctrl_count == 1
        && market_share < 70 - g.player_antitrust_level[g.cur_player]
        && Ext::from_f64(3.0) < rng.rnd() * Ext::from_f64(4.0)
    {
        return;
    }
    if target_corp == 0 {
        let mut max_assets: Ext = Ext::from_f64(0.0);
        for corp_id in 131..=1600 {
            if g.entity_status[corp_id] == 1
                && g.industry_num[corp_id] == industry
                && g.cur_player == g.corp_controller[corp_id]
                && max_assets < g.business_assets[corp_id]
            {
                max_assets = g.business_assets[corp_id];
                target_corp = corp_id;
            }
        }
    }
    let mut agency_name = s(__content.string(4971));
    if g.currency_code != "USA" {
        agency_name = s(__content.string(4972));
    }
    if g.currency_code == "UK" {
        agency_name = s(__content.string(4973));
    }
    if g.currency_code == "EUR" {
        agency_name = s(__content.string(4974));
    }
    if g.currency_code == "CAN" {
        agency_name = s(__content.string(4975));
    }
    if g.currency_code == "AUS" {
        agency_name = s(__content.string(4976));
    }
    let mut f = GovtFrame {
        corp_id: 1601,
        market_share,
        industry,
        target_corp,
        agency_name,
    };
    if govt_antitrust_issue_warning(g, ui, &mut f) {
        return;
    }
    let lvl = g.player_antitrust_level[g.cur_player];
    if g.difficulty_level == 1 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) && lvl == 1 {
        return;
    }
    if 1 < g.difficulty_level
        && g.sim_co_count < 570
        && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0)
        && g.player_antitrust_level[g.cur_player] < 3
    {
        return;
    }
    if !(10 < target_corp) {
        return;
    }
    update_corp_control(g, ui, rng, target_corp);
    let mut holder_id = 0;
    if g.largest_holder < 1 || g.cur_player != g.corp_controller[cast::<i32, _>(g.largest_holder)] {
        if 0 < find_stock_holding(g, g.cur_player, target_corp) {
            holder_id = g.cur_player;
        } else {
            for corp_id in 11..=1600 {
                if g.entity_status[corp_id] == 1
                    && g.cur_player == g.corp_controller[corp_id]
                    && 0 < find_stock_holding(g, corp_id, target_corp)
                {
                    holder_id = corp_id;
                    break;
                }
            }
        }
    } else {
        holder_id = cast::<i32, _>(g.largest_holder);
    }
    if holder_id == 0 {
        return;
    }
    let mut fine_amt = to_quad(g.business_assets[target_corp] * sf(0.2f32));
    if (cast::<Ext, _>(fine_amt)) < g.market_cap[target_corp] * Ext::from_f64(7.0) {
        fine_amt = to_quad(g.market_cap[target_corp] * Ext::from_f64(7.0));
    }
    if 3 < g.player_antitrust_level[g.cur_player] {
        fine_amt = to_quad((rng.rnd() * sf(0.25f32) + sf(1.5f32)) * cast::<Ext, _>(fine_amt));
    }
    if g.last_divest_corp == target_corp {
        fine_amt = to_quad(sf(1.1f32) * cast::<Ext, _>(fine_amt));
    }
    g.last_divest_corp = target_corp;
    g.cash[target_corp] = g.cash[target_corp] - cast::<Ext, _>(fine_amt);
    g.equity_income_ytd[target_corp] = g.equity_income_ytd[target_corp] - cast::<Ext, _>(fine_amt);
    calc_projected_earnings(g, ui, rng, target_corp);
    if g.rand_seed < 32000 {
        g.rand_seed += 1;
    }
    calc_projected_earnings(g, ui, rng, target_corp);
    if 10 < holder_id {
        calc_projected_earnings(g, ui, rng, holder_id);
    }
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, target_corp, time_secs);
    let mut warned_clause = PbStr::new();
    if g.player_antitrust_level[g.cur_player] == 1 {
        warned_clause =
            s(__content.string(4977));
    }
    let mut msg_text = s(__content.string(4978))
        + "\r"
        + "\r"
        + "The "
        + &f.agency_name
        + &warned_clause
        + __content.string(4979)
        + &g.ind_name[cast::<i32, _>(industry)]
        + __content.string(4980)
        + __content.string(4981)
        + &g.cur_player_name
        + __content.string(4982)
        + &str_s(cast::<Ext, _>(market_share))
        + "% of "
        + __content.string(4983)
        + "\r"
        + "\r"
        + __content.string(4984)
        + "of "
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(cast::<Ext, _>(fine_amt), b"################,")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + __content.string(4985)
        + __content.string(3849)
        + &g.ent_name[target_corp];
    if ctrl_count == 1 {
        msg_text = msg_text + ". ";
    } else {
        msg_text = msg_text
            + __content.string(4986)
            + &g.ent_name[holder_id]
            + " "
            + __content.string(4987)
            + &g.ent_name[target_corp]
            + ". ";
    }
    msg_text = msg_text
        + "\r"
        + "\r"
        + __content.string(4988)
        + "\r"
        + "\r";
    if 3 < g.player_antitrust_level[g.cur_player] {
        msg_text = msg_text
            + __content.string(4989)
            + __content.string(4990)
            + &g.cur_player_name
            + "."
            + "\r"
            + "\r";
    }
    msg_text = msg_text + __content.string(4991);
    if 1 < ctrl_count {
        msg_text = msg_text + __content.string(4992) + &g.ent_name[target_corp];
    }
    msg_text = msg_text + "....";
    ui.msgbox(&msg_text, 8208, &s(__content.string(4993)));
    g.news_msg = s(__content.string(4994)) + &g.ent_symbol[target_corp];
    post_news(g, ui, 0, 0);
    if 1 < ctrl_count {
        divest_stock(g, ui, rng, target_corp, holder_id, 1);
        if find_stock_holding(g, holder_id, target_corp) <= 0 {
            if g.growth_rate[target_corp] < 20 {
                let new_growth = to_quad(rng.rnd() * Ext::from_f64(12.0) + Ext::from_f64(20.0));
                g.growth_rate[target_corp] = cast::<i16, _>(new_growth);
            }
            update_control_chain(g, ui, rng, target_corp);
            update_control_chain(g, ui, rng, holder_id);
            if len(g.ent_symbol[holder_id].as_bytes()) < 16 {
                g.news_msg = s(__content.string(4995))
                    + &g.ent_symbol[target_corp]
                    + " by "
                    + &g.ent_symbol[holder_id];
                post_news(g, ui, 0, 0);
            }
            check_private_antitrust_suit(g, ui, rng, target_corp, industry);
        }
    }
    post_news(g, ui, 0, 0);
    g.player_antitrust_level[g.cur_player] += 1;
    g.antitrust_act_key = g.cur_quarter + g.cur_year;
    let _ = f.corp_id;
}

/// Port of GovtAntitrust_IssueWarning (0x68212b), GOSUB of GovtAntitrustEnforcement.
/// For a player without previous antitrust record: the official warning (market share in words, the
/// leading company to give up), `PlayerAntitrustLevel = 1` and the key of this quarter; then the routine
/// exits (returns `true`). Otherwise RETURN (`false`).
fn govt_antitrust_issue_warning(g: &mut Globals, ui: &mut dyn Ui, f: &mut GovtFrame) -> bool {
    let __content = crate::content::current();
    if g.player_antitrust_level[g.cur_player] != 0 {
        return false;
    }
    f.corp_id = f.market_share;
    let words = number_to_words(f.corp_id, 0, 1);
    let mut msg_text = s(__content.string(4996))
        + &ucase(g.cur_player_name.as_bytes(), CaseMode::Ansi)
        + "\r"
        + "\r"
        + "The "
        + &f.agency_name
        + __content.string(4997)
        + __content.string(4998)
        + &words
        + __content.string(4999)
        + &g.ind_name[cast::<i32, _>(f.industry)]
        + __content.string(5000)
        + __content.string(5001)
        + __content.string(5002)
        + __content.string(5003)
        + __content.string(5004)
        + "\r"
        + "\r"
        + __content.string(5005);
    if 10 < f.target_corp && g.cur_player == g.corp_controller[f.target_corp] {
        msg_text =
            msg_text + __content.string(5006) + &g.ent_name[f.target_corp];
    }
    msg_text = msg_text
        + "."
        + "\r"
        + "\r"
        + __content.string(5007);
    ui.msgbox(&msg_text, 8208, &s(__content.string(5008)));
    g.player_antitrust_level[g.cur_player] = 1;
    g.antitrust_act_key = g.cur_quarter + g.cur_year;
    true
}

/// Port of AntitrustReview_PendingMsg (0x511cfd), GOSUB of AntitrustReviewAcquisition.
/// Once per review (and only for `mode = 0`): the "under review / decision is pending" pop-up.
fn antitrust_review_pending_msg(g: &mut Globals, ui: &mut dyn Ui, pending_shown: &mut i32, mode: i32) {
    let __content = crate::content::current();
    if *pending_shown == 0 && mode == 0 {
        let mut quick_msg = s(__content.string(5009))
            + __content.string(5010)
            + __content.string(5011)
            + "\r\n"
            + "\r\n"
            + __content.string(5012);
        show_quick_message(g, ui, &mut quick_msg);
    }
    *pending_shown = 1;
}

/// Port of AntitrustReviewAcquisition (0x510aec) with its GOSUBs AntitrustReview_PendingMsg (0x511cfd)
/// and AntitrustReview_ExitGosub (0x511e11, the common exit returning the result).
/// Government review of `TmpExt1` (acquirer) taking over `TmpExt2` (target) — `TmpExt3` is the mode
/// (0 = a player's acquisition, 1 = merger, 2+ = AI deals that are only barred); the three are cleared.
/// For every industry 3..69, the assets of the current player's companies (mode < 2) or of the acquirer's
/// group (and, with `review_mode = 3`, of players' companies) plus the target group's must stay at or
/// below 55% of the industry when the deal changes the share; else the deal is barred, except under the
/// lenient policy (combined share below 60, the own share below 20 — 68..72 / 38..43 one deal in eight)
/// where the player is warned once of civil suits (YES/NO, NO cancels: `-industry`). A barred deal of
/// mode > 0 only gets the headline (unless `review_mode = 3` or already in the news). For the player the
/// ruling is shown as a report; with `review_mode = 1` and a group company of the target in the industry
/// the deal may go on if the acquirer (or the target's controlling holder) divests that company (YES):
/// then the review restarts. Returns 1 when allowed (also with the cheat or a target the player already
/// controls), else `-industry`.
pub fn antitrust_review_acquisition(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    review_mode: i32,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut ruling_lines: Vec<PbStr> = vec![PbStr::new(); 91];
    let mut warn_text = PbStr::new();
    let mut pending_shown = 0;
    let mut blocked_code = 0;
    let mut divester_id = 0;
    let mut overlap_corp_id;
    let mut industry_assets: i64;
    let mut player_assets: i64;
    let mut target_assets: i64;
    let acquirer_id = to_long(g.tmp_ext1);
    let target_id = to_long(g.tmp_ext2);
    let mode = to_long(g.tmp_ext3);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if g.antitrust_cheat_used == 1 {
        return 1;
    }
    loop {
        if g.cur_player == g.corp_controller[target_id] && mode < 2 {
            return 1;
        }
        let mut verdict = 1;
        let mut industry_id: i16 = 3;
        // (the locals keep the values of the last industry checked)
        industry_assets = 0;
        player_assets = 0;
        target_assets = 0;
        overlap_corp_id = 0;
        while industry_id <= 69 {
            industry_assets = 0;
            player_assets = 0;
            target_assets = 0;
            overlap_corp_id = 0;
            for corp_idx in 131..=1600 {
                if g.entity_status[corp_idx] != 1 || g.industry_num[corp_idx] != industry_id {
                    continue;
                }
                let ba = g.business_assets[corp_idx];
                industry_assets = to_quad(ba + cast::<Ext, _>(industry_assets));
                let controller_id = g.corp_controller[corp_idx];
                if g.cur_player == controller_id && mode < 2 {
                    player_assets = to_quad(ba + cast::<Ext, _>(player_assets));
                    if acquirer_id != corp_idx {
                        overlap_corp_id = corp_idx;
                    }
                    continue;
                }
                if acquirer_id == controller_id || g.corp_controller[acquirer_id] == controller_id {
                    if 10 < controller_id || review_mode == 3 {
                        player_assets = to_quad(ba + cast::<Ext, _>(player_assets));
                    }
                    continue;
                }
                if target_id == corp_idx || target_id == controller_id {
                    target_assets = to_quad(ba + cast::<Ext, _>(target_assets));
                    if target_id != corp_idx && overlap_corp_id == 0 && review_mode < 3 && mode == 0
                    {
                        overlap_corp_id = corp_idx;
                    }
                }
            }
            let (ia, pa, ta) = (
                cast::<Ext, _>(industry_assets),
                cast::<Ext, _>(player_assets),
                cast::<Ext, _>(target_assets),
            );
            if Ext::from_f64(0.0) < ia && Ext::from_f64(0.0) < pa && Ext::from_f64(0.0) < ta && sf(0.55f32) < (pa + ta) / ia {
                let new_share_pct = to_long(Ext::from_f64(100.0) * ((pa + ta) / ia));
                let mut cur_share_pct = to_long(Ext::from_f64(100.0) * (pa / ia));
                if cur_share_pct != new_share_pct {
                    if cur_share_pct < 1 {
                        cur_share_pct = 1;
                    }
                    let mut combined_limit = 60;
                    let mut player_limit = 20;
                    if (target_id + acquirer_id + g.cur_year + g.cur_quarter) % 8 == 2 {
                        combined_limit = rng.rnd_range(4, 0) + 68;
                        player_limit = rng.rnd_range(5, 0) + 38;
                    }
                    if new_share_pct < combined_limit
                        && cur_share_pct < player_limit
                        && pending_shown == 0
                        && mode == 0
                        && warn_text.is_empty()
                    {
                        antitrust_review_pending_msg(g, ui, &mut pending_shown, mode);
                        warn_text = s(
                            __content.string(5013),
                        ) + __content.string(5014)
                            + &g.ind_name[cast::<i32, _>(industry_id)]
                            + __content.string(4980)
                            + __content.string(5015)
                            + __content.string(5016)
                            + "\r"
                            + "\r"
                            + __content.string(5017)
                            + __content.string(5018)
                            + &g.ind_name[cast::<i32, _>(industry_id)]
                            + __content.string(5019)
                            + __content.string(5020)
                            + __content.string(5021)
                            + &number_to_words(new_share_pct, 0, 1)
                            + __content.string(5022)
                            + __content.string(5023)
                            + &number_to_words(cur_share_pct, 0, 1)
                            + " now."
                            + "\r"
                            + "\r"
                            + __content.string(5024)
                            + __content.string(5025)
                            + "\r"
                            + "\r"
                            + __content.string(5026);
                        let answer = ui.msgbox(
                            &warn_text,
                            8228,
                            &s(__content.string(5027)),
                        );
                        if answer != 6 {
                            let mut m = s(__content.string(5028));
                            show_quick_message(g, ui, &mut m);
                            return -(cast::<i32, _>(industry_id));
                        }
                        industry_id += 1;
                        continue;
                    }
                    blocked_code = -(cast::<i32, _>(industry_id));
                    verdict = blocked_code;
                    break;
                }
            }
            industry_id += 1;
        }
        if verdict == 1 {
            return 1;
        }
        let blocked_ind = to_integer((cast::<Ext, _>(verdict)).abs());
        if 0 < mode {
            g.news_msg = s(__content.string(5029))
                + &g.ent_symbol[acquirer_id]
                + " and "
                + &g.ent_symbol[target_id]
                + __content.string(5030);
            if is_in_recent_headlines(g, __content.bytes(5031)) == 0 && review_mode != 3 {
                post_news(g, ui, 0, target_id);
            } else {
                g.news_msg = PbStr::new();
            }
            return blocked_code;
        }
        antitrust_review_pending_msg(g, ui, &mut pending_shown, mode);
        verdict = 0;
        let mut agency_name = s(__content.string(5032));
        if 10 < acquirer_id && g.country_idx[acquirer_id] == 0 {
            verdict = 1;
        }
        let country_sel = g.country_idx[target_id];
        if matches!(
            country_sel,
            3 | 4 | 11 | 12 | 14..=17 | 19 | 20 | 23 | 25 | 30 | 32 | 36 | 41 | 43 | 45 | 50
        ) {
            verdict = 2;
        }
        if verdict == 1 {
            agency_name = s(__content.string(5033));
        } else if verdict == 2 {
            agency_name = s(__content.string(5034));
        }
        let indent = string_fill(12, b" ");
        ruling_lines[4] = s(__content.string(5035));
        ruling_lines[6] = agency_name;
        ruling_lines[7] = s(__content.string(5036));
        ruling_lines[8] =
            s(__content.string(5037)) + &g.ind_name[cast::<i32, _>(blocked_ind)] + __content.string(5038);
        ruling_lines[9] = s(__content.string(5039));
        ruling_lines[10] = s("of ") + &g.ent_name[target_id] + __content.string(5040);
        let (ia, pa, ta) = (
            cast::<Ext, _>(industry_assets),
            cast::<Ext, _>(player_assets),
            cast::<Ext, _>(target_assets),
        );
        let cur_pct = to_long(Ext::from_f64(100.0) * (pa / ia));
        let new_share_pct = to_long(Ext::from_f64(100.0) * ((pa + ta) / ia));
        ruling_lines[12] =
            s(__content.string(5041)) + &str_s(cast::<Ext, _>(cur_pct)) + __content.string(5042);
        ruling_lines[13] = s(__content.string(5043))
            + &str_s(cast::<Ext, _>(new_share_pct))
            + __content.string(5044);
        agency_name = g.ent_name[target_id].clone() + ".";
        if 0 < g.hold_stock_id[(0, target_id)] {
            agency_name = g.ent_symbol[target_id].clone() + __content.string(5045);
        }
        ruling_lines[14] = s(__content.string(5046)) + &agency_name;
        for line in ruling_lines.iter_mut().take(15).skip(4) {
            *line = indent.clone() + &*line;
        }
        ruling_lines[16] = s("@END");
        for (k, line) in ruling_lines.iter().enumerate().skip(1) {
            g.text_lines[cast::<i32, _>(k)] = line.clone();
        }
        g.text_box_title = s(__content.string(5047));
        g.entity_pick_mode = PbStr::new();
        show_text_report(g, ui, rng);
        verdict = 0;
        if 10 < overlap_corp_id && mode == 0 && review_mode == 1 {
            update_corp_control(g, ui, rng, overlap_corp_id);
            if g.cur_player == g.voting_controller
                && cast::<i32, _>(g.largest_holder) == acquirer_id
                && 10 < acquirer_id
            {
                verdict = 1;
                divester_id = acquirer_id;
            } else if target_id == g.voting_controller
                && 10 < g.largest_holder
                && target_id == g.corp_controller[cast::<i32, _>(g.largest_holder)]
            {
                verdict = 1;
                divester_id = cast::<i32, _>(g.largest_holder);
            }
        }
        if verdict == 1 {
            let text = s(__content.string(5048))
                + __content.string(5049)
                + &g.ent_name[divester_id]
                + __content.string(5050)
                + __content.string(5051)
                + &g.ent_name[overlap_corp_id]
                + "."
                + "\r"
                + "\r"
                + __content.string(5052);
            let answer = ui.msgbox(&text, 8227, &s(__content.string(5053)));
            if answer == 6 {
                divest_stock(g, ui, rng, overlap_corp_id, divester_id, 1);
            } else {
                verdict = 0;
            }
        }
        if verdict != 1 {
            return blocked_code;
        }
    }
}
