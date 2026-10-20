// D14: TaskQ owns rendering and drafts. DashQ owns placement and, for taskq.project-limits/1 (#159), the transport.
(function () {
  'use strict';
  const shared = TaskqResources;
  const API = '/api/taskq/resources/v1/', PROFILE = 'taskq.project-limits/1', LIFETIME = 300, POLL = 5000;
  const sec = () => Math.floor(Date.now() / 1000);
  const offline = 'Resource operation adapter unavailable; read-only.';
  const readonly = {capability: () => ({edit: false, reason: offline}),
    send: async () => ({state: 'rejected', reason: 'Resource operation adapter unavailable'})};
  const WHY = {unauthorized: 'not signed in, or not your grant', revoked: 'grant revoked or expired', scope: 'outside your project-limits grant',
    unsupported: 'not supported by this host', invalid: 'refused as invalid', conflict: 'settings changed on the host; review again',
    replay_mismatch: 'operation ID reused with other content', busy: 'too many pending changes; try later', clock_unknown: 'host clock unverified',
    expired: 'expired before the host wrote it', cancelled: 'cancelled before the host wrote it', unknown_operation: 'operation unknown here',
    unavailable: 'resource writes are off on this dashboard'};
  const why = (code, status) => WHY[code] || code || 'refused (HTTP ' + status + ')';
  const offers = x => (x.supported || []).some(s => s.policy_schema === PROFILE && (s.fields || []).includes('runtime_limit'));
  // a project-limits group, before or after `view` removed the profiles this actor cannot write
  const limits = g => !!g.limits || g.targets.some(offers);

  // The signed-in actor's grants, re-read at most once a minute; nothing is editable until they arrive.
  let grants = {state: 'none', list: [], csrf: '', reason: 'Checking resource grants…', at: 0};
  const frozen = new Map();
  function covering(t) {
    return grants.list.find(g => g.project_limits && g.installation_id === t.installation_id && g.generation === t.generation &&
      g.project_ids.includes(t.project_id) && g.runtime_ids.includes(t.runtime_id) && g.expires_at > sec());
  }
  function loadGrants() {
    if (grants.state === 'loading' || sec() - grants.at < 60) return;
    grants = {...grants, state: 'loading', at: sec()};
    fetch(API + 'grants', {headers: {accept: 'application/json'}})
      .then(async r => {
        const j = await r.json().catch(() => null);
        grants = r.ok && j && Array.isArray(j.grants) ? {state: 'ok', list: j.grants, csrf: j.csrf, reason: '', at: sec()}
          : {state: 'error', list: [], csrf: '', reason: 'Project limits read-only: ' + why(j && j.reason, r.status) + '.', at: sec()};
      })
      .catch(() => { grants = {state: 'error', list: [], csrf: '', reason: 'Project limits read-only: grants unavailable.', at: sec()}; })
      .finally(() => { for (const e of entries.values()) e.binding.repaint(); });
  }
  const wait = ms => new Promise(r => setTimeout(r, ms));
  async function call(path, body) {
    const init = body ? {method: 'POST', headers: {'content-type': 'application/json', 'x-dashq-csrf': grants.csrf}, body: JSON.stringify(body)} : {};
    const r = await fetch(API + path, init);
    return {status: r.status, ok: r.ok, j: await r.json().catch(() => null)};
  }
  // Production adapter. Only targets advertising the profile under a covering active grant keep an input; the rest
  // show their values read-only. One runtime per confirmation: each write changes the project revision.
  const live = {
    inline: true,
    view(g) {
      if (!g.targets.some(offers)) return g;
      loadGrants();
      return {...g, limits: true, targets: g.targets.map(x => offers(x) && covering(x.target) ? x : {...x, supported: []})};
    },
    capability(g) {
      if (!limits(g)) return {edit: false, reason: offline};
      if (grants.state !== 'ok') return {edit: false, reason: grants.reason};
      return g.targets.some(x => offers(x)) ? {edit: true} : {edit: false, reason: 'No active project-limits grant for this project; read-only.'};
    },
    async send(g, op) {
      if (op.ops.length !== 1) return {state: 'rejected', reason: 'Change one runtime at a time: each limit is its own reviewed write'};
      const o = op.ops[0], grant = covering(o.target);
      if (o.policy_schema !== PROFILE || !grant) return {state: 'rejected', reason: WHY.scope};
      // Check again resends the frozen submit; the op ID binds every field
      if (!frozen.has(op.id)) {
        const now = sec();
        frozen.set(op.id, {schema: 'taskq.resources/1', op_id: op.id, actor_id: grant.actor_id, grant_id: grant.grant_id, target: o.target,
          policy_schema: o.policy_schema, expected_revision: o.expected_revision, issued_at: now, expires_at: now + LIFETIME, changes: o.changes});
      }
      const body = frozen.get(op.id);
      let x = await call('operations', body);
      while (x.ok && x.j && x.j.state === 'pending' && sec() < body.expires_at) {
        await wait(POLL);
        x = await call('operations/' + encodeURIComponent(op.id));
      }
      const rc = x.j || {};
      if (!x.ok) return x.status === 404 ? {state: 'unknown', reason: WHY.unknown_operation} : {state: 'rejected', reason: why(rc.reason, x.status)};
      // the component decides applied from the fresh projection, never from this receipt
      if (rc.state === 'saved' || rc.state === 'applied') return {state: 'saved', saved_revision: rc.saved_revision};
      if (['rejected', 'expired', 'cancelled'].includes(rc.state)) return {state: 'rejected', reason: why(rc.state === 'rejected' ? rc.reason : rc.state)};
      return {state: 'unknown', reason: 'the host has not reported a write yet'};
    },
  };
  let active = null;
  const entries = new Map();
  function mount(el, groups, adapter = live) {
    const now = () => adapter.now ? adapter.now() : Math.floor(Date.now() / 1000);
    const view = g => adapter.view ? adapter.view(g) : g;
    const cap = g => shared.freshness(g, now()).fresh ? adapter.capability(g) : {edit: false, reason: 'Policy observation stale or unknown; read-only.'};
    // Work mode drops saved_revision, so it cannot show this profile applied: project limits are edited inline only
    const work = g => adapter.inline && limits(g) ? {edit: false, reason: 'Change project limits in the list; Work mode cannot confirm them.'} : cap(g);
    const summary = document.createElement('div'), open = document.createElement('button');
    const details = document.createElement('details'), label = document.createElement('summary');
    details.append(label, summary);
    open.type = 'button';
    open.textContent = '◎ Work mode…';
    el.replaceChildren(details, open);
    const source = document.createElement('p');
    source.className = 'detail';
    source.textContent = 'TaskQ admission observations · separate from AgentQ history';
    let blocks = new Map();
    function paint() {
      label.textContent = 'TaskQ policy · ' + groups.length + ' registered projects';
      const next = new Map();
      for (const g of groups) {
        let b = blocks.get(g.key);
        if (!b) {
          const wrap = document.createElement('div'), identity = document.createElement('p'), host = document.createElement('div');
          identity.className = 'detail';
          wrap.append(identity, host);
          // live: the shared single-project editor; an injected fixture adapter keeps the summary read-only
          b = {wrap, identity, binding: shared.mount(host, {now, capability: x => adapter.inline ? cap(x) : {edit: false, reason: cap(x).reason},
            send: (x, op) => adapter.inline && cap(x).edit ? adapter.send(x, op) : Promise.resolve({state: 'rejected', reason: cap(x).reason})})};
        }
        const t = g.targets[0].target;
        b.identity.textContent = g.labels.project + ' · installation ' + t.installation_id + ' · generation ' + t.generation;
        b.binding.update(view(g));
        next.set(g.key, b);
      }
      // reorder only on change, so a focused input is never detached
      if ([...summary.children].slice(1).map(c => c.dataset.key).join('\n') !== [...next.keys()].join('\n')) {
        for (const [k, b] of next) b.wrap.dataset.key = k;
        summary.replaceChildren(source, ...[...next.values()].map(b => b.wrap));
      }
      blocks = next;
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
      const binding = shared.mountWork(editor, {host: groups[0].labels.host, now, capability: work,
        send: (g, op) => work(g).edit ? adapter.send(g, op) : Promise.resolve({state: 'rejected', reason: work(g).reason})});
      active = {owner: el, dialog, update(gs) {
        groups = gs;
        const selected = [...editor.querySelectorAll('[data-rsc-select]:checked')].map(e => e.dataset.rscSelect);
        binding.update(gs.map(view));
        const denied = gs.find(g => selected.includes(g.key) && !work(g).edit);
        const missing = selected.some(key => !gs.some(g => g.key === key));
        note.textContent = 'TaskQ admission observations; separate from AgentQ history. ' + ((!gs.length || missing) ? 'Policy observation unavailable; read-only.' : denied ? work(denied).reason || 'Read-only.' : '');
        // The parent retains drafts on refresh; loss of capability must also stop an already-open review.
        editor.inert = binding.editing() && (!gs.length || missing || !!denied);
      }};
      active.update(groups);
      close.addEventListener('click', () => dialog.close());
      dialog.addEventListener('close', () => { if (active?.dialog === dialog) active = null; dialog.remove(); open.focus(); });
      dialog.showModal();
    });
    paint();
    return {update(gs) { groups = gs; paint(); if (active?.owner === el) active.update(gs); }, repaint() { paint(); if (active?.owner === el) active.update(groups); }};
  }
  function bind() {
    document.querySelectorAll('.node-policy[data-taskq-groups]').forEach(el => {
      const node = el.closest('.node, .connect-body');
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
  window.DashqResources = {bind, preserve, mount, live};
})();
