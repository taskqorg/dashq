// dashq.md D5: fetch /data on load, every 60 s and on Refresh; a failure keeps the last page and shows the banner.
'use strict';
let busy = false;
async function refresh() {
  if (busy) return;
  busy = true;
  const banner = document.getElementById('banner');
  const shown = document.querySelector('main time')?.textContent || 'unknown';
  try {
    const data = await fetch('/data' + location.search, { cache: 'no-store' });
    if (!data.ok) throw new Error('HTTP ' + data.status);
    const page = await fetch('/' + location.search, { cache: 'no-store' });
    if (!page.ok) throw new Error('HTTP ' + page.status);
    const main = new DOMParser().parseFromString(await page.text(), 'text/html').getElementById('main');
    if (!main) throw new Error('invalid page');
    // D10: new values keep open disclosures, focus and reading position
    // a disclosure is keyed by its id, else its nearest id and summary, so added rows do not shift the others
    const key = d => d.id || (d.parentElement.closest('[id]')?.id + ':' + d.querySelector('summary')?.textContent);
    const open = new Map([...document.querySelectorAll('main details')].map(d => [key(d), d.open]));
    const focus = document.activeElement?.id;
    const y = window.scrollY;
    document.getElementById('main').replaceWith(main);
    main.querySelectorAll('details').forEach(d => { if (open.has(key(d))) d.open = open.get(key(d)); });
    if (focus) document.getElementById(focus)?.focus({ preventScroll: true });
    window.scrollTo(0, y);
    bind();
  } catch (e) {
    banner.textContent = 'Refresh failed (' + e.message + '), showing data from ' + shown + '.';
    banner.hidden = false;
  } finally {
    busy = false;
  }
}
function bind() {
  document.getElementById('refresh').addEventListener('click', refresh);
}
bind();
refresh();
setInterval(refresh, 60000);
