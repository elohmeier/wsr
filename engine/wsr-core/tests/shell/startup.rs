//! Startup registration files and transient notices against the original.
use wsr_core::pb::real::cast;

use super::{common::orig_msgs, diff::*};
use wsr_core::{
    logic::{
        newgame::LocalTime,
        shell::{Env, Files, startup::*},
    },
    pb::{pbstr::PbStr, rng::Rng},
    state::Globals,
    ui::Scripted,
};
fn st(s: &str) -> PbStr {
    PbStr::from(s)
}
#[test]
fn registration_and_transient_files_match_original() {
    let __content = wsr_core::content::current();
    for variant in 0..10 {
        let mut g = Globals::default();
        g.app_title = st(__content.string(13661));
        g.data_path = st("C:\\WSR\\");
        g.win_version = st(if variant % 2 == 0 { "XP" } else { "10" });
        let mut files = Files::default();
        let answers = vec![
            "Jane Doe".to_string(),
            if variant == 7 { "12" } else { "12345" }.to_string(),
        ];
        let addr = if variant < 7 {
            0x47290f
        } else if variant == 7 {
            0x472ce9
        } else {
            0x473492
        };
        match variant {
            0 => {}
            1 => {
                files
                    .files
                    .insert("REGINFO.DAT".into(), b"\"Jane Doe|12345\"\r\n".to_vec());
            }
            2 | 3 => {
                files
                    .files
                    .insert("REGINFO.DAT".into(), b"\"-99\"\r\n".to_vec());
            }
            4 => {
                files
                    .files
                    .insert("REGINFO.DAT".into(), b"\"other data\"\r\n".to_vec());
            }
            5 => {
                files.files.insert("REGINFO.DAT".into(), vec![]);
            }
            6 => {
                files
                    .files
                    .insert("REGINFO.DAT".into(), b"\"|abc\"\r\n".to_vec());
            }
            8 => {
                files.files.insert(
                    "FLAGFILE.TXT".into(),
                    b"Welcome\r\nignored second line\r\n".to_vec(),
                );
                files.files.insert("SERIAL.INI".into(), b"example".to_vec());
            }
            9 => {
                files.files.insert("FLAGFILE.TXT".into(), vec![]);
                files.files.insert("SERIAL.INI".into(), b"example".to_vec());
            }
            _ => {}
        }
        let input_files = files
            .files
            .iter()
            .map(|(n, b)| (n.clone(), b.clone()))
            .collect();
        let env = Env::new(&LocalTime::HARNESS).with_files(files);
        let mut pg = g.clone();
        let mut rng = Rng::from_state(42);
        let mut ui = Scripted::default();
        ui.answers.extend(answers.iter().map(|s| st(s)));
        match addr {
            0x47290f => load_registration_info(&mut pg, &mut ui, &mut rng, &env),
            0x472ce9 => prompt_registration(&mut pg, &mut ui, &env),
            _ => process_flag_file_and_serial(&mut pg, &mut ui, &env),
        }
        let answer_refs: Vec<&str> = answers.iter().map(|s| s.as_str()).collect();
        let opts = Opts {
            rng: Some(42),
            answers: &answer_refs,
            files: input_files,
            no_default_files: true,
            ..Opts::default()
        };
        let Some(o) = run_original_with(addr, &opts, &g) else {
            return;
        };
        Compare::default().assert(&pg, &o.globals);
        assert_eq!(rng.state(), o.rng, "variant {variant}");
        assert_eq!(
            ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count(),
            orig_msgs(&o),
            "variant {variant}: {:?}",
            o.log
        );
        let expected: std::collections::BTreeMap<_, _> = o
            .files
            .iter()
            .filter(|(n, _)| n.as_str() != "TMP_QM.DAT")
            .map(|(n, b)| (n.clone(), b.clone()))
            .collect();
        assert_eq!(env.files.borrow().written, expected, "variant {variant}");
    }
}
#[test]
fn human_count_uses_name_marker() {
    let mut g = Globals::default();
    g.num_players = 5;
    for p in 1..=5 {
        g.player_name[p] = st("Player");
    }
    g.player_name[2] = st("...OUT...");
    g.is_computer_player[3] = 1;
    g.is_computer_player[4] = 2;
    let Some(o) = run_original_with(0x4a02b7, &Opts::default(), &g) else {
        return;
    };
    assert_eq!(count_active_human_players(&g), cast::<i32, _>(o.return_value));
    assert_eq!(o.return_value, 3);
}

#[derive(Default)]
struct ClockUi {
    now: LocalTime,
    answer: i32,
    messages: Vec<PbStr>,
}
impl wsr_core::ui::Ui for ClockUi {
    fn local_time(&self) -> LocalTime {
        self.now
    }
    fn msgbox(&mut self, t: &PbStr, style: i32, _: &PbStr) -> i32 {
        self.messages.push(t.clone());
        if style & 15 == 4 { self.answer } else { 1 }
    }
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, _: &PbStr) -> PbStr {
        PbStr::new()
    }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> wsr_core::ui::ReportAction {
        wsr_core::ui::ReportAction::Ok
    }
    fn news(&mut self, _: &PbStr) {}
}
#[test]
fn upgrade_dates_messages_urls_and_rng_match_original() {
    let __content = wsr_core::content::current();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !root.join("extracted/app/WSR.EXE").exists() {
        return;
    }
    let code = r#"import sys,struct
sys.path.insert(0,'tools/emu')
from game import *
e=Emu(default_files=False);base=e.api;messages=[];urls=[]
def api(name,a,ret):
 if name=='GetLocalTime':e.uc.mem_write(a[0],struct.pack('<8H',year,month,1,day,10,0,0,0));return 0
 if name=='MessageBoxA':messages.append(e.cstr(a[1]).hex());return answer if a[3]&15==4 else 1
 if name=='ShellExecuteA':urls.append(e.cstr(a[2]).hex());return 1
 return base(name,a,ret)
e.api=api
for k in range(120):
 year,month,day=[(2023,12,27),(2024,9,27),(2024,10,27),(2026,1,3),(2026,10,27),(2026,2,6)][k%6]
 answer=6 if k%2==0 else 7
 path='C:\\PBWIN80\\' if k%11==0 else 'C:\\WSR\\'
 # Resolve state keys from the generated layout, so this test does not duplicate addresses.
 fields={'DataPath':path,'AppTitle':'Wall Street Raider','VersionStr':'9.85','LicenseeName':'Jane Doe','RegistrationNum':'12345'}
 e.write_state({key:fields[name] for key,name,typ,bounds in layout() if name in fields})
 messages.clear();urls.clear();e.w32(RNG_STATE,k*98731+43)
 err,regs,args=e.call(0x472fcd,[]);assert err is None,err
 print(str(e.u32(RNG_STATE))+'\t'+';'.join(messages)+'\t'+';'.join(urls))
"#;
    let out = std::process::Command::new(root.join(".venv/bin/python"))
        .args(["-c", code])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let hex = |b: &[u8]| b.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let expected = String::from_utf8(out.stdout).unwrap();
    for (k, line) in expected.lines().enumerate() {
        let (year, month, day) = [
            (2023, 12, 27),
            (2024, 9, 27),
            (2024, 10, 27),
            (2026, 1, 3),
            (2026, 10, 27),
            (2026, 2, 6),
        ][k % 6];
        let mut g = Globals::default();
        g.app_title = st(__content.string(13661));
        g.version_str = st("9.85");
        g.licensee_name = st("Jane Doe");
        g.registration_num = st("12345");
        g.data_path = st(if k % 11 == 0 {
            "C:\\PBWIN80\\"
        } else {
            "C:\\WSR\\"
        });
        let now = LocalTime {
            year,
            month,
            day,
            ..LocalTime::HARNESS
        };
        let mut ui = ClockUi {
            now,
            answer: if k % 2 == 0 { 6 } else { 7 },
            ..ClockUi::default()
        };
        let mut rng = Rng::from_state(cast::<u32, _>(k) * 98731 + 43);
        let env = Env::new(&now);
        show_upgrade_notice(&mut g, &mut ui, &mut rng, &env);
        let msgs = ui
            .messages
            .iter()
            .map(|s| hex(s.as_bytes()))
            .collect::<Vec<_>>()
            .join(";");
        let urls = env
            .requests
            .borrow()
            .iter()
            .filter_map(|r| {
                if let wsr_core::logic::shell::HostRequest::OpenUrl(u) = r {
                    Some(hex(u.as_bytes()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(format!("{}\t{msgs}\t{urls}", rng.state()), line, "{__wsr_text_13924}{k}",__wsr_text_13924=__content.string(13924));
    }
}
