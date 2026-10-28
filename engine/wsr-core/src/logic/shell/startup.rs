//! Startup file handling and registration information retained by the original application.
use crate::pb::real::{Ext, cast};

use super::prelude::*;
use crate::pb::numeric::{InputReader, write_str};

/// Port of CountActiveHumanPlayers (0x4a02b7). The original tests the OUT name marker, not entity status.
pub fn count_active_human_players(g: &Globals) -> i32 {
    cast::<i32, _>((1..=g.num_players)
        .filter(|&p| g.player_name[p] != "...OUT..." && g.is_computer_player[p] != 1)
        .count())
}

/// Port of ProcessFlagFileAndSerial (0x473492): show the flag's first line, then remove both transient files.
pub fn process_flag_file_and_serial(g: &mut Globals, ui: &mut dyn Ui, env: &Env) {
    let flag = g.data_path.clone() + "flagfile.txt";
    let data = env.files.borrow().read(flag.as_bytes()).cloned();
    if let Some(data) = data {
        // LINE_INPUT$ returns an empty string at EOF here; the empty flag is still removed.
        let line = InputReader::new(&data).line_input();
        let mut line = PbStr(line);
        show_quick_message(g, ui, &mut line);
        env.files.borrow_mut().delete(flag.as_bytes());
    }
    env.files
        .borrow_mut()
        .delete((g.data_path.clone() + "SERIAL.INI").as_bytes());
}

/// Port of PromptRegistration (0x472ce9): store the user's supplied owner and order number with WRITE #.
pub fn prompt_registration(g: &mut Globals, ui: &mut dyn Ui, env: &Env) {
    let __content = crate::content::current();
    let mut notice = s(__content.string(10418))
        + __content.string(10419)
        + __content.string(10420)
        + __content.string(10421);
    show_quick_message(g, ui, &mut notice);
    g.licensee_name = ui.inputbox(
        &s(__content.string(10422)),
        &g.app_title,
        &s(__content.string(10423)),
    );
    g.registration_num = ui.inputbox(
        &s(__content.string(10424)),
        &g.app_title,
        &s(""),
    );
    if g.licensee_name.len() >= 5 && g.registration_num.len() >= 5 {
        let mut message = s(
            __content.string(10425),
        ) + __content.string(10426)
            + __content.string(10427)
            + &g.registration_num
            + ".\r\n\r\n"
            + __content.string(10428);
        show_quick_message(g, ui, &mut message);
        let mut data = write_str((g.licensee_name.clone() + "|" + &g.registration_num).as_bytes());
        data.extend_from_slice(b"\r\n");
        env.files
            .borrow_mut()
            .write((g.data_path.clone() + "REGINFO.DAT").as_bytes(), data);
    } else {
        ui.msgbox(&s(__content.string(10429)), 8208, &s(__content.string(10430)));
        g.licensee_name = PbStr::new();
        g.registration_num = PbStr::new();
    }
}

/// Port of LoadRegistrationInfo (0x47290f), including its TIMER reseed and occasional owner reminder.
pub fn load_registration_info(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let path = g.data_path.clone() + "REGINFO.DAT";
    let data = env.files.borrow().read(path.as_bytes()).cloned();
    let Some(data) = data else {
        prompt_registration(g, ui, env);
        return;
    };
    let Ok(record) = InputReader::new(&data).try_input_str() else {
        return;
    };
    if crate::pb::string::parse_count_any(&record, b"|") == 2 {
        g.licensee_name = parse_any(&record, b"|", 1);
        g.registration_num = parse_any(&record, b"|", 2);
        rng.randomize_timer(ui.local_time().timer_ms());
        if rng.rnd() * Ext::from_f64(5.0) >= Ext::from_f64(4.0) {
            let mut message = s(__content.string(10431))
                + &g.app_title
                + __content.string(10432)
                + &g.licensee_name
                + __content.string(10433)
                + &g.registration_num;
            show_quick_message(g, ui, &mut message);
        }
    } else if val(&record) == -Ext::from_f64(99.0) {
        if g.win_version == "XP" || g.cmd_line_mode == "INSTALL" {
            env.files.borrow_mut().delete(path.as_bytes());
        }
        prompt_registration(g, ui, env);
    }
}

/// Port of ShowUpgradeNotice (0x472fcd), including the historical date gates and two separate RND draws.
pub fn show_upgrade_notice(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    if g.data_path
        .to_display()
        .to_ascii_uppercase()
        .contains("PBWIN80")
    {
        return;
    }
    let now = ui.local_time();
    if now.year <= 2023 || (now.year == 2024 && now.month <= 9) {
        return;
    }
    let mut shown = false;
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) && now.day % 10 >= 6 {
        let message = s(__content.string(10434))
            + __content.string(10435)
            + __content.string(10436)
            + "==================================\r\r"
            + __content.string(10437)
            + __content.string(10438)
            + __content.string(10439)
            + __content.string(10440);
        shown = true;
        if ui.msgbox(&message, 8228, &s(__content.string(10441))) == 7 {
            return;
        }
        env.request(HostRequest::OpenUrl(s(
            __content.string(14458),
        )));
        if !g.licensee_name.is_empty() {
            let message = s(__content.string(10414))
                + &g.licensee_name
                + __content.string(10442)
                + &g.registration_num;
            ui.msgbox(&message, 8192, &s(__content.string(10416)));
        }
    }
    if !shown && rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0) {
        let message = s(__content.string(10443))
            + __content.string(10444)
            + &g.version_str
            + ".)\r\r"
            + __content.string(10445);
        if ui.msgbox(&message, 8228, &s(__content.string(10446))) != 7 {
            env.request(HostRequest::OpenUrl(s(
                __content.string(14459),
            )));
        }
    }
}
