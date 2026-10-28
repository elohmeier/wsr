// Public, asset-free smoke test against a headerless static host and project subpath.
// node web/hosting-browser-test.mjs http://127.0.0.1:PORT/wsr/
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
const [url] = process.argv.slice(2);
if (!url) throw new Error('Provide a static preview URL.');
const response = await fetch(url);
assert.ok(response.ok);
assert.equal(response.headers.has('Cross-Origin-Opener-Policy'), false, 'Use a headerless test server');
const session = 'wsr-hosting-' + process.pid;
function browser(...args) {
  const result = JSON.parse(execFileSync(process.env.AGENT_BROWSER_BIN || 'agent-browser',
    ['--session', session, '--json', ...args], { encoding: 'utf8', timeout: 40000 }));
  assert.ok(result.success, JSON.stringify(result.error));
  return result.data;
}
try {
  browser('open', url);
  browser('wait', '--fn', "!document.querySelector('#installer')?.disabled && !!document.querySelector('#installer')");
  assert.deepEqual(browser('eval', `({isolated:crossOriginIsolated, shared:typeof SharedArrayBuffer,
    scope:navigator.serviceWorker.controller?.scriptURL, newGame:document.querySelector('#new-game').disabled})`).result,
  { isolated: true, shared: 'function', scope: new URL('isolation-worker.js', url).href, newGame: true });
  // Use the normal navigation API: the CLI's reload command bypasses the service worker.
  browser('eval', 'window.__wsrReloadProbe = true; location.reload()');
  browser('wait', '--fn', "!window.__wsrReloadProbe && !document.querySelector('#installer')?.disabled && !!document.querySelector('#installer')");
  assert.equal(browser('eval', 'crossOriginIsolated').result, true);
  console.log('Static hosting: cold start and reload under a project subpath enable isolation and load worker/WASM without game resources.');
} catch (error) {
  console.error(browser('eval', `({isolated:crossOriginIsolated, text:document.body.innerText,
    controlled:!!navigator.serviceWorker.controller})`).result);
  throw error;
} finally { browser('close'); }
