// node web/content-browser-test.mjs INSTALLER COMPLETE_SAVE URL
// Each run owns a fresh browser profile; no personal browser storage is used.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { CONTENT_FILE } from './content.mjs';

const [installer, save, url = 'http://localhost:8000/'] = process.argv.slice(2);
if (!installer || !save) throw new Error('Provide the original installer and a complete save.');
const session = 'wsr-content-' + process.pid;
function browser(...args) {
  const response = JSON.parse(execFileSync(process.env.AGENT_BROWSER_BIN || 'agent-browser',
    ['--session', session, '--json', ...args], { encoding:'utf8', timeout:40000, maxBuffer:4 * 1024 * 1024 }));
  assert.ok(response.success, JSON.stringify(response.error));
  return response.data;
}
const evaluate = js => browser('eval', js).result;
const wait = js => browser('wait', '--fn', js);
const button = name => browser('find', 'role', 'button', 'click', '--name', name, '--exact');
const idle = () => wait("!document.querySelector('#dlg') && !document.querySelector('#installer').disabled");
function open() {
  browser('open', url);
  wait("!document.querySelector('#installer').disabled");
  assert.equal(evaluate('crossOriginIsolated'), true);
}
function importInstaller() {
  browser('upload', '#installer', resolve(installer)); idle();
  assert.equal(evaluate("document.querySelector('#new-game').disabled"), false);
}
function files() {
  return evaluate(`new Promise((resolve, reject) => {
    const request = indexedDB.open('wsr-local-game-files', 1);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const db = request.result, read = db.transaction('files').objectStore('files').getAll();
      read.onerror = () => { db.close(); reject(read.error); };
      read.onsuccess = async () => {
        db.close();
        const entries = await Promise.all(read.result.map(async file => {
          const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', file.bytes));
          return [file.name, [...digest].map(b => b.toString(16).padStart(2, '0')).join('')];
        }));
        resolve(Object.fromEntries(entries.sort(([a],[b]) => a.localeCompare(b))));
      };
    };
  })`);
}
function alterCachedPack(remove) {
  evaluate(`new Promise((resolve, reject) => {
    const request = indexedDB.open('wsr-local-game-files', 1);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const db = request.result, tx = db.transaction('files', 'readwrite'), store = tx.objectStore('files');
      if (${remove}) store.delete(${JSON.stringify(CONTENT_FILE)});
      else {
        const read = store.get(${JSON.stringify(CONTENT_FILE)});
        read.onsuccess = () => { const value = read.result; value.bytes[0] ^= 1; store.put(value); };
      }
      tx.oncomplete = () => { db.close(); resolve(true); };
      tx.onabort = () => { db.close(); reject(tx.error); };
    };
  })`);
}
try {
  open();
  assert.equal(evaluate("document.querySelector('#new-game').disabled"), true);
  browser('upload', '#save-file', resolve(save)); idle();
  assert.match(evaluate("document.querySelector('#error').textContent"), /Reopen the original installer/);
  importInstaller();
  assert.ok(files()[CONTENT_FILE], 'Extracted catalog was not persisted');
  browser('upload', '#save-file', resolve(save));
  wait("!!document.querySelector('#dlg') || !document.querySelector('#installer').disabled");
  if (evaluate("document.querySelector('#dlg')?.textContent.includes('invalid dividends')")) button('OK');
  idle();
  assert.ok(evaluate("!!document.querySelector('#desk')?.offsetWidth"));
  if (evaluate("document.querySelector('.wsr-clock')?.dataset.running === 'true'")) {
    browser('click', '#ticker-toggle'); idle();
  }
  browser('click', '#save-game');
  for (let i = 0; i < 5 && evaluate("!!document.querySelector('#dlg')"); i++) {
    const yes = evaluate("[...document.querySelectorAll('#dlg-buttons button')].some(b => b.textContent === 'Yes')");
    button(yes ? 'Yes' : 'OK');
  }
  idle();
  const baseline = files();
  assert.ok(Object.keys(baseline).some(name => /^GAME\d+\.DAT$/.test(name)), 'No saved game persisted');
  for (const remove of [true, false]) {
    alterCachedPack(remove);
    open();
    assert.equal(evaluate("document.querySelector('#new-game').disabled"), true,
      'Missing or malformed cached content must require reimport');
    const current = files();
    for (const [name, digest] of Object.entries(baseline)) {
      if (name !== CONTENT_FILE) assert.equal(current[name], digest, 'Startup changed ' + name);
    }
    importInstaller();
    assert.deepEqual(files(), baseline, 'Reimport changed previously stored game files');
  }
  open();
  assert.equal(evaluate("document.querySelector('#new-game').disabled"), false);
  assert.deepEqual(files(), baseline, 'Catalog did not survive the final reload');
  console.log('Runtime content browser: import, save/load, persisted reload, legacy-cache upgrade and malformed-cache recovery passed; all existing file hashes preserved.');
} finally {
  browser('close');
}
