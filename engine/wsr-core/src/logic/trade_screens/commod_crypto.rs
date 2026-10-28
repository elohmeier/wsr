//! The commodity desk and the crypto-currency trading platform.
//!
//! * [`commod_desk`]: `ShowCommodityDesk` (0x627a84), its dialog procedure `DlgProcCommodDesk` (0x628251)
//!   and button callbacks (`CommodDesk_*`), `CommodDesk_ShowTradeButtons` (0x629775),
//!   `CommodDesk_ResetSelection` (0x6299f6).
//! * [`crypto`]: `ShowDlgCrypto` (0x6b4357), `DlgProcCryptoColors` (0x6b4da7), the `DlgCrypto_*` button
//!   callbacks and `PromptCloseCryptoFutures` (0x6b6471).
//!
//! Both are modal DDT dialogs of command buttons. The port models each as a small state struct (button
//! captions, enabled flags, open flag: what a UI needs to draw it) and shows it through [`crate::ui::Ui::choose_enabled`]
//! in a loop, one choice per button click, until a callback ends the dialog or the user closes it
//! (`None` = the window's close box, `WM_CLOSE`, which runs no callback). The trades themselves are the
//! already-ported desk routines (`FuturesBuyOrCover`, `FuturesSellOrShort`, `BuyPhysCommod`,
//! `SellPhysCommod`, the position lists).
use crate::pb::real::{Ext, cast};


pub mod commod_desk;
pub mod crypto;

use crate::logic::finance::get_phys_commod_value;
use crate::logic::futures::{count_futures_positions, sort_futures_recs};
use crate::logic::reports_corp::consolidate_phys_commod_recs;
use crate::logic::reports_lists::{list_phys_commod_holdings, show_futures_positions};
use crate::logic::report::show_quick_message;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::turn::enable_main_controls;
use crate::logic::commod_ai::desk::{futures_buy_or_cover, futures_sell_or_short};
use crate::logic::commod_ai::physical::sell_phys_commod;
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{len, ltrim, parse_any, rtrim};
use crate::state::Globals;
use crate::ui::{Ui, id, mb};

pub(crate) fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// A dialog button: control id, caption (with the `&` accelerator marks) and enabled state.
#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub id: i32,
    pub caption: PbStr,
    pub enabled: bool,
}

impl Button {
    pub fn new(id: i32, caption: &str) -> Self {
        Button { id, caption: s(caption), enabled: true }
    }
}

/// Caption without the `&` accelerator marks (how the button reads).
pub fn plain_caption(caption: &PbStr) -> PbStr {
    PbStr(caption.as_bytes().iter().copied().filter(|&b| b != b'&').collect())
}

/// Shows command availability through [`Ui::choose_enabled`] and returns the clicked id (`None`: the
/// dialog was closed).
pub(crate) fn choose_button(ui: &mut dyn Ui, title: &PbStr, prompt: &[PbStr], buttons: &[Button]) -> Option<i32> {
    let options: Vec<PbStr> = buttons.iter().map(|b| plain_caption(&b.caption)).collect();
    let enabled: Vec<bool> = buttons.iter().map(|b| b.enabled).collect();
    ui.choose_enabled(title, prompt, &options, &enabled).map(|k| buttons[k].id)
}

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &[u8]) -> PbStr {
    rtrim(ltrim(x).as_bytes())
}

/// The common body of `CommodDesk_PositionsClick` (0x628ff8) and `DlgCrypto_ListPositions` (0x6b5d64)
/// after their own start: packs and sorts the records, picks the futures or the physical/crypto list
/// (`CommodTradeMode` becomes the list shown; with both kinds of positions a YES/NO/CANCEL MSGBOX asks),
/// shows it ([`show_futures_positions`] mode 4 / [`list_phys_commod_holdings`] mode 1) and trades the
/// picked record of `ActiveEntity`: a physical record or long futures position is sold
/// ([`sell_phys_commod`] after `ConsolidatePhysCommodRecs` for a company, [`futures_sell_or_short`] mode
/// 2), a short futures position covered ([`futures_buy_or_cover`] mode 2).
pub(crate) fn list_and_trade_positions(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    pack_holding_arrays(g, ui);
    // SortFuturesRecs packs again and returns any excess-loss recapture news.
    let events = sort_futures_recs(g);
    crate::logic::holdings::post_pack_events(g, ui, events);
    let events = sort_futures_recs(g);
    crate::logic::holdings::post_pack_events(g, ui, events);
    let ae = g.active_entity;
    let mut list_mode = PbStr::new();
    if get_phys_commod_value(g, ae, &PbStr::new()) == Ext::from_f64(0.0) && count_futures_positions(g, ae, 0) == 0 {
        let mut m = g.active_entity_name.clone()
            + __content.string(11334);
        show_quick_message(g, ui, &mut m);
        return;
    }
    let both = get_phys_commod_value(g, ae, &PbStr::new()) > Ext::from_f64(0.0) && count_futures_positions(g, ae, 0) > 0;
    if both {
        list_mode = s("PHYSICAL");
        let prompt = g.active_entity_name.clone()
            + __content.string(11335)
            + "\r"
            + "\r"
            + __content.string(11336);
        let answer = ui.msgbox(
            &prompt,
            mb::YESNOCANCEL | mb::ICONQUESTION | mb::TASKMODAL,
            &s(__content.string(11337)),
        );
        if answer == id::CANCEL {
            return;
        }
        if answer == id::YES {
            list_mode = s("FUTURES");
        }
    } else if 0 < count_futures_positions(g, ae, 0) {
        if g.commod_trade_mode == "PHYSICAL" {
            let mut m = s(__content.string(11338))
                + &g.active_entity_name
                + ". "
                + "\r\n"
                + "\r\n"
                + __content.string(11339);
            show_quick_message(g, ui, &mut m);
        }
        list_mode = s("FUTURES");
    } else if Ext::from_f64(0.0) < get_phys_commod_value(g, ae, &PbStr::new()) {
        if g.commod_trade_mode == "FUTURES" {
            let mut m = s(__content.string(11340))
                + &g.active_entity_name
                + "."
                + "\r\n"
                + "\r\n"
                + __content.string(11341);
            show_quick_message(g, ui, &mut m);
        }
        list_mode = s("PHYSICAL");
    }
    g.commod_trade_mode = list_mode.clone();
    let rec_idx;
    let pos_rec;
    if list_mode == "FUTURES" {
        rec_idx = show_futures_positions(g, ui, rng, g.active_entity, 4);
        if rec_idx < 1 || val(g.futures_recs[0].as_bytes()) < cast::<Ext, _>(rec_idx) {
            return;
        }
        pos_rec = g.futures_recs[rec_idx].clone();
    } else {
        rec_idx = list_phys_commod_holdings(g, ui, rng, g.active_entity, 1);
        if rec_idx < 1 || val(g.phys_commod_rec[0].as_bytes()) < cast::<Ext, _>(rec_idx) {
            return;
        }
        pos_rec = g.phys_commod_rec[rec_idx].clone();
    }
    if val(parse_any(pos_rec.as_bytes(), b"|", 2).as_bytes()) != cast::<Ext, _>(g.active_entity) {
        return;
    }
    g.cur_commodity = trim(parse_any(pos_rec.as_bytes(), b"|", 1).as_bytes());
    if g.cur_commodity.is_empty() || len(g.cur_commodity.as_bytes()) < 3 {
        return;
    }
    let ae = g.active_entity;
    if Ext::from_f64(0.0) <= val(parse_any(pos_rec.as_bytes(), b"|", 3).as_bytes()) {
        if g.commod_trade_mode == "PHYSICAL" {
            if 10 < ae {
                consolidate_phys_commod_recs(g, ae);
            }
            // SellPhysCommod(ActiveEntity, recIdx BYREF, 0, 0!, 0)
            sell_phys_commod(g, ui, rng, ae, rec_idx, 0, 0.0f32, 0);
        } else {
            futures_sell_or_short(g, ui, rng, 2, ae, rec_idx);
        }
    } else {
        futures_buy_or_cover(g, ui, rng, 2, ae, rec_idx);
    }
    enable_main_controls(g);
}
