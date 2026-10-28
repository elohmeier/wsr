//! Industries 1 (banks), 2 (insurers), 70 ( [runtime resource wording] ) and 71 (funds).
use crate::pb::real::Ext;


use super::{Ctx, s};
use crate::logic::finance::calc_capital_taxes;
use crate::logic::report::number_to_words;
use crate::logic::valuation::{price_pct_change_months, price_trend_streak};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::replace;
use crate::state::Globals;
use crate::ui::Ui;

const CRLF: &str = "\r\n";
/// Price-history series of the stock index and of the [runtime resource wording]  yield.
const SERIES_STOCK_INDEX: i32 = 0;
const SERIES_SHORT_BOND: i32 = 1603;

/// Part of BuildIndustryOutlookText (0x56f33b): the texts for industries 1, 2 (0x58165b .. 0x5825c0),
/// 70 (0x56f3c9) and 71 (0x56f5b7).
pub(super) fn financial_industries(g: &mut Globals, _ui: &mut dyn Ui, _rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    if c.industry_id < 3 {
        c.body_text = PbStr::new();
        if c.industry_id == 1 {
            banks(g, c);
        } else if c.industry_id == 2 {
            insurers(g, c);
        }
        if !g.ind_outlook_text.is_empty() {
            g.ind_outlook_text = g.ind_outlook_text.clone() + CRLF + CRLF;
        }
        return;
    }
    if c.industry_id == 70 {
        c.body_text = s(__content.string(7549))
            + __content.string(7550)
            + __content.string(7551)
            + __content.string(7552)
            + CRLF
            + CRLF
            + __content.string(7553)
            + __content.string(7554)
            + __content.string(7555)
            + __content.string(7556)
            + CRLF
            + CRLF;
        if 10 < g.sel_entity {
            let sel = g.sel_entity;
            if calc_capital_taxes(g, sel) > Ext::from_f64(0.0) && g.industry_num[g.sel_entity] == 70 && g.holding_co_tax_due > Ext::from_f64(0.0) {
                c.body_text = c.body_text.clone()
                    + __content.string(7557)
                    + __content.string(7558)
                    + __content.string(7559)
                    + &g.sel_corp_name
                    + ". "
                    + CRLF
                    + CRLF;
            }
        }
        g.ind_outlook_text = c.body_text.clone();
        return;
    }
    // industry 71
    c.body_text = s(__content.string(7560))
        + __content.string(7561)
        + __content.string(7562)
        + __content.string(7563)
        + __content.string(7564)
        + CRLF
        + CRLF
        + __content.string(7565)
        + __content.string(7566)
        + CRLF
        + CRLF;
    g.ind_outlook_text = c.body_text.clone();
}

/// Banks (0x581675): interest-rate level and trend, credit lines, recession and subprime losses.
fn banks(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    c.trend_val = price_trend_streak(g, SERIES_SHORT_BOND);
    let bir = g.base_interest_rate;
    if !(Ext::from_f32(6.0f32) > bir) {
        if Ext::from_f32(8.0f32) < bir {
            c.clause_text = s(__content.string(7567));
            c.cmp_val = 10;
            if c.cmp_val < price_pct_change_months(g, SERIES_SHORT_BOND, 6) && 0 < c.trend_val {
                c.clause_text = c.clause_text.clone() + "er";
            }
        }
    } else {
        c.clause_text = s(__content.string(7568));
        c.cmp_val = -10;
        if price_pct_change_months(g, SERIES_SHORT_BOND, 6) < c.cmp_val && c.trend_val < 0 {
            c.clause_text = c.clause_text.clone() + "ered";
        }
    }
    c.body_text = s(__content.string(7569));
    if bir < Ext::from_f32(6.0f32) || Ext::from_f32(8.0f32) < bir {
        c.body_text = c.body_text.clone() + &c.clause_text + __content.string(7570);
        if 0 < g.subprime_crisis_state {
            c.body_text = c.body_text.clone()
                + __content.string(7571);
        }
    } else {
        c.body_text = s(__content.string(7572))
            + __content.string(7573)
            + __content.string(7574);
    }
    if !(g.prime_rate > Ext::from_f32(8.0f32)) {
        if g.prime_rate < Ext::from_f32(6.75f32) && Ext::from_f32(1.5f32) < g.gdp_growth {
            c.body_text = c.body_text.clone()
                + __content.string(7575)
                + __content.string(7576);
        }
    } else {
        c.body_text = c.body_text.clone()
            + __content.string(7577)
            + __content.string(7578);
    }
    c.head_text = PbStr::new();
    if c.trend_val < 1 {
        if c.trend_val < 0 {
            c.head_text = s("fall");
        }
    } else {
        c.head_text = s("ris");
    }
    if !c.head_text.is_empty() {
        c.body_text = c.body_text.clone()
            + __content.string(7579)
            + &c.head_text
            + __content.string(7580)
            + &number_to_words(c.trend_val, 0, 0)
            + " months";
        c.head_text = s(__content.string(7581)) + &g.central_bank_name + __content.string(7582);
        if c.trend_val < 1 || g.monetary_policy != 2 {
            if c.trend_val < 0 && g.monetary_policy == 0 {
                c.head_text = c.head_text.clone() + __content.string(7583);
            } else {
                c.head_text = PbStr::new();
            }
        } else {
            c.head_text = c.head_text.clone() + __content.string(7584);
        }
        c.body_text = c.body_text.clone() + &c.head_text + ". ";
    }
    c.head_text = PbStr::new();
    if g.gdp_growth < Ext::from_f32(0.0f32) && g.gdp_growth_target < -Ext::from_f32(2.0f32) {
        c.head_text = s(__content.string(7585))
            + __content.string(7586);
    }
    if g.subprime_crisis_state == -2 {
        c.head_text = c.head_text.clone()
            + __content.string(7587)
            + __content.string(7588);
    }
    g.ind_outlook_text = c.body_text.clone() + &c.head_text;
}

/// Insurers (0x581c9d): interest rates vs. investment returns, subprime securities, stock market trend.
fn insurers(g: &mut Globals, c: &mut Ctx) {
    let __content = crate::content::current();
    c.price_streak = price_trend_streak(g, SERIES_STOCK_INDEX);
    if c.price_streak == 0 {
        c.trend_val = 0;
    } else {
        // BYREF months = priceStreak: a falling streak (< 0) gives 0
        c.trend_val = price_pct_change_months(g, SERIES_STOCK_INDEX, c.price_streak);
    }
    c.head_text = PbStr::new();
    let bir = g.base_interest_rate;
    if !(9.2f32 < bir) {
        if bir < Ext::from_f32(5.5f32) {
            c.body_text =
                s(__content.string(7589))
                    + __content.string(7590)
                    + __content.string(7591);
            if c.price_streak < 0 {
                c.body_text = c.body_text.clone()
                    + __content.string(7592)
                    + &number_to_words(c.price_streak, 0, 0)
                    + __content.string(1095);
            } else {
                c.cmp_val = 0;
                if price_pct_change_months(g, SERIES_STOCK_INDEX, 6) < c.cmp_val {
                    c.body_text =
                        c.body_text.clone() + __content.string(7593);
                }
            }
            if g.subprime_crisis_state == -2 {
                c.body_text = c.body_text.clone()
                    + CRLF
                    + CRLF
                    + __content.string(7594)
                    + __content.string(7595)
                    + __content.string(7596);
            }
        }
    } else {
        c.body_text = s(__content.string(7597))
            + __content.string(7598)
            + __content.string(7599)
            + __content.string(7600);
        let rising = if c.price_streak < 1 {
            c.cmp_val = 0;
            c.cmp_val < price_pct_change_months(g, SERIES_STOCK_INDEX, 6)
        } else {
            true
        };
        if rising && -1 < c.price_streak {
            c.body_text = c.body_text.clone()
                + __content.string(7601);
            c.body_text = replace(c.body_text.as_bytes(), __content.bytes(7602), __content.bytes(7603));
        }
        c.body_text = c.body_text.clone() + ". ";
        if g.subprime_crisis_state == -2 {
            c.body_text = c.body_text.clone()
                + __content.string(7604)
                + __content.string(7605);
        }
    }
    if !c.body_text.is_empty() && 0 < g.subprime_crisis_state {
        c.body_text = c.body_text.clone()
            + CRLF
            + CRLF
            + __content.string(7606)
            + __content.string(7607)
            + __content.string(7608);
    }
    if !c.body_text.is_empty() {
        c.body_text = c.body_text.clone() + CRLF + CRLF;
    }
    c.body_text = c.body_text.clone()
        + __content.string(7609)
        + __content.string(7610);
    if c.trend_val < 10 || c.price_streak < 4 {
        if c.trend_val < -5 && c.price_streak < 0 {
            c.head_text = s(__content.string(7611))
                + &number_to_words(c.price_streak, 0, 0)
                + " "
                + __content.string(7612)
                + &number_to_words(c.trend_val, 0, 1)
                + __content.string(7613);
        } else {
            c.trend_val = price_pct_change_months(g, SERIES_STOCK_INDEX, 6);
            if 4 < c.trend_val.abs() {
                c.para_text = s("are up");
                if c.trend_val < 0 {
                    c.para_text = s(__content.string(7614));
                }
                if c.trend_val < -14 {
                    c.para_text = replace(c.para_text.as_bytes(), b"fallen", __content.bytes(7615));
                }
                c.head_text = s(__content.string(7616))
                    + &c.para_text
                    + &number_to_words(c.trend_val, 0, 1);
            }
        }
    } else {
        c.head_text = s(__content.string(7617))
            + &number_to_words(c.price_streak, 0, 0)
            + __content.string(7618)
            + &number_to_words(c.trend_val, 0, 1)
            + __content.string(7613);
    }
    if c.head_text.is_empty() && c.trend_val == 0 && c.price_streak == 0 {
        c.head_text = s(__content.string(7619));
    }
    if g.start_year == g.cur_year && g.cur_quarter < 3 {
        c.head_text = PbStr::new();
    }
    c.head_text = c.head_text.clone() + ". ";
    g.ind_outlook_text = c.body_text.clone() + &c.head_text;
}
