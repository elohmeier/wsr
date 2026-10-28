// The first navigation on a static host installs a worker, then reloads once so
// the document and module workers receive cross-origin isolation headers.
export async function prepareIsolation(): Promise<boolean> {
  const base = new URL(import.meta.env.BASE_URL, location.href);
  const key = 'wsr-isolation-reload:' + base.href;
  if (crossOriginIsolated) {
    try { sessionStorage.removeItem(key); } catch { /* Storage can be unavailable. */ }
    return true;
  }
  if (!isSecureContext || !('serviceWorker' in navigator))
    throw new Error('This host needs HTTPS and service-worker support, or cross-origin isolation response headers.');
  document.getElementById('app')!.textContent = 'Preparing browser isolation…';
  await Promise.race([
    (async () => {
      await navigator.serviceWorker.register(new URL('isolation-worker.js', base), { scope: base.href });
      await navigator.serviceWorker.ready;
      if (!navigator.serviceWorker.controller) await new Promise<void>(resolve => {
        navigator.serviceWorker.addEventListener('controllerchange', () => resolve(), { once: true });
      });
    })(),
    new Promise((_, reject) => setTimeout(() => reject(new Error('Browser isolation setup timed out. Reload normally to retry.')), 15000)),
  ]);
  if (sessionStorage.getItem(key))
    throw new Error('This browser could not enable cross-origin isolation. Use a host with isolation response headers.');
  sessionStorage.setItem(key, '1');
  location.reload();
  return false;
}
