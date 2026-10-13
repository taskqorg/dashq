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
    document.getElementById('main').replaceWith(main);
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
