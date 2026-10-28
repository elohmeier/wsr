//! The player's option purchase `BuyOptions` (0x5e340d) with its expiry-month chain `ShowOptionChain`
//! (0x5f0388) and the circular-ownership check `CheckCircularOptionHolding` (0x606e94).

// Keep the original's statement shapes.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord
)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::select::{get_selected_player_etf, in_box_symbol};
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::{balance_cash_t_bills, calc_cur_month_of_year};
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::{find_holding_slot, find_stock_holding};
use crate::logic::market::update_market_display;
use crate::logic::newgame_helpers::pick_etf_holding_stock;
use crate::logic::options::{
    add_or_remove_option_rec, calc_option_price, calc_spread_factor, get_max_options_pct, get_optioned_stock_pct,
    merge_matching_option_rec,
};
use crate::logic::report::number_to_words;
use crate::logic::reports_lists::show_option_positions;
use crate::logic::research::get_etf_description;
use crate::logic::restructure::merge::show_runtime_error;
use crate::logic::sim_entity::asset_sales::calc_group_stake_pct;
use crate::logic::sim_entity::etf::find_option_counterparty;
use crate::logic::valuation::{calc_stock_price, check_stock_split};
use crate::pb::ext80::Ext80;
use crate::pb::numeric::{str_num_ext, val_ext};
use crate::pb::string::mcase;
use crate::pb::numeric::Cux;

const FMT: &str = "* ##############.00";

/// `STR$(x)` of an 80-bit EXT value (15 digits).
fn str_e(x: Ext80) -> PbStr {
    PbStr::from(str_num_ext(x, 15).as_str())
}

/// `FIX(x)` on the x87 (truncation toward zero).
fn fix_e(x: Ext80) -> Ext80 {
    let r = x.frndint();
    if r.abs().cmp_val(x.abs()) == std::cmp::Ordering::Greater {
        if x.is_neg() {
            r.fadd(Ext80::from_i64(1))
        } else {
            r.fsub(Ext80::from_i64(1))
        }
    } else {
        r
    }
}

/// Port of ShowOptionChain (0x5f0388).
///
/// Lists the next 24 expiry months of `opt_type` (1 call / 2 put) options on `corp_id` with strike
/// `strike` (for 100% of the shares) as a pick list (`@C<row>` links, `EntityPickMode = "GetCorp"`): per
/// month the per-share `price_side` ("Bid": the option value, "Ask": value x `CalcSpreadFactor`, at least
/// 0.01; shown at least 0.05) and the cost / proceeds of 1%. Returns the picked row as
/// `"<month>|<year>|<ask>|<bid>"` (per-share prices), or "" when nothing was picked (`TargetCorp` is
/// cleared). `SharesOutstanding = 0` raises ERROR 151 (ShowRuntimeError "OptionMonthSelect"; returns "").
pub fn show_option_chain(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id_ref: i32,
    strike: Ext,
    opt_type: i32,
    price_side: &PbStr,
) -> PbStr {
    let __content = crate::content::current();
    let mut chain_recs: Vec<PbStr> = vec![PbStr::new(); 101];
    let corp_id = corp_id_ref;
    let mut opt_kind_str = s("Call");
    if opt_type == 2 {
        opt_kind_str = s("Put");
    }
    if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
        // ERROR 151 -> ON ERROR: ShowRuntimeError(ERR, "OptionMonthSelect", "", corpIdRef)
        show_runtime_error(g, ui, 151, &s(__content.string(12306)), &PbStr::new(), corp_id_ref);
        return PbStr::new();
    }
    let stock_price_str = fmt_trim(g.market_cap[corp_id] / g.shares_outstanding[corp_id], FMT);
    let progress = g.sim_co_count;
    let cur_month: Ext = crate::pb::numeric::double(calc_cur_month_of_year(g, progress)); // stored as DOUBLE
    let mut exp_month = to_long(cur_month);
    let mut exp_year = g.cur_year;
    let mut row = 1;
    while row <= 24 {
        exp_month = exp_month + 1;
        if 12 < exp_month {
            exp_month = 1;
            exp_year = exp_year + 1;
        }
        let mut link_tag = s("000") + &str_trim(cast::<Ext, _>(row));
        link_tag = s("@C") + &right(link_tag.as_bytes(), 4);
        let shares = g.shares_outstanding[corp_id];
        let bid_price = calc_option_price(g, ui, rng, corp_id, strike, exp_year, exp_month, opt_type, 0) / shares;
        let spread = calc_spread_factor(opt_type, g.market_cap[corp_id], strike);
        let mut ask_price = spread * bid_price;
        if ask_price < sf(0.01f32) {
            ask_price = sf(0.01f32);
        }
        let mut month_label = g.month_name[exp_month].clone() + "," + &str_s(cast::<Ext, _>(exp_year)) + &string_fill(12, b".");
        month_label = mid(month_label.as_bytes(), 1, 18);
        let mut shown_price = Cux::default();
        let price_label = if *price_side == "Bid" {
            shown_price.set(bid_price);
            s(__content.string(12307))
        } else {
            shown_price.set(ask_price);
            if shown_price.get() < sf(0.05f32) {
                shown_price.set(sf(0.05f32));
            }
            s("Cost")
        };
        let mut pct_cost_str = fmt_trim(shown_price.get() * g.shares_outstanding[corp_id], FMT);
        if val(pct_cost_str.as_bytes()) < sf(0.01f32) {
            pct_cost_str = s("0.01");
        }
        g.text_lines[row] = month_label
            + " "
            + price_side
            + __content.string(12308)
            + &fmt(shown_price.get(), "##############,.00")
            + "  ("
            + &price_label
            + __content.string(12309)
            + &trim(g.currency_symbol.as_bytes())
            + &pct_cost_str
            + " "
            + &g.million_prefix_uc
            + ".)"
            + &string_fill(80, b" ")
            + &link_tag;
        chain_recs[cast::<usize, _>(row)] = str_s(cast::<Ext, _>(exp_month))
            + "|"
            + &str_trim(cast::<Ext, _>(exp_year))
            + "|"
            + &str_trim(ask_price)
            + "|"
            + &str_trim(bid_price);
        row += 1;
    }
    g.text_lines[row + 2] = s(__content.string(12310));
    g.text_lines[row + 4] = s("@END");
    g.text_box_title = s("     ")
        + &opt_kind_str
        + __content.string(12311)
        + &fmt_trim(strike / g.shares_outstanding[corp_id], "#####,.00")
        + "   "
        + &g.ent_symbol[corp_id]
        + __content.string(12312)
        + &stock_price_str;
    g.entity_pick_mode = s("GetCorp");
    g.list_item_noun = lcase(opt_kind_str.as_bytes(), CaseMode::Ansi);
    g.text_box_mode = s("50");
    g.target_corp = 0;
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    if g.target_corp == 0 {
        PbStr::new()
    } else {
        let picked = g.target_corp;
        g.target_corp = 0;
        if 0 <= picked && (cast::<usize, _>(picked)) < chain_recs.len() {
            chain_recs[cast::<usize, _>(picked)].clone()
        } else {
            PbStr::new()
        }
    }
}

/// Port of CheckCircularOptionHolding (0x606e94).
///
/// May company `buyer_id` (players are never checked) buy calls / write puts on `stock_id`? Not if
/// `stock_id` holds stock of the buyer (over 5%, or any stake under the same controller: "Circular
/// [runtime resource wording] "), nor if `stock_id` owns the buyer indirectly ("... Prohibited"): it is
/// the largest holder of the buyer's largest holder, holds 20%+ (the most of any holder) of the buyer's
/// largest holder, or is found walking up the chain of largest holders from there (up to 500 steps; the
/// 500th step ends the check without a finding). Returns 1 (after the message box) when prohibited,
/// else 0. Side effect: `UpdateCorpControl` of the entities walked.
pub fn check_circular_option_holding(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, buyer_id: i32, stock_id: i32) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut result = 0;
    let mut extra_text = PbStr::new();
    let mut msg_text;
    let mut chain_depth = 0;
    let mut circular_id = 0;
    let mut best_pct = 0;
    let mut grand_holder_id = 0;
    if !(g.num_players < buyer_id) {
        return result;
    }
    let buyer = buyer_id;
    let hold_slot = find_stock_holding(g, stock_id, buyer);
    if 0 < hold_slot {
        let hold_pct = cast::<i32, _>(g.hold_stock_pct[(hold_slot, stock_id)]);
        if g.corp_controller[buyer] == g.corp_controller[stock_id] || 5 < hold_pct {
            if g.corp_controller[buyer] == g.corp_controller[stock_id] {
                extra_text = s(__content.string(12313)) + &g.cur_player_name;
            }
            msg_text = g.ent_name[buyer].clone()
                + __content.string(12314)
                + &g.ent_name[stock_id]
                + __content.string(12315)
                + &str_s(cast::<Ext, _>(hold_pct))
                + __content.string(2928)
                + &g.ent_name[buyer]
                + &extra_text
                + __content.string(12316)
                + __content.string(12317)
                + __content.string(12318);
            if g.currency_code == "USA" {
                msg_text = replace(msg_text.as_bytes(), __content.bytes(12319), b"S.E.C.");
            }
            ui.msgbox(&msg_text, 8256, &s(__content.string(12320)));
            return 1;
        }
    }
    update_corp_control(g, ui, rng, buyer);
    let holder_id = cast::<i32, _>(g.largest_holder);
    if 10 < holder_id {
        update_corp_control(g, ui, rng, holder_id);
        grand_holder_id = cast::<i32, _>(g.largest_holder);
        if 10 < grand_holder_id && stock_id == grand_holder_id {
            circular_id = grand_holder_id;
        }
        if circular_id < 1 {
            for walk_id in 11..=1600 {
                if grand_holder_id == walk_id || holder_id == walk_id {
                    continue;
                }
                if g.entity_status[walk_id] != 1 || g.hold_stock_id[(0, walk_id)] == 0 {
                    continue;
                }
                let slot = find_stock_holding(g, walk_id, holder_id);
                if slot <= 0 {
                    continue;
                }
                let hold_pct = cast::<i32, _>(g.hold_stock_pct[(slot, walk_id)]);
                if 19 < hold_pct && best_pct < hold_pct && stock_id == walk_id {
                    circular_id = walk_id;
                    best_pct = hold_pct;
                }
            }
            if circular_id == 0 && 10 < grand_holder_id {
                let mut walk_id = grand_holder_id;
                while 10 < walk_id {
                    chain_depth = chain_depth + 1;
                    if stock_id == walk_id {
                        circular_id = walk_id;
                        if 2 < chain_depth {
                            extra_text = s(" long");
                        }
                        break;
                    }
                    if 500 <= chain_depth {
                        return result;
                    }
                    update_corp_control(g, ui, rng, walk_id);
                    walk_id = cast::<i32, _>(g.largest_holder);
                }
            }
        }
    }
    if circular_id == 0 {
        return result;
    }
    msg_text = g.ent_name[buyer].clone()
        + __content.string(12321)
        + &g.ent_name[stock_id]
        + __content.string(12322)
        + &g.ent_name[buyer]
        + __content.string(12323)
        + &extra_text
        + __content.string(12324)
        + __content.string(12325)
        + __content.string(12326);
    if g.currency_code == "USA" {
        msg_text = replace(msg_text.as_bytes(), __content.bytes(12319), b"S.E.C.");
    }
    if grand_holder_id == stock_id && 10 < holder_id {
        msg_text = replace(msg_text.as_bytes(), __content.bytes(12327), g.ent_name[holder_id].as_bytes());
    }
    ui.msgbox(&msg_text, 8256, &s(__content.string(12328)));
    result = 1;
    result
}

/// The ON ERROR handler of BuyOptions (0x5e9177): `ShowRuntimeError(ERR, "FrontBuyOptions", "TGT =" +
/// STR$(corpId) + "  " + errInfo, ActiveEntity)`; the routine ends.
fn buy_options_error(g: &mut Globals, ui: &mut dyn Ui, err: i32, corp_id: i32, err_info: &PbStr) {
    let __content = crate::content::current();
    let diag = s("TGT =") + &str_s(cast::<Ext, _>(corp_id)) + "  " + err_info;
    show_runtime_error(g, ui, err, &s(__content.string(12329)), &diag, g.active_entity);
}

/// Port of BuyOptions (0x5e340d).
///
/// The player buys `opt_type_ref` (1 call / 2 put) options for the acting entity, or for an ETF he
/// advises (YES/NO/CANCEL; the bond and leveraged funds 1581..1585 may not trade options). Suspended in
/// the first month of the game. Checks, each ending with a message: liquidity, banks/insurers (calls only
/// to buy back, puts only as hedges), the stock (symbol entry, [`in_box_symbol`] with `InBox(3)`): one
/// position per stock for funds, no options on the own stock, sector funds' charters, insider rules
/// during a pending merger, circular holdings ([`check_circular_option_holding`]), the exchange's
/// position limit (`GetMaxOptionsPct`), distressed companies, share price 5..1000, the 1000-record
/// limit, no public float, insiders' / companies' hedging limits.
///
/// With short positions in the type on the stock the player may buy them back (YES): the record (picked
/// in `ShowOptionPositions` when several) is priced at the current option value x `CalcSpreadFactor`.
/// Otherwise the strike (75%..125% of the share price, at least 5) is entered and an expiry month picked
/// from [`show_option_chain`]. The percent (up to 10%, the public float, the exchange limit and what the
/// liquid funds incl. half the unused credit line pay for) is entered and confirmed; the cost includes a
/// 1% commission. A new position needs a counterparty (`FindOptionCounterparty`, risk fraction 0.25 /
/// 0.15 / 0.1 by the player's net worth); the cost is paid from cash (T-bills, then bank debt); a
/// buy-back books the gain or loss (capital gain of the player or extraordinary item) and shrinks or
/// removes the short record. Confirmation, `UpdateMarketDisplay`, news, revaluation of the stock
/// (`CalcStockPrice` mode 1). `time_secs` is `VAL(RIGHT$(TIME$, 2))` for the market display and valuation.
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn buy_options(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, opt_type_ref: i32, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x5e9177 (buy_options_error) for the ERROR 151 paths
    let err_info = PbStr::new();
    let mut msg_text;
    let mut msg_part = PbStr::new();
    let mut msg_part2;
    let mut answer = 0;
    let mut short_rec = 0;
    let mut net_pct: i32 = 0;
    let mut owned_pct: i16 = 0;
    let mut is_buyback = PbStr::new();
    let mut short_qty: i32 = 0;
    let mut max_pct: i64 = 0;
    let mut max_opt_pct: i32 = 0;
    let mut ok_flag = 0;
    let mut exp_month: i32 = 0;
    let mut exp_year: i32 = 0;
    let mut strike = Ext80::from_i64(0);
    let mut orig_premium = Ext80::from_i64(0);
    let mut ask_per_share = Ext80::from_i64(0);
    let mut bid_total: Ext = Ext::from_f64(0.0);
    let mut buyback_gain: Ext = Ext::from_f64(0.0);

    if g.cur_year == g.start_year && g.cur_quarter == 1 && g.sim_co_count < 503 {
        let mut m = s(__content.string(12330))
            + &str_s(cast::<Ext, _>(g.cur_year))
            + __content.string(12331);
        show_quick_message(g, ui, &mut m);
        return;
    }
    let opt_type = opt_type_ref;
    let price_side = s("Ask");
    let mut opt_kind_str = s("call");
    if opt_type == 2 {
        opt_kind_str = s("put");
    }
    let mut size_factor: Ext = sf(0.25f32);
    let cp = g.cur_player;
    calc_entity_financials(g, ui, rng, cp);
    if g.exchange_rate * Ext::from_f64(20000.0) < g.net_worth {
        size_factor = sf(0.15f32);
    }
    if g.exchange_rate * Ext::from_f64(50000.0) < g.net_worth {
        size_factor = sf(0.1f32);
    }
    let mut buyer_id = g.active_entity;
    let mut buyer_name = g.active_entity_name.clone();
    let mut etf_id = get_selected_player_etf(g, ui, rng);
    if 1585 < etf_id {
        msg_text = g.ent_name[etf_id].clone()
            + " ("
            + &g.ent_symbol[etf_id]
            + __content.string(3017)
            + __content.string(10156)
            + &g.ent_name[g.fund_adviser[etf_id]]
            + ". "
            + __content.string(12332)
            + "\r"
            + "\r"
            + __content.string(3020)
            + &g.ent_name[etf_id]
            + __content.string(12333)
            + &ucase(opt_kind_str.as_bytes(), CaseMode::Ansi)
            + __content.string(12334)
            + "\r"
            + "\r"
            + __content.string(3567)
            + &g.active_entity_name
            + " buy "
            + &opt_kind_str
            + "s.)";
        answer = ui.msgbox(&msg_text, 8483, &s(__content.string(11901)));
        if answer == 2 {
            return;
        }
        if answer == 7 {
            etf_id = 0;
        } else {
            buyer_id = etf_id;
            buyer_name = g.ent_name[etf_id].clone();
        }
    } else if 1580 < etf_id {
        msg_part = s(__content.string(12335));
        if 1583 < etf_id {
            msg_part = s(__content.string(12336));
        }
        msg_part2 = g.active_entity_name.clone();
        if g.active_entity < 11 {
            msg_part2 = s(__content.string(12337));
        }
        msg_text = s("While ")
            + &g.ent_name[etf_id]
            + __content.string(12338)
            + &msg_part
            + __content.string(12339)
            + "\r"
            + "\r"
            + __content.string(12340)
            + &msg_part2
            + ".";
        ui.msgbox(&msg_text, 8208, &s(__content.string(12341)));
        etf_id = 0;
    }
    balance_cash_t_bills(g, buyer_id);
    calc_entity_financials(g, ui, rng, buyer_id);
    if g.unused_credit_line == Ext::from_f64(0.0) && !(g.t_bills[buyer_id] + g.cash[buyer_id] > Ext::from_f64(0.0)) {
        msg_text = s(__content.string(11729))
            + __content.string(11730);
        if 10 < buyer_id {
            let with = g.ent_symbol[buyer_id].clone() + " has";
            msg_text = replace(msg_text.as_bytes(), __content.bytes(1506), with.as_bytes());
        }
        ui.msgbox(&msg_text, 8208, &s(__content.string(12342)));
        return;
    }
    g.tmp_symbol = PbStr::new();
    if 10 < buyer_id && g.industry_num[buyer_id] < 3 {
        net_pct = find_option_positions(g, ui, buyer_id, 0, 1, 0);
        msg_part = s("Banks");
        if g.industry_num[buyer_id] == 2 {
            msg_part = s(__content.string(2267));
        }
        if net_pct != -11 && net_pct != 12 && opt_type == 1 {
            msg_text = msg_part.clone()
                + __content.string(12343)
                + __content.string(12344)
                + "\r\n"
                + "\r\n"
                + __content.string(12345);
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
        if opt_type == 2 && g.tmp_subsid_stock_value == Ext::from_f64(0.0) {
            msg_text = msg_part.clone()
                + __content.string(12346)
                + &buyer_name
                + __content.string(12347);
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
    }
    msg_text = s(__content.string(11815));
    msg_part = s("you (") + &g.cur_player_name + ") wish";
    if 10 < buyer_id {
        msg_part = buyer_name.clone() + " wishes";
    }
    msg_text = msg_text
        + __content.string(12348)
        + &msg_part
        + __content.string(12349)
        + &opt_kind_str
        + __content.string(12350)
        + __content.string(12351)
        + "\r\n"
        + "\r\n";
    g.input_dlg_label = PbStr::new();
    g.input_dlg_prompt = msg_text.clone();
    g.input_dlg_title = s(__content.string(12352)) + &mcase(opt_kind_str.as_bytes(), CaseMode::Ansi) + __content.string(12353);
    if 10 < buyer_id {
        g.input_dlg_title = g.input_dlg_title.clone() + " for " + &g.ent_symbol[buyer_id];
    }
    g.lookup_filter = PbStr::new();
    if 1585 < etf_id {
        g.action_code = s("ETF_TRADES");
    }
    in_box_symbol(g, ui, rng);
    g.action_code = PbStr::new();
    msg_text = PbStr::new();
    if g.tmp_symbol.is_empty() {
        return;
    }
    let corp_id = g.target_corp;
    g.target_corp = 0;
    if opt_type_ref == 1 {
        answer = find_option_positions(g, ui, buyer_id, corp_id, 1, 0);
        if answer == -1 || answer == 2 {
            short_rec = 1;
        }
    } else {
        answer = find_option_positions(g, ui, buyer_id, corp_id, 2, 0);
        if answer == -31 || answer == 32 {
            short_rec = 1;
        }
    }
    if 1585 < etf_id && 0 < answer {
        short_rec = short_rec + 2;
    }
    if short_rec == 2 {
        msg_text = buyer_name.clone()
            + __content.string(12354)
            + &opt_kind_str
            + __content.string(12355)
            + &opt_kind_str
            + " "
            + __content.string(12356)
            + &g.ent_name[corp_id]
            + "."
            + "\r\n"
            + "\r\n"
            + __content.string(3246);
        show_quick_message(g, ui, &mut msg_text);
        return;
    }
    if buyer_id == corp_id && short_rec == 0 {
        let mut m = buyer_name.clone() + __content.string(12357);
        show_quick_message(g, ui, &mut m);
        return;
    }
    if short_rec == 0 && opt_type == 2 && 10 < buyer_id && g.industry_num[buyer_id] < 3 {
        let slot = cast::<i32, _>(cast::<i16, _>(find_holding_slot(g, buyer_id, corp_id)));
        if !(slot != 0 && cast::<i32, _>(g.hold_stock_id[(slot, buyer_id)]) == corp_id) {
            msg_text = s("Banks");
            msg_part = s(__content.string(12358));
            if g.industry_num[buyer_id] == 2 {
                msg_text = s(__content.string(2267));
                msg_part = s(__content.string(12359));
            }
            msg_text = msg_text
                + __content.string(12360)
                + &msg_part
                + __content.string(12361)
                + __content.string(12362)
                + "\r\n"
                + "\r\n"
                + __content.string(12345);
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
    }
    // Buying back a short position goes straight to the pricing (0x5e6ff6).
    let mut buyback_entry = false;
    if short_rec == 1 {
        if buyer_id == corp_id {
            answer = 6;
            let mut m = buyer_name.clone()
                + __content.string(12363);
            show_quick_message(g, ui, &mut m);
        } else {
            msg_text = s(__content.string(12364))
                + &opt_kind_str
                + __content.string(12365)
                + __content.string(9409)
                + &g.ent_name[corp_id]
                + "."
                + "\r"
                + "\r"
                + __content.string(12366);
            answer = ui.msgbox(&msg_text, 8483, &s(__content.string(12367)));
            if answer == 2 {
                return;
            }
            if answer == 7 && 1585 < etf_id {
                msg_text = s(__content.string(12368))
                    + &buyer_name
                    + __content.string(12369)
                    + &opt_kind_str
                    + __content.string(12370)
                    + __content.string(12371)
                    + &opt_kind_str
                    + __content.string(12372)
                    + &g.ent_name[corp_id]
                    + __content.string(12373);
                show_quick_message(g, ui, &mut msg_text);
                return;
            }
        }
        if answer == 6 {
            is_buyback = s("Y");
            short_rec = 0;
            let mut short_count = 0;
            for rec_idx in 1..=1000 {
                let field = |g: &Globals, n: i32| val(parse_any(g.option_recs[rec_idx].as_bytes(), b"|", n).as_bytes());
                if field(g, 1) == cast::<Ext, _>(corp_id)
                    && field(g, 2) == cast::<Ext, _>(opt_type)
                    && field(g, 3) == cast::<Ext, _>(buyer_id)
                    && field(g, 4) <= Ext::from_f64(0.0)
                {
                    short_count = short_count + 1;
                    short_rec = rec_idx;
                }
            }
            if 1 < short_count {
                let list_mode: i64 = if opt_type == 2 { 4 } else { 3 };
                short_rec = show_option_positions(g, ui, rng, buyer_id, cast::<i32, _>(list_mode), corp_id);
                if short_rec < 1 {
                    return;
                }
            } else if short_count < 1 {
                short_rec = 0;
                let mut m = s(__content.string(12374));
                show_quick_message(g, ui, &mut m);
            }
            if 0 < short_rec {
                let opt_rec = g.option_recs[short_rec].clone();
                let field_e = |n: i32| val_ext(parse_any(opt_rec.as_bytes(), b"|", n).as_bytes());
                short_qty = field_e(4).to_i32();
                max_pct = (cast::<i64, _>(short_qty)).abs();
                strike = field_e(5).abs();
                exp_month = field_e(6).to_i32();
                exp_year = field_e(7).to_i32();
                orig_premium = field_e(8);
                bid_total = calc_option_price(g, ui, rng, corp_id, Ext::from(strike.into_ext()), exp_year, exp_month, opt_type, 0);
                if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
                    g.shares_outstanding[corp_id] = Ext::from_f64(2.0);
                    check_stock_split(g, corp_id);
                    buy_options_error(g, ui, 151, corp_id, &s(__content.string(12375)));
                    return;
                }
                let spread = calc_spread_factor(opt_type, g.market_cap[corp_id], Ext::from(strike.into_ext()));
                ask_per_share = Ext80::from_ext(spread * (bid_total / g.shares_outstanding[corp_id]));
                buyback_entry = true;
            }
        } else {
            short_rec = 0;
        }
    }
    if !buyback_entry {
        if 1585 < etf_id && pick_etf_holding_stock(g, ui, rng, buyer_id, corp_id) == 0 {
            msg_text = s(__content.string(12376))
                + __content.string(12377);
            if buyer_id == 1593 {
                msg_text = msg_text
                    + &buyer_name
                    + __content.string(12378)
                    + __content.string(11930)
                    + &g.ent_name[corp_id]
                    + __content.string(11931)
                    + &g.country_name[g.country_idx[corp_id]]
                    + __content.string(11932);
            } else {
                msg_text = msg_text
                    + &g.ent_name[corp_id]
                    + __content.string(11933)
                    + &g.ind_name[cast::<i32, _>(g.industry_num[corp_id])]
                    + __content.string(1037)
                    + __content.string(11934)
                    + &buyer_name
                    + __content.string(11935)
                    + "\r"
                    + "\r"
                    + &get_etf_description(buyer_id)
                    + "\r"
                    + __content.string(11936);
            }
            if g.industry_num[corp_id] == 71 {
                msg_text = buyer_name.clone() + __content.string(12379);
            }
            let text = msg_text.clone() + "\r" + "\r" + __content.string(12380);
            ui.msgbox(&text, 8208, &s(__content.string(12381)));
            return;
        }
        msg_text = PbStr::new();
        msg_part = PbStr::new();
        if short_rec == 0 && 10 < buyer_id && buyer_id < 131 {
            msg_text = s("Banks");
            msg_part = s(__content.string(12358));
            if g.industry_num[buyer_id] == 2 {
                msg_text = s(__content.string(2267));
                msg_part = s(__content.string(12359));
            }
            if opt_type == 1 {
                msg_text = msg_text
                    + __content.string(12360)
                    + &msg_part
                    + __content.string(12361)
                    + __content.string(12382);
            } else {
                msg_text = PbStr::new();
                msg_part = PbStr::new();
            }
        }
        if 10 < g.merger_target && corp_id == g.merger_target && etf_id == 0 {
            let stake = calc_group_stake_pct(g, cp, corp_id);
            net_pct = to_long(g.tmp_ext1 + cast::<Ext, _>(stake));
            if 5 <= net_pct {
                msg_text = s(__content.string(11718))
                    + &g.ent_name[corp_id]
                    + __content.string(11719)
                    + __content.string(11720)
                    + &g.ent_symbol[corp_id]
                    + __content.string(11721)
                    + __content.string(11722)
                    + &g.ent_name[g.merger_acquirer]
                    + "."
                    + "\r"
                    + "\r"
                    + __content.string(11723)
                    + "\r"
                    + "\r"
                    + __content.string(11724)
                    + __content.string(11725)
                    + &g.ent_symbol[corp_id]
                    + __content.string(11726)
                    + __content.string(12383)
                    + "\r"
                    + "\r"
                    + __content.string(11728)
                    + &number_to_words(net_pct, 0, 1)
                    + ".";
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
        }
        if opt_type == 1
            && msg_text.is_empty()
            && etf_id == 0
            && check_circular_option_holding(g, ui, rng, buyer_id, corp_id) == 1
        {
            msg_text = s(__content.string(12384));
            update_corp_control(g, ui, rng, corp_id);
            if g.public_pct == 0 {
                msg_text = PbStr::new();
            }
        }
        max_opt_pct = get_max_options_pct(g, corp_id, opt_type, &s("L"), 0);
        if max_opt_pct == 0 && msg_text.is_empty() {
            msg_text = s(__content.string(12385))
                + &opt_kind_str
                + __content.string(12386)
                + __content.string(12387)
                + __content.string(12388)
                + __content.string(12389)
                + __content.string(12390)
                + "\r"
                + "\r"
                + __content.string(12391)
                + &opt_kind_str
                + " "
                + __content.string(12392)
                + &g.ent_name[corp_id]
                + __content.string(12393)
                + __content.string(12394);
            if opt_type == 1 {
                msg_text = replace(msg_text.as_bytes(), b"100%", b"40%");
            }
        }
        ok_flag = 1;
        if g.credit_rating_num[corp_id] == 1 && !(g.proj_qtr_earnings[corp_id] > Ext::from_f64(0.0)) {
            ok_flag = 0;
        }
        if g.credit_rating_num[corp_id] == 1 && g.industry_num[corp_id] == 1 {
            ok_flag = 0;
        }
        if g.credit_rating_num[corp_id] == 0 {
            ok_flag = 0;
        }
        if ok_flag == 0 && msg_text.is_empty() {
            msg_text = s(__content.string(11715))
                + &g.ent_name[corp_id]
                + __content.string(11716)
                + __content.string(11717);
        }
        ok_flag = 0;
        if !msg_text.is_empty() {
            ui.msgbox(&msg_text, 8208, &s(__content.string(11731)));
            return;
        }
        if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
            buy_options_error(g, ui, 151, corp_id, &err_info);
            return;
        }
        let share_price = g.market_cap[corp_id] / g.shares_outstanding[corp_id];
        if share_price < Ext::from_f64(5.0) || Ext::from_f64(1000.0) < share_price {
            msg_part = s(__content.string(12395)) + &trim(g.currency_symbol.as_bytes()) + "5";
            if Ext::from_f64(1000.0) < share_price {
                msg_part = s(__content.string(12396)) + &trim(g.currency_symbol.as_bytes()) + "1000";
            }
            msg_text = s(__content.string(12397))
                + &g.ent_name[corp_id]
                + ", a "
                + &msg_part
                + &g.currency_suffix
                + __content.string(1071);
            msg_part2 = s(__content.string(3311));
            if Ext::from_f64(1000.0) < share_price {
                msg_part2 = PbStr::new();
            }
            if g.cur_player == g.corp_controller[corp_id] {
                msg_text = msg_text + __content.string(12398) + &msg_part2 + __content.string(12399);
            }
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
        is_buyback = s("N");
        if Ext::from_f64(998.0) <= val(g.option_recs[0].as_bytes()) {
            let mut m = s(__content.string(12400))
                + __content.string(12401)
                + __content.string(11692);
            show_quick_message(g, ui, &mut m);
            return;
        }
        update_corp_control(g, ui, rng, corp_id);
        if g.public_pct == 0 {
            let mut m = s(__content.string(12402))
                + &g.ent_name[corp_id]
                + __content.string(12403);
            show_quick_message(g, ui, &mut m);
            return;
        }
        max_pct = 10;
        let public_pct = cast::<i64, _>(g.public_pct);
        if public_pct < max_pct && opt_type == 1 {
            max_pct = public_pct;
        }
        if public_pct - 1 < max_pct && 1585 < etf_id && opt_type == 1 {
            max_pct = public_pct - 1;
            if max_pct == 0 {
                let mut m = s(__content.string(12402))
                    + &g.ent_name[buyer_id]
                    + __content.string(12404)
                    + __content.string(12405)
                    + &g.ent_name[corp_id]
                    + __content.string(12406)
                    + "of "
                    + &g.ent_name[corp_id]
                    + __content.string(12407);
                show_quick_message(g, ui, &mut m);
                return;
            }
        }
        let mut hedge_check = 0;
        if opt_type_ref == 2 {
            if g.cur_player == g.corp_controller[corp_id] {
                hedge_check = 1;
            }
            if 10 < buyer_id && g.industry_num[buyer_id] < 3 {
                hedge_check = 1;
            }
            if max_opt_pct < 10 {
                hedge_check = 1;
            }
        } else {
            net_pct = 10;
        }
        if 1585 < etf_id {
            hedge_check = 0;
        }
        if hedge_check == 1 {
            net_pct = get_optioned_stock_pct(g, ui, buyer_id, corp_id, 0);
            let slot = cast::<i32, _>(cast::<i16, _>(find_holding_slot(g, buyer_id, corp_id)));
            if 0 < slot && cast::<i32, _>(g.hold_stock_id[(slot, buyer_id)]) == corp_id {
                owned_pct = g.hold_stock_pct[(slot, buyer_id)];
            }
            net_pct = cast::<i32, _>(owned_pct) - net_pct;
            if !(10 < buyer_id && g.industry_num[buyer_id] < 3) && g.cur_player != g.corp_controller[corp_id] {
                net_pct = 10;
            }
            if net_pct < 1 {
                msg_text = s(__content.string(12408))
                    + &g.ent_symbol[corp_id]
                    + __content.string(12409)
                    + __content.string(12410)
                    + __content.string(12411)
                    + __content.string(12412)
                    + &trim(str_num(cast::<Ext, _>(owned_pct), 7).as_bytes())
                    + __content.string(12413)
                    + __content.string(12414)
                    + __content.string(12415)
                    + "\r"
                    + "\r";
                if 10 < buyer_id {
                    msg_part = s(__content.string(12416));
                    if g.industry_num[buyer_id] == 1 {
                        msg_part = s(__content.string(12417));
                    }
                    if g.industry_num[buyer_id] == 2 {
                        msg_part = s(__content.string(12418));
                    }
                    msg_text = s("A")
                        + &msg_part
                        + __content.string(12419)
                        + __content.string(12420);
                    if g.industry_num[buyer_id] < 3 {
                        msg_text = msg_text + "." + "\r" + "\r";
                    } else {
                        msg_text = msg_text
                            + __content.string(12421)
                            + "\r"
                            + "\r";
                    }
                }
                if owned_pct == 0 {
                    if g.cur_player == buyer_id {
                        msg_text = msg_text + __content.string(12422);
                    } else {
                        msg_text = msg_text + &buyer_name + __content.string(12423);
                    }
                    msg_text = msg_text + __content.string(12424) + &g.ent_symbol[corp_id] + "." + "\r" + "\r";
                } else {
                    msg_text = msg_text
                        + __content.string(12425)
                        + &g.ent_symbol[corp_id]
                        + __content.string(12426)
                        + "\r"
                        + "\r";
                }
                if (0 < find_option_positions(g, ui, buyer_id, corp_id, 1, 0)
                    || Ext::from_f64(30.0) < (cast::<Ext, _>(find_option_positions(g, ui, buyer_id, corp_id, 2, 0))).abs())
                    && !(10 < buyer_id && buyer_id < 131)
                {
                    msg_text = msg_text + __content.string(12427) + "\r" + "\r";
                }
                msg_text = msg_text + __content.string(12428);
                msg_part = s(__content.string(3253));
                if 10 < buyer_id && g.cur_player != g.corp_controller[corp_id] {
                    msg_part = s(__content.string(11731));
                }
                ui.msgbox(&msg_text, 8208, &msg_part);
                return;
            }
            let skip = max_opt_pct < net_pct && (cast::<i64, _>(max_opt_pct)) < max_pct;
            if !skip && (cast::<i64, _>(net_pct)) < max_pct {
                max_pct = cast::<i64, _>(net_pct);
                msg_text = s(__content.string(12429))
                    + &str_s(cast::<Ext, _>(max_pct))
                    + __content.string(2163)
                    + &g.ent_symbol[corp_id]
                    + __content.string(12430)
                    + __content.string(12431)
                    + __content.string(12432)
                    + "\r"
                    + "\r";
                if 10 < buyer_id {
                    msg_part = s(__content.string(12433));
                    if g.industry_num[buyer_id] == 2 {
                        msg_part = s(__content.string(2267));
                    }
                    if 2 < g.industry_num[buyer_id] {
                        msg_part = s(__content.string(12434));
                    }
                    msg_text = buyer_name.clone()
                        + __content.string(12435)
                        + &str_s(cast::<Ext, _>(max_pct))
                        + __content.string(2163)
                        + &g.ent_symbol[corp_id]
                        + ". "
                        + &msg_part
                        + __content.string(12436)
                        + __content.string(12437);
                    if g.industry_num[buyer_id] < 3 {
                        msg_text = msg_text + "." + "\r" + "\r";
                    } else {
                        msg_text = msg_text + __content.string(12438) + __content.string(12439) + "\r" + "\r";
                    }
                }
                if max_pct == cast::<i64, _>(owned_pct) {
                    msg_part = s(__content.string(12440));
                    if 10 < buyer_id {
                        msg_part = g.ent_symbol[buyer_id].clone() + __content.string(12441);
                    }
                    msg_text = msg_text
                        + &msg_part
                        + &PbStr::from(str_num(cast::<Ext, _>(owned_pct), 7).as_str())
                        + __content.string(2163)
                        + &g.ent_symbol[corp_id]
                        + ".";
                } else {
                    msg_part = s("your");
                    if 10 < buyer_id {
                        msg_part = g.ent_symbol[buyer_id].clone() + "'s";
                    }
                    msg_text = msg_text
                        + "All but"
                        + &str_s(cast::<Ext, _>(max_pct))
                        + "% of "
                        + &msg_part
                        + __content.string(9545)
                        + &g.ent_symbol[corp_id]
                        + __content.string(12442);
                }
                if (0 < find_option_positions(g, ui, buyer_id, corp_id, 1, 0)
                    || Ext::from_f64(30.0) < (cast::<Ext, _>(find_option_positions(g, ui, buyer_id, corp_id, 2, 0))).abs())
                    && !(10 < buyer_id && buyer_id < 131)
                {
                    msg_text = msg_text + "\r" + "\r" + __content.string(12427);
                }
                msg_part = s(__content.string(12443));
                if 10 < buyer_id {
                    msg_part = s("Hedging");
                }
                let title = msg_part.clone() + __content.string(12444);
                ui.msgbox(&msg_text, 8192, &title);
            }
        }
        if max_opt_pct < net_pct && (cast::<i64, _>(max_opt_pct)) < max_pct {
            let mut exch_limit_pct = 100;
            if opt_kind_str == "call" {
                exch_limit_pct = 40;
            }
            msg_text = s(__content.string(12445))
                + &opt_kind_str
                + " "
                + __content.string(12446)
                + &str_s(cast::<Ext, _>(exch_limit_pct - max_opt_pct))
                + __content.string(2163)
                + &g.ent_symbol[corp_id]
                + __content.string(12447)
                + __content.string(12448)
                + &opt_kind_str
                + "s is"
                + &str_s(cast::<Ext, _>(exch_limit_pct))
                + "%."
                + "\r"
                + "\r"
                + __content.string(12449)
                + &opt_kind_str
                + __content.string(12365)
                + __content.string(12450)
                + &str_s(cast::<Ext, _>(max_opt_pct))
                + __content.string(2163)
                + &g.ent_name[corp_id]
                + ".";
            max_pct = cast::<i64, _>(max_opt_pct);
            if etf_id < 1586 {
                ui.msgbox(&msg_text, 8256, &s(__content.string(12451)));
            }
        }
        if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
            buy_options_error(g, ui, 151, corp_id, &err_info);
            return;
        }
        let mut min_strike = (g.market_cap[corp_id] / g.shares_outstanding[corp_id] * sf(0.75f32)).abs();
        if min_strike < Ext::from_f64(5.0) {
            min_strike = Ext::from_f64(5.0);
        }
        let max_strike = (g.market_cap[corp_id] / g.shares_outstanding[corp_id] * sf(1.25f32)).abs();
        msg_text = s(__content.string(12452))
            + &opt_kind_str
            + __content.string(12453)
            + &fmt_trim(min_strike, FMT)
            + __content.string(12454)
            + &fmt_trim(max_strike, FMT)
            + __content.string(12455);
        let entered = loop {
            let price_str = fmt_trim(g.market_cap[corp_id] / g.shares_outstanding[corp_id], FMT);
            let title = g.ent_symbol[corp_id].clone() + __content.string(12456) + &price_str;
            let input_str = ui.inputbox(&msg_text, &title, &price_str);
            let v = val_ext(remove_any(input_str.as_bytes(), b"$ ,%").as_bytes());
            if v.is_zero() {
                return;
            }
            let vf = v.into_ext();
            if !(vf < min_strike - sf(0.01f32) || max_strike + sf(0.01f32) < vf) {
                break v;
            }
        };
        strike = entered.fmul(Ext80::from_ext(g.shares_outstanding[corp_id])).abs();
    }

    // The expiry month (ShowOptionChain) and the percent; a buy-back enters at the pricing.
    let pct_to_buy: i32;
    let mut ask_total: Ext80;
    'chain: loop {
        if !buyback_entry {
            let opt_rec = loop {
                let r = show_option_chain(g, ui, rng, corp_id, Ext::from(strike.into_ext()), opt_type, &price_side);
                if !r.is_empty() {
                    break r;
                }
                answer = ui.msgbox(
                    &s(__content.string(12457)),
                    8723,
                    &s(__content.string(12458)),
                );
                if answer != 7 {
                    return;
                }
            };
            let field_e = |n: i32| val_ext(parse_any(opt_rec.as_bytes(), b"|", n).as_bytes());
            exp_month = field_e(1).to_i32();
            exp_year = field_e(2).to_i32();
            ask_per_share = field_e(3);
            bid_total = Ext::from((Ext80::from_ext(g.shares_outstanding[corp_id]).fmul(field_e(4))).into_ext());
        }
        buyback_entry = false;
        // 0x5e6ff6
        let shares = Ext80::from_ext(g.shares_outstanding[corp_id]);
        ask_total = ask_per_share.fmul(shares);
        if ask_total.fdiv(shares).into_ext() < sf(0.05f32) {
            let mut min_ask = Cux::default();
            min_ask.set_ext(ask_total.fdiv(shares));
            if min_ask.get() < sf(0.05f32) {
                min_ask.set(sf(0.05f32));
            }
            ask_total = min_ask.get_ext().fmul(shares);
        }
        calc_entity_financials(g, ui, rng, buyer_id);
        if is_buyback != "Y" {
            g.liquid_funds = g.t_bills[buyer_id] + g.cash[buyer_id] + g.unused_credit_line * sf(0.5f32);
        }
        if ask_total.fmul(Ext80::from_i64(max_pct)).into_ext() < sf(0.01f32) {
            ask_total = Ext80::from_ext(sf(0.01f32));
        }
        if g.liquid_funds < ask_total.into_ext() {
            max_pct = 0;
        } else {
            if !(ask_total.into_ext() > Ext::from_f64(0.0)) {
                buy_options_error(g, ui, 151, corp_id, &err_info);
                return;
            }
            let tmp_val = Ext80::from_ext(g.liquid_funds).fdiv(ask_total);
            let mut afford_pct = fix_e(tmp_val).to_i64();
            if afford_pct < 0 {
                afford_pct = max_pct;
            }
            if afford_pct < max_pct {
                max_pct = afford_pct;
            }
        }
        if max_pct < 1 {
            if is_buyback == "Y" {
                msg_part = s(__content.string(12459));
                if 10 < buyer_id {
                    msg_part = buyer_name.clone() + " needs";
                }
                msg_text = s(__content.string(12460))
                    + &opt_kind_str
                    + __content.string(12461)
                    + &g.ent_name[corp_id]
                    + ". "
                    + &msg_part
                    + __content.string(12462);
                show_quick_message(g, ui, &mut msg_text);
                return;
            }
            msg_text = s(__content.string(12463))
                + &opt_kind_str
                + __content.string(12464)
                + &g.ent_symbol[corp_id]
                + " stock."
                + "\r"
                + "\r"
                + __content.string(12465)
                + __content.string(12466);
            if 10 < buyer_id {
                let with = g.ent_symbol[buyer_id].clone() + " lacks";
                msg_text = replace(msg_text.as_bytes(), __content.bytes(12467), with.as_bytes());
            }
            answer = ui.msgbox(&msg_text, 8483, &s(__content.string(12046)));
            if answer == 6 {
                continue 'chain;
            }
            return;
        }
        msg_text = PbStr::new();
        if is_buyback == "Y" {
            msg_part = s("back ");
        } else {
            if buyer_id < 11 {
                msg_text = s(__content.string(12468));
            }
            msg_part = PbStr::new();
        }
        msg_text = msg_text
            + __content.string(12469)
            + &msg_part
            + &opt_kind_str
            + __content.string(12470)
            + &str_s(cast::<Ext, _>(max_pct))
            + __content.string(2163)
            + &g.ent_symbol[corp_id]
            + __content.string(12471);
        let ask_ps = ask_total.fdiv(shares).into_ext();
        if val(fmt_trim(Ext::from(ask_ps), FMT).as_bytes()) < sf(0.01f32) {
            msg_text = msg_text + __content.string(12472);
        } else {
            msg_text = msg_text + &fmt_trim(Ext::from(ask_ps), FMT) + ". ";
        }
        msg_text = msg_text + __content.string(12473) + &str_s(cast::<Ext, _>(max_pct)) + __content.string(12474);
        let mut corp_label = g.ent_name[corp_id].clone();
        if 8 < len(corp_label.as_bytes()) {
            corp_label = g.ent_symbol[corp_id].clone();
        }
        let pct = loop {
            let title = s("Buy ")
                + &corp_label
                + &str_s(cast::<Ext, _>(exp_month))
                + "/"
                + &str_trim(cast::<Ext, _>(exp_year))
                + " "
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(strike.into_ext() / g.shares_outstanding[corp_id], FMT)
                + " "
                + &mcase(opt_kind_str.as_bytes(), CaseMode::Ansi)
                + "s";
            let input_str = ui.inputbox(&msg_text, &title, &str_s(cast::<Ext, _>(max_pct)));
            let p = to_long(val(remove_any(input_str.as_bytes(), b"$ ,%").as_bytes()));
            if p < 1 {
                return;
            }
            if cast::<i64, _>(p) <= max_pct {
                ok_flag = 1;
            }
            if ok_flag == 1 {
                break p;
            }
        };
        pct_to_buy = pct;
        break;
    }
    let mut pct_to_buy = pct_to_buy;
    let shares = Ext80::from_ext(g.shares_outstanding[corp_id]);
    // askTotal * pctToBuy * 1.01 (a SINGLE constant)
    let total_cost_e = ask_total.fmul(Ext80::from_i64(cast::<i64, _>(pct_to_buy))).fmul(Ext80::from_f32(1.01_f32));
    let total_cost = total_cost_e.into_ext();
    msg_text = s(__content.string(12475))
        + "\r"
        + "\r"
        + __content.string(12476)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(Ext::from(total_cost), "* ###,.00")
        + " "
        + &g.million_prefix_lc
        + __content.string(2954)
        + __content.string(12477)
        + "\r"
        + "\r"
        + __content.string(12478)
        + "\r"
        + "\r";
    if is_buyback == "Y" && short_qty != 0 {
        let cover_frac = if short_qty.abs() == pct_to_buy {
            Ext80::from_i64(1)
        } else {
            Ext80::from_i64(cast::<i64, _>(pct_to_buy)).fdiv(Ext80::from_i64(cast::<i64, _>(short_qty.abs())))
        };
        let tmp_val = orig_premium.fmul(cover_frac).abs().fsub(total_cost_e).into_ext();
        if Ext::from_f64(0.0) < tmp_val {
            msg_text = msg_text
                + __content.string(12479)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(Ext::from(tmp_val), "* #############,.00")
                + " "
                + &g.million_prefix_lc
                + __content.string(12480);
            if 1585 < buyer_id && g.industry_num[buyer_id] == 71 {
                msg_text = remove(msg_text.as_bytes(), __content.bytes(12481));
            }
        }
    }
    answer = ui.msgbox(&msg_text, 8227, &s(__content.string(12482)));
    if answer != 6 {
        return;
    }
    let mut new_rec = str_trim(cast::<Ext, _>(corp_id))
        + "|"
        + &str_s(cast::<Ext, _>(opt_type))
        + "|"
        + &str_s(cast::<Ext, _>(buyer_id))
        + "|"
        + &str_s(cast::<Ext, _>(pct_to_buy))
        + "|"
        + &str_e(strike)
        + "|"
        + &str_s(cast::<Ext, _>(exp_month))
        + "|"
        + &str_s(cast::<Ext, _>(exp_year))
        + "|";
    if is_buyback != "Y" {
        orig_premium = total_cost_e;
        new_rec = new_rec + &str_e(orig_premium);
        net_pct = find_option_counterparty(g, ui, rng, &new_rec, Ext::from(total_cost), size_factor);
        if net_pct < 11 {
            msg_text = PbStr::new();
            if 1 < pct_to_buy {
                msg_text = s("\r") + "\r" + __content.string(12483);
            }
            let text = s(__content.string(12484))
                + __content.string(12485)
                + __content.string(12486)
                + &msg_text
                + "\r"
                + "\r"
                + __content.string(12487);
            ui.msgbox(&text, 8208, &s(__content.string(12488)));
            return;
        }
    }
    g.cash[buyer_id] = g.cash[buyer_id] - total_cost;
    if g.cash[buyer_id] < Ext::from_f64(0.0) {
        balance_cash_t_bills(g, buyer_id);
    }
    if g.cash[buyer_id] < Ext::from_f64(0.0) {
        let overdraft = g.cash[buyer_id].abs();
        g.bank_debt[buyer_id] = overdraft + g.bank_debt[buyer_id];
        let bank = cast::<i32, _>(g.lending_bank[buyer_id]);
        g.cash[bank] = g.cash[bank] - overdraft;
        g.cash[buyer_id] = Ext::from_f64(0.0);
    }
    let bought_pct = pct_to_buy;
    if is_buyback == "Y" && short_qty != 0 {
        if short_qty.abs() == pct_to_buy {
            buyback_gain = Ext::from(orig_premium.abs().fsub(total_cost_e).into_ext());
            pct_to_buy = 0;
        } else {
            let cover_frac = Ext80::from_i64(cast::<i64, _>(pct_to_buy)).fdiv(Ext80::from_i64(cast::<i64, _>(short_qty.abs())));
            buyback_gain = Ext::from(orig_premium.abs().fmul(cover_frac).fsub(total_cost_e).into_ext());
            orig_premium = Ext80::from_i64(1).fsub(cover_frac).fmul(orig_premium);
            pct_to_buy = pct_to_buy + short_qty;
        }
        if g.cur_player == buyer_id {
            let cp = g.cur_player;
            g.player_cap_gains[cp] = buyback_gain + g.player_cap_gains[cp];
        } else {
            g.extraordinary_pl[buyer_id] = to_quad(buyback_gain + cast::<Ext, _>(g.extraordinary_pl[buyer_id]));
        }
    }
    msg_text = str_trim(cast::<Ext, _>(corp_id))
        + "|"
        + &str_s(cast::<Ext, _>(opt_type))
        + "|"
        + &str_s(cast::<Ext, _>(buyer_id))
        + "|"
        + &str_s(cast::<Ext, _>(pct_to_buy))
        + "|"
        + &str_e(strike)
        + "|"
        + &str_s(cast::<Ext, _>(exp_month))
        + "|"
        + &str_s(cast::<Ext, _>(exp_year))
        + "|"
        + &str_e(orig_premium);
    if is_buyback == "Y" {
        add_or_remove_option_rec(g, ui, &PbStr::new(), short_rec);
        if pct_to_buy != 0 {
            add_or_remove_option_rec(g, ui, &msg_text, 0);
        }
    } else if merge_matching_option_rec(g, &msg_text) != 1 {
        add_or_remove_option_rec(g, ui, &msg_text, 0);
    }
    if 1585 < etf_id && 0 < g.fund_bonus_year[buyer_id] {
        g.fund_bonus_year[buyer_id] = g.cur_year;
    }
    g.turn_action_count = g.turn_action_count + 1;
    msg_part2 = s(".");
    let so = g.shares_outstanding[corp_id];
    let ask_ps = ask_total.fdiv(shares).into_ext();
    if bid_total / so < ask_ps {
        msg_part2 = s(" of ") + &fmt_trim(bid_total / so, FMT) + ".";
    }
    msg_part = s(__content.string(1506));
    if 10 < buyer_id {
        msg_part = g.ent_symbol[buyer_id].clone() + " has";
    }
    let mut opt_rec = s(__content.string(12489))
        + "\r"
        + "\r"
        + &msg_part
        + __content.string(12490)
        + &opt_kind_str
        + __content.string(12491)
        + &g.month_name[exp_month]
        + ","
        + &str_s(cast::<Ext, _>(exp_year))
        + __content.string(12492)
        + &fmt_trim(strike.into_ext() / so, FMT)
        + __content.string(12493)
        + __content.string(12494)
        + &number_to_words(bought_pct, 0, 0)
        + __content.string(12495)
        + &g.ent_name[corp_id]
        + " ("
        + &g.ent_symbol[corp_id]
        + "), or ";
    msg_part = fmt_trim(cast::<Ext, _>(bought_pct) * so, FMT) + " " + &g.million_prefix_lc + "illion";
    opt_rec = opt_rec + &msg_part + __content.string(12496) + "\r" + "\r";
    if sf(0.01f32) < val(fmt_trim(Ext::from(ask_ps), FMT).as_bytes()) {
        opt_rec = opt_rec
            + __content.string(12497)
            + &fmt_trim(Ext::from(ask_ps), FMT)
            + __content.string(12498)
            + __content.string(12499)
            + &msg_part2
            + "\r"
            + "\r";
    }
    opt_rec = opt_rec
        + __content.string(12500)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(Ext::from(total_cost), "* #############,.00")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + "."
        + "\r"
        + "\r";
    if is_buyback == "Y" {
        msg_part = s("gain");
        if buyback_gain < Ext::from_f64(0.0) {
            msg_part = s("loss");
        }
        opt_rec = opt_rec
            + __content.string(12501)
            + &msg_part
            + " of: "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(buyback_gain.abs(), FMT)
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "."
            + "\r"
            + "\r";
        net_pct = find_option_positions(g, ui, buyer_id, corp_id, opt_type, 0);
        if net_pct == 0 || net_pct == 1 || net_pct == 31 {
            msg_text = s("your");
            if 10 < buyer_id {
                msg_text = g.ent_symbol[buyer_id].clone() + "'s";
            }
            opt_rec = opt_rec
                + __content.string(12502)
                + &msg_text
                + " short "
                + &opt_kind_str
                + __content.string(12503)
                + &g.ent_symbol[corp_id]
                + ".";
        } else {
            msg_text = s(__content.string(12504));
            if 10 < buyer_id {
                msg_text = g.ent_symbol[buyer_id].clone() + __content.string(12505);
            }
            opt_rec = opt_rec + &msg_text + " short " + &opt_kind_str + __content.string(12503) + &g.ent_name[corp_id] + ".";
        }
    } else {
        g.has_options_outstanding[corp_id] = 1;
        opt_rec = opt_rec + __content.string(12506) + "short. ";
        opt_rec = opt_rec
            + "The "
            + &opt_kind_str
            + __content.string(12507)
            + __content.string(12508);
        if opt_type == 1 {
            opt_rec = opt_rec
                + __content.string(12509)
                + __content.string(12510)
                + __content.string(12511)
                + "\r"
                + "\r";
        } else {
            opt_rec = opt_rec
                + __content.string(12512)
                + __content.string(12513)
                + "\r"
                + "\r";
        }
        opt_rec = opt_rec
            + "A "
            + &opt_kind_str
            + __content.string(12514)
            + __content.string(12515)
            + __content.string(12516)
            + __content.string(12517);
        if g.player_exercise_options[g.cur_player] == 0 {
            opt_rec = opt_rec + __content.string(12518);
        }
        opt_rec = opt_rec + "\r" + "\r";
    }
    ui.msgbox(&opt_rec, 8256, &s(__content.string(12519)));
    update_market_display(g, ui, rng, time_secs);
    msg_part = s(__content.string(12520));
    if is_buyback == "Y" {
        msg_part = s(__content.string(9865));
    }
    g.news_msg = g.ent_symbol[buyer_id].clone()
        + &msg_part
        + &str_s(cast::<Ext, _>(exp_month))
        + "/"
        + &str_trim(cast::<Ext, _>(exp_year))
        + " "
        + &opt_kind_str
        + __content.string(12521)
        + &g.ent_symbol[corp_id];
    post_news(g, ui, 0, 0);
    g.valuation_mode = 1;
    calc_stock_price(g, ui, rng, corp_id, time_secs);
}
