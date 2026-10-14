// D13: placement/navigation only. TaskQ owns rendering and revision-bound card state.
const boardCards = new Map();
let boardFilter = null;
const boardId = name => 'board-' + encodeURIComponent(name);
const questionId = (name, n) => boardId(name) + '-q-' + n;
function renderBoards() {
  const root = document.getElementById('boards');
  if (!root) return;
  const boards = JSON.parse(root.dataset.boards), blocks = root.querySelector('.board-blocks');
  const projects = boards.map(b => ({id: b.project, name: b.project, board: {kind: 'github', url: b.url},
    complete: b.source.state === 'ok' && !b.source.incomplete && Array.isArray(b.tasks), closed: b.counts.closed, tasks: b.tasks || []}));
  const open = new Map([...blocks.querySelectorAll('details[id]')].map(d => [d.id, d.open]));
  const kept = new Set(), fragments = document.createDocumentFragment();
  const aggregate = document.createElement('div');
  aggregate.innerHTML = TaskqProjectBlocks.aggregate(projects);
  fragments.append(aggregate);
  for (const [index, p] of projects.entries()) {
    const b = boards[index], wrap = document.createElement('div');
    wrap.id = boardId(p.id);
    wrap.innerHTML = TaskqProjectBlocks.summary(p);
    const freshness = document.createElement('p');
    freshness.className = 'detail';
    freshness.textContent = b.source.state + ' · ' + (b.source.age_s == null ? 'age unknown' : b.source.age_s + ' s old') + (b.source.reason ? ' · ' + b.source.reason : '');
    wrap.append(freshness);
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
            capability: () => ({answer: false, reason: 'Answer adapter has not been qualified; answer on the board.'}),
            send: () => Promise.reject(new Error('Answering is disabled'))
          })};
          boardCards.set(id, card);
        }
        wrap.append(card.el);
        card.widget.update([{board: p.id, row}]);
      }
      if (boardFilter && (!boardFilter.project || boardFilter.project === p.id) && row.state !== boardFilter.state) continue;
      const el = document.createElement('div');
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
      empty.textContent = boardFilter?.state === 'closed' ? 'Closed total is unknown; open the board.' : 'No matching tasks observed.';
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
  // Stable IDs let the page refresh restore keyboard focus after moving the retained container.
  for (const group of blocks.children) {
    group.querySelectorAll('button,a,summary,textarea').forEach((e, i) => { if (!e.id) e.id = (group.id || 'boards-all') + '-control-' + i; });
  }
  root.querySelector('.board-fallback').hidden = true;
}
document.addEventListener('click', e => {
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
