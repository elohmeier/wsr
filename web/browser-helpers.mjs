import assert from 'node:assert/strict';

// Supports both synchronous and asynchronous agent-browser wrappers.
export async function acknowledgeStartupQuote(browser) {
  await browser('wait', '--fn', "!!document.querySelector('#dlg[data-acknowledgement]')");
  const { result } = await browser('eval',
    "[...document.querySelectorAll('#dlg button')].map(button => button.textContent.trim())");
  assert.deepEqual(result, ['OK'], 'The automatic startup quote must offer only OK');
  await browser('find', 'role', 'button', 'click', '--name', 'OK', '--exact');
  await browser('wait', '--fn', "!document.querySelector('#dlg')");
}
