//! VARICHEK.DLL!HiScore (0x4010ce): one record, its obfuscated score marker, and a per-process write guard.
use crate::pb::real::cast;

use crate::{
    logic::newgame::LocalTime,
    pb::{
        ext80::Ext80,
        numeric::{InputReader, str_num_ext, val_ext, write_str},
        pbstr::PbStr,
        rng::Rng,
        string::{ltrim, parse_any, rtrim},
    },
    ui::Ui,
};

#[derive(Default)]
pub struct HighScores {
    attempted: bool,
}
pub struct Result {
    pub value: i32,
    pub written: Option<Vec<u8>>,
}

impl HighScores {
    /// `score = -1` displays the record. Other values attempt an update once per application process.
    /// `data` is SCORES.WSR; a successful update returns its exact WRITE # bytes to the host.
    pub fn call(
        &mut self,
        ui: &mut dyn Ui,
        data: Option<&[u8]>,
        data_path: &PbStr,
        name: &PbStr,
        score: i64,
    ) -> Result {
    let __content = crate::content::current();
        let result = |value, written| Result { value, written };
        if score != -1 {
            if self.attempted {
                return result(1, None);
            }
            self.attempted = true;
        }
        let title = PbStr::from(__content.string(5680));
        let Some(data) = data else {
            ui.msgbox(
                &(PbStr::from(__content.string(5681)) + data_path),
                4096,
                &title,
            );
            return result(1, None);
        };
        let record = match InputReader::new(data).try_input_str() {
            Ok(record) => record,
            Err(code) => {
                ui.msgbox(
                    &PbStr::from(format!("{__wsr_text_5682}{code}{__wsr_text_5683}",__wsr_text_5682=__content.string(5682), __wsr_text_5683=__content.string(5683)).as_str()),
                    4096,
                    &title,
                );
                return result(1, None);
            }
        };
        let old_score = val_ext(parse_any(&record, b"|", 2).as_bytes()).to_i64();
        if score <= old_score {
            if score == -1 && old_score > 100 {
                let old_name = parse_any(&record, b"|", 1).to_display().trim().to_string();
                let date = parse_any(&record, b"|", 3).to_display().trim().to_string();
                let value = crate::pb::numeric::format_ext(
                    Ext80::from_i64(old_score),
                    b"* ########################,",
                );
                let value = String::from_utf8_lossy(&value).trim().to_string();
                ui.msgbox(&PbStr::from(format!("{__wsr_text_5684}{value} million, by {old_name}{__wsr_text_5685}{date}.",__wsr_text_5684=__content.string(5684), __wsr_text_5685=__content.string(5685)).as_str()), 4096, &title);
            }
            return result(
                if score == -1 {
                    Ext80::from_i64(old_score).to_i32()
                } else {
                    1
                },
                None,
            );
        }
        let now = ui.local_time();
        let number = str_num_ext(Ext80::from_i64(score), 15);
        let text = format!(
            "{} | {} | {:02}-{:02}-{:04} | ",
            name.to_display(),
            number,
            now.month,
            now.day,
            now.year
        );
        let marker = score_marker(score, &now);
        let mut bytes = write_str(&(PbStr::from(text.as_str()) + &marker + " |").0);
        bytes.extend_from_slice(b"\r\n");
        result(2, Some(bytes))
    }
}

fn score_marker(score: i64, now: &LocalTime) -> PbStr {
    let mut rng = Rng::default();
    rng.randomize_timer(now.timer_ms());
    let letter = |rng: &mut Rng| {
        b'A' + cast::<u8, _>(Ext80::from_ext(rng.rnd())
            .fmul(Ext80::from_i64(25))
            .to_i32())
    };
    let mut bytes: Vec<u8> = (0..6).map(|_| letter(&mut rng)).collect();
    let number = str_num_ext(Ext80::from_i64(score), 15);
    let digits = rtrim(ltrim(number.as_bytes()).as_bytes());
    for &c in digits.as_bytes() {
        bytes.push(b'L'.wrapping_add(cast::<u8, _>(val_ext(&[c]).to_i32())));
    }
    bytes.extend((0..5).map(|_| letter(&mut rng)));
    let n = digits.len();
    bytes.push(if n < 3 {
        b'A'
    } else {
        (cast::<u8, _>(n)).wrapping_add(if n > 28 { 68 } else { 62 })
    });
    PbStr(bytes)
}
