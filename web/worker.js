// The synchronous engine runs here; modal UI waits on the page's SharedArrayBuffer answer.
import { extractInstaller } from './installer.mjs';
import { parseCompanies } from './companies.mjs';
import { FileJournal } from './file-journal.mjs';
import { CONTENT_FILE } from './content.mjs';
const journal = new FileJournal();
let w, ctrl, data, lastLink = new Uint8Array();
let startupQuoteShown = false;
let coachSession = crypto.randomUUID(), gameRevision = 0, coachSnapshot = null;
function observe() {
  coachSnapshot = { sessionId: coachSession, revision: gameRevision, capturedAt: new Date().toISOString(), observation: json(call(w.wsr_observe)) };
  postMessage({ type: 'observation', snapshot: coachSnapshot });
  return coachSnapshot;
}
const dec = new TextDecoder('windows-1252');
const utf8 = new TextDecoder();
const enc = new TextEncoder();
const str = (p, n) => dec.decode(new Uint8Array(w.memory.buffer, p, n).slice());
const readJson = (p, n) => JSON.parse(utf8.decode(new Uint8Array(w.memory.buffer, p, n)));
function ask(req) {
  Atomics.store(ctrl, 0, 0);
  postMessage({ type: 'ui', ...req });
  Atomics.wait(ctrl, 0, 0);
  return ctrl[1];
}
function toWasm(bytes) {
  const p = w.wsr_alloc(bytes.length + 4);
  new DataView(w.memory.buffer).setUint32(p, bytes.length, true);
  new Uint8Array(w.memory.buffer, p + 4, bytes.length).set(bytes);
  return p;
}
const host = {
  host_local_time: (p) => {
    const t = new Date();
    new Int32Array(w.memory.buffer, p, 7).set([t.getFullYear(), t.getMonth() + 1, t.getDate(), t.getHours(), t.getMinutes(), t.getSeconds(), t.getMilliseconds()]);
  },
  host_msgbox: (p, n, style, t, tn) => {
    if (style === 0 && tn === 0) { postMessage({type:'notice',text:str(p,n)}); return 1; }
    return ask({ kind:'msgbox', text:str(p,n), style, title:str(t,tn) });
  },
  host_inputbox: (p, n, t, tn, d, dn) => {
    ask({ kind: 'inputbox', prompt: str(p, n), title: str(t, tn), value: str(d, dn) });
    return toWasm(data.slice(0, ctrl[2]));
  },
  host_report: (t, tn, p, n, yesNo, options = 0) => {
    const r = ask({ kind: 'report', title: str(t, tn), lines: str(p, n).split('\r\n'), yesNo: !!yesNo,
      analystRating: options & 1024 ? options & 7 : null, ratingTrade: !!(options & 256), stockChart: !!(options & 512) });
    if (r === 3) lastLink = data.slice(0, ctrl[2]);
    return r;
  },
  host_last_link: () => toWasm(lastLink),
  host_news: (p, n) => postMessage({ type: 'news', text: str(p, n) }),
  host_choose: (p, n) => ask({ kind: 'choose', ...readJson(p, n) }),
  host_chart: (p, n) => ask({ kind: 'chart', ...readJson(p, n) }),
  host_diagram: (p, n) => ask({kind:'ownership', ...readJson(p,n)}),
  host_option_station: (p,n) => {
    const button=ask({kind:'optionStation', ...readJson(p,n)}), fields=data.slice(0,ctrl[2]);
    const bytes=new Uint8Array(4+fields.length); new DataView(bytes.buffer).setInt32(0,button,true);bytes.set(fields,4);return toWasm(bytes);
  },
};
function take(p) {
  const n = new DataView(w.memory.buffer).getUint32(p, true);
  const bytes = new Uint8Array(w.memory.buffer, p + 4, n).slice();
  w.wsr_free(p, n + 4);
  return bytes;
}
function call(fn, bytes, ...args) {
  if (bytes == null) return take(fn(...args));
  const p = w.wsr_alloc(bytes.length);
  try {
    new Uint8Array(w.memory.buffer, p, bytes.length).set(bytes);
    return take(fn(p, bytes.length, ...args));
  } finally { w.wsr_free(p, bytes.length); }
}
const json = (bytes) => JSON.parse(utf8.decode(bytes));
const command = (c, arg = 0, text = new Uint8Array()) => json(call((p, n) => w.wsr_command(c, arg, p, n), text));
function showStartupQuote() {
  if (startupQuoteShown) return;
  const files = new Set(command(0).files.map(file => file.name));
  if (!files.has(CONTENT_FILE) || !files.has('QUOTEWSR.DAT')) return;
  startupQuoteShown = true;
  command(8);
}
function putFile(name, bytes) {
  const nb = enc.encode(name), p = w.wsr_alloc(nb.length);
  try {
    new Uint8Array(w.memory.buffer, p, nb.length).set(nb);
    const value = json(call((d, n) => w.wsr_put_file(p, nb.length, d, n), bytes));
    if (!value.ok) throw new Error(value.error);
    return value;
  } finally { w.wsr_free(p, nb.length); }
}
function changes() {
  return json(call(w.wsr_file_changes)).map(({ name, deleted }) => ({ name,
    bytes: deleted ? null : call(w.wsr_get_file, enc.encode(name)) }));
}
onmessage = async ({ data: m }) => {
  try {
    if (m.cmd === 'ack') { journal.acknowledge(m.revision); return; }
    if (m.cmd === 'init') {
      ctrl = new Int32Array(m.shared, 0, 4); data = new Uint8Array(m.shared, 16);
      const { instance } = await WebAssembly.instantiateStreaming(fetch(new URL('./wsr.wasm', import.meta.url)), { host });
      w = instance.exports;
      for (const f of m.files || []) {
        try { putFile(f.name, f.bytes); }
        catch (error) {
          if (f.name.toUpperCase() !== CONTENT_FILE) throw error;
          postMessage({ type:'notice', text:'Reopen the installer to restore runtime game content. Your saves are preserved.' });
        }
      }
      postMessage({ type: 'ready', value: command(0), coachSession });
      showStartupQuote();
      return;
    }
    if (m.cmd === 'observe') {
      postMessage({ type: 'result', id: m.id, value: observe(), observation: true });
      return;
    }
    const changingScope = m.cmd === 'load' || (m.cmd === 'action' && (m.action === 1 || (m.action === 2 && m.arg === 244)));
    if (changingScope) {
      coachSession = crypto.randomUUID(); coachSnapshot = null;
      postMessage({ type: 'coach-scope', sessionId: coachSession });
    }
    // Publish a safe baseline before any foreground command can block in a dialog.
    if (!changingScope && (m.cmd === 'report' || (m.cmd === 'action' && m.action !== 5))) observe();
    let value, imported = [];
    if (m.cmd === 'installer') {
      const extracted = await extractInstaller(m.bytes, (text) => postMessage({ type:'progress', text }));
      const existing = new Set(command(0).files.map((f) => f.name));
      // The installer ships CORPNAME.ORI as @DUMMY; the Windows Customizer creates the
      // real backup on first use. Keep a complete original template in the browser.
      imported = extracted.filter((f) => !existing.has(f.name) && f.name !== 'CORPNAME.ORI');
      try { parseCompanies(call(w.wsr_get_file, enc.encode('CORPNAME.ORI'))); }
      catch { imported.push({name:'CORPNAME.ORI',bytes:extracted.find(f => f.name === 'CORPNAME.DAT').bytes}); }
      for (const f of imported) putFile(f.name, f.bytes);
      value = command(0);
    }
    else if (m.cmd === 'load') value = json(call(w.wsr_open_game, m.bytes, 0,
      +(m.name?.match(/^GAME(\d+)\.DAT$/i)?.[1] || 1)));
    else if (m.cmd === 'customize') {
      parseCompanies(m.bytes);
      let validBackup = true;
      try { parseCompanies(call(w.wsr_get_file, enc.encode('CORPNAME.ORI'))); } catch { validBackup = false; }
      if (!validBackup) {
        const bytes = call(w.wsr_get_file, enc.encode('CORPNAME.DAT'));
        putFile('CORPNAME.ORI', bytes); imported.push({name:'CORPNAME.ORI',bytes});
      }
      putFile('CORPNAME.DAT', m.bytes);
      imported.push({name:'CORPNAME.DAT',bytes:m.bytes});
      value = command(0);
    }
    else if (m.cmd === 'restore') {
      const existing = new Set(command(0).files.map(f => f.name));
      for (const f of m.files) if (!existing.has(f.name)) putFile(f.name, f.bytes);
      value = command(0);
    }
    else if (m.cmd === 'flush') value = command(0);
    else if (m.cmd === 'action') value = command(m.action, m.arg, m.text);
    else if (m.cmd === 'report') {
      value = json(call(w.wsr_show_report, null, m.kind, m.entity));
      if (value.ok) value = command(0);
    } else if (m.cmd === 'getfile') value = call(w.wsr_get_file, enc.encode(m.name));
    else throw new Error('Unknown worker command');
    if (m.cmd !== 'getfile') {
      gameRevision++;
      if (!(m.cmd === 'action' && m.action === 5)) observe();
    }
    journal.record([...imported, ...changes()]);
    postMessage({ type: 'result', id: m.id, value, gameRevision, ...journal.snapshot() });
    if (m.cmd === 'installer' || m.cmd === 'restore') showStartupQuote();
  } catch (e) { postMessage({ type: 'error', id: m.id, error: e.message || String(e) }); }
};
