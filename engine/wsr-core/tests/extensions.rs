//! Run the actual extension DLLs in their own PowerBASIC runtimes.
use std::{collections::VecDeque, path::PathBuf, process::Command};
use wsr_core::{
    logic::{
        extensions::{high_score::HighScores, law_firm, remarks::Remarks},
        newgame::LocalTime,
    },
    pb::pbstr::PbStr,
    ui::{ReportAction, Ui},
};
fn st(s: &str) -> PbStr {
    PbStr::from(s)
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn oracle(code: &str) -> Option<Vec<String>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !root.join("extracted/app/WSR.EXE").exists() {
        return None;
    }
    let code =
        format!("import sys,ast,struct\nsys.path.insert(0,'tools/emu')\nfrom dll import *\n{code}");
    let out = Command::new(root.join(".venv/bin/python"))
        .args(["-c", &code])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect(),
    )
}
struct Host {
    now: LocalTime,
    choices: VecDeque<usize>,
    messages: Vec<PbStr>,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            now: LocalTime::HARNESS,
            choices: VecDeque::new(),
            messages: vec![],
        }
    }
}
impl Ui for Host {
    fn local_time(&self) -> LocalTime {
        self.now
    }
    fn msgbox(&mut self, t: &PbStr, _: i32, _: &PbStr) -> i32 {
        self.messages.push(t.clone());
        1
    }
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, _: &PbStr) -> PbStr {
        PbStr::new()
    }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _: &PbStr) {}
    fn choose(&mut self, _: &PbStr, _: &[PbStr], _: &[PbStr]) -> Option<usize> {
        self.choices.pop_front()
    }
}
#[test]
fn remarks_pool_matches_dll_through_refill() {
    let __content = wsr_core::content::current();
    let Some(expected) = oracle(
        "e=DllEmu('extracted/app/wsrdll.dll',budget=5000000)\nfor i in range(60):\n r=e.call(0x4015e6,[('v',1)]);assert r[0] is None,r;print(e.ret_str.hex())",
    ) else {
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pack = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("content-{}.pack", std::process::id()));
    std::fs::create_dir_all(pack.parent().unwrap()).unwrap();
    let extracted = Command::new("node").arg(root.join("web/extract-content.mjs"))
        .arg(root.join("extracted/app")).arg(&pack).output().unwrap();
    assert!(extracted.status.success(), "{}", String::from_utf8_lossy(&extracted.stderr));
    let content = wsr_core::content::ContentPack::parse(&std::fs::read(&pack).unwrap()).unwrap();
    std::fs::remove_file(pack).unwrap();
    let mut remarks = Remarks::default();
    for (k, expected) in expected.iter().enumerate() {
        assert_eq!(
            hex(remarks.next(&LocalTime::HARNESS, &content).as_bytes()),
            *expected,
            "{__wsr_text_13841}{k}"
        ,__wsr_text_13841=__content.string(13841));
    }
}
#[test]
fn law_firm_all_wording_and_close_match_dll() {
    let __content = wsr_core::content::current();
    let Some(expected) = oracle(
        r#"e=DllEmu('extracted/app/lawdll.dll',budget=5000000)
base=e.api
second=0
def api(name,a,ret):
 if name=='GetLocalTime':e.uc.mem_write(a[0],struct.pack('<8H',2000,1,1,3,10,0,second,0));return 0
 return base(name,a,ret)
e.api=api
for firm in range(1,4):
 for second in range(60):
  e.answers=['click:1'];e.log=[]
  r=e.call(0x4010ca,[('v',firm),('vs','$'),('vs','M')]);assert r[0] is None,r
  l=next(x for x in e.log if x.startswith('MSGBOX'));t=l.split("'Law Firm Selection' ",1)[1].rsplit(' type=',1)[0]
  print(ast.literal_eval(t).encode('latin-1').hex())
"#,
    ) else {
        return;
    };
    let mut k = 0;
    for firm in 1..=3 {
        for second in 0..60 {
            let mut ui = Host {
                now: LocalTime {
                    second,
                    ..LocalTime::HARNESS
                },
                choices: VecDeque::from([3]),
                ..Host::default()
            };
            assert_eq!(law_firm(&mut ui, firm, &st("$"), &st("M")), firm);
            assert_eq!(
                hex(ui.messages[0].as_bytes()),
                expected[k],
                "{__wsr_text_13842}{firm}{__wsr_text_13843}{second}"
            ,__wsr_text_13842=__content.string(13842), __wsr_text_13843=__content.string(13843));
            k += 1;
        }
    }
    let mut ui = Host::default();
    assert_eq!(law_firm(&mut ui, 0, &st("$"), &st("M")), 2);
    assert!(ui.messages.is_empty());
}
#[test]
fn high_score_exact_record_and_guard_match_dll() {
    let __content = wsr_core::content::current();
    let Some(expected) = oracle(
        r#"e=DllEmu('extracted/app/varichek.dll',files={'SCORES.WSR':open('extracted/app/SCORES.WSR','rb').read()},default_files=False,budget=5000000)
for score,name in [(-1,'2'),(200,'Alice'),(999,'Bobby'),(-1,'2')]:
 r=e.call(0x4010ce,[('v',score&0xffffffff),('v',(score>>32)&0xffffffff),('vs',name),('vs','')]);assert r[0] is None,r
 print(str(r[1]['eax'])+' '+bytes(e.vfs.files['SCORES.WSR']).hex())
"#,
    ) else {
        return;
    };
    let mut bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted/app/SCORES.WSR"),
    )
    .unwrap();
    let mut scores = HighScores::default();
    let mut ui = Host::default();
    for ((score, name), expected) in [(-1, "2"), (200, "Alice"), (999, "Bobby"), (-1, "2")]
        .into_iter()
        .zip(expected)
    {
        let r = scores.call(&mut ui, Some(&bytes), &st(""), &st(name), score);
        if let Some(b) = r.written {
            bytes = b;
        }
        assert_eq!(format!("{} {}", r.value as u32, hex(&bytes)), expected);
    }
    assert_eq!(
        ui.messages.last().unwrap().to_display(),
        __content.string(14426)
    );
}

#[test]
fn high_score_numeric_formats_match_dll() {
    let __content = wsr_core::content::current();
    let Some(expected) = oracle(
        r#"for score in [1,99,100,123456789,999999999999999,1000000000000000,9000000000000000000]:
 e=DllEmu('extracted/app/varichek.dll',files={'SCORES.WSR':b'"Old|0|01-01-1999|x|"\r\n'},default_files=False,budget=5000000)
 r=e.call(0x4010ce,[('v',score&0xffffffff),('v',score>>32),('vs','Alice'),('vs','')]);assert r[0] is None,r
 print(bytes(e.vfs.files['SCORES.WSR']).hex())
"#,
    ) else {
        return;
    };
    for (score, expected) in [
        1,
        99,
        100,
        123456789,
        999999999999999,
        1000000000000000,
        9000000000000000000i64,
    ]
    .into_iter()
    .zip(expected)
    {
        let mut ui = Host::default();
        let mut scores = HighScores::default();
        let r = scores.call(
            &mut ui,
            Some(b"\"Old|0|01-01-1999|x|\"\r\n"),
            &st(""),
            &st("Alice"),
            score,
        );
        assert_eq!(r.value, 2);
        assert_eq!(hex(&r.written.unwrap()), expected, "{__wsr_text_13844}{score}",__wsr_text_13844=__content.string(13844));
    }
}
