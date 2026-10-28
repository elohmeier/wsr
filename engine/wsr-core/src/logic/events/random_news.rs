//! Random corporate and market news events (`RandomCorpNewsEvent`) and the profit-margin adjustment
//! they use (`AdjustProfitMarginWithMisc`).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::calc_entity_financials;
use crate::logic::news::is_in_recent_headlines;
use crate::logic::report::number_to_words;
use crate::logic::valuation::{
    price_pct_change_months, price_trend_direction, price_trend_streak, update_price_history_tick,
};
use crate::pb::string::{lcase, mcase, parse_array, parse_count, remove, CaseMode};

/// Port of AdjustProfitMarginWithMisc (0x4aee22).
/// Adds `margin_delta` to `ProfitMargin(corp_id)` (corps 11.. only, delta <> 0); for an operating corp
/// (industry 3..69) whose `EntMiscData` has more than 30 `|` fields also adds it to field 30
/// (`FORMAT$(…, "###.0")`) and rebuilds the record from fields 1..count-1, each followed by `|`.
pub fn adjust_profit_margin_with_misc(g: &mut Globals, corp_id: i32, margin_delta: i16) {
    if corp_id <= 10 || margin_delta == 0 {
        return;
    }
    let corp = corp_id;
    let industry = g.industry_num[corp];
    let delta = margin_delta;
    g.profit_margin[corp] = cast::<i16, _>(cast::<i32, _>(delta) + cast::<i32, _>(g.profit_margin[corp]));
    let field_count = parse_count(g.ent_misc_data[corp].as_bytes(), b"|");
    if field_count > 30 && delta != 0 && industry > 2 && industry < 70 {
        let misc_text = g.ent_misc_data[corp].clone();
        // DIM fields(1 TO fieldCount): PARSE fills element i with field i
        let mut fields = parse_array(misc_text.as_bytes(), b"|", cast::<usize, _>(field_count));
        let mut misc_val = val(fields[29].as_bytes());
        misc_val = cast::<Ext, _>(delta) + misc_val;
        fields[29] = fmt_trim(misc_val, b"###.0");
        let mut text = PbStr::new();
        for f in fields.iter().take(cast::<usize, _>((field_count - 1).max(0))) {
            text = text + f + "|";
        }
        g.ent_misc_data[corp] = text;
    }
}

/// Where a branch of [`random_corp_news_event`] continues.
enum Flow {
    /// `LAB_004aedf4`: leave without posting (and without resetting the cooldown).
    Exit,
    /// `LAB_004ab3c6`: a disaster headline (if any) is turned into the news and a loss, then `Apply`.
    Disaster,
    /// `LAB_004aeb8c`: apply market cap factor / loss / margin change to the corp, post `NewsMsg`.
    Apply,
}

/// `RTRIM$(LTRIM$(CurrencySymbol))`.
fn cur_sym(g: &Globals) -> PbStr {
    trim(g.currency_symbol.as_bytes())
}

/// Port of RandomCorpNewsEvent (0x4a5a08).
/// Called from the ticker loop: unless `RND * DifficultyLevel < 0.5` or the cooldown
/// `RandCorpNewsCooldown` is still running, picks a random entity 11..1600 (active, not a fund) and an
/// event number `RND * 2000`: lawsuits, layoffs, industry-specific disasters, mineral finds, contracts,
/// patents, tax cases, embezzlement, frauds, pension charges, the superfund case, management changes, and
/// market news (bitcoin ban, euro debt crisis, Nigerian oil, bond/economy/commodity trends, crop
/// reports, stock-market support, deposit seizures). Corp events scale `MarketCap`, charge `Cash` and
/// `ExtraordinaryPL`, may move the profit margin, and re-project the earnings; the headline is posted and
/// the cooldown set to `VAL(RIGHT$(TIME$, 2)) + 20` (`time_secs` = those seconds). Most market-news
/// branches post their own headline and leave without touching the cooldown.
///
/// `NewsMsg` is not cleared at entry: a pending headline is posted (attributed to the corp) by every
/// branch that reaches the common end without setting one.
pub fn random_corp_news_event(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError and exit (no error is expected on valid state)
    let dl = cast::<Ext, _>(g.difficulty_level);
    if rng.rnd() * dl < sf(0.5f32) {
        return;
    }
    if g.rand_corp_news_cooldown > 0 {
        g.rand_corp_news_cooldown -= 1;
    }
    if g.rand_corp_news_cooldown > 0 {
        return;
    }
    let mut corp_id = rng.rnd_range(11, 1600);
    if g.entity_status[corp_id] != 1 || g.industry_num[corp_id] == 71 {
        return;
    }
    g.tmp_symbol = g.ent_symbol[corp_id].clone();
    let corp_label = g.ent_name[corp_id].clone() + " (" + &g.tmp_symbol + ")";
    let mut cap_mult: f32 = cast::<f32, _>(Ext::from_f32(1.0f32));
    let industry = g.industry_num[corp_id];
    calc_entity_financials(g, ui, rng, corp_id);
    let event_roll = to_long(rng.rnd() * Ext::from_f64(2000.0));
    let sel = cast::<Ext, _>(event_roll);

    // locals (zeroed by the frame setup)
    let mut headline = PbStr::new();
    let mut phrase: PbStr;
    let mut phrase2;
    let mut loss_amt: i64 = 0;
    let mut margin_delta: i16 = 0;
    let mut change_val: i32;
    let mut streak: i32 = 0;
    let pick_num: i32;

    let flow = 'ev: {
        if sel < Ext::from_f64(5.0) {
            // ---- consumer / securities / price-fixing litigation
            if g.antitrust_defendant < 131 || g.antitrust_defendant != corp_id {
                headline = PbStr::from(__content.string(5313));
                if g.rand_seed % 22 < 8 || industry == 70 {
                    if g.sec_suit_count == 2 {
                        break 'ev Flow::Exit;
                    }
                    g.sec_suit_count += 1;
                    headline = PbStr::from(__content.string(5314));
                } else {
                    if g.consumer_suit_count == 3 {
                        break 'ev Flow::Exit;
                    }
                    g.consumer_suit_count += 1;
                }
            } else {
                if g.price_fix_suit_count == 2 {
                    break 'ev Flow::Exit;
                }
                g.price_fix_suit_count += 1;
                headline = PbStr::from(__content.string(5315));
            }
            loss_amt = to_quad(g.market_cap[corp_id] * Ext::from_f64(5.0));
            cap_mult = 0.9f32;
            phrase = cur_sym(g) + &fmt_trim(cast::<Ext, _>(loss_amt), b"* ###,") + " " + &g.million_prefix_lc + "il.";
            g.news_msg = headline.clone() + __content.string(5316) + &phrase;
            phrase = corp_label.clone();
            if len(phrase.as_bytes()) + len(g.news_msg.as_bytes()) > 70 {
                phrase = g.tmp_symbol.clone();
            }
            g.news_msg = phrase.clone() + &g.news_msg;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(10.0) {
            // ---- management change, else seasonal crop news
            if g.industry_num[corp_id] <= 69
                && len(g.new_mgmt_list.as_bytes()) <= 4
                && g.corp_controller[corp_id] > 10
            {
                headline = PbStr::from(__content.string(5317));
                phrase = PbStr::from(__content.string(5318));
                if g.rand_seed % 13 < 6 {
                    headline = PbStr::from(__content.string(5319));
                    phrase = PbStr::from(__content.string(5320));
                }
                if g.rand_seed % 27 < 9 {
                    phrase = PbStr::from("fired.");
                }
                if g.rand_seed % 7 == 1 {
                    phrase = PbStr::from(__content.string(5321));
                }
                phrase2 = PbStr::from("000") + &trim(str_s(cast::<Ext, _>(corp_id)).as_bytes());
                phrase2 = right(phrase2.as_bytes(), 4) + "/";
                if instr(1, g.new_mgmt_list.as_bytes(), phrase2.as_bytes()) == 0 {
                    g.new_mgmt_list = g.new_mgmt_list.clone() + &phrase2;
                }
                phrase2 = corp_label.clone();
                if len(phrase2.as_bytes()) + len(headline.as_bytes()) + len(phrase.as_bytes()) > 55 {
                    phrase2 = g.tmp_symbol.clone();
                }
                g.news_msg = headline.clone() + " of " + &phrase2 + " " + &phrase;
                break 'ev Flow::Apply;
            }
            if g.cur_quarter == g.crop_news_quarter {
                break 'ev Flow::Apply;
            }
            g.crop_news_quarter = g.cur_quarter;
            if g.rand_seed % 2 == 1 && g.cur_quarter == 2 {
                g.news_msg = PbStr::from(__content.string(5322));
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"soar", b"climb");
                }
                g.corn_price = cast::<f32, _>(cast::<f32, _>(sf(1.23f32) * cast::<Ext, _>(g.corn_price)));
                g.corn_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.2f32) * cast::<Ext, _>(g.corn_target_price)));
            } else if g.rand_seed % 2 == 1 && g.cur_quarter == 3 {
                g.news_msg = PbStr::from(__content.string(5323));
                if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"Europe", b"Ukraine");
                }
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    g.news_msg = PbStr::from(__content.string(5324));
                }
                if cast::<Ext, _>(g.oil_price) > sf(1.5f32) {
                    g.news_msg = PbStr::from(__content.string(5325));
                }
                g.wheat_price = cast::<f32, _>(cast::<f32, _>(sf(1.3f32) * cast::<Ext, _>(g.wheat_price)));
                g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.45f32) * cast::<Ext, _>(g.wheat_target_price)));
            } else if g.cur_quarter % 2 != 0 || cast::<Ext, _>(g.corn_price) <= sf(3.5f32) {
                if cast::<Ext, _>(g.wheat_price) <= Ext::from_f64(4.0) {
                    if (cast::<Ext, _>(g.wheat_price)) < sf(3.3f32) || (cast::<Ext, _>(g.corn_price)) < sf(2.9f32) {
                        headline = PbStr::from("wheat");
                        if (cast::<Ext, _>(g.corn_price)) < sf(2.9f32) {
                            headline = PbStr::from("corn");
                        }
                        g.news_msg = PbStr::from(__content.string(5326))
                            + &headline
                            + __content.string(5327);
                    } else if cast::<Ext, _>(g.wheat_price) > sf(11.3f32) || cast::<Ext, _>(g.corn_price) > sf(10.2f32) {
                        headline = PbStr::from("wheat");
                        phrase = PbStr::from(__content.string(5328));
                        if cast::<Ext, _>(g.corn_price) > sf(10.2f32) {
                            headline = PbStr::from("corn");
                            phrase = PbStr::from(__content.string(5329));
                        }
                        g.news_msg = PbStr::from(__content.string(5330))
                            + &phrase
                            + __content.string(5331)
                            + &headline;
                        g.corn_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corn_price) + sf(0.5f32)));
                    }
                } else {
                    g.news_msg = PbStr::from(__content.string(5332));
                    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                        g.news_msg = replace(g.news_msg.as_bytes(), b"soften", b"slide");
                    }
                    g.wheat_price = cast::<f32, _>(cast::<f32, _>(sf(0.77f32) * cast::<Ext, _>(g.wheat_price)));
                    g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_target_price) - Ext::from_f64(1.0)));
                }
            } else {
                g.news_msg = PbStr::from(__content.string(5333));
                if g.currency_code != "USA" {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"USDA", __content.bytes(1550));
                }
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"tumble", __content.bytes(2549));
                }
                g.corn_price = cast::<f32, _>(cast::<f32, _>(sf(0.8f32) * cast::<Ext, _>(g.corn_price)));
                g.corn_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corn_target_price) - Ext::from_f64(1.0)));
            }
            corp_id = 0;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(11.0) {
            // ---- bitcoin ban
            if g.bitcoin_ban_news_done > 0 {
                break 'ev Flow::Exit;
            }
            pick_num = rng.rnd_range(9, 53);
            if pick_num == 15 || pick_num == 16 || pick_num == 26 || pick_num == 50 {
                break 'ev Flow::Exit;
            }
            headline = g.country_name[pick_num].clone();
            g.news_msg = PbStr::from(__content.string(5334))
                + &mcase(headline.as_bytes(), CaseMode::Ansi)
                + __content.string(5335);
            g.bitcoin_price = to_long(sf(0.96f32) * cast::<Ext, _>(g.bitcoin_price));
            if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0) {
                g.bitcoin_target_price = to_long(sf(0.9f32) * cast::<Ext, _>(g.bitcoin_target_price));
            } else {
                g.bitcoin_target_price = to_long(sf(1.2f32) * cast::<Ext, _>(g.bitcoin_target_price));
            }
            g.bitcoin_ban_news_done = 1;
            corp_id = 0;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(15.0) {
            // ---- layoffs
            if g.layoff_news_count > 3 {
                break 'ev Flow::Exit;
            }
            if g.profit_margin[corp_id] >= 6 || g.corp_controller[corp_id] <= 10 {
                break 'ev Flow::Exit;
            }
            if industry < 3 || industry == 70 {
                break 'ev Flow::Exit;
            }
            if g.earnings_y1[corp_id] + g.earnings_ytd[corp_id] >= Ext::from_f64(0.0) {
                break 'ev Flow::Exit;
            }
            g.news_msg = corp_label.clone() + __content.string(5336);
            let r = rng.rnd();
            if r * Ext::from_f64(3.0) > Ext::from_f64(1.0) && (cast::<Ext, _>(g.gdp_growth)) < Ext::from_f64(0.0) {
                change_val = rng.rnd_range(2, 12);
                change_val *= 1000;
                g.news_msg = corp_label.clone() + " to cut" + &str_s(cast::<Ext, _>(change_val)) + " jobs.";
            }
            if g.profit_margin[corp_id] < -4 {
                g.news_msg = PbStr::from(__content.string(5337)) + &g.tmp_symbol + __content.string(5338);
            }
            let ba = g.business_assets[corp_id];
            g.business_assets[corp_id] = g.business_assets[corp_id] - ba * sf(0.07f32);
            loss_amt = to_quad(ba * sf(0.07f32));
            margin_delta = 7;
            g.layoff_news_count += 1;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(16.0) {
            // ---- euro sovereign debt news
            corp_id = 0;
            if g.euro_crisis_news_cooldown > 0 {
                g.euro_crisis_news_cooldown -= 1;
            }
            if g.euro_crisis_news_cooldown > 0 {
                break 'ev Flow::Apply;
            }
            g.euro_crisis_news_cooldown = 10;
            headline = PbStr::from("Spanish");
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                headline = PbStr::from("Italian");
            }
            if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) {
                headline = PbStr::from("Greek");
            }
            let cap_factor: f32;
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(1.07f32) * cast::<Ext, _>(g.market_valuation)));
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    g.market_val_target = cast::<f64, _>(sf(1.12f32) * g.market_val_target);
                }
                g.news_msg = PbStr::from(__content.string(5339)) + &headline + __content.string(5340);
                cap_factor = cast::<f32, _>(sf(0.1f32) * rng.rnd() + Ext::from_f64(1.0));
            } else {
                g.market_valuation = cast::<f32, _>(cast::<f32, _>(sf(0.92f32) * cast::<Ext, _>(g.market_valuation)));
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    g.market_val_target = cast::<f64, _>(sf(0.88f32) * g.market_val_target);
                }
                g.news_msg = PbStr::from(__content.string(5341)) + &headline + __content.string(5342);
                cap_factor = cast::<f32, _>(sf(0.09f32) * rng.rnd() + sf(0.89f32));
            }
            for idx in 11..=1600 {
                if g.entity_status[idx] == 1 {
                    g.market_cap[idx] = cast::<Ext, _>(cap_factor) * g.market_cap[idx];
                }
            }
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(50.0) {
            // ---- industry-specific disasters, lawsuits, recalls ...
            let sv = industry;
            if sv == 1 {
                headline = PbStr::from(__content.string(5343));
                if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                    headline = replace(headline.as_bytes(), __content.bytes(5344), __content.bytes(5189));
                }
                let r = rng.rnd();
                if r * Ext::from_f64(2.0) > Ext::from_f64(1.0) || g.cur_year < g.start_year + 5 {
                    phrase = fmt_trim(g.market_cap[corp_id] * Ext::from_f64(3.0), b"* ###,");
                    headline = PbStr::from(__content.string(5345))
                        + &cur_sym(g)
                        + &phrase
                        + " "
                        + &g.million_prefix_lc
                        + "il."
                        + &g.currency_suffix;
                }
                if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                    headline = PbStr::from(__content.string(5346));
                }
                let r = rng.rnd();
                if r * Ext::from_f64(6.0) > Ext::from_f64(5.0) && g.exchange_rate * Ext::from_f64(2000.0) < cast::<Ext, _>(g.bank_consumer_loans[corp_id]) {
                    let write_off = cast::<Ext, _>(g.bank_consumer_loans[corp_id]) * sf(0.05f32);
                    phrase = fmt_trim(write_off, b"* ###,");
                    headline = PbStr::from(__content.string(5347))
                        + &cur_sym(g)
                        + &phrase
                        + " "
                        + &g.million_prefix_lc
                        + "il."
                        + &g.currency_suffix
                        + __content.string(5348);
                    g.bank_consumer_loans[corp_id] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[corp_id]) - write_off);
                    g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - write_off;
                }
                break 'ev Flow::Disaster;
            }
            if sv == 2 {
                // insurer: catastrophe losses, posted here
                let reserves = g.ins_reserves[corp_id];
                loss_amt = to_quad((rng.rnd() * sf(0.2f32) + sf(0.1f32)) * reserves);
                phrase2 = cur_sym(g) + &fmt_trim(cast::<Ext, _>(loss_amt), b"* ###,");
                cap_mult = 0.85f32;
                phrase = PbStr::from(__content.string(5349));
                if g.rand_seed % 3 == 1 {
                    phrase = PbStr::from(__content.string(5350));
                }
                if g.rand_seed % 3 == 2 {
                    phrase = PbStr::from("flood");
                }
                g.news_msg = g.tmp_symbol.clone() + __content.string(5351) + &phrase + __content.string(5352);
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"loss", b"claims");
                }
                g.news_msg = g.news_msg.clone() + &phrase2 + " " + &g.million_prefix_lc + "il.";
                g.insur_uw_profit_ytd[corp_id] = g.insur_uw_profit_ytd[corp_id] - cast::<Ext, _>(loss_amt);
                g.investor_sentiment[corp_id] = g.investor_sentiment[corp_id] - 15;
                if rng.rnd() * Ext::from_f64(9.0) > Ext::from_f64(8.0) {
                    loss_amt = to_quad(cast::<Ext, _>(loss_amt) * sf(0.25f32));
                    margin_delta = 5;
                    adjust_profit_margin_with_misc(g, corp_id, margin_delta);
                    cap_mult = 1.2f32;
                    g.news_msg = g.tmp_symbol.clone() + __content.string(5353);
                    g.investor_sentiment[corp_id] = g.investor_sentiment[corp_id] + 30;
                }
                g.market_cap[corp_id] = cast::<Ext, _>(cap_mult) * g.market_cap[corp_id];
                update_price_history_tick(g, ui, rng, corp_id);
                g.cash[corp_id] = g.cash[corp_id] - cast::<Ext, _>(loss_amt);
                g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - cast::<Ext, _>(loss_amt);
                calc_projected_earnings(g, ui, rng, corp_id);
                post_news(g, ui, corp_id, 0);
                break 'ev Flow::Exit;
            }
            match sv {
                3 | 67 | 68 => {
                    if g.plant_disaster_count > 3 {
                        break 'ev Flow::Exit;
                    }
                    headline = PbStr::from(__content.string(5354));
                    if g.rand_seed % 3 == 1 {
                        headline = PbStr::from(__content.string(5355));
                    }
                    if g.rand_seed % 3 == 2 && industry == 3 {
                        phrase = PbStr::from(__content.string(5356));
                        if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                            phrase = PbStr::from(__content.string(5357));
                        }
                        if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                            phrase = PbStr::from(__content.string(5358));
                        }
                        headline = PbStr::from(__content.string(5359)) + &phrase;
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5360));
                    }
                    g.plant_disaster_count += 1;
                }
                4 | 21 | 28 => {
                    headline = PbStr::from(__content.string(5361));
                    if g.cur_quarter > 2 && industry < 28 {
                        headline = PbStr::from(__content.string(5362));
                    }
                    if g.rand_seed % 3 == 1 && industry == 4 && g.start_year + 5 < g.cur_year {
                        phrase = PbStr::from("wheat");
                        if g.rand_seed % 7 > 2 {
                            phrase = PbStr::from("grain");
                        }
                        if g.rand_seed % 7 > 3 {
                            phrase = PbStr::from("fruit");
                        }
                        if g.rand_seed % 7 > 4 {
                            phrase = PbStr::from(__content.string(5363));
                        }
                        if g.rand_seed % 7 > 5 {
                            phrase = PbStr::from("chicken");
                        }
                        phrase2 = PbStr::from("cancer");
                        let r5 = rng.rnd() * Ext::from_f64(5.0);
                        if r5 > Ext::from_f64(1.0) {
                            phrase2 = PbStr::from(__content.string(5364));
                        }
                        if r5 > Ext::from_f64(2.0) {
                            phrase2 = PbStr::from(__content.string(5365));
                        }
                        if r5 > Ext::from_f64(3.0) {
                            phrase2 = PbStr::from(__content.string(5366));
                        }
                        if r5 > Ext::from_f64(4.0) {
                            phrase2 = PbStr::from(__content.string(5367));
                        }
                        headline = PbStr::from(__content.string(5368)) + &phrase + __content.string(5369) + &phrase2;
                        if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                            headline = PbStr::from(__content.string(5370));
                        }
                    } else if industry == 21 && g.cur_quarter < 3 {
                        headline = PbStr::from(__content.string(5371));
                    } else if industry == 28 {
                        headline = PbStr::from(__content.string(5372));
                        if g.rand_seed % 3 == 1 {
                            headline = PbStr::from(__content.string(5373));
                        }
                        if g.rand_seed % 3 == 2 {
                            headline = PbStr::from(__content.string(5374));
                        }
                    }
                }
                5 => {
                    phrase = PbStr::from(__content.string(5375));
                    if rng.rnd() * Ext::from_f64(2.0) < Ext::from_f64(1.0) {
                        phrase = PbStr::from(__content.string(5376));
                    }
                    headline = phrase.clone() + __content.string(5377);
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5378));
                    }
                }
                6 => {
                    headline = PbStr::from(__content.string(5379));
                    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                        headline = PbStr::from(__content.string(5380));
                    }
                }
                7 => {
                    let rs = g.rand_seed % 12;
                    headline = PbStr::from(__content.string(5381));
                    if rs == 1 {
                        headline = PbStr::from(__content.string(5382));
                    }
                    if rs == 2 {
                        g.news_msg = PbStr::from(__content.string(5383)) + &corp_label + " tanker";
                        headline = PbStr::new();
                        let ba = g.business_assets[corp_id];
                        g.business_assets[corp_id] = g.business_assets[corp_id] - ba * sf(0.05f32);
                        g.extraordinary_pl[corp_id] =
                            to_quad(cast::<Ext, _>(g.extraordinary_pl[corp_id]) - ba * sf(0.05f32));
                        cap_mult = 0.9f32;
                    }
                    if rs == 3 {
                        headline = PbStr::from(__content.string(5384));
                    }
                    if rs == 4 {
                        headline = PbStr::from(__content.string(5385));
                    }
                    if rs == 5 {
                        headline = PbStr::from(__content.string(5386));
                    }
                    if rs == 6 {
                        headline = PbStr::from(__content.string(5387));
                    }
                    if rs == 7 {
                        headline = PbStr::from(__content.string(5388));
                    }
                    if rs == 8 {
                        phrase = PbStr::from(__content.string(4682));
                        if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                            phrase = PbStr::from(__content.string(4683));
                        }
                        if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                            phrase = PbStr::from("Nigeria");
                        }
                        headline = PbStr::from(__content.string(5389)) + &phrase;
                    }
                    if rs == 9 {
                        streak = rng.rnd_range(1, 52);
                        headline = PbStr::from(__content.string(5390))
                            + &mcase(g.country_name[streak].as_bytes(), CaseMode::Ansi);
                    }
                    if rs == 10 {
                        if g.country_idx[corp_id] == 38 {
                            break 'ev Flow::Exit;
                        }
                        phrase = PbStr::from("Arctic");
                        if rng.rnd() * Ext::from_f64(2.0) < Ext::from_f64(1.0) {
                            phrase = PbStr::from(__content.string(5391));
                        }
                        headline = PbStr::from(__content.string(5392)) + &phrase + __content.string(5393);
                    }
                    if rs == 11 {
                        headline = PbStr::from(__content.string(5394));
                    }
                }
                8 => {
                    let rs = g.rand_seed % 5;
                    headline = PbStr::from(__content.string(5395));
                    if rs == 1 {
                        headline = PbStr::from(__content.string(5396));
                    }
                    if rs == 2 {
                        headline = PbStr::from(__content.string(5397));
                    }
                    if rs == 3 {
                        headline = PbStr::from(__content.string(5398));
                    }
                    if rs == 4 {
                        headline = PbStr::from(__content.string(5399));
                    }
                }
                9 | 50 => {
                    phrase = PbStr::from("Liberia");
                    if g.rand_seed % 4 == 3 {
                        phrase = PbStr::from("Benin");
                    }
                    if g.rand_seed % 4 == 2 {
                        phrase = PbStr::from(__content.string(5400));
                    }
                    if g.rand_seed % 4 == 1 {
                        phrase = PbStr::from("Bolivia");
                    }
                    headline = PbStr::from(__content.string(5401)) + &phrase;
                    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                        headline = PbStr::from(__content.string(5402));
                        if g.rand_seed % 3 == 1 {
                            headline = PbStr::from(__content.string(5403));
                        }
                        phrase = PbStr::from(__content.string(5404));
                        if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                            phrase = PbStr::from(__content.string(5405));
                        }
                        if g.rand_seed % 3 == 2 {
                            headline = PbStr::from(__content.string(5406)) + &phrase;
                            if g.cur_year > 2015 {
                                headline = PbStr::from(__content.string(5407));
                                if len(corp_label.as_bytes()) > 17 {
                                    headline = replace(headline.as_bytes(), __content.bytes(5408), b"axed");
                                }
                            }
                        }
                    }
                    if g.rand_seed % 5 == 3 {
                        streak = rng.rnd_range(1, 6);
                        phrase = PbStr::from(__content.string(4682));
                        if streak == 2 {
                            phrase = PbStr::from("Ecuador");
                        }
                        if streak == 3 {
                            phrase = PbStr::from("Bolivia");
                        }
                        if streak == 4 {
                            phrase = PbStr::from(__content.string(4680));
                        }
                        headline = PbStr::from(__content.string(5409)) + &phrase + __content.string(5410);
                    }
                    if g.rand_seed % 7 == 6 {
                        phrase2 = PbStr::from("Sumatra");
                        let r5 = rng.rnd() * Ext::from_f64(5.0);
                        if r5 > Ext::from_f64(1.0) {
                            phrase2 = PbStr::from(__content.string(5411));
                        }
                        if r5 > Ext::from_f64(2.0) {
                            phrase2 = PbStr::from(__content.string(5412));
                        }
                        if r5 > Ext::from_f64(3.0) {
                            phrase2 = PbStr::from("Borneo");
                        }
                        if r5 > Ext::from_f64(4.0) {
                            phrase2 = PbStr::from("Zambian");
                        }
                        headline = PbStr::from(" ") + &phrase2 + __content.string(5413);
                        if instr(1, corp_label.as_bytes(), b"COPPER") > 0 && r5 > Ext::from_f64(3.0) {
                            headline = PbStr::from(
                                __content.string(5414),
                            );
                        }
                    }
                    if industry != 50 || g.rand_seed % 13 != 11 || g.gold_strike_stage > 1 {
                        break 'ev Flow::Disaster;
                    }
                    // ---- South African gold miners' strike
                    let settle = rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0) || g.ind_growth_rate[50] < 11;
                    if settle {
                        g.gold_price = cast::<f32, _>(cast::<f32, _>(sf(0.93f32) * cast::<Ext, _>(g.gold_price)));
                        g.gold_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.9f32) * cast::<Ext, _>(g.gold_target_price)));
                        g.news_msg = PbStr::from(__content.string(5415));
                        let f = g.ind_growth_forecast[50];
                        g.ind_growth_forecast[50] = cast::<i16, _>(to_quad(cast::<Ext, _>(f) * sf(0.9f32)));
                        if g.ind_growth_rate[50] > 5 {
                            let r = g.ind_growth_rate[50];
                            g.ind_growth_rate[50] = cast::<i16, _>(to_quad(cast::<Ext, _>(r) * sf(0.8f32)));
                        }
                        g.gold_strike_stage = 2;
                    } else {
                        g.ind_demand[50] = to_quad(cast::<Ext, _>(g.ind_demand[50]) * sf(1.1f32));
                        let f = g.ind_growth_forecast[50];
                        g.ind_growth_forecast[50] = cast::<i16, _>(to_quad(rng.rnd() * Ext::from_f64(4.0) + Ext::from_f64(8.0) + cast::<Ext, _>(f)));
                        g.ind_growth_rate[50] = cast::<i16, _>(cast::<i32, _>(g.ind_growth_rate[50]) + 7);
                        for idx in 11..=1600 {
                            if g.entity_status[idx] == 1 && g.industry_num[idx] == 50 {
                                if g.country_idx[idx] == 42 {
                                    g.profit_margin[idx] = cast::<i16, _>(cast::<i32, _>(g.profit_margin[idx]) - 5);
                                } else {
                                    g.profit_margin[idx] = cast::<i16, _>(cast::<i32, _>(g.profit_margin[idx]) + 3);
                                }
                            }
                        }
                        if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0) {
                            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.88f32) * cast::<Ext, _>(g.gold_target_price)));
                        } else {
                            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.13f32) * cast::<Ext, _>(g.gold_target_price)));
                        }
                        g.gold_price = cast::<f32, _>(cast::<f32, _>(sf(1.08f32) * cast::<Ext, _>(g.gold_price)));
                        g.news_msg = PbStr::from(__content.string(5416));
                        streak = to_long(rng.rnd() * Ext::from_f64(30.0) + Ext::from_f64(10.0));
                        if g.gold_strike_stage == 1 {
                            g.news_msg = PbStr::from(__content.string(5417))
                                + &str_s(cast::<Ext, _>(streak))
                                + __content.string(5418);
                            g.gold_price = cast::<f32, _>(cast::<f32, _>(sf(1.05f32) * cast::<Ext, _>(g.gold_price)));
                        }
                        g.gold_target_price = cast::<f32, _>(cast::<f32, _>(sf(1.15f32) * cast::<Ext, _>(g.gold_target_price)));
                    }
                    headline = PbStr::new();
                    g.gold_strike_stage += 1;
                }
                10 | 17 => {
                    headline = PbStr::from(__content.string(5419));
                    if g.rand_seed % 2 == 1 {
                        headline = PbStr::from(__content.string(5420));
                    }
                }
                11 => {
                    headline = PbStr::from(__content.string(5421));
                    if g.rand_seed % 3 == 1 {
                        headline = PbStr::from(__content.string(5422));
                    }
                }
                12 | 18 | 19 => {
                    phrase = PbStr::from("EPA");
                    if g.country_idx[corp_id] > 0 {
                        phrase = PbStr::from(__content.string(5423));
                    }
                    headline = PbStr::from(__content.string(5424)) + &phrase;
                    if g.rand_seed % 3 == 1 && g.strike_corp_code == 0 {
                        g.strike_corp_code = cast::<i16, _>(corp_id + 20000);
                        margin_delta = -9;
                        g.news_msg = corp_label.clone() + __content.string(5425);
                        cap_mult = 0.88f32;
                        headline = PbStr::new();
                    }
                    if g.rand_seed % 3 == 2 && industry == 18 {
                        headline = PbStr::from(__content.string(5426));
                    }
                    if g.rand_seed % 7 == 1 && industry == 19 {
                        headline = PbStr::from(__content.string(5427));
                    }
                }
                13 => {
                    phrase = PbStr::from("EPA");
                    if g.country_idx[corp_id] > 0 {
                        phrase = g.country_name[g.country_idx[corp_id]].clone() + __content.string(5428);
                    }
                    headline = PbStr::from(__content.string(5429)) + &phrase;
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5430));
                    }
                }
                14 => {
                    headline = PbStr::from(__content.string(5431));
                    if g.rand_seed % 7 > 3 {
                        headline = PbStr::from(__content.string(5432));
                    }
                    if g.rand_seed % 7 == 6 {
                        headline = PbStr::from(__content.string(5433));
                    }
                }
                15 => {
                    headline = PbStr::from(__content.string(5434));
                    if g.rand_seed % 3 == 0 {
                        headline = PbStr::from(__content.string(5435));
                    }
                    if g.rand_seed % 3 == 1 {
                        headline = PbStr::from(__content.string(5436));
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5437));
                    }
                }
                16 => {
                    headline = PbStr::from(__content.string(5438));
                    if g.rand_seed % 5 == 0 {
                        headline = headline.clone() + __content.string(5439);
                    } else {
                        headline = headline.clone() + __content.string(5440);
                    }
                    if g.rand_seed % 5 == 1 {
                        headline = PbStr::from(__content.string(5441));
                    }
                    if g.rand_seed % 5 == 2 {
                        headline = PbStr::from(__content.string(5442));
                    }
                    if g.rand_seed % 5 == 3 {
                        headline = PbStr::from(__content.string(5443));
                    }
                    if g.rand_seed % 5 == 4 {
                        headline = PbStr::from(__content.string(5444));
                    }
                }
                20 | 24 => {
                    headline = PbStr::from(__content.string(5445));
                    if industry == 20 {
                        headline = headline.clone() + __content.string(5446);
                    }
                    if g.rand_seed % 4 == 1 {
                        headline = PbStr::from(__content.string(5447));
                    }
                    phrase = PbStr::from("estuary");
                    if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                        phrase = PbStr::from(__content.string(5448));
                    }
                    if g.rand_seed % 4 == 2 {
                        headline = PbStr::from(__content.string(5449)) + &phrase;
                    }
                }
                22 => {
                    headline = PbStr::from(__content.string(5450));
                }
                23 => {
                    headline = PbStr::from(__content.string(5451));
                    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                        headline = replace(headline.as_bytes(), b"sink", b"capsize");
                    }
                    if instr(1, g.ent_name[corp_id].as_bytes(), b"TANKER") > 0 {
                        headline = replace(headline.as_bytes(), b"ship", __content.bytes(5452));
                    }
                    let rs = g.rand_seed % 7;
                    if rs == 1 {
                        headline = PbStr::from(__content.string(5453));
                    }
                    if rs == 2 {
                        headline = PbStr::from(__content.string(5454));
                    }
                    if rs == 3 {
                        headline = PbStr::from(__content.string(5455));
                    }
                    if rs == 4 {
                        headline = PbStr::from(__content.string(5456));
                    }
                    if rs == 5 {
                        headline = PbStr::from(__content.string(5457));
                    }
                }
                25 => {
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5458));
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5459));
                    }
                }
                26 | 27 => {
                    headline = PbStr::from(__content.string(5460));
                    if g.rand_seed % 5 > 3 {
                        headline = PbStr::from(__content.string(5461));
                    }
                    if industry == 27 && g.rand_seed % 6 > 1 {
                        headline = PbStr::from(__content.string(5462));
                    }
                    if industry == 26 && rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(" loses ") + &g.million_prefix_lc + __content.string(5463);
                    }
                }
                29 => {
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5464));
                    }
                }
                30 => {
                    headline = PbStr::from(__content.string(5465));
                    phrase = PbStr::from(__content.string(5466));
                    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                        phrase = PbStr::from(__content.string(5467));
                    }
                    if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
                        headline =
                            PbStr::from(__content.string(5468)) + &g.million_prefix_lc + __content.string(5469) + &phrase + __content.string(5470);
                    }
                }
                31 => {
                    headline = PbStr::from(__content.string(5471));
                    if g.rand_seed % 2 == 1 {
                        headline = PbStr::from(__content.string(5472));
                    }
                }
                32 => {
                    headline = PbStr::from(__content.string(5473));
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5474));
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5475));
                    }
                }
                33 => {
                    headline = PbStr::from(__content.string(5476));
                    phrase = PbStr::from(__content.string(5477));
                    if g.rand_seed % 3 == 1 {
                        phrase = PbStr::from(__content.string(5478));
                    }
                    if g.rand_seed % 3 == 2 {
                        phrase = PbStr::from("the air");
                    }
                    headline = headline.clone() + &phrase;
                }
                34 => {
                    headline = PbStr::from(__content.string(5479));
                    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                        headline = headline.clone() + __content.string(5480);
                    } else if g.rand_seed % 3 == 2 {
                        headline = headline.clone() + __content.string(5481);
                    }
                    if g.rand_seed % 3 == 1 {
                        headline = PbStr::from(__content.string(5482));
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5483));
                    }
                }
                35 => {
                    headline = PbStr::from(__content.string(5484));
                    if g.rand_seed % 2 == 1 {
                        headline = PbStr::from(__content.string(5485));
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5486));
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5487));
                    }
                }
                36 => {
                    headline = PbStr::from(__content.string(5488));
                    if g.rand_seed % 3 == 0 {
                        headline = PbStr::from(__content.string(5489));
                        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                            headline = PbStr::from(__content.string(5490));
                        }
                    }
                    if g.rand_seed % 3 == 1 {
                        headline = PbStr::new();
                        g.news_msg = g.tmp_symbol.clone() + __content.string(5491);
                        margin_delta = -10;
                        cap_mult = 0.9f32;
                    }
                }
                37 => {
                    headline = PbStr::from(__content.string(5492));
                    if g.country_idx[corp_id] == 0 {
                        headline = PbStr::from(__content.string(5493));
                    }
                    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                        phrase2 = PbStr::new();
                        if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                            phrase2 = PbStr::from(__content.string(5494));
                        }
                        phrase = PbStr::from("cocoa");
                        let rs = g.rand_seed % 5;
                        if rs == 1 {
                            phrase = PbStr::from(__content.string(5495));
                        }
                        if rs == 2 {
                            phrase = PbStr::from(__content.string(5496));
                        }
                        if rs == 3 {
                            phrase = PbStr::from(__content.string(5497));
                        }
                        if rs == 4 {
                            phrase = PbStr::from("coffee");
                        }
                        headline = phrase2.clone() + __content.string(5498) + &phrase + __content.string(2546);
                    }
                }
                38 => {
                    headline = PbStr::from(__content.string(5499));
                    if g.rand_seed % 2 == 1 {
                        headline = PbStr::from(__content.string(5500));
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5501));
                    }
                }
                39 => {
                    phrase = PbStr::from(__content.string(5502));
                    phrase2 = PbStr::new();
                    if g.rand_seed % 3 == 1 {
                        phrase = PbStr::from(__content.string(5503));
                    }
                    if g.rand_seed % 3 == 2 {
                        phrase = PbStr::new();
                        phrase2 = PbStr::from(" looted");
                    }
                    headline =
                        PbStr::from(__content.string(5504)) + &phrase + __content.string(5505) + &phrase2;
                    if g.rand_seed % 2 == 0 {
                        headline = PbStr::from(__content.string(5506));
                    }
                }
                40 => {
                    headline = PbStr::from(__content.string(5507));
                    if g.rand_seed % 4 == 1 && g.country_idx[corp_id] != 8 {
                        phrase = PbStr::from(__content.string(5508));
                        if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                            phrase = PbStr::from(__content.string(5509));
                        }
                        headline = PbStr::from(__content.string(5510)) + &phrase + " govt.";
                    }
                }
                41..=48 => {
                    let rs = g.rand_seed % 7;
                    headline = PbStr::from(__content.string(5511));
                    if rs == 1 {
                        headline = PbStr::from(__content.string(5512));
                    }
                    if rs == 2 {
                        phrase = PbStr::from(__content.string(5513));
                        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                            phrase = PbStr::from(__content.string(5514));
                        }
                        headline = PbStr::from(__content.string(5515)) + &phrase + " group";
                    }
                    if rs == 3 {
                        headline = PbStr::from(__content.string(5516));
                        if industry == 44 && rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0) {
                            headline = PbStr::from(__content.string(5517));
                        }
                    }
                    if rs == 4 {
                        headline = PbStr::from(__content.string(5518));
                    }
                    if rs == 5 {
                        headline = PbStr::from(__content.string(5519));
                    }
                    if rs == 6 {
                        headline = PbStr::from(__content.string(5520));
                    }
                }
                49 => {
                    phrase = PbStr::from("s down");
                    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                        phrase = PbStr::from(__content.string(5521));
                    }
                    headline = PbStr::from(__content.string(5522)) + &phrase;
                }
                51 => {
                    headline = PbStr::from(__content.string(5523));
                    if g.rand_seed % 3 == 1 {
                        headline = PbStr::from(__content.string(5524));
                    }
                    streak = rng.rnd_range(6, 23);
                    if g.rand_seed % 3 == 2 {
                        headline = PbStr::from(__content.string(5525))
                            + &str_s(cast::<Ext, _>(streak))
                            + __content.string(5526);
                    }
                }
                52 => {
                    headline = PbStr::from(__content.string(5527));
                    if g.rand_seed % 2 == 1 {
                        phrase = PbStr::from("Europe");
                        if g.country_idx[corp_id] > 0 {
                            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) && g.country_idx[corp_id] != 8 {
                                phrase = PbStr::from("China");
                            } else {
                                phrase = PbStr::from("U.S.");
                            }
                        }
                        headline = PbStr::from(__content.string(5528)) + &phrase;
                    }
                }
                53 | 54 => {
                    phrase = PbStr::from("bombing");
                    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                        phrase = PbStr::from(__content.string(5529));
                    }
                    headline = PbStr::from(__content.string(5530)) + &phrase;
                }
                55 => {
                    headline = PbStr::from(__content.string(5531));
                    if g.rand_seed % 2 == 1 {
                        headline = PbStr::from(__content.string(5532));
                    }
                }
                56 => {
                    headline = PbStr::from(__content.string(5533));
                    if g.rand_seed % 3 == 0 {
                        headline = PbStr::from(__content.string(5534));
                    }
                    if g.rand_seed % 3 == 1 && g.country_idx[corp_id] == 0 {
                        headline = PbStr::from(__content.string(5535));
                    }
                }
                57 => {
                    headline = PbStr::from(__content.string(5536));
                    if g.rand_seed % 3 == 1 {
                        headline = PbStr::from(__content.string(5537));
                    }
                    if g.rand_seed % 3 == 2 {
                        headline = PbStr::from(__content.string(5538));
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5539));
                    }
                    if rng.rnd() * Ext::from_f64(10.0) < Ext::from_f64(1.0) {
                        headline = PbStr::from(__content.string(5540));
                    }
                }
                58 | 66 => {
                    headline = PbStr::from(__content.string(5541));
                    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                        headline = PbStr::from(__content.string(5542));
                    }
                    if g.rand_seed % 4 == 1 {
                        phrase = PbStr::from(" fined");
                        if rng.rnd() * Ext::from_f64(2.0) < Ext::from_f64(1.0) {
                            phrase = PbStr::from(__content.string(5543));
                        }
                        headline = phrase.clone() + __content.string(5544);
                    } else if g.rand_seed % 4 == 2 {
                        g.news_msg = g.tmp_symbol.clone() + __content.string(5545);
                        if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                            g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(5546), __content.bytes(5547));
                            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) && g.country_idx[corp_id] > 0 {
                                g.news_msg = replace(g.news_msg.as_bytes(), b"soccer", __content.bytes(5548));
                            }
                        }
                        headline = PbStr::new();
                        margin_delta = 8;
                        cap_mult = 1.25f32;
                    }
                }
                59 => {
                    phrase = PbStr::from(" Vegas");
                    if rng.rnd() * Ext::from_f64(5.0) > sf(3.5f32) {
                        phrase = PbStr::from(" Macao");
                    }
                    if rng.rnd() * Ext::from_f64(5.0) > sf(3.8f32) {
                        phrase = PbStr::from(" Reno");
                    }
                    if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) {
                        phrase = PbStr::from(" Manila");
                    }
                    if rng.rnd() * Ext::from_f64(5.0) > sf(4.2f32) {
                        phrase = PbStr::from(" Tahoe");
                    }
                    headline = phrase.clone() + __content.string(5549);
                    let mut tsunami = false;
                    if instr(1, phrase.as_bytes(), b"Manila") >= 1 && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                        headline = replace(headline.as_bytes(), __content.bytes(5349), b"tsunami");
                        tsunami = true;
                    }
                    if !tsunami
                        && instr(1, phrase.as_bytes(), b"Tahoe") > 0
                        && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0)
                    {
                        headline = replace(headline.as_bytes(), __content.bytes(5349), __content.bytes(5550));
                    }
                    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                        headline = replace(headline.as_bytes(), __content.bytes(5551), __content.bytes(5552));
                    }
                    if g.rand_seed % 3 == 1 {
                        headline =
                            PbStr::from(__content.string(5553)) + &phrase + __content.string(5554);
                    }
                }
                60 => {
                    headline = PbStr::from(__content.string(5555));
                    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                        headline = replace(headline.as_bytes(), b"Russia", b"China");
                    }
                    if g.rand_seed % 7 == 5 {
                        headline = PbStr::from(__content.string(5556));
                    }
                    if g.rand_seed % 7 == 3 {
                        headline = PbStr::from(__content.string(5557));
                    }
                }
                61 => {
                    headline = PbStr::from(__content.string(5558));
                    if g.rand_seed % 2 == 1 {
                        headline = PbStr::from(__content.string(5559));
                    }
                }
                62 => {
                    headline = PbStr::from(__content.string(5560));
                    if g.rand_seed % 2 == 1 {
                        headline = PbStr::from(__content.string(5561));
                    }
                    if g.rand_seed % 7 == 2 {
                        headline = PbStr::from(__content.string(5562));
                    }
                }
                63 => {
                    headline = PbStr::from(__content.string(5563));
                    if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                        headline = PbStr::from(__content.string(5564));
                    }
                }
                64 => {
                    phrase = PbStr::from(__content.string(5565));
                    if g.rand_seed % 3 == 1 {
                        phrase = PbStr::new();
                    }
                    if g.rand_seed % 3 == 2 {
                        phrase = PbStr::from(__content.string(5566));
                    }
                    headline = PbStr::from(__content.string(5567)) + &phrase;
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5568));
                    }
                }
                65 => {
                    headline = PbStr::from(__content.string(5569));
                    if g.rand_seed % 2 == 1 {
                        headline = PbStr::from(__content.string(5570));
                    }
                    if rng.rnd() * Ext::from_f64(7.0) > Ext::from_f64(6.0) {
                        headline = PbStr::from(__content.string(5571));
                    }
                }
                69 => {
                    headline = PbStr::from(__content.string(5572));
                    if g.rand_seed % 4 == 1 {
                        headline = PbStr::from(__content.string(5573));
                    }
                    if g.rand_seed % 4 == 2 {
                        headline = PbStr::from(__content.string(5574));
                    }
                    if g.rand_seed % 4 == 3 {
                        phrase = PbStr::from("EPA");
                        if g.country_idx[corp_id] > 0 {
                            phrase = PbStr::from(__content.string(5575));
                        }
                        headline = PbStr::from(__content.string(5576)) + &phrase;
                    }
                }
                70 => {
                    headline = PbStr::from(__content.string(5577));
                    if g.country_idx[corp_id] == 0 {
                        headline = PbStr::from(__content.string(5578));
                    }
                }
                _ => {}
            }
            break 'ev Flow::Disaster;
        }
        if sel < Ext::from_f64(60.0) {
            // ---- mineral finds, large contracts, patents
            let sv = industry;
            if matches!(sv, 5 | 6 | 7 | 8 | 9 | 11 | 12 | 15 | 31 | 33 | 50) {
                headline = PbStr::from(__content.string(5579));
                if g.rand_seed % 25 < 15 {
                    headline = PbStr::from(__content.string(5580));
                    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                        headline = replace(headline.as_bytes(), b"Huge", b"Large");
                    }
                }
                g.news_msg = headline.clone() + " on " + &corp_label + __content.string(5581);
                if industry == 7 && g.rand_seed % 6 == 1 {
                    g.news_msg = g.tmp_symbol.clone() + __content.string(5582);
                }
                margin_delta = 8;
                cap_mult = 1.17f32;
            } else if matches!(sv, 8 | 10 | 14 | 30 | 53 | 54 | 55 | 61..=64) {
                phrase = PbStr::from(" wins");
                if g.rand_seed % 3 == 1 {
                    phrase = PbStr::from(__content.string(5583));
                }
                if g.rand_seed % 3 == 2 {
                    phrase = PbStr::from(__content.string(5584));
                }
                change_val = cast::<i32, _>(industry);
                phrase2 = match change_val {
                    10 => PbStr::from(__content.string(5585)),
                    14 => PbStr::from(__content.string(5586)),
                    30 => PbStr::from(__content.string(5587)),
                    61 => PbStr::from(__content.string(5588)),
                    62 => lcase(g.ind_name[change_val].as_bytes(), CaseMode::Ansi) + " ",
                    63 => PbStr::from(__content.string(5589)),
                    64 => PbStr::from(__content.string(5590)),
                    _ => PbStr::new(),
                };
                g.news_msg = phrase.clone() + " large " + &phrase2 + __content.string(5591);
                if len(corp_label.as_bytes()) + len(g.news_msg.as_bytes()) < 69 {
                    g.news_msg = corp_label.clone() + &g.news_msg;
                } else {
                    g.news_msg = g.tmp_symbol.clone() + &g.news_msg;
                }
                margin_delta = 9;
                cap_mult = 1.2f32;
            } else if matches!(sv, 4 | 20..=22 | 35 | 51) {
                let mut f4: f32 = cast::<f32, _>(-Ext::from_f32(10.0f32));
                if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(2.0) {
                    f4 = 10.0f32;
                    phrase = PbStr::from("patent");
                    if industry == 4 || industry == 20 || industry == 21 {
                        phrase = PbStr::from(__content.string(5592));
                    }
                    headline = PbStr::from(__content.string(5593)) + &phrase;
                } else {
                    headline = PbStr::from(__content.string(5594));
                    if len(corp_label.as_bytes()) > 26 {
                        headline = remove(headline.as_bytes(), b" costly");
                    }
                    loss_amt = to_quad(cast::<Ext, _>(g.difficulty_level + 2) * g.market_cap[corp_id]);
                }
                margin_delta = to_integer(cast::<Ext, _>(f4));
                cap_mult = cast::<f32, _>((cast::<Ext, _>(f4) + Ext::from_f64(100.0)) * sf(0.01f32));
                g.news_msg = corp_label.clone() + &headline;
            } else {
                break 'ev Flow::Exit;
            }
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(62.0) {
            // ---- tax case
            if matches!(industry, 4..=12 | 14 | 15 | 20..=22 | 30 | 31 | 33 | 50 | 51 | 53 | 54 | 55 | 61..=64)
                && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0)
            {
                break 'ev Flow::Exit;
            }
            let mut tax_amount = to_quad(g.market_cap[corp_id] * Ext::from_f64(5.0));
            if g.rand_seed % 2 == 1 {
                cap_mult = 1.15f32;
                headline = PbStr::from(" wins ");
                tax_amount = to_quad(-(cast::<Ext, _>(tax_amount)));
            } else {
                cap_mult = 0.85f32;
                headline = PbStr::from(" loses ");
            }
            loss_amt = tax_amount;
            g.extra_items_ytd[corp_id] = g.extra_items_ytd[corp_id] - cast::<Ext, _>(tax_amount);
            phrase = fmt_trim((cast::<Ext, _>(tax_amount)).abs(), b"###,");
            g.news_msg = corp_label.clone()
                + &headline
                + &cur_sym(g)
                + &phrase
                + " "
                + &g.million_prefix_lc
                + __content.string(5595);
            phrase = PbStr::from("case.");
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                phrase = PbStr::from(__content.string(5596));
            }
            g.news_msg = g.news_msg.clone() + &phrase;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(64.0) {
            // ---- Nigerian oil production
            if g.peak_oil_crisis == 1 {
                break 'ev Flow::Exit;
            }
            if g.nigeria_oil_news_cooldown > 1 {
                g.nigeria_oil_news_cooldown -= 2;
                break 'ev Flow::Exit;
            }
            if g.nigeria_oil_news_cooldown == 0 {
                phrase = PbStr::from(__content.string(5597));
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    phrase = PbStr::from(__content.string(5598));
                }
                if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
                    phrase = PbStr::from(__content.string(5599));
                }
                headline = PbStr::from(__content.string(5600)) + &phrase;
                if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                    let r = rng.rnd();
                    g.oil_target_price = cast::<f32, _>(cast::<f32, _>((r * sf(0.2f32) + sf(1.1f32)) * cast::<Ext, _>(g.oil_target_price)));
                }
                g.oil_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_price) * sf(1.02f32)));
            } else {
                phrase = PbStr::from(__content.string(5601));
                if rng.rnd() * Ext::from_f64(2.0) < Ext::from_f64(1.0) {
                    phrase = PbStr::from(__content.string(5602));
                }
                headline = PbStr::from(__content.string(5603)) + &phrase;
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    let r = rng.rnd();
                    g.oil_target_price = cast::<f32, _>(cast::<f32, _>((sf(0.9f32) - r * sf(0.2f32)) * cast::<Ext, _>(g.oil_target_price)));
                }
                g.oil_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_price) * sf(0.985f32)));
            }
            g.nigeria_oil_news_cooldown += 49;
            if !headline.is_empty() {
                g.news_msg = PbStr::from(__content.string(5604)) + &headline;
            }
            if instr(1, phrase.as_bytes(), b"port f") > 0 {
                g.news_msg = replace(g.news_msg.as_bytes(), b"Nigeri", b"Liby");
            }
            corp_id = 0;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(68.0) {
            // ---- government bond yield streaks
            change_val = price_trend_streak(g, 1602);
            streak = price_trend_streak(g, 1603);
            pick_num = if cast::<Ext, _>(streak.abs()) <= (cast::<Ext, _>(change_val)).abs() { change_val } else { streak };
            if (cast::<Ext, _>(pick_num)).abs() > Ext::from_f64(4.0) {
                phrase = PbStr::from("risen");
                if pick_num < 0 {
                    phrase = PbStr::from("dropped");
                    if rng.rnd() * Ext::from_f64(2.0) != Ext::from_f64(0.0) {
                        phrase = PbStr::from("fallen");
                    }
                }
                g.news_msg = PbStr::from(__content.string(5605))
                    + &phrase
                    + " for"
                    + &number_to_words(pick_num, 5, 0)
                    + __content.string(5606);
                if streak == pick_num {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"Long", b"Short");
                }
                post_news(g, ui, 0, 0);
            }
            break 'ev Flow::Exit;
        }
        if sel < Ext::from_f64(75.0) {
            // ---- embezzlement / currency losses
            headline = PbStr::from(__content.string(5607));
            if g.rand_seed % 18 > 6 {
                headline = PbStr::from(__content.string(5608));
            }
            phrase = PbStr::from("Greek");
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                phrase = PbStr::from("Irish");
            }
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
                phrase = PbStr::from("Italian");
            }
            if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) {
                phrase = PbStr::from("Spanish");
            }
            if g.rand_seed % 18 > 10 && industry < 3 {
                headline = PbStr::from(__content.string(5609)) + &phrase + __content.string(5610);
            }
            if g.rand_seed % 18 > 15 {
                headline = PbStr::from(__content.string(5611));
            }
            loss_amt = to_quad(g.market_cap[corp_id] * Ext::from_f64(7.0));
            cap_mult = 0.88f32;
            phrase2 = fmt_trim(cast::<Ext, _>(loss_amt), b"* ###,");
            g.news_msg = g.tmp_symbol.clone()
                + &headline
                + __content.string(5612)
                + &cur_sym(g)
                + &phrase2
                + " "
                + &g.million_prefix_lc
                + "il."
                + &g.currency_suffix;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(80.0) {
            // ---- state of the economy
            if g.cur_quarter != g.econ_news_quarter {
                phrase = PbStr::new();
                change_val = price_trend_streak(g, 1604);
                let gdp = cast::<Ext, _>(g.gdp_growth);
                if gdp <= sf(4.5f32) {
                    if gdp >= Ext::from_f64(0.0) {
                        if change_val < 6 {
                            break 'ev Flow::Exit;
                        }
                        headline = PbStr::from(__content.string(5613))
                            + &number_to_words(change_val, 0, 0)
                            + " months";
                    } else {
                        headline = PbStr::from(__content.string(5614));
                        if gdp < sf(-8.5f32) {
                            headline = PbStr::from(__content.string(5615));
                        }
                        let target = cast::<Ext, _>(g.gdp_growth_target);
                        if gdp + Ext::from_f64(2.0) < target && target > Ext::from_f64(1.0) {
                            phrase = PbStr::from(__content.string(5616));
                        }
                    }
                } else {
                    headline = PbStr::from(__content.string(5617));
                    if (cast::<Ext, _>(g.gdp_growth_target)) < sf(2.5f32) {
                        phrase = PbStr::from(__content.string(5618));
                    }
                }
                g.news_msg = PbStr::from(__content.string(5619)) + &headline + &phrase;
                if change_val > 4 && gdp > sf(2.2f32) {
                    g.news_msg = PbStr::from(__content.string(5620))
                        + &number_to_words(change_val, 5, 0)
                        + __content.string(5621);
                }
                post_news(g, ui, 0, 0);
                g.econ_news_quarter = g.cur_quarter;
            }
            break 'ev Flow::Exit;
        }
        if sel < Ext::from_f64(82.0) {
            // ---- corn
            change_val = price_pct_change_months(g, 10, 6);
            if (cast::<Ext, _>(change_val)).abs() > Ext::from_f64(15.0) {
                phrase = PbStr::from("up");
                if change_val < 0 {
                    phrase = PbStr::from("down");
                }
                g.news_msg = PbStr::from(__content.string(5622))
                    + &phrase
                    + &number_to_words(change_val, 13, 1)
                    + __content.string(5623);
            }
            streak = price_trend_streak(g, 10);
            if (cast::<Ext, _>(streak)).abs() > Ext::from_f64(3.0) {
                phrase = PbStr::from("risen");
                if streak < 0 {
                    phrase = PbStr::from(__content.string(1057));
                }
                g.news_msg = PbStr::from(__content.string(5624))
                    + &phrase
                    + " for"
                    + &number_to_words(streak, 15, 0)
                    + __content.string(5621);
            }
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
            break 'ev Flow::Exit;
        }
        if sel < Ext::from_f64(84.0) {
            // ---- wheat
            change_val = price_pct_change_months(g, 9, 12);
            if (cast::<Ext, _>(change_val)).abs() > Ext::from_f64(19.0) {
                phrase = PbStr::from("up");
                if change_val < 0 {
                    phrase = PbStr::from("down");
                }
                g.news_msg = PbStr::from(__content.string(5625))
                    + &phrase
                    + &number_to_words(change_val, 13, 1)
                    + __content.string(1347);
                streak = price_trend_streak(g, 9);
                if (cast::<Ext, _>(streak)).abs() > Ext::from_f64(6.0) {
                    phrase = PbStr::from("risen");
                    if streak < 0 {
                        phrase = PbStr::from("fallen");
                    }
                    g.news_msg = PbStr::from(__content.string(5626))
                        + &phrase
                        + " for"
                        + &number_to_words(streak, 17, 0)
                        + __content.string(5627);
                }
                post_news(g, ui, 0, 0);
            }
            break 'ev Flow::Exit;
        }
        if sel < Ext::from_f64(86.0) {
            // ---- silver
            streak = price_trend_streak(g, 8);
            if (cast::<Ext, _>(streak)).abs() > Ext::from_f64(4.0) {
                phrase = PbStr::from("rising");
                if streak < 0 {
                    phrase = PbStr::from("falling");
                }
                g.news_msg = PbStr::from(__content.string(5628))
                    + &phrase
                    + " for"
                    + &number_to_words(streak, 12, 0)
                    + " months";
                post_news(g, ui, 0, 0);
            }
            break 'ev Flow::Exit;
        }
        if sel < Ext::from_f64(88.0) {
            // ---- gold
            change_val = price_pct_change_months(g, 7, 12);
            if (cast::<Ext, _>(change_val)).abs() > Ext::from_f64(19.0) {
                phrase = PbStr::from("up");
                if change_val < 0 {
                    phrase = PbStr::from("off");
                }
                g.news_msg = PbStr::from(__content.string(5629))
                    + &phrase
                    + &number_to_words(change_val, 20, 1)
                    + __content.string(5630);
                post_news(g, ui, 0, 0);
            }
            break 'ev Flow::Exit;
        }
        if sel < Ext::from_f64(90.0) {
            // ---- Brent crude
            if is_in_recent_headlines(g, b"Brent") > 0 || is_in_recent_headlines(g, b"gyrates") > 0 {
                break 'ev Flow::Exit;
            }
            streak = price_trend_streak(g, 6);
            if (cast::<Ext, _>(streak)).abs() >= Ext::from_f64(6.0) {
                headline = PbStr::from("fall");
                if streak > 0 {
                    headline = PbStr::from("ris");
                }
                g.news_msg = PbStr::from(__content.string(5631))
                    + &headline
                    + __content.string(5632)
                    + &fmt_trim(cast::<Ext, _>(g.oil_price) * Ext::from_f64(100.0), b"* ###.00");
                if g.peak_oil_crisis == 1 && (cast::<Ext, _>(g.oil_price)) < sf(0.95f32) {
                    g.news_msg = PbStr::from(__content.string(5633));
                }
                post_news(g, ui, 0, 0);
            }
            break 'ev Flow::Exit;
        }
        if sel < Ext::from_f64(100.0) {
            // ---- earnings guidance
            if industry <= 1 || industry >= 70 || (cast::<Ext, _>(g.insider_earn_tip_corp)).abs() == cast::<Ext, _>(corp_id) {
                break 'ev Flow::Apply;
            }
            let old_proj = g.proj_qtr_earnings[corp_id];
            let raise = rng.rnd() * Ext::from_f64(5.0) <= cast::<Ext, _>(g.gdp_growth) && rng.rnd() * Ext::from_f64(4.0) <= Ext::from_f64(3.0);
            if raise {
                margin_delta = 11;
                adjust_profit_margin_with_misc(g, corp_id, margin_delta);
                calc_projected_earnings(g, ui, rng, corp_id);
                margin_delta = 0;
                cap_mult = 1.17f32;
                phrase = PbStr::from(__content.string(5634));
                if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    phrase = PbStr::from(__content.string(5635));
                }
                headline = PbStr::from(__content.string(5636)) + &phrase;
                let p = g.proj_qtr_earnings[corp_id];
                if p < old_proj {
                    headline = PbStr::from(__content.string(5637));
                }
                if old_proj < Ext::from_f64(0.0) && p >= Ext::from_f64(0.0) {
                    headline = PbStr::from(__content.string(5638));
                }
                if old_proj < Ext::from_f64(0.0) && p < Ext::from_f64(0.0) && old_proj < p {
                    headline = PbStr::from(__content.string(5639));
                }
                g.news_msg = g.tmp_symbol.clone() + &headline;
                break 'ev Flow::Apply;
            }
            margin_delta = -7;
            cap_mult = 0.9f32;
            adjust_profit_margin_with_misc(g, corp_id, margin_delta);
            margin_delta = 0;
            calc_projected_earnings(g, ui, rng, corp_id);
            let p = g.proj_qtr_earnings[corp_id];
            headline = PbStr::from(__content.string(5640));
            if g.rand_seed % 3 == 1 || old_proj == Ext::from_f64(0.0) || old_proj < p {
                headline = PbStr::from(__content.string(5641));
            }
            if old_proj < p && p < Ext::from_f64(0.0) {
                headline = PbStr::from(__content.string(5642));
            }
            if old_proj > Ext::from_f64(0.0) && p < Ext::from_f64(0.0) {
                headline = PbStr::from(__content.string(5643));
            }
            phrase = corp_label.clone();
            if len(phrase.as_bytes()) + len(headline.as_bytes()) > 61 {
                phrase = g.tmp_symbol.clone();
            }
            g.news_msg = phrase.clone() + &headline;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(105.0) {
            // ---- patent / trade secret / copyright award against a competitor
            let ip_kind = if matches!(industry, 4 | 10 | 14 | 18 | 20..=22 | 24 | 30 | 32 | 35 | 38..=40 | 49 | 51 | 61 | 64) {
                1
            } else if matches!(industry, 26 | 27 | 29) {
                2
            } else {
                0
            };
            if ip_kind != 0 {
                let mut idx = 1;
                loop {
                    change_val = rng.rnd_range(131, 1600);
                    if g.entity_status[change_val] == 1
                        && g.corp_controller[change_val] != g.corp_controller[corp_id]
                        && g.industry_num[change_val] == industry
                    {
                        idx = 700;
                    }
                    idx += 1;
                    if idx >= 701 {
                        break;
                    }
                }
                if change_val > 0 {
                    let cv = change_val;
                    loss_amt = to_quad(g.market_cap[cv] * -Ext::from_f64(5.0));
                    g.cash[cv] = g.cash[cv] + cast::<Ext, _>(loss_amt);
                    g.extraordinary_pl[cv] = to_quad(cast::<Ext, _>(g.extraordinary_pl[cv]) + cast::<Ext, _>(loss_amt));
                    g.market_cap[cv] = sf(0.92f32) * g.market_cap[cv];
                    update_price_history_tick(g, ui, rng, cv);
                    cap_mult = 1.13f32;
                    headline = PbStr::from("patent");
                    if g.rand_seed % 3 == 1 {
                        headline = PbStr::from(__content.string(5644));
                    }
                    if ip_kind == 2 {
                        headline = PbStr::from(__content.string(5645));
                    }
                    phrase = cur_sym(g);
                    phrase2 = fmt_trim((cast::<Ext, _>(loss_amt)).abs(), b"##########");
                    phrase = phrase.clone() + &phrase2 + " " + &g.million_prefix_lc + "il. ";
                    if val(phrase2.as_bytes()) > Ext::from_f64(0.0) {
                        phrase2 = PbStr::from(__content.string(5646));
                        if g.rand_seed % 4 == 1 {
                            phrase2 = PbStr::from(__content.string(5647));
                        }
                        if g.rand_seed % 4 == 2 {
                            phrase2 = PbStr::from(__content.string(5648));
                        }
                        if g.rand_seed % 4 == 3 {
                            phrase2 = PbStr::from(__content.string(5649));
                        }
                        g.news_msg = g.tmp_symbol.clone()
                            + " wins "
                            + &phrase
                            + &headline
                            + " "
                            + &phrase2
                            + " "
                            + &g.ent_symbol[cv];
                    }
                }
            }
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(108.0) {
            // ---- superfund liability (U.S. property)
            let ci = g.country_idx[corp_id];
            if matches!(ci, 0 | 2 | 6 | 15 | 16 | 25 | 30 | 31 | 43 | 45 | 50) {
                if g.superfund_corp > 0 {
                    break 'ev Flow::Exit;
                }
                cap_mult = 0.82f32;
                loss_amt = to_quad(g.market_cap[corp_id] * Ext::from_f64(7.0));
                g.superfund_corp = cast::<i16, _>(corp_id);
                let cy = cast::<Ext, _>(g.cur_year);
                g.superfund_end_year = to_long(int(rng.rnd() * Ext::from_f64(3.0)) + Ext::from_f64(2.0) + cy);
                g.news_msg = g.tmp_symbol.clone() + __content.string(5650);
            }
            break 'ev Flow::Apply;
        }
        if sel < cast::<Ext, _>(g.difficulty_level + 109) {
            // ---- accounting fraud
            if g.corp_controller[corp_id] < 11 && rng.rnd() * Ext::from_f64(9.0) > Ext::from_f64(1.0) {
                break 'ev Flow::Exit;
            }
            if g.last_fraud_corp == corp_id {
                break 'ev Flow::Exit;
            }
            if corp_id >= 131 && g.business_assets[corp_id] > Ext::from_f64(400.0) {
                streak = to_long(g.business_assets[corp_id] * sf(0.3f32));
                g.extraordinary_pl[corp_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[corp_id]) - cast::<Ext, _>(streak));
                g.business_assets[corp_id] = sf(0.7f32) * g.business_assets[corp_id];
                loss_amt = to_quad(rng.rnd() * Ext::from_f64(300.0));
            } else {
                let total_assets = g.total_assets;
                if g.net_worth > Ext::from_f64(500.0) {
                    loss_amt = to_quad(g.net_worth * sf(1.1f32));
                }
                if g.total_assets < cast::<Ext, _>(loss_amt) || loss_amt == 0 {
                    loss_amt = to_quad((rng.rnd() * sf(0.6f32) + sf(0.2f32)) * total_assets);
                }
            }
            change_val = to_long(g.market_cap[corp_id] / Ext::from_f64(30.0));
            change_val = change_val * 100 + 1000;
            phrase = PbStr::from(__content.string(5651)) + &fmt_trim(cast::<Ext, _>(change_val), b"* ###,");
            if rng.rnd() * Ext::from_f64(2.0) < Ext::from_f64(1.0) {
                phrase = PbStr::from(__content.string(5652))
                    + &cur_sym(g)
                    + &fmt_trim(cast::<Ext, _>(loss_amt) + cast::<Ext, _>(streak), b"* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + "il.";
            }
            phrase2 = PbStr::from(__content.string(5653));
            if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
                phrase2 = PbStr::from(__content.string(5654));
            }
            headline = g.ent_symbol[corp_id].clone() + __content.string(5655) + &phrase2 + "; ";
            phrase2 = PbStr::from(__content.string(5656));
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                phrase2 = PbStr::from(__content.string(5657));
            }
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                phrase2 = PbStr::from(__content.string(5658));
            }
            if rng.rnd() * Ext::from_f64(5.0) < Ext::from_f64(1.0) {
                phrase2 = PbStr::from(__content.string(5659));
            }
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                headline = phrase2.clone() + __content.string(5660) + &g.ent_symbol[corp_id] + " -- ";
            }
            cap_mult = cast::<f32, _>(rng.rnd() * sf(0.5f32) + sf(0.2f32));
            if loss_amt > 0 {
                g.news_msg = headline.clone() + &phrase;
                margin_delta = -7;
            }
            if rng.rnd() * Ext::from_f64(8.0) < Ext::from_f64(1.0) && g.market_cap[corp_id] > Ext::from_f64(650.0) {
                g.news_msg =
                    PbStr::from(__content.string(5661)) + &g.ent_symbol[corp_id] + __content.string(5662);
            }
            g.last_fraud_corp = corp_id;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(115.0) {
            // ---- pension plan charge / surplus
            headline = g.ent_symbol[corp_id].clone() + " ";
            loss_amt = to_quad(g.market_cap[corp_id] * Ext::from_f64(9.0));
            if g.exchange_rate * sf(50000.0f32) < cast::<Ext, _>(loss_amt) {
                loss_amt = 30000;
            }
            cap_mult = 0.7f32;
            phrase = PbStr::from(__content.string(5663))
                + &cur_sym(g)
                + &fmt_trim(cast::<Ext, _>(loss_amt), b"* ###,")
                + " "
                + &g.million_prefix_lc
                + "il."
                + &g.currency_suffix;
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                cap_mult = 1.18f32;
                loss_amt = to_quad(-(cast::<Ext, _>(loss_amt)));
                phrase = PbStr::from(__content.string(5664));
            }
            g.news_msg = headline.clone() + &phrase;
            break 'ev Flow::Apply;
        }
        if sel < Ext::from_f64(130.0) {
            // ---- stock market support / deposit seizures
            if price_trend_streak(g, 0) < -5 && g.stabilize_news_count < 2 {
                g.news_msg = PbStr::from(__content.string(5665));
                if g.stabilize_news_count == 1 {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"Chinese", b"U.S.");
                }
                g.stabilize_news_count += 1;
            } else if (cast::<Ext, _>(g.gdp_growth)) < -Ext::from_f64(6.0)
                && g.deposit_seize_news_count < 2
                && price_trend_direction(g, 1604, 2, 1) <= 0
                && price_trend_direction(g, 1604, 4, 4) < 0
                && price_trend_direction(g, 1604, 6, 3) < 0
            {
                g.news_msg = PbStr::from(__content.string(5666));
                if g.deposit_seize_news_count == 1 {
                    g.news_msg = replace(g.news_msg.as_bytes(), b"Europe", __content.bytes(5667));
                    g.news_msg = replace(g.news_msg.as_bytes(), b"falter", b"weaken");
                }
                g.deposit_seize_news_count += 1;
            }
            break 'ev Flow::Apply;
        }
        Flow::Exit
    };

    match flow {
        Flow::Exit => return,
        Flow::Disaster => {
            // LAB_004ab3c6
            if !headline.is_empty() {
                cap_mult = cast::<f32, _>(rng.rnd() * sf(0.15f32) + sf(0.8f32));
                if industry < 3 || industry > 69 {
                    if instr(1, headline.as_bytes(), __content.bytes(3710)) == 0 {
                        loss_amt = to_quad(cast::<Ext, _>(g.difficulty_level + 2) * g.market_cap[corp_id]);
                    }
                } else {
                    loss_amt = to_quad(g.business_assets[corp_id] * sf(0.07f32));
                    g.business_assets[corp_id] = sf(0.93f32) * g.business_assets[corp_id];
                }
                phrase = corp_label.clone();
                if len(phrase.as_bytes()) + len(headline.as_bytes()) > 60 {
                    phrase = g.tmp_symbol.clone();
                }
                g.news_msg = phrase.clone() + &headline + ".";
            }
        }
        Flow::Apply => {}
    }

    // LAB_004aeb8c
    if corp_id > 10 {
        g.market_cap[corp_id] = cast::<Ext, _>(cap_mult) * g.market_cap[corp_id];
        update_price_history_tick(g, ui, rng, corp_id);
        g.cash[corp_id] = g.cash[corp_id] - cast::<Ext, _>(loss_amt);
        if instr(1, g.news_msg.as_bytes(), __content.bytes(5668)) < 1 && instr(1, g.news_msg.as_bytes(), __content.bytes(5669)) <= 0 {
            g.extraordinary_pl[corp_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[corp_id]) - cast::<Ext, _>(loss_amt));
        }
        if margin_delta != 0 {
            adjust_profit_margin_with_misc(g, corp_id, margin_delta);
        }
        calc_projected_earnings(g, ui, rng, corp_id);
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, corp_id, 0);
    }
    g.rand_corp_news_cooldown = to_long(cast::<Ext, _>(time_secs) + Ext::from_f64(20.0));
}
