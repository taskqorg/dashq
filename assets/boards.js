// D13: placement/navigation only. TaskQ owns rendering and revision-bound card state.
const boardCards = new Map();
let boardFilter = null;
const boardId = name => 'board-' + encodeURIComponent(name);
const questionId = (name, n) => boardId(name) + '-q-' + n;
// DashQ board rows -> TaskQ project model; a failed or incomplete read is partial, so counters read `?`.
const boardProjects = boards => boards.map(b => ({id: b.project, name: b.project, board: {kind: 'github', url: b.url},
  complete: b.source.state === 'ok' && !b.source.incomplete && Array.isArray(b.tasks), closed: b.counts.closed, tasks: b.tasks || []}));
function controlKey(e) {
  const d = e.dataset;
  if (d.tqp) return d.tqp + (d.state ? '-' + d.state : '');
  if (d.qac) return d.qac + (d.choice ? '-' + d.choice : '');
  if (e.tagName === 'SUMMARY') return 'summary' + (e.parentElement.className ? '-' + e.parentElement.className : '');
  if (e.tagName === 'A') return 'link-' + encodeURIComponent(e.getAttribute('href') || '');
  return e.tagName.toLowerCase();
}
function renderBoards() {
  const root = document.getElementById('boards');
  if (!root) return;
  const boards = JSON.parse(root.dataset.boards), blocks = root.querySelector('.board-blocks');
  const projects = boardProjects(boards);
  const open = new Map([...blocks.querySelectorAll('details[id]')].map(d => [d.id, d.open]));
  // overview.md: a counter whose value changed gets a brief local tint; a counter new to a shown board was 0 (hidden)
  const counted = new Map([...blocks.querySelectorAll('.tqp-counts button[id]')].map(b => [b.id, b.textContent.trim()]));
  const shownBefore = new Set([...blocks.children].map(e => e.id));
  const kept = new Set(), fragments = document.createDocumentFragment();
  const aggregate = document.createElement('div');
  aggregate.innerHTML = TaskqProjectBlocks.aggregate(projects);
  fragments.append(aggregate);
  aggregate.id = 'boards-all';
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
    if (!p.complete && b.source.at != null) {
      const last = document.createElement('details');
      last.id = wrap.id + '-last-known';
      last.open = open.get(last.id) || false;
      last.innerHTML = '<summary>Last-known counts · not current</summary>' + TaskqProjectBlocks.counters(b.counts, p.id, p.name + ' last known');
      wrap.append(last);
    }
    const tasks = document.createElement('details');
    tasks.id = wrap.id + '-tasks';
    tasks.open = open.get(tasks.id) || (boardFilter && (!boardFilter.project || boardFilter.project === p.id));
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
          card = {el, widget: TaskqAnswerCard.mount(el, {
            capability: () => ({answer: false, reason: card.source.state === 'ok'
              ? 'Answer adapter has not been qualified; answer on the board.'
              : 'Board observation ' + card.source.state + ': ' + (card.source.reason || 'current revision unavailable') + '; answer on the board.'}),
            send: () => Promise.reject(new Error('Answering is disabled'))
          })};
          boardCards.set(id, card);
        }
        card.source = b.source;
        wrap.append(card.el);
        card.widget.update([{board: p.id, row}]);
      }
      if (boardFilter && (!boardFilter.project || boardFilter.project === p.id) && row.state !== boardFilter.state) continue;
      const el = document.createElement('div');
      el.id = wrap.id + '-task-' + row.n;
      el.innerHTML = TaskqProjectBlocks.task(p, row);
      const failed = b.failed?.find(f => f.number === row.n);
      if (failed) {
        const reason = document.createElement('p');
        reason.textContent = failed.reason || 'No failure reason recorded.';
        el.append(reason);
      }
      tasks.append(el);
      shown++;
    }
    if (!shown) {
      const empty = document.createElement('p');
      empty.textContent = boardFilter?.state === 'closed' ? 'Closed total is unknown; open the board.' : p.complete ? 'No matching tasks observed.' : b.source.at == null ? 'No board observation received yet.' : 'No matching tasks in the retained observation; current state unknown.';
      tasks.append(empty);
    }
    wrap.append(tasks);
    if (b.attempts?.length) {
      const live = document.createElement('p');
      live.className = 'detail'; live.textContent = 'Live: ' + b.attempts.map(a => a.host + ' #' + a.task).join(' · ');
      wrap.append(live);
    }
    fragments.append(wrap);
  }
  for (const id of boardCards.keys()) if (!kept.has(id)) boardCards.delete(id);
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
}
if (typeof module === 'object') module.exports = {boardProjects};
else document.addEventListener('click', e => {
  const question = e.target.closest('[data-board-question]');
  if (question) {
    const el = document.getElementById(questionId(question.dataset.boardQuestion, question.dataset.task));
    if (el) { e.preventDefault(); el.scrollIntoView({block: 'center'}); el.querySelector('a')?.focus({preventScroll: true}); }
    return;
  }
  if (!e.target.closest('#boards')) return;
  TaskqProjectBlocks.handle(e, {
    open: url => location.assign(url),
    project: id => { boardFilter = null; renderBoards(); const d = document.getElementById(boardId(id) + '-tasks'); d.open = true; d.querySelector('summary').focus(); },
    tasks: (project, state) => { boardFilter = {project, state}; renderBoards(); document.querySelector('#boards details[open] summary')?.focus(); }
  });
});
