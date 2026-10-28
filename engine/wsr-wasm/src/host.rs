//! `Ui` implementation that calls the host page synchronously (wasm32 only).
//!
//! The engine runs in a Web Worker. Each import below posts a request to the page and blocks the worker
//! with `Atomics.wait` until the user has answered (see `web/worker.js`), so game code can call
//! `ui.msgbox(…)` synchronously, exactly like the original's modal dialogs. Strings are Windows-1252
//! bytes. String results come back as buffers the host allocated with `wsr_alloc` (`[len: u32][bytes]`).
use wsr_core::pb::real::{Ext, cast};


use wsr_core::pb::pbstr::PbStr;
use wsr_core::ui::{ReportAction, ReportOptions, Ui};

#[link(wasm_import_module = "host")]
unsafe extern "C" {
    /// Writes year, month, day, hour, minute, second and milliseconds as seven i32 values.
    fn host_local_time(out: *mut i32);
    fn host_msgbox(text: *const u8, text_len: usize, style: i32, title: *const u8, title_len: usize) -> i32;
    fn host_inputbox(
        prompt: *const u8,
        prompt_len: usize,
        title: *const u8,
        title_len: usize,
        default: *const u8,
        default_len: usize,
    ) -> *mut u8;
    /// Returns 0 = OK/YES, 1 = NO, 2 = closed, 3 = link (then `host_last_link` gives the code).
    fn host_report(title: *const u8, title_len: usize, lines: *const u8, lines_len: usize, yes_no: i32, options: i32) -> i32;
    fn host_last_link() -> *mut u8;
    fn host_news(text: *const u8, text_len: usize);
    /// UTF-8 JSON describing a choice or the chart controls; -1 cancels a choice, 1 trades a chart.
    fn host_choose(json: *const u8, len: usize) -> i32;
    fn host_chart(json: *const u8, len: usize) -> i32;
    fn host_diagram(json: *const u8, len: usize) -> i32;
    fn host_option_station(json: *const u8, len: usize) -> *mut u8;
    fn host_form(json: *const u8, len: usize) -> *mut u8;
    fn host_symbol_input(json: *const u8, len: usize) -> *mut u8;
}

/// Take ownership of a `[len: u32][bytes]` buffer allocated by the host via `wsr_alloc`.
fn take(p: *mut u8) -> Vec<u8> {
    if p.is_null() {
        return Vec::new();
    }
    unsafe {
        let len = cast::<usize, _>(u32::from_le_bytes(*(p as *const [u8; 4])));
        let v = std::slice::from_raw_parts(p.add(4), len).to_vec();
        crate::wsr_free(p, len + 4);
        v
    }
}

/// Talks to the page through the worker's blocking bridge.
#[derive(Default)]
pub struct HostUi;

impl Ui for HostUi {
    fn safety_checks(&self) -> bool { true }
    fn random_remark(&mut self) -> PbStr {
        let now = self.local_time();
        crate::CONTENT.with(|content| {
            content.borrow().as_ref().map(|content|
                crate::REMARKS.with(|r| r.borrow_mut().next(&now, content))
            ).unwrap_or_default()
        })
    }
    fn high_score(&mut self, path: &PbStr, name: &PbStr, score: i64) -> i32 {
        let file = path.clone() + "SCORES.WSR";
        let data = crate::FILES.with(|f| f.borrow().read(file.as_bytes()).cloned());
        let result = crate::HIGH_SCORES.with(|h| h.borrow_mut().call(self, data.as_deref(), path, name, score));
        if let Some(bytes) = result.written { self.write_file(&file, &bytes); }
        result.value
    }
    fn write_file(&mut self, name: &PbStr, data: &[u8]) {
        crate::FILES.with(|f| f.borrow_mut().write(name.as_bytes(), data.to_vec()));
    }
    // The page's save status acknowledges IndexedDB's committed transaction after this command
    // returns. A blocking modal here would claim success before those bytes can reach storage.
    fn save_confirmation(&mut self, _: &PbStr, _: &PbStr) {}
    fn local_time(&self) -> wsr_core::logic::newgame::LocalTime {
        let mut fields = [0i32; 7];
        unsafe { host_local_time(fields.as_mut_ptr()) };
        wsr_core::logic::newgame::LocalTime {
            year: fields[0], month: fields[1], day: fields[2], hour: fields[3],
            minute: fields[4], second: fields[5], millis: fields[6],
        }
    }
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        let (t, h) = (text.as_bytes(), title.as_bytes());
        unsafe { host_msgbox(t.as_ptr(), t.len(), style, h.as_ptr(), h.len()) }
    }
    fn inputbox(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr) -> PbStr {
        let (p, t, d) = (prompt.as_bytes(), title.as_bytes(), default.as_bytes());
        PbStr(take(unsafe { host_inputbox(p.as_ptr(), p.len(), t.as_ptr(), t.len(), d.as_ptr(), d.len()) }))
    }
    fn form(&mut self, form: &wsr_core::ui::Form) -> Option<wsr_core::ui::FormResponse> {
        let json = crate::presentation::form_json(form);
        let bytes = take(unsafe { host_form(json.as_ptr(), json.len()) });
        Some(crate::presentation::form_response(&bytes, form))
    }
    fn symbol_input(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr, companies: &[wsr_core::ui::SymbolChoice]) -> PbStr {
        let json = serde_json::json!({
            "title": title.to_display(), "prompt": prompt.to_display(), "value": default.to_display(),
            "companies": companies.iter().map(|c| serde_json::json!({
                "id": c.id, "name": c.name.to_display(), "symbol": c.symbol.to_display()
            })).collect::<Vec<_>>()
        }).to_string();
        PbStr(take(unsafe { host_symbol_input(json.as_ptr(), json.len()) }))
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool) -> ReportAction {
        self.report_with_actions(title, lines, yes_no, &ReportOptions::default())
    }
    fn report_with_actions(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool, options: &ReportOptions) -> ReportAction {
        // lines joined with CR LF (they never contain CR LF themselves)
        let mut buf = Vec::new();
        for (k, l) in lines.iter().enumerate() {
            if k > 0 {
                buf.extend_from_slice(b"\r\n");
            }
            buf.extend_from_slice(l.as_bytes());
        }
        let t = title.as_bytes();
        let flags = options.analyst_rating.unwrap_or(0).clamp(0, 7)
            | (i32::from(options.rating_trade) << 8) | (i32::from(options.stock_chart) << 9)
            | (i32::from(options.analyst_rating.is_some()) << 10);
        match unsafe { host_report(t.as_ptr(), t.len(), buf.as_ptr(), buf.len(), cast::<i32, _>(yes_no), flags) } {
            1 => ReportAction::No,
            2 => ReportAction::Closed,
            3 => ReportAction::Link(PbStr(take(unsafe { host_last_link() }))),
            4 => ReportAction::Chart,
            5 => ReportAction::Rating,
            _ => ReportAction::Ok,
        }
    }
    fn news(&mut self, headline: &PbStr) {
        let h = headline.as_bytes();
        unsafe { host_news(h.as_ptr(), h.len()) }
    }
    fn ownership_diagram(&mut self, spec: &wsr_core::logic::info_screens::ownership::OwnershipDiagram) -> ReportAction {
        let json=crate::presentation::ownership_json(spec);
        match unsafe {host_diagram(json.as_ptr(),json.len())} {
            -1=>ReportAction::Closed, 0=>ReportAction::Ok,
            id=>ReportAction::Link(PbStr::from(format!("@C{id:04}").as_str())),
        }
    }
    fn option_station(&mut self, spec: &wsr_core::logic::trade_screens::option_station::OptionStation)
        -> Option<Vec<wsr_core::logic::trade_screens::option_station::StationAction>> {
        let json=crate::presentation::option_json(spec);
        let bytes=take(unsafe {host_option_station(json.as_ptr(),json.len())});
        Some(crate::presentation::option_actions(&bytes,spec))
    }
    fn choose(&mut self, title: &PbStr, prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
        let js = |s: &PbStr| crate::summary::json_str(&s.to_display());
        let json = format!("{{\"title\":{},\"prompt\":[{}],\"options\":[{}]}}", js(title),
            prompt.iter().map(js).collect::<Vec<_>>().join(","),
            options.iter().map(js).collect::<Vec<_>>().join(","));
        let choice = unsafe { host_choose(json.as_ptr(), json.len()) };
        usize::try_from(choice).ok().filter(|&k| k < options.len())
    }
    fn chart(&mut self, spec: &wsr_core::logic::info_screens::chart::ChartSpec) -> bool {
        use wsr_core::logic::info_screens::chart::ChartControlKind;
        let controls = spec.controls.iter().map(|c| {
            let kind = match c.kind { ChartControlKind::Line => "line", ChartControlKind::Label => "label", ChartControlKind::Button => "button" };
            format!("{{\"id\":{},\"kind\":\"{kind}\",\"text\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"style\":{}}}",
                c.id, crate::summary::json_str(&c.text.to_display()), c.x, c.y, c.w, c.h, c.style)
        }).collect::<Vec<_>>().join(",");
        let history = crate::presentation::chart_history_json(spec);
        let json = format!("{{\"title\":{},\"width\":{},\"height\":{},\"controls\":[{controls}],\"tradeLabel\":{},\"history\":[{history}],\"entity\":{},\"endYear\":{},\"endMonth\":{}}}",
            crate::summary::json_str(&spec.title.to_display()), spec.width, spec.height,
            spec.trade_label.as_ref().map(|s| crate::summary::json_str(&s.to_display())).unwrap_or_else(|| "null".into()),
            spec.entity, spec.end_year, spec.end_month);
        unsafe { host_chart(json.as_ptr(), json.len()) != 0 }
    }
}
