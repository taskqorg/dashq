// dashq.md D5: fetch /data on load, every 60 s and on Refresh; a failure keeps the last page and shows the banner.
'use strict';
let busy = false;
async function refresh() {
  // a reader's text selection would be lost by the swap: keep it, try again next time
  if (busy || !getSelection().isCollapsed) return;
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
    const selected = document.querySelector('.connect-select[aria-pressed="true"]')?.getAttribute('aria-controls');
    const active = document.activeElement;
    const focus = active?.id;
    const selection = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement
      ? [active.selectionStart, active.selectionEnd, active.selectionDirection] : null;
    // Keep editable drafts, never copy hidden authority/CSRF fields or stale enabled state.
    const drafts = new Map([...document.querySelectorAll('main input[id]:not([type=hidden]):not([type=file]):not([type=password]), main textarea[id], main select[id]')]
      .map(e => [e.id, { value: e.value, checked: e.checked }]));
    const y = window.scrollY;
    // overview.md: the final number at once, a brief local tint where it changed; row and Attention swap together
    const before = new Map([...document.querySelectorAll('main [data-n]')].map(e => [e.id, e.dataset.n]));
    // boards.md hides zero counters: a counter new to a shown board was 0
    const was = e => before.has(e.id) ? before.get(e.id) : e.matches('.c') && document.getElementById(e.closest('[id]:not(.c)')?.id) ? '0' : undefined;
    main.querySelectorAll('[data-n]').forEach(e => { const n = was(e); if (n !== undefined && n !== e.dataset.n) e.classList.add('tint'); });
    document.getElementById('main').replaceWith(main);
    main.querySelectorAll('details').forEach(d => { if (open.has(key(d))) d.open = open.get(key(d)); });
    if (selected) selectConnection(selected);
    drafts.forEach((draft, id) => {
      const e = document.getElementById(id);
      if (e) { e.value = draft.value; if ('checked' in e) e.checked = draft.checked; }
    });
    if (focus) {
      const e = document.getElementById(focus);
      e?.focus({ preventScroll: true });
      if (selection && selection[0] !== null && e?.setSelectionRange) e.setSelectionRange(...selection);
    }
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
  document.getElementById('refresh')?.addEventListener('click', refresh);
}
function selectConnection(id) {
  const panel = document.getElementById(id);
  if (!panel?.matches('.connect-body')) return;
  document.querySelectorAll('.connect-body').forEach(e => { e.hidden = e !== panel; });
  document.querySelectorAll('.connect-select').forEach(e => {
    const selected = e.getAttribute('aria-controls') === id;
    e.setAttribute('aria-pressed', selected);
    e.setAttribute('aria-expanded', selected);
  });
}
function revealConnection() {
  const id = location.hash.slice(1);
  selectConnection(id);
  document.getElementById('select-' + id)?.scrollIntoView();
}
document.addEventListener('click', e => {
  const row = e.target.closest('.connect-select');
  if (row) selectConnection(row.getAttribute('aria-controls'));
});
bind();
revealConnection();
addEventListener('hashchange', revealConnection);
refresh();
setInterval(refresh, 60000);
