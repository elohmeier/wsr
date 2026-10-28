// Node test of the blocking UI bridge: node web/bridge-test.mjs path/to/GAMExx.DAT
// A worker thread runs wsr.wasm; the engine's report request blocks it (Atomics.wait) until the main thread
// answers, exactly like web/worker.js in the browser.
import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

if (isMainThread) {
  const shared = new SharedArrayBuffer(16 + 4096);
  const ctrl = new Int32Array(shared, 0, 4);
  const w = new Worker(new URL(import.meta.url), { workerData: { shared, save: process.argv[2] } });
  let requests = 0, completed = false;
  w.on('message', (m) => {
    if (m.type === 'ui') {
      requests++;
      console.log(`UI request: ${m.kind} "${m.title}" with ${m.lines?.length} lines; answering OK after 200 ms`);
      setTimeout(() => { ctrl[1] = 0; Atomics.store(ctrl, 0, 1); Atomics.notify(ctrl, 0); }, 200);
    } else if (m.type === 'done') {
      assert.equal(JSON.parse(m.result).ok, true);
      assert.ok(requests > 0, 'Engine did not request a UI response');
      assert.ok(m.blockedMs >= 190, 'Engine continued before the delayed UI response');
      completed = true;
      console.log('worker:', JSON.stringify(m));
    } else console.log('worker:', JSON.stringify(m));
  });
  w.on('error', (e) => { throw e; });
  w.on('exit', (code) => { assert.equal(code, 0); assert.ok(completed, 'Worker exited without a result'); });
} else {
  const { shared, save } = workerData;
  const ctrl = new Int32Array(shared, 0, 4);
  const dec = new TextDecoder('windows-1252');
  let w;
  const str = (p, n) => dec.decode(new Uint8Array(w.memory.buffer, p, n).slice());
  const ask = (req) => { Atomics.store(ctrl, 0, 0); parentPort.postMessage({ type: 'ui', ...req }); Atomics.wait(ctrl, 0, 0); return ctrl[1]; };
  const host = {
    host_local_time: (p) => new Int32Array(w.memory.buffer, p, 7).set([2000, 1, 3, 10, 0, 0, 0]),
    host_choose: () => -1, host_chart: () => 0, host_diagram: () => -1, host_option_station: () => 0, host_msgbox: (tp, tl, style, hp, hl) => ask({ kind: 'msgbox', text: str(tp, tl), title: str(hp, hl) }),
    host_inputbox: () => 0,
    host_report: (tp, tl, lp, ll, yn) => ask({ kind: 'report', title: str(tp, tl), lines: str(lp, ll).split('\r\n'), yesNo: !!yn }),
    host_last_link: () => 0,
    host_news: (p, n) => parentPort.postMessage({ type: 'news', text: str(p, n) }),
  };
  ({ exports: w } = (await WebAssembly.instantiate(readFileSync(new URL('wsr.wasm', import.meta.url)), { host })).instance);
  const bytes = readFileSync(save);
  const p = w.wsr_alloc(bytes.length);
  new Uint8Array(w.memory.buffer, p, bytes.length).set(bytes);
  w.wsr_load_save(p, bytes.length, 0);
  const t0 = Date.now();
  const r = w.wsr_show_report(1, 131);
  const len = new DataView(w.memory.buffer).getUint32(r, true);
  parentPort.postMessage({ type: 'done', result: new TextDecoder().decode(new Uint8Array(w.memory.buffer, r + 4, len)), blockedMs: Date.now() - t0 });
  process.exit(0);
}
