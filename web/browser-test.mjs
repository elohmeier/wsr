// node web/browser-test.mjs wsr985.exe /path/to/GAME01.DAT http://localhost:8899/
// Runs the real browser + worker + WASM in an isolated agent-browser session.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
const [installer, save, url = 'http://localhost:8899/'] = process.argv.slice(2);
if (!installer || !save) throw new Error('Provide the original installer and a complete saved game.');
const session = 'wsr-regression-'+process.pid;
const bin = process.env.AGENT_BROWSER_BIN || 'agent-browser';
function browser(...args) {
  const result = JSON.parse(execFileSync(bin,['--session',session,'--json',...args],{encoding:'utf8',timeout:40000,maxBuffer:4*1024*1024}));
  assert.ok(result.success,JSON.stringify(result.error));
  return result.data;
}
const evaluate = js => browser('eval',js).result;
const wait = js => browser('wait','--fn',js);
const click = selector => browser('click',selector);
const fill = (selector,value) => browser('fill',selector,String(value));
const button = name => browser('find','role','button','click','--name',name,'--exact');
const idle = () => wait("!document.querySelector('#dlg') && !document.querySelector('#symbol')?.readOnly && !document.querySelector('#installer').disabled");
function settleEarnings() {
  for (let i=0;i<5;i++) {
    wait("!!document.querySelector('#dlg') || (!document.querySelector('#symbol')?.readOnly && !document.querySelector('#installer').disabled)");
    if (!evaluate("!!document.querySelector('#dlg')")) return;
    assert.match(evaluate("document.querySelector('#dlg-title').textContent"), /LATEST EARNINGS REPORT/);
    button('OK');
  }
  idle();
}
const delay = ms => new Promise(resolve => setTimeout(resolve,ms));
try {
  browser('open',url);
  browser('set','viewport','1366','768');browser('set','media','light');
  wait("!document.querySelector('#installer').disabled");
  browser('upload','#installer',resolve(installer)); idle();
  browser('upload','#save-file',resolve(save));
  wait("document.querySelector('#dlg')?.textContent.includes('invalid dividends') || (!document.querySelector('#dlg') && !document.querySelector('#installer').disabled)");
  if (evaluate("document.querySelector('#dlg')?.textContent.includes('invalid dividends')")) button('OK');
  idle();
  assert.equal(evaluate("document.querySelector('#desk').hidden"),false);
  assert.equal(evaluate("getComputedStyle(document.documentElement).colorScheme"),'dark');
  assert.equal(evaluate("document.querySelector('.wsr-brand img').naturalWidth"),998);
  assert.equal(evaluate("document.querySelectorAll('.wsr-economy button').length"),10);
  if (evaluate("document.querySelector('.wsr-clock')?.dataset.running === 'true'")) { click('#ticker-toggle'); idle(); }
  assert.ok(evaluate("['.wsr-entity','#balance-sheet','#market','.wsr-command-deck','.wsr-status-strip','.wsr-news-panel','.wsr-ticker','.wsr-quick-panel','.wsr-quotes'].every(s=>document.querySelector(s).getBoundingClientRect().bottom<=innerHeight)"),'The complete desk must fit a 1366 by 768 viewport');
  assert.equal(evaluate("document.documentElement.scrollWidth"),1366);
  fill('#symbol','AAPL'); browser('press','Enter'); idle();
  assert.match(evaluate("document.querySelector('#selected-name').textContent"),/APPLE/i);
  // Keep the very same input and focus alive across real background ticks.
  evaluate("window.focusProbe={node:document.querySelector('#symbol'),disabled:0,ticks:0}; window.focusObserver=new MutationObserver(ms=>window.focusProbe.disabled+=ms.filter(m=>m.attributeName==='disabled').length); focusObserver.observe(focusProbe.node,{attributes:true}); window.originalPost=Worker.prototype.postMessage; Worker.prototype.postMessage=function(m,...args){if(m.cmd==='action'&&m.action===5)focusProbe.ticks++;return originalPost.call(this,m,...args)}");
  click('#ticker-toggle'); fill('#symbol','');
  for (const letter of 'AAPL') { browser('type','#symbol',letter); await delay(200); }
  assert.deepEqual(evaluate("({same:focusProbe.node===document.querySelector('#symbol'),focused:document.activeElement===focusProbe.node,value:focusProbe.node.value,disabled:focusProbe.disabled,ticked:focusProbe.ticks>2})"),
    {same:true,focused:true,value:'AAPL',disabled:0,ticked:true});
  // A foreground submit arriving during an outstanding tick must be queued.
  evaluate("window.slowProbe={waiting:false}; Worker.prototype.postMessage=function(m,...args){if(m.cmd==='action'&&m.action===5&&!slowProbe.waiting){slowProbe.waiting=true;setTimeout(()=>originalPost.call(this,m,...args),700);return;}return originalPost.call(this,m,...args)}");
  wait('slowProbe.waiting');
  browser('press','Enter'); idle();
  assert.match(evaluate("document.querySelector('#selected-name').textContent"),/APPLE/i);
  evaluate('Worker.prototype.postMessage=originalPost;focusObserver.disconnect()');
  click('#ticker-toggle'); idle();
  click('[data-control="951"]'); idle();
  // Existing saves may already contain this entry (the action toggles it).
  if (!evaluate("document.querySelector('#watchlist')?.textContent.includes('AAPL')")) {click('[data-control="951"]');idle();}
  assert.match(evaluate("document.querySelector('#watchlist').textContent"),/AAPL.*APPLE/i);
  click('#watchlist button'); idle();
  click('[data-players]');wait("!!document.querySelector('#players')");button('Close');idle();
  click('[data-companies]');wait("!!document.querySelector('#companies')");button('Close');idle();
  click('[data-control="114"]');
  wait("document.querySelector('#dlg .chart .hlc-bar line') !== null");
  assert.ok(evaluate("[...document.querySelectorAll('.chart .hlc-bar line')].some(p=>p.getAttribute('y1')!==p.getAttribute('y2'))"),'The chart must render the engine history');
  browser('hover','.chart svg');
  wait("document.querySelector('.lc-tooltip-container') !== null");
  assert.match(evaluate("document.querySelector('.lc-tooltip-container').textContent"),/High|Month-end close|Low/);
  button('Close'); idle();
  click('[data-control="102"]'); button('Diagram of Holdings');
  wait("document.querySelector('#dlg .ownership') !== null");
  assert.ok(evaluate("document.querySelectorAll('.ownership rect').length")>0);
  button('Close'); button('CLOSE'); idle();
  click('[data-control="107"]'); button('Advanced Option Trading');
  wait("document.querySelector('.option-grid') !== null");
  assert.equal(evaluate("document.querySelectorAll('[data-grid]').length"),32);
  assert.equal(evaluate("getComputedStyle(document.querySelector('#dlg')).backgroundColor"),'rgb(23, 43, 67)');
  assert.ok(evaluate("[...document.querySelectorAll('#dlg-buttons button')].every(b=>b.getBoundingClientRect().bottom <= Math.min(innerHeight,document.querySelector('#dlg').getBoundingClientRect().bottom)-4)"),'Option buttons must fit inside the dialog');
  const bounds = evaluate("document.querySelector('[data-option-bounds]').textContent");
  const strike = bounds.match(/\(([\d,.]+) to/)[1].replaceAll(',','');
  const expiry = bounds.match(/Exp\. Date: (\d+\/\d+)/)[1];
  browser('select','[aria-label="Line 1 buy or sell"]','B');
  fill('[aria-label="Line 1 strike"]',strike);
  fill('[aria-label="Line 1 expiry"]',expiry);
  fill('[aria-label="Line 1 quantity"]','1');
  button('Help');
  assert.ok(evaluate("document.querySelectorAll('.documentation details').length")>0);
  click('.documentation [data-close]');
  assert.equal(evaluate("document.querySelector('[aria-label=\"Line 1 expiry\"]').value"),expiry);
  // Quick validation notices leave the sheet usable, without a blocking OK dialog.
  fill('[aria-label="Line 1 expiry"]','1/1900');
  evaluate("window.currentGrid=document.querySelector('.option-grid')");
  button('Calculate');
  wait("document.querySelector('.option-grid') !== currentGrid && !!document.querySelector('#dlg')");
  assert.match(evaluate("document.querySelector('#notices').textContent"),/expiration.*out of allowable range/i);
  assert.equal(evaluate("document.querySelector('[data-sonner-toaster]').dataset.sonnerTheme"),'dark');
  assert.equal(evaluate("document.querySelector('[aria-label=\"Line 1 strike\"]').value"),strike);
  fill('[aria-label="Line 1 expiry"]',expiry);
  evaluate("window.currentGrid=document.querySelector('.option-grid')");
  button('Calculate');
  wait("document.querySelector('.option-grid') !== currentGrid && !!document.querySelector('#dlg')");
  assert.notEqual(evaluate("document.querySelector('.option-grid tbody tr').lastElementChild.textContent"),'—');
  button('Clear all'); button('Close'); button('CLOSE'); idle();
  // Help/manual works without sending a Windows program request.
  click('#help'); fill('.documentation [data-search]','margin');
  assert.ok(evaluate("document.querySelectorAll('.documentation details').length")>0);
  click('.documentation [data-mode="glossary"]'); fill('.documentation [data-search]','bond');
  assert.match(evaluate("document.querySelector('.documentation [data-content]').textContent"),/bond/i);
  click('.documentation [data-mode="manual"]');
  assert.ok(evaluate("document.querySelectorAll('.documentation .manual img[src^=\"blob:\"]').length")>0);
  assert.equal(evaluate("document.querySelectorAll('.documentation script,.documentation iframe,[onclick]').length"),0);
  click('.documentation [data-close]');
  click('#customizer'); fill('.customizer [data-search]','CMB');
  assert.equal(evaluate("document.querySelectorAll('.customizer [name=country] option').length"),54);
  fill('.customizer [name=name]','Browser Regression Bank');
  fill('.customizer [name=symbol]','FDIC'); click('.customizer [type=submit]');
  assert.match(evaluate("document.querySelector('.customizer [data-status]').textContent"),/reserved/);
  fill('.customizer [name=symbol]','ZTEST'); click('.customizer [type=submit]');
  browser('press','Escape');
  assert.ok(evaluate("!!document.querySelector('.customizer [data-discard]')"),'Escape must keep unsaved company edits open');
  click('.customizer [data-save]');
  wait("document.querySelector('.customizer [data-status]').textContent.startsWith('Saved.')");
  click('.customizer [data-close]');
  click('#save-game');
  // Some games ask to confirm the slot or a cheat-mode save.
  for (let i=0;i<5 && evaluate("!!document.querySelector('#dlg')");i++) {
    const labels = evaluate("[...document.querySelectorAll('#dlg-buttons button')].map(b=>b.textContent)");
    if (labels.includes('Yes')) button('Yes'); else button('OK');
  }
  idle();
  assert.ok(evaluate("[...document.querySelectorAll('#saved-games option')].some(o=>/^GAME\\d{2}\\.DAT$/.test(o.value))"));
  browser('open',url); wait("!document.querySelector('#customizer').disabled");
  click('#customizer'); fill('.customizer [data-search]','ZTEST');
  browser('select','.customizer [data-list]','11');
  assert.equal(evaluate("document.querySelector('.customizer [name=name]').value"),'Browser Regression Bank');
  click('.customizer [data-restore]'); click('.customizer [data-save]');
  wait("document.querySelector('.customizer [data-status]').textContent.startsWith('Saved.')");
  click('.customizer [data-close]'); click('#resume'); idle();
  assert.equal(evaluate("document.querySelector('#desk').hidden"),false);
  // Reopening the installer preserves saves and templates already in IndexedDB.
  const savedNames = evaluate("[...document.querySelectorAll('#saved-games option')].map(o=>o.value)");
  click('#menus [data-slot=menubar-trigger]:first-child');click('[data-files-menu]');
  browser('upload','#installer',resolve(installer)); idle();
  assert.deepEqual(evaluate("[...document.querySelectorAll('#saved-games option')].map(o=>o.value)"),savedNames);
  assert.equal(evaluate("document.querySelector('#error').hidden"),true);
  // Build a new game using the edited future-game template.
  click('#customizer');
  fill('.customizer [name=name]','Browser Regression Bank');
  fill('.customizer [name=symbol]','ZTEST');
  click('.customizer [type=submit]');click('.customizer [data-save]');
  wait("document.querySelector('.customizer [data-status]').textContent.startsWith('Saved.')");
  click('.customizer [data-close]');
  browser('open',url);wait("!document.querySelector('#new-game').disabled");click('#new-game');
  for(let i=0;i<20;i++){
    wait("!!document.querySelector('#dlg') || !document.querySelector('#installer').disabled");
    if(!evaluate("!!document.querySelector('#dlg')"))break;
    const title=evaluate("document.querySelector('#dlg-title').textContent");
    evaluate("window.previousDialog=document.querySelector('#dlg')");
    if(evaluate("!!document.querySelector('#dlg-input')")){
      const prompt=evaluate("document.querySelector('[for=dlg-input]').textContent");
      const value=title==='Number of players'?'2':prompt.startsWith('Player 1')?'Browser Alice':prompt.startsWith('Player 2')?'Browser Bobby':prompt.startsWith('Game Length')?'1':prompt.startsWith('Difficulty')?'2':prompt.startsWith('Starting Money')?'500':null;
      if(value!==null)fill('#dlg-input',value);
      button('OK');
    }else{
      const labels=evaluate("[...document.querySelectorAll('#dlg-buttons button')].map(b=>b.textContent)");
      button(title.endsWith('Number of Players?')?'No':labels.includes('Yes')?'Yes':'OK');
    }
    wait("document.querySelector('#dlg') !== previousDialog && (!!document.querySelector('#dlg') || !document.querySelector('#installer').disabled)");
  }
  idle();
  if (evaluate("document.querySelector('.wsr-clock')?.dataset.running === 'true'")) { click('#ticker-toggle'); settleEarnings(); }
  click('[data-players]');wait("!!document.querySelector('#players')");
  assert.match(evaluate("document.querySelector('#players').textContent"),/Browser Alice.*Browser Bobby/s);
  button('Close');idle();
  fill('#symbol','ZTEST');browser('press','Enter');settleEarnings();
  assert.equal(evaluate("document.querySelector('#selected-name').textContent"),'Browser Regression Bank');
  // Restoring the future-game template must leave this game's custom company intact.
  click('#customizer');click('.customizer [data-restore]');click('.customizer [data-save]');
  wait("document.querySelector('.customizer [data-status]').textContent.startsWith('Saved.')");
  click('.customizer [data-close]');
  fill('#symbol','ZTEST');browser('press','Enter');settleEarnings();
  assert.equal(evaluate("document.querySelector('#selected-name').textContent"),'Browser Regression Bank');
  console.log('Browser: dark-only theme under light system appearance, complete 1366×768 desk, secondary research lists, live typing, queued submit, watchlist, chart/tooltips, ownership, option calculation/help, local docs, Customizer validation/persistence/restore, save/reload, installer preservation and customized new-game startup passed.');
} catch (err) {
  console.error(evaluate("({dialog:document.querySelector('#dlg')?document.querySelector('#dlg').textContent:'',customizer:document.querySelector('.customizer [data-status]')?.textContent,error:document.querySelector('#error:not([hidden])')?.textContent})"));
  throw err;
} finally { browser('close'); }
