// D13: placement/navigation only. TaskQ owns rendering and revision-bound card state.
const boardCards = new Map();
let boardFilter = null;
const boardId = name => 'board-' + encodeURIComponent(name);
const questionId = (name, n) => boardId(name) + '-q-' + n;
// DashQ board rows -> TaskQ project model; a failed or incomplete read is partial, so counters read `?`.
const boardProjects = (boards, host = null) => boards.map(b => ({id: b.project, name: b.project, board: {kind: 'github', url: b.url},
  complete: !host && b.source.state === 'ok' && !b.source.incomplete && Array.isArray(b.tasks), closed: host ? null : b.counts.closed,
  tasks: (b.tasks || []).filter(t => !host || b.attempts?.some(a => a.current && a.task === t.n && a.host_id?.toLowerCase() === host.toLowerCase()))}));
// answers.md Route (#41): the server decides per board and account. GET /answer?board= returns a CSRF token when
// this account may answer there, else the reason (503 while the deployment has no `answer` config). Read once per page.
const answerAccess = new Map();
const answerCapability = board => {
  const a = answerAccess.get(board);
  return a && a.csrf ? {answer: true} : {answer: false, reason: a ? a.reason : 'Checking answer access…'};
};
function answerAccessLoad(board) {
  if (answerAccess.has(board)) return;
  answerAccess.set(board, {reason: 'Checking answer access…'});
  fetch('/answer?board=' + encodeURIComponent(board), {credentials: 'same-origin'})
    .then(r => r.json().then(v => r.ok && v.csrf ? {csrf: v.csrf} : {reason: (v.reason || 'Answers unavailable') + '; answer on the board.'}))
    .catch(e => ({reason: 'Answer access unknown (' + e.message + '); answer on the board.'}))
    .then(a => { answerAccess.set(board, a); for (const c of boardCards.values()) if (c.board === board) c.widget.update(c.rows); });
}
// One TaskQ operation as the route's form; the receipt's `outcome` is the card state. A lost or unreadable reply
// rejects, so the card shows unknown and its retry resends the same op_id and payload.
async function answerSend(board, op) {
  const f = new URLSearchParams({board, csrf: answerAccess.get(board).csrf, task: op.task, revision: op.revision, op_id: op.id});
  if (op.choice != null) f.set('choice', op.choice); else f.set('text', op.text);
  const v = await (await fetch('/answer', {method: 'POST', body: f, credentials: 'same-origin'})).json();
  return {state: v.outcome, reason: v.reason, next: v.next};
}
// #179: TaskQ owns offers/rendering/op identity; DashQ only gates and transports them.
const taskControlAccess = new Map(), taskControlRows = new Map();
function taskControlAccessLoad(board) {
  if (taskControlAccess.has(board)) return;
  taskControlAccess.set(board, {reason: 'Checking task control access…'});
  fetch('/task/control?board=' + encodeURIComponent(board), {credentials: 'same-origin', signal: AbortSignal.timeout(10000)})
    .then(r => r.json().then(v => r.ok && v.csrf ? {csrf: v.csrf} : {reason: v.reason || 'Task controls unavailable'}))
    .catch(() => ({reason: 'Task control access unknown'}))
    .then(a => { taskControlAccess.set(board, a); updateTaskControls(); });
}
function taskControlReason(c) {
  const a = taskControlAccess.get(c.board);
  if (!a?.csrf || a.denied) return a?.reason || 'Checking task control access…';
  if (c.source.state !== 'ok' || c.source.incomplete) return 'Board observation ' + c.source.state + '; current controls unavailable';
  if (!Number.isSafeInteger(c.row.revision) || c.row.revision < 0 || !Array.isArray(c.row.controls)) return 'Canonical controls or revision unavailable';
  if (c.sending || c.receipt?.outcome === 'unknown') return 'Check delivery before another operation';
  if (c.op && ['accepted', 'stale'].includes(c.receipt?.outcome) && c.row.revision === c.op.revision) return 'Waiting for a fresh board revision';
  return '';
}
function updateTaskControls() {
  for (const [id, c] of taskControlRows) {
    const el = document.getElementById(id);
    if (!el) continue;
    const reason = taskControlReason(c), a = taskControlAccess.get(c.board);
    const field = el.querySelector('fieldset');
    field.disabled = !!reason;
    el.querySelector('[data-control-reason]').textContent = reason;
    el.querySelector('[role="status"]').textContent = c.sending ? 'Sending…' : c.receipt ?
      c.receipt.outcome + (c.receipt.reason ? ': ' + c.receipt.reason : '') + (c.receipt.next ? ' · ' + c.receipt.next : '') : '';
    const retry = el.querySelector('[data-control-retry]');
    retry.hidden = c.receipt?.outcome !== 'unknown';
    retry.disabled = c.sending || !a?.csrf;
  }
}
async function taskControlSend(c) {
  if (c.sending) return;
  const uncertain = c.receipt?.outcome === 'unknown';
  c.sending = true; updateTaskControls();
  try {
    const form = new URLSearchParams({board: c.board, csrf: taskControlAccess.get(c.board).csrf,
      task: c.op.task, revision: c.op.revision, op_id: c.op.id, action: c.op.action});
    const response = await fetch('/task/control', {method: 'POST', body: form, credentials: 'same-origin', signal: AbortSignal.timeout(75000)});
    const v = await response.json();
    const matches = v.task === c.op.task && v.op_id === 'dashq.control.' + c.op.id;
    if (!response.ok && v.op_id === null && v.outcome === 'rejected') {
      taskControlAccess.set(c.board, {...taskControlAccess.get(c.board), denied: true, reason: v.reason || 'Task controls refused'});
    } else if (matches) {
      taskControlAccess.set(c.board, {csrf: taskControlAccess.get(c.board).csrf});
    }
    c.receipt = matches && ['accepted', 'stale', 'rejected', 'unknown'].includes(v.outcome) ? v
      : !response.ok && v.outcome === 'rejected' && v.op_id === null ? {outcome: uncertain ? 'unknown' : 'rejected', reason: v.reason}
      : {outcome: 'unknown', reason: 'No matching receipt; retry the same operation'};
  } catch (_) { c.receipt = {outcome: 'unknown', reason: 'Delivery uncertain; retry the same operation'}; }
  c.sending = false; updateTaskControls();
}
function renderTaskControls(el, board, row, source) {
  const id = el.id + '-controls';
  let c = taskControlRows.get(id);
  if (!c) { c = {board}; taskControlRows.set(id, c); }
  Object.assign(c, {row, source});
  const host = document.createElement('div'); host.id = id; host.className = 'task-control-adapter';
  const field = document.createElement('fieldset');
  field.setAttribute('aria-label', 'Task controls #' + row.n);
  field.setAttribute('aria-describedby', id + '-reason');
  field.innerHTML = TaskqTaskControls.render({...row, controls: Array.isArray(row.controls) ? row.controls : []}, {key: id, url: row.url});
  const reason = document.createElement('p'); reason.id = id + '-reason'; reason.dataset.controlReason = ''; reason.className = 'detail';
  const status = document.createElement('p'); status.setAttribute('role', 'status');
  const retry = document.createElement('button'); retry.type = 'button'; retry.dataset.controlRetry = id; retry.textContent = 'Check delivery';
  host.append(field, reason, status, retry); el.append(host);
  taskControlAccessLoad(board);
}

function controlKey(e) {
  const d = e.dataset;
  if (d.control) return 'control-' + d.control;
  if (d.controlRetry) return 'check-delivery';
  if (d.tqp) return d.tqp + (d.state ? '-' + d.state : '');
  if (d.qac) return d.qac + (d.choice ? '-' + d.choice : '');
  if (e.tagName === 'SUMMARY') return 'summary' + (e.parentElement.className ? '-' + e.parentElement.className : '');
  if (e.tagName === 'A') return 'link-' + encodeURIComponent(e.getAttribute('href') || '');
  return e.tagName.toLowerCase();
}
function renderBoards() {
  if (typeof TaskqTaskWorkspace !== 'undefined') return renderTaskWorkspace();
  const root = document.getElementById('boards');
  if (!root) return;
  const boards = JSON.parse(root.dataset.boards), blocks = root.querySelector('.board-blocks');
  const host = new URLSearchParams(location.search).get('host');
  const projects = boardProjects(boards, host);
  const open = new Map([...blocks.querySelectorAll('details[id]')].map(d => [d.id, d.open]));
  // overview.md: a counter whose value changed gets a brief local tint; a counter new to a shown board was 0 (hidden)
  const counted = new Map([...blocks.querySelectorAll('.tqp-counts button[id]')].map(b => [b.id, b.textContent.trim()]));
  const shownBefore = new Set([...blocks.children].map(e => e.id));
  const kept = new Set(), fragments = document.createDocumentFragment();
  const aggregate = document.createElement('div');
  aggregate.innerHTML = TaskqProjectBlocks.aggregate(projects);
  fragments.append(aggregate);
  aggregate.id = 'boards-all';
  if (host) {
    const scope = document.createElement('p'), all = document.createElement('a');
    scope.textContent = 'Host: ' + host + ' · observed current attempts only; assigned queue unknown. ';
    const url = new URL(location.href); url.searchParams.delete('host'); url.hash = '';
    all.href = '/projects' + url.search; all.dataset.viewLink = ''; all.textContent = 'All hosts & unassigned';
    scope.append(all); aggregate.append(scope);
  }
  const coverage = document.createElement('p');
  coverage.className = 'detail';
  coverage.textContent = 'Current coverage: ' + projects.filter(p => p.complete).length + ' of ' + projects.length + ' boards fresh and complete. Unavailable boards are excluded from current totals.';
  aggregate.append(coverage);
  for (const [index, p] of projects.entries()) {
    const b = boards[index], wrap = document.createElement('div');
    wrap.id = boardId(p.id);
    wrap.innerHTML = TaskqProjectBlocks.summary(p);
    const freshness = document.createElement('p');
    freshness.className = 'detail';
    freshness.textContent = b.source.state + ' · ' + (b.source.age_s == null ? 'age unknown' : b.source.age_s + ' s old') + (b.source.reason ? ' · ' + b.source.reason : '');
    wrap.append(freshness);
    // v4.6.30 no longer counts Closed; its authoritative coverage remains on the board.
    const closed = document.createElement('a');
    closed.href = b.url + '?q=is%3Aissue+is%3Aclosed'; closed.textContent = 'Closed on board ↗';
    wrap.append(closed);
    if (!p.complete && b.source.at != null) {
      const last = document.createElement('details');
      last.id = wrap.id + '-last-known';
      last.open = open.get(last.id) || false;
      last.innerHTML = '<summary>Last-known counts · not current</summary>' + TaskqProjectBlocks.counters(b.counts, p.id, p.name + ' last known');
      wrap.append(last);
    }
    const tasks = document.createElement('details');
    tasks.id = wrap.id + '-tasks';
    tasks.open = open.has(tasks.id) ? open.get(tasks.id) : index === 0;
    const summary = document.createElement('summary');
    summary.textContent = 'Tasks' + (boardFilter && (!boardFilter.project || boardFilter.project === p.id) ? ' · ' + boardFilter.state : '');
    tasks.append(summary);
    let shown = 0;
    for (const row of p.tasks) {
      const question = row.state === 'ask' || (row.state === 'review' && row.question);
      if (question) {
        const id = questionId(p.id, row.n);
        kept.add(id);
        let card = boardCards.get(id);
        if (!card) {
          const el = document.createElement('div');
          el.id = id;
          card = {el, board: p.id, widget: TaskqAnswerCard.mount(el, {
            capability: board => card.source.state === 'ok' && !card.source.incomplete ? answerCapability(board)
              : {answer: false, reason: 'Board observation ' + card.source.state + ': ' + (card.source.reason || 'current revision unavailable') + '; answer on the board.'},
            send: answerSend
          })};
          boardCards.set(id, card);
        }
        card.source = b.source;
        wrap.append(card.el);
        card.rows = [{board: p.id, row}];
        card.widget.update(card.rows);
        answerAccessLoad(p.id);
      }
      if (boardFilter && (!boardFilter.project || boardFilter.project === p.id) && row.state !== boardFilter.state) continue;
      const el = document.createElement('div');
      el.id = wrap.id + '-task-' + row.n;
      el.innerHTML = TaskqProjectBlocks.task(p, row);
      // TaskQ R6 3 (#1565): an off-host session's progress from its host's record; TaskQ links here, never codex:// off host
      if (row.session) {
        const s = row.session, progress = document.createElement('p');
        progress.className = 'detail';
        progress.textContent = 'Progress on ' + s.host + ' (' + s.runtime + '): ' + s.stage + ' · ' + (s.age_s == null ? 'update age unknown' : 'updated ' + s.age_s + ' s ago');
        el.append(progress);
      }
      const failed = b.failed?.find(f => f.number === row.n);
      if (failed) {
        const reason = document.createElement('p');
        reason.textContent = failed.reason || 'No failure reason recorded.';
        el.append(reason);
      }
      renderTaskControls(el, p.id, row, b.source);
      tasks.append(el);
      shown++;
    }
    if (!shown) {
      const empty = document.createElement('p');
      empty.textContent = host ? 'No matching current attempt evidence; host assignment and queue coverage are unknown.' : boardFilter?.state === 'closed' ? 'Closed total is unknown; open the board.' : p.complete ? 'No matching tasks observed.' : b.source.at == null ? 'No board observation received yet.' : 'No matching tasks in the retained observation; current state unknown.';
      tasks.append(empty);
    }
    wrap.append(tasks);
    if (b.attempts?.length && !host) {
      const live = document.createElement('p');
      live.className = 'detail'; live.textContent = 'Observed attempts (heartbeat freshness applies): ' + b.attempts.map(a => a.host + ' #' + a.task).join(' · ');
      wrap.append(live);
    }
    fragments.append(wrap);
  }
  for (const id of boardCards.keys()) if (!host && !kept.has(id)) boardCards.delete(id);
  blocks.replaceChildren(fragments);
  // The page refresh restores focus and drafts by ID. A control ID names its board/task/card and its action/state,
  // never its DOM position, so rows added or removed by a refresh cannot move focus to another control.
  const used = new Set();
  blocks.querySelectorAll('button,a,summary,textarea').forEach(e => {
    if (e.id) return;
    const base = e.parentElement.closest('[id]').id + '-' + controlKey(e);
    let id = base;
    for (let n = 2; used.has(id); n++) id = base + '-' + n;
    used.add(id);
    e.id = id;
  });
  blocks.querySelectorAll('.tqp-counts button').forEach(b => {
    const was = counted.has(b.id) ? counted.get(b.id) : shownBefore.has(b.parentElement.closest('[id]').id) ? '0' : undefined;
    if (was !== undefined && was !== b.textContent.trim()) b.classList.add('tint');
  });
  root.querySelector('.board-fallback').hidden = true;
  updateTaskControls();
}
if (typeof module === 'object') module.exports = {boardProjects};
else document.addEventListener('click', e => {
  const action = e.target.closest('[data-a="control"],[data-control-retry]');
  if (action) {
    const c = taskControlRows.get(action.dataset.key || action.dataset.controlRetry);
    if (!c || c.sending) return;
    if (action.dataset.controlRetry) {
      if (c.receipt?.outcome === 'unknown' && taskControlAccess.get(c.board)?.csrf) taskControlSend(c);
    } else if (!taskControlReason(c) && c.row.controls.includes(action.dataset.control)) {
      const prefix = Array.from(crypto.getRandomValues(new Uint8Array(8)), b => b.toString(16).padStart(2, '0')).join('');
      c.op = TaskqTaskControls.op(c.row, action.dataset.control, prefix);
      c.receipt = null; taskControlSend(c);
    }
    return;
  }
  const question = e.target.closest('[data-board-question]');
  if (question) {
    if (question.hasAttribute('data-view-link')) return;
    const el = document.getElementById(questionId(question.dataset.boardQuestion, question.dataset.task));
    if (el) { e.preventDefault(); el.scrollIntoView({block: 'center'}); el.querySelector('a')?.focus({preventScroll: true}); }
    return;
  }
  if (!e.target.closest('#boards')) return;
  TaskqProjectBlocks.handle(e, {
    open: url => location.assign(url),
    project: id => { boardFilter = null; renderBoards(); const d = document.getElementById(boardId(id) + '-tasks'); d.open = true; d.querySelector('summary').focus(); },
    tasks: (project, state) => {
      boardFilter = {project, state}; renderBoards();
      document.querySelectorAll('#boards details[id$="-tasks"]').forEach(d => { if (!project || d.id === boardId(project) + '-tasks') d.open = true; });
      document.querySelector('#boards details[open] summary')?.focus();
    }
  });
});
