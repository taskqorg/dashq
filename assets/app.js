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
    const version = document.querySelector('.version-select[aria-pressed="true"]')?.getAttribute('aria-controls');
    const outcome = document.querySelector('[data-result-outcome][aria-expanded="true"]')?.dataset.resultOutcome;
    const active = document.activeElement;
    const focus = active?.id;
    const selection = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement
      ? [active.selectionStart, active.selectionEnd, active.selectionDirection] : null;
    // Keep editable drafts, never copy hidden authority/CSRF fields or stale enabled state.
    const drafts = new Map([...document.querySelectorAll('main input[id]:not([type=hidden]):not([type=file]):not([type=password]), main textarea[id], main select[id]')]
      .map(e => [e.id, { value: e.value, checked: e.checked }]));
    const y = window.scrollY;
    // overview.md: the final number at once, a brief local tint where it changed; row and Attention swap together
    // board counters tint in renderBoards()
    const before = new Map([...document.querySelectorAll('main [data-n]')].map(e => [e.id, e.dataset.n]));
    main.querySelectorAll('[data-n]').forEach(e => { if (before.has(e.id) && before.get(e.id) !== e.dataset.n) e.classList.add('tint'); });
    DashqResources.preserve(main);
    const oldBoards = document.getElementById('boards'), newBoards = main.querySelector('#boards');
    if (oldBoards && newBoards) {
      oldBoards.dataset.boards = newBoards.dataset.boards;
      newBoards.replaceWith(oldBoards);
    }
    document.getElementById('main').replaceWith(main);
    renderBoards();
    main.querySelectorAll('details').forEach(d => { if (open.has(key(d))) d.open = open.get(key(d)); });
    if (selected) selectConnection(selected);
    if (version) selectVersion(version);
    if (outcome) selectOutcome(outcome);
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
    DashqResources.bind();
    banner.textContent = 'Refresh failed (' + e.message + '), showing data from ' + shown + '.';
    banner.hidden = false;
  } finally {
    busy = false;
  }
}
function bind() {
  document.getElementById('refresh')?.addEventListener('click', refresh);
  document.querySelectorAll('.aq-edit').forEach(box => { aqRender(box); aqLoad(box); });
  DashqResources.bind();
}
// agentq-settings.md G1–G3 (#63): the host plans the draft, review shows its exact diff, apply confirms that plan,
// results come from the host and its readback. Per installation: CSRF, operations, the reviewed plan, a pending apply ID.
const aq = new Map();
const AQ = { planning: 'Asking the host for a plan…', planned: 'Plan ready for review', accepted: 'Accepted · host configuration not yet confirmed',
  configured: 'Configured · host readback matches; reload the client if the plan says so', verified: 'Verified · a new client turn confirmed it',
  rejected: 'Rejected · nothing changed, draft kept', partial: 'Partial · some profiles changed', unknown: 'Unknown · reconcile before any new change' };
const aqState = inst => { if (!aq.has(inst)) aq.set(inst, { ops: [] }); return aq.get(inst); };
const aqBox = inst => [...document.querySelectorAll('.aq-edit')].find(b => b.dataset.installation === inst);
const aqDraft = inst => [...document.querySelectorAll('select[data-installation]')].filter(e => e.dataset.installation === inst && e.value)
  .map(e => ({ profile: e.dataset.profile, field: e.dataset.field, value: e.value }));
const aqId = () => [...crypto.getRandomValues(new Uint8Array(16))].map(b => b.toString(16).padStart(2, '0')).join('');
function aqEl(tag, text, cls) { const e = document.createElement(tag); if (text) e.textContent = text; if (cls) e.className = cls; return e; }
async function aqCall(inst, path, body) {
  const init = body ? { method: 'POST', headers: { 'content-type': 'application/json', 'x-dashq-csrf': aqState(inst).csrf || '' }, body: JSON.stringify(body) } : {};
  const r = await fetch(path, { cache: 'no-store', ...init });
  return [r.status, await r.json().catch(() => ({ reason: 'HTTP ' + r.status }))];
}
function aqKeep(inst, op) {
  const s = aqState(inst);
  s.ops = [op, ...s.ops.filter(o => o.operation !== op.operation)];
}
async function aqLoad(box) {
  const inst = box.dataset.installation, s = aqState(inst);
  try {
    const [st, v] = await aqCall(inst, '/agentq/configure?installation=' + encodeURIComponent(inst));
    if (st !== 200) throw new Error(v.reason);
    Object.assign(s, { csrf: v.csrf, ops: v.operations, error: null });
  } catch (e) { s.error = 'Status unavailable (' + e.message + '); showing the last known.'; }
  const b = aqBox(inst);
  if (b) aqRender(b);
}
// re-read the same operation every 2 s for at most 60 s while the host has not answered; never a new request
async function aqWatch(inst, id) {
  for (let i = 0; i < 30; i++) {
    await new Promise(r => setTimeout(r, 2000));
    const [st, v] = await aqCall(inst, '/agentq/configure/' + id).catch(() => [0, {}]);
    if (st === 200) aqKeep(inst, v);
    const b = aqBox(inst);
    if (b) aqRender(b);
    if (st === 200 && !(v.status === 'planning' || (v.status === 'accepted' && v.state !== 'resulted'))) return;
  }
}
async function aqSubmit(inst, body) {
  const s = aqState(inst);
  try {
    const [st, v] = await aqCall(inst, '/agentq/configure', body);
    if (st !== 202) { s.error = 'Refused: ' + (v.reason || st) + '. Draft kept.'; return null; }
    s.error = null;
    aqKeep(inst, v);
    aqWatch(inst, v.operation);
    return v;
  } catch (e) {
    // a lost reply: the same operation ID is sent again on retry, never a new one
    s.error = 'No reply (' + e.message + '); retry sends the same request.';
    return null;
  } finally { const b = aqBox(inst); if (b) aqRender(b); }
}
document.addEventListener('click', async e => {
  const btn = e.target.closest('.aq-edit [data-aq]');
  if (!btn) return;
  const box = btn.closest('.aq-edit'), inst = box.dataset.installation, s = aqState(inst), act = btn.dataset.aq;
  if (act === 'review') {
    const changes = aqDraft(inst);
    if (!changes.length) { s.error = 'No change in the draft.'; return aqRender(box); }
    s.planId = s.planId && JSON.stringify(s.planChanges) === JSON.stringify(changes) ? s.planId : aqId();
    s.planChanges = changes;
    const v = await aqSubmit(inst, { installation: inst, operation: s.planId, changes });
    if (v) { s.review = v.operation; s.planId = null; }
  } else if (act === 'apply') {
    s.applyId = s.applyId || aqId();
    const v = await aqSubmit(inst, { installation: inst, operation: s.applyId, plan: s.review });
    if (v) { s.applyId = null; s.review = null; }
  } else if (act === 'check') {
    aqWatch(inst, btn.dataset.op);
  }
});
// G1: an edited draft invalidates the confirmation
document.addEventListener('change', e => {
  const inst = e.target.dataset?.installation;
  if (!inst || !e.target.matches('select')) return;
  const s = aqState(inst);
  if (s.review) { s.review = null; s.error = 'Draft changed: review again.'; }
  const b = aqBox(inst);
  if (b) aqRender(b);
});
function aqRender(box) {
  const inst = box.dataset.installation, s = aqState(inst), out = box.querySelector('.aq-out');
  const apply = s.ops.find(o => o.kind === 'apply'), plan = s.ops.find(o => o.operation === s.review);
  // G1: lock the confirmed diff while an apply is unresolved; one mutation per installation
  const busy = apply && (apply.state === 'queued' || apply.state === 'delivered');
  document.querySelectorAll('select[data-installation]').forEach(e => { if (e.dataset.installation === inst) e.disabled = !!busy; });
  box.querySelector('[data-aq=review]').disabled = !!busy;
  out.replaceChildren();
  if (s.error) out.append(aqEl('p', s.error, 'aq-ask'));
  if (plan) out.append(aqPlan(box, plan, busy));
  if (apply) {
    const good = apply.status === 'configured' || apply.status === 'verified';
    const p = aqEl('p', AQ[apply.status] || apply.status, good ? 'aq-good' : apply.status === 'accepted' ? '' : 'aq-ask');
    if (apply.reason) p.append(aqEl('small', 'Reason: ' + apply.reason));
    p.append(aqEl('small', apply.changes.map(c => c.profile + ' ' + c.field + ' → ' + c.value).join(' · ') + ' · operation ' + apply.operation));
    out.append(p);
    if (apply.status === 'unknown' || apply.status === 'partial' || apply.status === 'accepted') {
      const b = aqEl('button', 'Check this operation again');
      Object.assign(b, { type: 'button' });
      b.dataset.aq = 'check'; b.dataset.op = apply.operation;
      out.append(b);
    }
  }
}
function aqPlan(box, op, busy) {
  const d = aqEl('div', '', 'aq-plan');
  if (op.status !== 'planned') {
    d.append(aqEl('p', AQ[op.status] || op.status, op.status === 'planning' ? '' : 'aq-ask'));
    if (op.reason) d.append(aqEl('small', 'Reason: ' + op.reason));
    return d;
  }
  const h = op.host;
  d.append(aqEl('b', 'Host plan · revision ' + h.revision));
  for (const p of h.profiles || []) {
    const row = aqEl('div', '', 'aq-fact');
    row.append(aqEl('span', p.id + (p.aliases?.length ? ' · aliases ' + p.aliases.join(', ') : '')));
    row.append(aqEl('span', (p.changes || []).map(c => c.field + ' ' + c.old + ' → ' + c.new).join(' · ') + (p.reload ? ' · reload needed' : '')));
    d.append(row);
    if (p.conflicts?.length) d.append(aqEl('small', 'Conflicts: ' + p.conflicts.join(', '), 'aq-ask'));
  }
  // a newer snapshot revision invalidates this confirmation (G1, A3)
  const stale = h.revision !== box.dataset.revision, why = !h.writable ? 'Blocked: ' + (h.blocked || []).join(', ')
    : stale ? 'Settings changed since this plan: review again.' : busy ? 'Another change is still unresolved.' : '';
  if (why) d.append(aqEl('p', why, 'aq-ask'));
  const b = aqEl('button', 'Apply this plan');
  Object.assign(b, { type: 'button', disabled: !!why });
  b.dataset.aq = 'apply';
  d.append(b);
  return d;
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
function selectVersion(id) {
  const panel = document.getElementById(id);
  if (!panel?.matches('.version-body')) return;
  document.querySelectorAll('.version-body').forEach(e => { e.hidden = e !== panel; });
  document.querySelectorAll('.version-select').forEach(e => {
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
  const version = e.target.closest('.version-select');
  if (version) selectVersion(version.getAttribute('aria-controls'));
  const row = e.target.closest('.connect-select');
  if (row) selectConnection(row.getAttribute('aria-controls'));
});
renderBoards();
bind();
revealConnection();
addEventListener('hashchange', revealConnection);
refresh();
setInterval(refresh, 60000);

function selectOutcome(outcome) {
  document.querySelectorAll('[data-result-outcome]').forEach(b => {
    const selected = b.dataset.resultOutcome === outcome;
    b.setAttribute('aria-expanded', String(selected));
    const panel = document.getElementById(b.getAttribute('aria-controls'));
    if (panel) panel.hidden = !selected;
  });
}
document.addEventListener('click', e => {
  const b = e.target.closest('[data-result-outcome]');
  if (b) selectOutcome(b.getAttribute('aria-expanded') === 'true' ? null : b.dataset.resultOutcome);
});
document.addEventListener('change', e => {
  const key = e.target.dataset.resultsFilter;
  if (!key) return;
  const url = new URL(location.href);
  if (e.target.value) url.searchParams.set(key, e.target.value); else url.searchParams.delete(key);
  url.hash = 'aqresults';
  location.assign(url);
});
