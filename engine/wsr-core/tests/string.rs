//! Checks `wsr_core::pb::string` against vectors produced by running WSR.EXE's runtime under emulation
//! (tools/emu/pbstring.py -> tests/fixtures/string.txt).

use std::cmp::Ordering;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::string::*;

const FIXTURE: &str = include_str!("fixtures/string.txt");

enum Tok {
    Int(i32),
    Str(Vec<u8>),
    Err,
}

fn tok(t: &str) -> Tok {
    if let Some(h) = t.strip_prefix('x') {
        Tok::Str((0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect())
    } else if let Some(e) = t.strip_prefix('!') {
        { let _: i32 = e.parse().unwrap(); Tok::Err }
    } else {
        Tok::Int(t.parse().unwrap())
    }
}

fn s(t: &Tok) -> &[u8] {
    match t {
        Tok::Str(v) => v,
        _ => panic!("expected string"),
    }
}

fn i(t: &Tok) -> i32 {
    match t {
        Tok::Int(v) => *v,
        _ => panic!("expected int"),
    }
}

fn mode(m: i32) -> CaseMode {
    if m == 0 {
        CaseMode::Ansi
    } else {
        CaseMode::Oem
    }
}

#[test]
fn string_vectors() {
    let mut n = 0;
    let mut fails = Vec::new();
    for line in FIXTURE.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (lhs, rhs) = line.split_once(" = ").unwrap();
        let mut it = lhs.split(' ');
        let name = it.next().unwrap();
        let a: Vec<Tok> = it.map(tok).collect();
        let mut r: Vec<Tok> = rhs.split(' ').map(tok).collect();
        // A raised runtime error is recorded as `!n` before the result; the functions return "" then.
        if let Some(Tok::Err) = r.first() {
            r.remove(0);
        }
        let got: Vec<Vec<u8>> = match name {
            "MID" => vec![mid(s(&a[0]), i(&a[1]), i(&a[2])).0],
            "RIGHT" => vec![right(s(&a[0]), i(&a[1])).0],
            "MIDSET" => {
                let mut t = PbStr(s(&a[0]).to_vec());
                mid_assign(&mut t, i(&a[1]), i(&a[2]), s(&a[3]));
                vec![t.0]
            }
            "LTRIM" => vec![ltrim(s(&a[0])).0],
            "RTRIM" => vec![rtrim(s(&a[0])).0],
            "UCASE" => vec![ucase(s(&a[0]), mode(i(&a[1]))).0],
            "LCASE" => vec![lcase(s(&a[0]), mode(i(&a[1]))).0],
            "MCASE" => vec![mcase(s(&a[0]), mode(i(&a[1]))).0],
            "INSTR" => vec![instr(i(&a[0]), s(&a[1]), s(&a[2])).to_string().into_bytes()],
            "INSTRANY" => vec![instr_any(i(&a[0]), s(&a[1]), s(&a[2])).to_string().into_bytes()],
            "PARSE" => vec![parse(s(&a[0]), s(&a[1]), i(&a[2])).0],
            "PARSEANY" => vec![parse_any(s(&a[0]), s(&a[1]), i(&a[2])).0],
            "PARSECOUNT" => vec![parse_count(s(&a[0]), s(&a[1])).to_string().into_bytes()],
            "PARSECOUNTANY" => vec![parse_count_any(s(&a[0]), s(&a[1])).to_string().into_bytes()],
            "PARSEARR" => parse_array(s(&a[0]), s(&a[1]), i(&a[2]) as usize).into_iter().map(|x| x.0).collect(),
            "REPLACE" => vec![replace(s(&a[2]), s(&a[0]), s(&a[1])).0],
            "REPLACEANY" => vec![replace_any(s(&a[2]), s(&a[0]), s(&a[1])).0],
            "REMOVE" => vec![remove(s(&a[0]), s(&a[1])).0],
            "REMOVEANY" => vec![remove_any(s(&a[0]), s(&a[1])).0],
            "RETAINANY" => vec![retain_any(s(&a[0]), s(&a[1])).0],
            "EXTRACT" => vec![extract(i(&a[0]), s(&a[1]), s(&a[2])).0],
            "REMAIN" => vec![remain(i(&a[0]), s(&a[1]), s(&a[2])).0],
            "STRINGS" => vec![string_fill(i(&a[0]), s(&a[1])).0],
            "STRINGC" => vec![string_chr(i(&a[0]), i(&a[1])).0],
            "SPACE" => vec![space(i(&a[0])).0],
            "CHR" => {
                let codes: Vec<i32> = a[1..].iter().map(i).collect();
                assert_eq!(codes.len() as i32, i(&a[0]));
                vec![chr(&codes).0]
            }
            "ASC" => vec![asc(s(&a[0])).to_string().into_bytes()],
            "LEN" => vec![len(s(&a[0])).to_string().into_bytes()],
            "CMP" => {
                let c = match compare(s(&a[0]), s(&a[1])) {
                    Ordering::Less => -1,
                    Ordering::Equal => 0,
                    Ordering::Greater => 1,
                };
                vec![c.to_string().into_bytes()]
            }
            _ => panic!("unknown vector {name}"),
        };
        let want: Vec<Vec<u8>> = r
            .iter()
            .map(|t| match t {
                Tok::Str(v) => v.clone(),
                Tok::Int(v) => v.to_string().into_bytes(),
                Tok::Err => panic!("misplaced error token"),
            })
            .collect();
        if got != want {
            fails.push(format!("{line}\n    got {:?}", got.iter().map(|g| PbStr(g.clone())).collect::<Vec<_>>()));
        }
        n += 1;
    }
    assert!(n > 20000, "only {n} vectors");
    assert!(fails.is_empty(), "{} of {n} vectors failed:\n{}", fails.len(), fails[..fails.len().min(40)].join("\n"));
}
