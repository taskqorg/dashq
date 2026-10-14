// D14: TaskQ owns rendering and drafts. DashQ owns placement; production has no write adapter.
(function () {
  'use strict';
  const shared = TaskqResources;
  const readonly = {capability: () => ({edit: false, reason: 'Resource operation adapter unavailable; read-only.'}),
    send: async () => ({state: 'rejected', reason: 'Resource operation adapter unavailable'})};
  let active = null;
  const entries = new Map();
  function mount(el, groups, adapter = readonly) {
    const now = () => adapter.now ? adapter.now() : Math.floor(Date.now() / 1000);
    const cap = g => shared.freshness(g, now()).fresh ? adapter.capability(g) : {edit: false, reason: 'Policy observation stale or unknown; read-only.'};
    const summary = document.createElement('div'), open = document.createElement('button');
    const details = document.createElement('details'), label = document.createElement('summary');
    details.append(label, summary);
    open.type = 'button';
    open.textContent = '◎ Work mode…';
    el.replaceChildren(details, open);
    function paint() {
      label.textContent = 'TaskQ policy · ' + groups.length + ' registered projects';
      const source = document.createElement('p');
      source.className = 'detail';
      source.textContent = 'TaskQ admission observations · separate from AgentQ history';
      summary.replaceChildren(source);
      for (const g of groups) {
        const block = document.createElement('div');
        const identity = document.createElement('p');
        const t = g.targets[0].target;
        identity.className = 'detail';
        identity.textContent = g.labels.project + ' · installation ' + t.installation_id + ' · generation ' + t.generation;
        block.innerHTML = shared.render(g, {draft: null}, {edit: false, reason: cap(g).reason}, now());
        summary.append(identity, block);
      }
      open.disabled = !groups.length;
    }
    open.addEventListener('click', () => {
      if (active) active.dialog.close();
      const dialog = document.createElement('dialog');
      dialog.className = 'work-dialog';
      dialog.setAttribute('aria-label', 'TaskQ Work mode');
      const close = document.createElement('button'), note = document.createElement('p'), editor = document.createElement('div');
      close.type = 'button'; close.textContent = 'Close';
      note.className = 'detail';
      dialog.append(close, note, editor);
      document.body.append(dialog);
      const binding = shared.mountWork(editor, {host: groups[0].labels.host, now, capability: cap,
        send: (g, op) => cap(g).edit ? adapter.send(g, op) : Promise.resolve({state: 'rejected', reason: cap(g).reason})});
      active = {owner: el, dialog, update(gs) {
        groups = gs;
        binding.update(gs);
        const denied = gs.find(g => !cap(g).edit);
        note.textContent = 'TaskQ admission observations; separate from AgentQ history. ' + (!gs.length ? 'Policy observation unavailable; read-only.' : denied ? cap(denied).reason || 'Read-only.' : '');
        // The parent retains drafts on refresh; loss of capability must also stop an already-open review.
        editor.inert = binding.editing() && (!gs.length || !!denied);
      }};
      active.update(groups);
      close.addEventListener('click', () => dialog.close());
      dialog.addEventListener('close', () => { if (active?.dialog === dialog) active = null; dialog.remove(); open.focus(); });
      dialog.showModal();
    });
    paint();
    return {update(gs) { groups = gs; paint(); if (active?.owner === el) active.update(gs); }};
  }
  function bind() {
    document.querySelectorAll('.node-policy[data-taskq-groups]').forEach(el => {
      const node = el.closest('.node');
      try {
        const groups = JSON.parse(el.dataset.taskqGroups);
        if (!entries.has(node.id)) entries.set(node.id, {el, binding: mount(el, groups)});
        else entries.get(node.id).binding.update(groups);
        if (groups.length) node.querySelector('.policy')?.setAttribute('hidden', '');
      } catch (_) { el.textContent = 'TaskQ policy unavailable; read-only.'; }
    });
  }
  function preserve(main) {
    for (const [id, entry] of entries) {
      const next = main.querySelector('#' + CSS.escape(id) + ' .node-policy');
      if (next) { entry.el.dataset.taskqGroups = next.dataset.taskqGroups; next.replaceWith(entry.el); }
      else { if (active?.owner === entry.el) active.update([]); entries.delete(id); }
    }
  }
  window.DashqResources = {bind, preserve, mount};
})();
