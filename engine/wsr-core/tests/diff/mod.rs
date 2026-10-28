//! Differential testing against the ORIGINAL game code: runs a WSR.EXE routine under CPU emulation
//! (`tools/emu/game.py`, Unicorn) on the same [`Globals`] as the Rust port and compares the results.
//!
//! Use from an integration test with `mod diff;`. Tests are skipped (with a message) when
//! `extracted/app/WSR.EXE` or `uv` is not available.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use wsr_core::pb::{ext80::Ext80, pbstr::PbStr, real::Ext};
use wsr_core::state::Globals;

/// Generated state layout; its doc comments carry name, type and DIM bounds of every field.
const STATE_RS: &str = include_str!("../../src/state.rs");

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Python with unicorn/pefile/capstone: `$WSR_EMU_PYTHON`, else the repo's mise-managed `.venv`
/// (`mise install && mise run setup`), else `uv run --with …` (slow: resolves an environment per call).
fn python_cmd() -> Command {
    if let Ok(p) = std::env::var("WSR_EMU_PYTHON") {
        return Command::new(p);
    }
    let venv = repo_root().join(".venv/bin/python");
    if venv.exists() {
        return Command::new(venv);
    }
    let mut c = Command::new("uv");
    c.args([
        "run", "-q", "--with", "unicorn", "--with", "pefile", "--with", "capstone", "python",
    ]);
    c
}

/// A `tools/emu/game.py serve` process: boots the original once and forks per request.
struct Server {
    _child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

static POOL: std::sync::Mutex<Vec<Server>> = std::sync::Mutex::new(Vec::new());

fn spawn_server() -> Option<Server> {
    use std::process::Stdio;
    let mut c = python_cmd();
    c.current_dir(repo_root())
        .arg("-I")
        .arg(repo_root().join("tools/emu/game.py"))
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = c.spawn().ok()?;
    let stdin = child.stdin.take()?;
    let stdout = std::io::BufReader::new(child.stdout.take()?);
    Some(Server {
        _child: child,
        stdin,
        stdout,
    })
}

fn json_list(argv: &[String]) -> String {
    let items: Vec<String> = argv
        .iter()
        .map(|a| {
            let mut o = String::from("\"");
            for c in a.chars() {
                match c {
                    '"' => o.push_str("\\\""),
                    '\\' => o.push_str("\\\\"),
                    c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                    c => o.push(c),
                }
            }
            o.push('"');
            o
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// Run `game.py <argv…>`: through a pooled server (fast), or a standalone process when
/// `WSR_EMU_NO_SERVER` is set. A failure is re-run standalone to capture its output for the message.
fn exec_game(argv: &[String]) -> Result<(), String> {
    use std::io::{BufRead, Write};
    let standalone = || -> Result<(), String> {
        let o = python_cmd()
            .current_dir(repo_root())
            .arg("-I")
            .arg(repo_root().join("tools/emu/game.py"))
            .args(argv)
            .output()
            .map_err(|e| format!("running tools/emu/game.py: {e}"))?;
        if o.status.success() {
            Ok(())
        } else {
            Err(format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ))
        }
    };
    if std::env::var_os("WSR_EMU_NO_SERVER").is_some() {
        return standalone();
    }
    let srv = POOL.lock().unwrap().pop();
    let Some(mut srv) = srv.or_else(spawn_server) else {
        return standalone();
    };
    let mut line = String::new();
    let ok = writeln!(srv.stdin, "{}", json_list(argv)).is_ok()
        && srv.stdin.flush().is_ok()
        && srv.stdout.read_line(&mut line).is_ok_and(|n| n > 0);
    if !ok {
        return standalone(); // server died: drop it
    }
    POOL.lock().unwrap().push(srv);
    if line.trim() == "0" {
        Ok(())
    } else {
        standalone()
    }
}

/// Why the harness can't run here, or `None` if it can.
pub fn unavailable() -> Option<String> {
    let exe = repo_root().join("extracted/app/WSR.EXE");
    let reason = if !exe.exists() {
        Some(format!("{} not found", exe.display()))
    } else {
        match Command::new("uv").arg("--version").output() {
            Ok(o) if o.status.success() => None,
            _ => Some("`uv` not found on PATH".into()),
        }
    };
    assert!(
        reason.is_none() || std::env::var_os("WSR_REQUIRE_ORIGINAL").is_none(),
        "Original-executable comparisons required: {}",
        reason.as_deref().unwrap_or_default()
    );
    reason
}

/// Result of an emulated run.
pub struct Original {
    pub globals: Globals,
    /// UI/API log: `MSGBOX 'caption' 'text' type=.. -> answer`, `API Name(args) -> rv`, `PBERR n in 0x..`.
    pub log: Vec<String>,
    pub return_value: u32,
    /// Elapsed emulation time of the routine itself (seconds), as measured by the harness.
    pub elapsed: f64,
    /// RND state dword after the run.
    pub rng: u32,
    /// Files the run wrote or created (contents) or deleted (`None`), keyed by upper-case base name.
    pub files: HashMap<String, Option<Vec<u8>>>,
    /// ST0 at return (FUNCTION AS EXT/SINGLE/DOUBLE …), if the FPU stack was not empty.
    pub return_float: Option<Ext>,
    /// The same as 80-bit value, 20 hex digits (sign/exponent first).
    pub return_float_raw: Option<String>,
    /// EAX as a string, if it points to a live BSTR (FUNCTION AS STRING).
    pub return_str: Option<PbStr>,
    /// Every dialog the run created (DIALOG NEW), in creation order.
    pub dialogs: Vec<Dialog>,
}

/// A PB DDT dialog as the harness's window manager saw it at the end of the run.
#[derive(Debug, Clone)]
pub struct Dialog {
    pub title: PbStr,
    /// How it was closed: "default click:1", "click:2", "close", "DestroyWindow", "FORCED END", "" (open).
    pub closed_by: String,
    pub controls: Vec<Control>,
}

#[derive(Debug, Clone)]
pub struct Control {
    pub id: i32,
    pub class: String,
    pub text: PbStr,
    /// Listbox / combobox items.
    pub items: Vec<PbStr>,
    pub cursel: i32,
}

impl Dialog {
    pub fn control(&self, id: i32) -> Option<&Control> {
        self.controls.iter().find(|c| c.id == id)
    }
}

/// Run the original routine at `addr` with BYREF LONG `args` on state `g`; `answers` are popped by
/// successive MessageBoxA calls (`IDYES`, `IDNO`, `IDOK`, ... or a number). Returns `None` (after
/// printing why) if the harness is unavailable; panics if the emulation faults.
pub fn run_original(
    addr: u32,
    args: &[i32],
    g: &Globals,
    answers: &[&str],
) -> Option<(Globals, Vec<String>)> {
    let specs: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    run_original_with(
        addr,
        &Opts {
            args: specs,
            answers,
            ..Opts::default()
        },
        g,
    )
    .map(|o| (o.globals, o.log))
}

/// Exact BYREF EXT argument, including low bits not representable by a DOUBLE.
pub fn ext_arg(value: impl Into<Ext>) -> String {
    let raw = value.into().raw();
    format!("e:x87:{:04x}{:016x}", raw.se, raw.mant)
}

/// Options for [`run_original_with`].
#[derive(Default)]
pub struct Opts<'a> {
    /// Harness arg specs: `5` BYREF LONG, `v:5` BYVAL, `i:5` BYREF INTEGER, `q:`/`e:`/`d:`/`f:` BYREF
    /// QUAD/EXT/DOUBLE/SINGLE, `s:text` BYREF STRING, `vs:text` BYVAL STRING (see tools/emu/game.py).
    pub args: Vec<String>,
    /// MessageBoxA answers, popped in order (`IDYES`, `IDNO`, `IDOK`, ... or a number).
    pub answers: &'a [&'a str],
    /// Game routines whose calls are logged as `CALL Name(arg->value, ...) [from 0x..]`.
    pub hooks: Vec<u32>,
    /// RND state (dword 0x6c3280) to start with.
    pub rng: Option<u32>,
    /// Optional instruction budget for diagnosing callbacks that can loop indefinitely.
    pub budget: Option<u64>,
    /// Virtual files (name -> contents). Names are matched case-insensitively by base name; files not
    /// given are taken from extracted/app/ unless `no_default_files`.
    pub files: Vec<(String, Vec<u8>)>,
    pub no_default_files: bool,
    /// Routines made to return 0 immediately: (address, number of stdcall dword args).
    pub skips: Vec<(u32, u32)>,
}

/// On-disk cache of original runs (`engine/target/emu-cache/<key>/`). The emulated original is
/// deterministic, so its result depends only on the inputs: the command line (minus temp paths), the
/// state snapshot, the virtual input files, `WSR.EXE` and the harness sources (`tools/emu/*.py`).
/// Re-running a test whose world did not change skips the emulator. `WSR_EMU_NO_CACHE=1` disables it;
/// the size is capped (`WSR_EMU_CACHE_MB`, default 1000; delete the directory to reset).
fn emu_cache_dir() -> PathBuf {
    std::env::var_os("WSR_EMU_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/emu-cache"))
}

/// Digest of `WSR.EXE` and the harness sources, computed once per test process.
fn harness_digest() -> u128 {
    static D: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    *D.get_or_init(|| {
        let mut h = Digest::default();
        let root = repo_root();
        h.feed(&std::fs::read(root.join("extracted/app/WSR.EXE")).unwrap_or_default());
        let mut py: Vec<PathBuf> = std::fs::read_dir(root.join("tools/emu"))
            .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        py.retain(|p| p.extension().is_some_and(|x| x == "py"));
        py.sort();
        for p in py {
            h.feed(p.file_name().unwrap().as_encoded_bytes());
            h.feed(&std::fs::read(&p).unwrap_or_default());
        }
        h.finish()
    })
}

/// Two independent 64-bit FNV-1a lanes (not cryptographic; collisions are irrelevant here).
struct Digest(u64, u64);

impl Default for Digest {
    fn default() -> Self {
        Digest(0xcbf2_9ce4_8422_2325, 0x6c62_272e_07bb_0142)
    }
}

impl Digest {
    fn feed(&mut self, b: &[u8]) {
        for &x in (b.len() as u64).to_le_bytes().iter().chain(b) {
            self.0 = (self.0 ^ x as u64).wrapping_mul(0x0100_0000_01b3);
            self.1 = (self.1 ^ x as u64)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                .rotate_left(29);
        }
    }
    fn finish(&self) -> u128 {
        ((self.0 as u128) << 64) | self.1 as u128
    }
}

fn cache_key(argv: &[String], temp_paths: &[&PathBuf], opts: &Opts) -> Option<String> {
    if std::env::var_os("WSR_EMU_NO_CACHE").is_some() {
        return None;
    }
    let mut h = Digest::default();
    h.feed(&harness_digest().to_le_bytes());
    for a in argv {
        let mut a = a.clone();
        for p in temp_paths {
            a = a.replace(&p.display().to_string(), "<tmp>");
        }
        h.feed(a.as_bytes());
    }
    // the state snapshot (written to the --state path just before)
    h.feed(&std::fs::read(temp_paths[0]).ok()?);
    for (name, data) in &opts.files {
        h.feed(name.as_bytes());
        h.feed(data);
    }
    Some(format!("{:032x}", h.finish()))
}

/// Size budget of the cache in bytes (`WSR_EMU_CACHE_MB`, default 1000). The current size is measured
/// once per test process and then tracked; entries beyond the budget are not stored.
fn cache_budget_left(add: u64) -> bool {
    static USED: std::sync::OnceLock<std::sync::atomic::AtomicU64> = std::sync::OnceLock::new();
    fn dir_size(p: &Path) -> u64 {
        std::fs::read_dir(p)
            .map(|rd| {
                rd.flatten()
                    .map(|e| match e.metadata() {
                        Ok(m) if m.is_dir() => dir_size(&e.path()),
                        Ok(m) => m.len(),
                        Err(_) => 0,
                    })
                    .sum()
            })
            .unwrap_or(0)
    }
    let used = USED.get_or_init(|| std::sync::atomic::AtomicU64::new(dir_size(&emu_cache_dir())));
    let max = std::env::var("WSR_EMU_CACHE_MB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1000)
        << 20;
    used.fetch_add(add, Ordering::Relaxed) + add <= max
}

fn cache_store(key: &str, p_out: &Path, p_log: &Path, p_res: &Path, d_out: &Path) {
    let len = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let files: u64 = std::fs::read_dir(d_out)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
                .sum()
        })
        .unwrap_or(0);
    if !cache_budget_left(len(p_out) + len(p_log) + len(p_res) + files) {
        return;
    }
    let dir = emu_cache_dir();
    let tmp = dir.join(format!(".tmp-{key}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let ok = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(tmp.join("files"))?;
        std::fs::copy(p_out, tmp.join("out.json"))?;
        std::fs::copy(p_log, tmp.join("log.txt"))?;
        if let Ok(rd) = std::fs::read_dir(d_out) {
            for e in rd.flatten() {
                std::fs::copy(e.path(), tmp.join("files").join(e.file_name()))?;
            }
        }
        // res.json last: its presence marks a complete entry
        std::fs::copy(p_res, tmp.join("res.json"))?;
        Ok(())
    })();
    let destination = dir.join(key);
    // Earlier harness versions removed cached output after a hit, leaving incomplete directories.
    if destination.exists() && !destination.join("res.json").exists() {
        let _ = std::fs::remove_dir_all(&destination);
    }
    if ok.is_ok() && std::fs::rename(&tmp, destination).is_ok() {
        return;
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Like [`run_original`] with all options; also returns the return value (EAX) and final RND state.
pub fn run_original_with(addr: u32, opts: &Opts, g: &Globals) -> Option<Original> {
    if let Some(why) = unavailable() {
        assert!(
            std::env::var_os("WSR_REQUIRE_ORIGINAL").is_none(),
            "Original-executable comparison required for 0x{addr:x}: {why}"
        );
        eprintln!("SKIPPED differential run of 0x{addr:x}: {why}");
        return None;
    }
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let tag = format!(
        "wsr-diff-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let dir = std::env::temp_dir();
    let p_in = dir.join(format!("{tag}-in.json"));
    let p_out = dir.join(format!("{tag}-out.json"));
    let p_log = dir.join(format!("{tag}-log.txt"));
    let p_res = dir.join(format!("{tag}-res.json"));
    std::fs::write(&p_in, g.to_json()).unwrap();
    let root = repo_root();
    let _ = &root;
    let mut argv: Vec<String> = vec!["run".into(), format!("{addr:x}")];
    for (k, p) in [
        ("--state", &p_in),
        ("--out", &p_out),
        ("--log", &p_log),
        ("--result", &p_res),
    ] {
        argv.push(k.into());
        argv.push(p.display().to_string());
    }
    for a in &opts.args {
        argv.push(format!("--arg={a}"));
    }
    for a in opts.answers {
        argv.push(format!("--answer={a}"));
    }
    for h in &opts.hooks {
        argv.push(format!("--hook={h:x}"));
    }
    if let Some(r) = opts.rng {
        argv.push(format!("--rng={r}"));
    }
    if let Some(b) = opts
        .budget
        .or_else(|| std::env::var("WSR_EMU_BUDGET").ok()?.parse().ok())
    {
        argv.push(format!("--budget={b}"));
    }
    let d_in = dir.join(format!("{tag}-files-in"));
    let d_out = dir.join(format!("{tag}-files-out"));
    if !opts.files.is_empty() {
        std::fs::create_dir_all(&d_in).unwrap();
    }
    for (k, (name, data)) in opts.files.iter().enumerate() {
        let p = d_in.join(format!("f{k}"));
        std::fs::write(&p, data).unwrap();
        argv.push(format!("--file={name}={}", p.display()));
    }
    if opts.no_default_files {
        argv.push("--no-default-files".into());
    }
    for (a, n) in &opts.skips {
        argv.push(format!("--skip={a:x}:{n}"));
    }
    // emulator timeout in seconds: generous by default (long original routines under parallel test load)
    argv.push("--timeout".into());
    argv.push(std::env::var("WSR_EMU_TIMEOUT").unwrap_or_else(|_| "600".into()));
    argv.push("--files-out".into());
    argv.push(d_out.display().to_string());
    // WSR_EMU_KEEP=1: keep the input snapshot and print the game.py command line (for profiling/debugging)
    let keep = std::env::var_os("WSR_EMU_KEEP").is_some();
    if keep {
        eprintln!("game.py {}", argv.join(" "));
    }
    let key = cache_key(&argv, &[&p_in, &p_out, &p_log, &p_res, &d_in, &d_out], opts);
    let cached = key
        .as_ref()
        .map(|k| emu_cache_dir().join(k))
        .filter(|d| d.join("res.json").exists())
        .or_else(|| {
            let root = std::env::var_os("WSR_EMU_CACHE_READ_DIR")?;
            let path = PathBuf::from(root).join(key.as_ref()?);
            path.join("res.json").exists().then_some(path)
        });
    let from_cache = cached.is_some();
    let (p_out, p_log, p_res, d_out) = if let Some(c) = cached {
        (
            c.join("out.json"),
            c.join("log.txt"),
            c.join("res.json"),
            c.join("files"),
        )
    } else {
        let res = exec_game(&argv);
        if let Err(msg) = res {
            panic!("emulation of 0x{addr:x} failed:\n{msg}");
        }
        if let Some(k) = &key {
            cache_store(k, &p_out, &p_log, &p_res, &d_out);
        }
        (p_out, p_log, p_res, d_out)
    };
    if !keep {
        let _ = std::fs::remove_file(&p_in);
    }
    let mut out = g.clone();
    out.from_json(&std::fs::read_to_string(&p_out).unwrap())
        .unwrap();
    let log = std::fs::read_to_string(&p_log)
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    let res = std::fs::read_to_string(&p_res).unwrap();
    let m = parse_flat(&res).unwrap_or_default();
    let num = |k: &str| m.get(k).and_then(|v| v.num()).unwrap_or(0.0);
    let (return_value, elapsed, rng) = (
        num("return_value") as u32,
        num("elapsed"),
        num("rng") as u32,
    );
    let mut files = HashMap::new();
    if let Some(J::List(l)) = m.get("files_written") {
        for n in l {
            if let J::Str(n) = n {
                let n = String::from_utf8_lossy(n).into_owned();
                files.insert(n.clone(), Some(std::fs::read(d_out.join(&n)).unwrap()));
            }
        }
    }
    if let Some(J::List(l)) = m.get("files_deleted") {
        for n in l {
            if let J::Str(n) = n {
                files.insert(String::from_utf8_lossy(n).into_owned(), None);
            }
        }
    }
    let jstr = |k: &str| match m.get(k) {
        Some(J::Str(b)) => Some(PbStr(b.clone())),
        _ => None,
    };
    let return_float_raw = jstr("return_float_raw").map(|s| s.to_display());
    let return_float = return_float_raw
        .as_deref()
        .and_then(parse_raw_ext)
        .or_else(|| m.get("return_float").and_then(|v| v.float(false)));
    let return_str = jstr("return_str");
    let mut dialogs = Vec::new();
    for i in 0..num("dialogs") as usize {
        let mut controls = Vec::new();
        if let Some(J::List(ids)) = m.get(&format!("dlg{i}.ids")) {
            for id in ids.iter().filter_map(|x| x.num()) {
                let pre = format!("dlg{i}.c{}.", id as i32);
                let items = match m.get(&(pre.clone() + "items")) {
                    Some(J::List(l)) => l
                        .iter()
                        .filter_map(|x| {
                            if let J::Str(b) = x {
                                Some(PbStr(b.clone()))
                            } else {
                                None
                            }
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                controls.push(Control {
                    id: id as i32,
                    class: jstr(&(pre.clone() + "class"))
                        .map(|s| s.to_display())
                        .unwrap_or_default(),
                    text: jstr(&(pre.clone() + "text")).unwrap_or_default(),
                    items,
                    cursel: num(&(pre + "cursel")) as i32,
                });
            }
        }
        dialogs.push(Dialog {
            title: jstr(&format!("dlg{i}.title")).unwrap_or_default(),
            closed_by: jstr(&format!("dlg{i}.closed_by"))
                .map(|s| s.to_display())
                .unwrap_or_default(),
            controls,
        });
    }
    if !from_cache {
        for p in [&p_out, &p_log, &p_res] {
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::remove_dir_all(&d_out);
    }
    let _ = std::fs::remove_dir_all(&d_in);
    Some(Original {
        globals: out,
        log,
        return_value,
        elapsed,
        rng,
        files,
        return_float,
        return_float_raw,
        return_str,
        dialogs,
    })
}

// ---------------------------------------------------------------- comparison

#[derive(Clone, Debug, PartialEq)]
enum J {
    Null,
    Num(String),
    Str(Vec<u8>),
    List(Vec<J>),
    Other,
}

impl J {
    fn num(&self) -> Option<f64> {
        match self {
            J::Num(s) => s.parse().ok(),
            _ => None,
        }
    }
    fn float(&self, extended: bool) -> Option<Ext> {
        match self {
            J::Num(s) => s.parse::<f64>().ok().map(Ext::from_f64),
            J::Str(s) if extended => std::str::from_utf8(s)
                .ok()
                .and_then(|s| s.strip_prefix("x87:"))
                .and_then(parse_raw_ext),
            _ => None,
        }
    }
    fn nonfinite(&self) -> bool {
        *self == J::Null || self.float(true).is_some_and(|x| !x.is_finite())
    }
    fn show(&self) -> String {
        match self {
            J::Null => "NaN/inf".into(),
            J::Num(s) => s.clone(),
            J::Str(b) => format!("{:?}", String::from_utf8_lossy(b)),
            J::List(_) => "[..]".into(),
            J::Other => "?".into(),
        }
    }
}

fn parse_raw_ext(s: &str) -> Option<Ext> {
    if s.len() != 20 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(Ext(Ext80 {
        se: u16::from_str_radix(&s[..4], 16).ok()?,
        mant: u64::from_str_radix(&s[4..], 16).ok()?,
    }))
}

/// Minimal parser for the flat snapshot objects (nested objects are skipped as `Other`).
fn parse_flat(s: &str) -> Result<HashMap<String, J>, String> {
    let b = s.as_bytes();
    let mut i = 0;
    fn ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
    }
    fn val(b: &[u8], i: &mut usize) -> Result<J, String> {
        ws(b, i);
        match b.get(*i) {
            Some(b'"') => {
                *i += 1;
                let mut v = Vec::new();
                loop {
                    let c = *b.get(*i).ok_or("eof")?;
                    *i += 1;
                    match c {
                        b'"' => return Ok(J::Str(v)),
                        b'\\' => {
                            let e = *b.get(*i).ok_or("eof")?;
                            *i += 1;
                            match e {
                                b'u' => {
                                    let h = std::str::from_utf8(&b[*i..*i + 4])
                                        .map_err(|e| e.to_string())?;
                                    v.push(u32::from_str_radix(h, 16).map_err(|e| e.to_string())?
                                        as u8);
                                    *i += 4;
                                }
                                b'b' => v.push(8),
                                b'f' => v.push(12),
                                b'n' => v.push(b'\n'),
                                b'r' => v.push(b'\r'),
                                b't' => v.push(b'\t'),
                                c => v.push(c),
                            }
                        }
                        c => v.push(c),
                    }
                }
            }
            Some(b'[') => {
                *i += 1;
                let mut l = Vec::new();
                loop {
                    ws(b, i);
                    if b.get(*i) == Some(&b']') {
                        *i += 1;
                        return Ok(J::List(l));
                    }
                    l.push(val(b, i)?);
                    ws(b, i);
                    if b.get(*i) == Some(&b',') {
                        *i += 1;
                    }
                }
            }
            Some(b'{') => {
                let mut depth = 0;
                let mut in_str = false;
                while *i < b.len() {
                    let c = b[*i];
                    *i += 1;
                    if in_str {
                        if c == b'\\' {
                            *i += 1;
                        } else if c == b'"' {
                            in_str = false;
                        }
                    } else if c == b'"' {
                        in_str = true;
                    } else if c == b'{' || c == b'[' {
                        depth += 1;
                    } else if c == b'}' || c == b']' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
                Ok(J::Other)
            }
            Some(b'n') => {
                *i += 4;
                Ok(J::Null)
            }
            Some(b't') => {
                *i += 4;
                Ok(J::Other)
            }
            Some(b'f') => {
                *i += 5;
                Ok(J::Other)
            }
            _ => {
                let st = *i;
                while *i < b.len()
                    && matches!(b[*i], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
                {
                    *i += 1;
                }
                if st == *i {
                    return Err(format!("unexpected byte at {st}"));
                }
                Ok(J::Num(String::from_utf8_lossy(&b[st..*i]).into_owned()))
            }
        }
    }
    let mut m = HashMap::new();
    ws(b, &mut i);
    if b.get(i) != Some(&b'{') {
        return Err("expected object".into());
    }
    i += 1;
    loop {
        ws(b, &mut i);
        match b.get(i) {
            Some(b'}') | None => break,
            Some(b',') => {
                i += 1;
                continue;
            }
            _ => {}
        }
        let k = match val(b, &mut i)? {
            J::Str(k) => String::from_utf8_lossy(&k).into_owned(),
            _ => return Err("expected key".into()),
        };
        ws(b, &mut i);
        i += 1; // ':'
        let v = val(b, &mut i)?;
        m.insert(k, v);
    }
    Ok(m)
}

/// One field of the state layout (parsed from the doc comments of state.rs).
pub struct Field {
    pub key: String,
    pub name: String,
    pub typ: String,
    pub bounds: Vec<(i32, i32)>,
}

pub fn layout() -> Vec<Field> {
    let mut out = Vec::new();
    for ln in STATE_RS.lines() {
        let Some(rest) = ln.trim_start().strip_prefix("/// `") else {
            continue;
        };
        let Some((decl, tail)) = rest.split_once("` at 0x") else {
            continue;
        };
        let Some(addr) = tail.strip_suffix('.') else {
            continue;
        };
        let Some((lhs, typ)) = decl.rsplit_once(" AS ") else {
            continue;
        };
        let (name, bounds) = if let Some(d) = lhs.strip_prefix("DIM ") {
            let (n, b) = d.split_once('(').unwrap();
            let b = b.trim_end_matches(')');
            let bounds = b
                .split(", ")
                .map(|r| {
                    let (lo, hi) = r.split_once(" TO ").unwrap();
                    (lo.parse().unwrap(), hi.parse().unwrap())
                })
                .collect();
            (n.to_string(), bounds)
        } else {
            (lhs.to_string(), Vec::new())
        };
        if name.contains(' ') || !addr.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        out.push(Field {
            key: addr.to_string(),
            name,
            typ: typ.to_string(),
            bounds,
        });
    }
    out
}

/// Field-by-field comparison of two states. Floats (EXT, DOUBLE, SINGLE) are equal within a relative
/// tolerance in extended precision; integer and CURRENCY fields compare exactly.
pub struct Compare {
    pub rel_tol: f64,
    pub abs_tol: f64,
    pub ignore: Vec<String>,
    pub max_report: usize,
    pub exact_ext: bool,
    allowed_nonfinite: Vec<(String, usize)>,
}

impl Default for Compare {
    fn default() -> Self {
        Compare {
            rel_tol: 1e-12,
            abs_tol: 0.0,
            ignore: Vec::new(),
            max_report: 40,
            exact_ext: false,
            allowed_nonfinite: Vec::new(),
        }
    }
}

impl Compare {
    pub fn rel_tol(mut self, t: impl Into<Ext>) -> Self {
        self.rel_tol = t.into().to_f64();
        self
    }
    pub fn abs_tol(mut self, t: impl Into<Ext>) -> Self {
        self.abs_tol = t.into().to_f64();
        self
    }
    /// Skip a field by original name (e.g. `NewsMsg`).
    pub fn ignore(mut self, name: &str) -> Self {
        self.ignore.push(name.to_string());
        self
    }

    /// Explicit diagnostic exemption for cells already non-finite in a damaged input fixture.
    /// New invalid cells elsewhere still fail; normal comparisons have no exemptions.
    pub fn allow_nonfinite_from(mut self, input: &Globals) -> Self {
        let snapshot = parse_flat(&input.to_json()).unwrap();
        for field in layout() {
            if !matches!(field.typ.as_str(), "EXT" | "DOUBLE" | "SINGLE") {
                continue;
            }
            match snapshot.get(&field.key) {
                Some(v) if v.nonfinite() => self.allowed_nonfinite.push((field.key, 0)),
                Some(J::List(values)) => {
                    for (index, value) in values.iter().enumerate() {
                        if value.nonfinite() {
                            self.allowed_nonfinite.push((field.key.clone(), index));
                        }
                    }
                }
                _ => {}
            }
        }
        self
    }

    /// Differences as `Name(i, j): left vs right` lines.
    pub fn diffs(&self, a: &Globals, b: &Globals) -> Vec<String> {
        self.diffs_json(&a.to_json(), &b.to_json())
    }

    /// Compare complete snapshots; exposed so malformed snapshots can exercise the comparator.
    pub fn diffs_json(&self, a: &str, b: &str) -> Vec<String> {
        let ja = parse_flat(a).expect("invalid left snapshot");
        let jb = parse_flat(b).expect("invalid right snapshot");
        let mut out = Vec::new();
        for f in layout() {
            if self.ignore.contains(&f.name) {
                continue;
            }
            let float = matches!(f.typ.as_str(), "EXT" | "DOUBLE" | "SINGLE");
            let (Some(x), Some(y)) = (ja.get(&f.key), jb.get(&f.key)) else {
                out.push(format!("{} ({}): missing in a snapshot", f.name, f.key));
                continue;
            };
            match (x, y) {
                (J::List(xs), J::List(ys)) => {
                    let expected: usize = f
                        .bounds
                        .iter()
                        .map(|(lo, hi)| (hi - lo + 1) as usize)
                        .product();
                    if f.bounds.is_empty() || xs.len() != expected || ys.len() != expected {
                        out.push(format!(
                            "{}: array shape differs ({} / {}, expected {expected})",
                            f.name,
                            xs.len(),
                            ys.len()
                        ));
                        continue;
                    }
                    for (k, (p, q)) in xs.iter().zip(ys).enumerate() {
                        if float
                            && p.nonfinite()
                            && q.nonfinite()
                            && self.allowed_nonfinite.contains(&(f.key.clone(), k))
                        {
                            continue;
                        }
                        if !self.same(&f.typ, p, q) {
                            out.push(format!(
                                "{}{}: {} vs {}",
                                f.name,
                                subscript(&f.bounds, k),
                                p.show(),
                                q.show()
                            ));
                        }
                    }
                }
                (p, q) => {
                    if f.bounds.is_empty()
                        && float
                        && p.nonfinite()
                        && q.nonfinite()
                        && self.allowed_nonfinite.contains(&(f.key.clone(), 0))
                    {
                        continue;
                    }
                    if !f.bounds.is_empty() || !self.same(&f.typ, p, q) {
                        out.push(format!("{}: {} vs {}", f.name, p.show(), q.show()));
                    }
                }
            }
        }
        out
    }

    fn same(&self, typ: &str, p: &J, q: &J) -> bool {
        if matches!(typ, "EXT" | "DOUBLE" | "SINGLE") {
            if let (Some(x), Some(y)) = (p.float(typ == "EXT"), q.float(typ == "EXT")) {
                if !x.is_finite() || !y.is_finite() {
                    return false;
                }
                if typ == "EXT" && self.exact_ext {
                    return x.raw() == y.raw();
                }
                let d = (x - y).abs();
                return x.is_finite()
                    && y.is_finite()
                    && (d <= self.abs_tol || d <= self.rel_tol * x.abs().max(y.abs()));
            }
            return false;
        } else if typ.starts_with("STRING") {
            return matches!((p, q), (J::Str(x), J::Str(y)) if x == y);
        } else if let (J::Num(x), J::Num(y)) = (p, q) {
            return matches!((exact_integer(x), exact_integer(y)), (Some(a), Some(b)) if a == b);
        }
        false
    }

    /// Panics with a list of differences (by field name and subscripts) if `a` and `b` differ.
    /// `a` is conventionally the Rust port's result, `b` the original's.
    #[track_caller]
    pub fn assert(&self, a: &Globals, b: &Globals) {
        let d = self.diffs(a, b);
        if !d.is_empty() {
            let n = d.len();
            let mut msg = format!("{n} differences (left = Rust port, right = original):\n");
            for l in d.iter().take(self.max_report) {
                msg.push_str("  ");
                msg.push_str(l);
                msg.push('\n');
            }
            if n > self.max_report {
                msg.push_str(&format!("  ... and {} more\n", n - self.max_report));
            }
            panic!("{msg}");
        }
    }
}

/// Numeric equivalence of integer tokens, including 5.0/5e0, without rounding through f64.
fn exact_integer(text: &str) -> Option<i128> {
    let mut parts = text.split(['e', 'E']);
    let mantissa = parts.next()?;
    let exponent = parts
        .next()
        .map(str::parse::<i32>)
        .transpose()
        .ok()?
        .unwrap_or(0);
    if parts.next().is_some() {
        return None;
    }
    let mut parts = mantissa.split('.');
    let whole = parts.next()?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return None;
    }
    let digits = format!("{whole}{fraction}");
    let value = digits.parse::<i128>().ok()?;
    if value == 0 {
        return Some(0);
    }
    let scale = exponent.checked_sub(i32::try_from(fraction.len()).ok()?)?;
    let factor = 10i128.checked_pow(scale.unsigned_abs())?;
    if scale >= 0 {
        value.checked_mul(factor)
    } else if value % factor == 0 {
        Some(value / factor)
    } else {
        None
    }
}

fn subscript(bounds: &[(i32, i32)], flat: usize) -> String {
    let mut r = flat as i64;
    let mut idx = Vec::new();
    for &(lo, hi) in bounds {
        let c = (hi - lo + 1) as i64;
        idx.push((lo as i64 + r % c).to_string());
        r /= c;
    }
    format!("({})", idx.join(", "))
}

/// Field-by-field comparison with the default tolerance (relative 1e-12 for floats).
#[track_caller]
pub fn assert_same(a: &Globals, b: &Globals) {
    Compare::default().assert(a, b)
}

/// Small deterministic PRNG for building random test states (xorshift64*).
pub struct Rand(pub u64);

impl Rand {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    /// Uniform in lo..=hi.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % ((hi - lo + 1) as u64)) as i64
    }
    /// True with probability p.
    pub fn chance(&mut self, p: impl Into<Ext>) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p.into().to_f64()
    }
}

/// Run `f(seed)` for every seed on up to `available_parallelism` threads (each thread uses its own pooled
/// emulator server). Use for differential tests that loop over many independent seeds; a panic in any
/// seed fails the test (the failing seed is printed). `WSR_SEED_THREADS=n` limits concurrent emulators;
/// `WSR_ONLY_SEED=n` runs only seed `n` (debugging).
pub fn par_seeds(seeds: impl IntoIterator<Item = u64>, f: impl Fn(u64) + Sync) {
    let mut seeds: Vec<u64> = seeds.into_iter().collect();
    if let Some(only) = std::env::var("WSR_ONLY_SEED")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        seeds.retain(|&s| s == only);
    }
    // Optional bounded development sweep; the default still runs every declared seed.
    if let Ok(limit) = std::env::var("WSR_SEED_LIMIT") {
        let limit = limit
            .parse::<usize>()
            .expect("WSR_SEED_LIMIT must be a positive integer");
        assert!(limit > 0, "WSR_SEED_LIMIT must be positive");
        seeds.truncate(limit);
    }
    assert!(
        !seeds.is_empty() || std::env::var_os("WSR_REQUIRE_ORIGINAL").is_none(),
        "Required differential comparison selected no seeds"
    );
    struct SeedGuard(u64);
    impl Drop for SeedGuard {
        fn drop(&mut self) {
            if std::thread::panicking() {
                eprintln!("(par_seeds: failed in seed {})", self.0);
            }
        }
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let n = std::env::var("WSR_SEED_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()))
        .max(1)
        .min(seeds.len().max(1));
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| {
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&seed) = seeds.get(k) else { break };
                    let guard = SeedGuard(seed);
                    f(seed);
                    std::mem::forget(guard);
                }
            });
        }
    });
}
