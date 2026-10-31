// D13/#186: TaskQ owns workspace rendering/state; DashQ supplies observations and guarded transports.
let taskWorkspace, taskWorkspaceView, pmComposer, pmDraft = {text: ''}, pmAccess = {targets: []}, pmSelected = 0;
const workspaceOps = new Map(), workspaceAppearance = {};
function workspaceState(boards, host = null) {
  const states = TASKQ_STATES.map(([id, name]) => ({id, name}));
  const projects = boardProjects(boards, host).map((p, i) => {
    const b = boards[i];
    return {name: p.name, root: p.id, issues: p.board.url, board_kind: 'github', complete: p.complete,
      alive: b.source.state === 'ok' && !b.source.incomplete, saved: true, focus: [], mode: 'unknown',
      tasks: p.tasks.map(t => ({...t, labels: t.labels || [], host: t.session?.host || t.host, runtime: t.session?.runtime || t.runtime, session_reason: t.session ? 'Progress on '+t.session.host+' ('+t.session.runtime+'): '+t.session.stage+' · '+(t.session.age_s == null?'update age unknown':'updated '+t.session.age_s+'s ago') : undefined, deps: t.deps || [], blocked: t.blocked || [], steps: t.steps || [],
        controls: Number.isSafeInteger(t.revision) && b.source.state === 'ok' && !b.source.incomplete && Array.isArray(t.controls) ? t.controls : [],
        age: t.age_s == null ? 'age unknown' : t.age_s + 's', failed: b.failed?.find(f => f.number === t.n)?.reason || t.failed,
        key: p.board.url + '/' + t.n})),
      counts: states.map(s => ({state: s.id, name: s.name, n: p.complete ? b.counts[s.id] ?? null : null})),
      partial: p.complete ? [] : [b.source.reason || (host ? 'Host queue coverage unknown' : 'Board ' + b.source.state)], open: p.complete ? p.tasks.length : null};
  });
  const partial = projects.filter(p => !p.complete).map(p => p.name);
  return {platform: 'browser', font: parseFloat(getComputedStyle(document.documentElement).fontSize), theme: matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light',
    host: host || 'Configured boards', now: Math.floor(Date.now()/1000), version: '4.6.52', service: {state: 'unknown'},
    projects, display: {states, series: [], notify: [], periods: [], run: false,
      all: {counts: states.map(s => ({state: s.id, name: s.name, n: partial.length || projects.some(p => p.counts.find(c => c.state === s.id)?.n == null) ? null : projects.reduce((n,p) => n + p.counts.find(c => c.state === s.id).n, 0)})), partial, open: partial.length ? null : projects.reduce((n,p) => n+p.tasks.length,0)}},
    ai: {draft: ''}, notify: {projects: {}, host: {}, status: 'unsupported'}, stop_modes: false, messages: {complete: false}};
}
function workspaceNotice(text) { document.getElementById('workspace-notice').textContent = text; }
async function workspaceSend(m) {
  if (m.a === 'answer') {
    const b = JSON.parse(document.getElementById('boards').dataset.boards).find(b => b.project === m.root);
    const control = !!m.op.action, route = control ? '/task/control' : '/answer';
    const key = m.root + '#' + m.op.id;
    const prior = workspaceOps.get(key);
    const unknown = reason => ({state:'unknown',reason,next:'Check delivery with the same operation'});
    let result;
    try {
      if (prior && JSON.stringify(prior.op) !== JSON.stringify(m.op)) throw Error('Operation identity changed');
      if (!prior && (!b || b.source.state !== 'ok' || b.source.incomplete)) result = {state:'rejected',reason:'Current board revision unavailable; nothing was written'};
      else if (control && !['start','stop','later','retry'].includes(m.op.action)) result = {state:'rejected',reason:'This action remains on the board; it is not enabled in DashQ'};
      else {
        const access = await (await fetch(route+'?board='+encodeURIComponent(m.root), {signal:AbortSignal.timeout(10000)})).json();
        if (!access.csrf) result = prior ? unknown(access.reason || 'Access unavailable') : {state:'rejected',reason:access.reason || 'Access unavailable'};
        else {
          const f = new URLSearchParams({board:m.root,csrf:access.csrf,task:m.op.task,revision:m.op.revision,op_id:m.op.id});
          if (control) f.set('action',m.op.action); else if (m.op.choice != null) f.set('choice',m.op.choice); else f.set('text',m.op.text);
          workspaceOps.set(key,{op:structuredClone(m.op)});
          const v = await (await fetch(route,{method:'POST',body:f,signal:AbortSignal.timeout(65000)})).json();
          const expected = (control ? 'dashq.control.' : 'dashq.')+m.op.id;
          result = v.op_id === expected && String(v.task) === String(m.op.task) ? {state:v.outcome,reason:v.reason,next:v.next}
            : unknown(v.reason || 'No matching authoritative receipt');
        }
      }
    } catch(e) { result = unknown(e.message); }
    taskWorkspace.receive({answerReceipt:{root:m.root,id:m.op.id,...result}});
    return;
  }
  if (m.a === 'open') {
    // TaskQ supplies a board/task link, never a command or off-host native URI.
    const u = m.url || m.value;
    const boards = JSON.parse(document.getElementById('boards').dataset.boards);
    if (typeof u === 'string' && boards.some(b => u === b.url || u.startsWith(b.url+'/') || u.startsWith(b.url+'?'))) window.open(u,'_blank','noopener');
    else workspaceNotice('This destination is unavailable here. Use the configured board links below.');
    return;
  }
  if (m.a === 'set' && (m.font || m.theme)) { for(const key of ['font','theme']) if(m[key]) workspaceAppearance[key]=m[key]; taskWorkspaceView.S = {...taskWorkspaceView.S,...workspaceAppearance}; taskWorkspace.draw(); return; }
  if (m.a === 'set' && m.navigation) return;
  if (['ready','size','resize','resize-end','drag','view-state','answer-local'].includes(m.a)) return;
  if (m.a === 'workspace' && m.open?.page === 'answer') return workspaceAnswerOpen(m.open);
  if (m.a === 'workspace' && m.open?.page === 'conversations') {
    workspaceNotice('Send one PM request below; continue the dialogue in its native session.');
    document.getElementById('pm-composer').scrollIntoView({block:'center'}); document.getElementById('pm-draft')?.focus();
    return;
  }
  workspaceNotice('This native TaskQ operation is unavailable in DashQ. Existing answer, task and resource permissions remain separate.');
}
// #187: TaskQ's "Answer in workspace…" asks the host for its native answer page. DashQ mounts the pinned answer card
// (D13) for that one question on the guarded /answer route, keyed by project like the board fallback.
function workspaceAnswerOpen(o) {
  const boards = JSON.parse(document.getElementById('boards').dataset.boards);
  const b = boards.find(b => b.project === o.root) || boards.find(b => b.url === o.board);
  if (!b || !Number.isSafeInteger(o.task)) return workspaceNotice('This question is not on a configured board. Answer on the board.');
  let card = boardCards.get('workspace-answer');
  if (!card) {
    const el = document.getElementById('workspace-answer');
    card = {el, widget: TaskqAnswerCard.mount(el, {
      capability: board => card.source?.state === 'ok' && !card.source.incomplete ? answerCapability(board)
        : {answer: false, reason: 'Board observation ' + (card.source?.state || 'unknown') + '; answer on the board.'},
      send: answerSend})};
    boardCards.set('workspace-answer', card);
  }
  Object.assign(card, {board: b.project, n: o.task, rows: []});
  workspaceAnswerUpdate(boards);
  answerAccessLoad(b.project);
  workspaceNotice('Answer #' + o.task + ' of ' + b.project + ' below; the answer permission stays separate.');
  card.el.scrollIntoView({block: 'center'});
}
function workspaceAnswerUpdate(boards) {
  const card = boardCards.get('workspace-answer'); if (!card) return;
  const b = boards.find(b => b.project === card.board), row = b?.tasks?.find(t => t.n === card.n);
  card.source = b?.source;
  // ponytail: a question gone from the board keeps the card's last receipt until another question is opened.
  if (row) card.rows = [{board: card.board, row}];
  card.widget.update(card.rows);
}
function renderTaskWorkspace() {
  const root = document.getElementById('boards'); if (!root) return;
  const boards = JSON.parse(root.dataset.boards), host = new URLSearchParams(location.search).get('host');
  if (!taskWorkspace) {
    const blocks = root.querySelector('.board-blocks');
    blocks.innerHTML = '<div id="workspace-notice" role="status">Actions require their separate actor grants. Native host operations are unavailable here.</div><div id="taskq-workspace"><div class="menu"></div></div><div id="workspace-answer"></div><div id="workspace-coverage"></div><h3>PM request</h3><label for="pm-target">TaskQ target</label> <select id="pm-target"></select><div id="pm-composer"></div>';
    taskWorkspaceView = TaskqTaskWorkspace.createState();
    taskWorkspace = TaskqTaskWorkspace.create(taskWorkspaceView,{menu:blocks.querySelector('.menu'),send:workspaceSend});
    taskWorkspace.mount();
    document.getElementById('pm-target').addEventListener('change',e=>{pmSelected=Number(e.target.value);pmComposer.paint();});
    const send = async request => {
      try {
        const response = await fetch('/pm/request',{method:'POST',headers:{'content-type':'application/json','x-dashq-csrf':pmAccess.csrf},body:JSON.stringify(request),signal:AbortSignal.timeout(30000)});
        const v = await response.json();
        return v.schema === 'taskq.pm/1' ? v : {schema:'taskq.pm/1',actor_id:request.actor_id,op_id:request.op_id,revision:request.revision,outcome:'unknown',reason:v.reason || 'Delivery unknown; check the same request ID'};
      } catch (_) { return {schema:'taskq.pm/1',actor_id:request.actor_id,op_id:request.op_id,revision:request.revision,outcome:'unknown',reason:'Delivery unknown; check the same request ID'}; }
    };
    pmComposer = TaskqPmComposer.mount(document.getElementById('pm-composer'),{
      load:()=>pmDraft, save:d=>{pmDraft=structuredClone(d);document.getElementById('pm-target').disabled=!!d.request;},
      context:()=>pmAccess.targets[pmSelected] || {reason:pmAccess.reason || 'No fresh authorized TaskQ PM target. Ask its owner to enable a PM grant.'},
      id:()=>Array.from(crypto.getRandomValues(new Uint8Array(16)),b=>b.toString(16).padStart(2,'0')).join(''), now:()=>Math.floor(Date.now()/1000),send,reconcile:send});
  }
  taskWorkspace.receive({...workspaceState(boards,host),...workspaceAppearance});
  workspaceAnswerUpdate(boards);
  const coverage = document.getElementById('workspace-coverage'); coverage.replaceChildren();
  for (const b of boards) {
    const p = document.createElement('p'); p.id=boardId(b.project);
    p.textContent=b.project+' · '+b.source.state+' · '+(b.source.age_s == null?'age unknown':b.source.age_s+'s old')+(b.source.reason?' · '+b.source.reason:'')+' · ';
    const closed=document.createElement('a');closed.href=b.url+'?q=is%3Aissue+is%3Aclosed';closed.textContent='Closed on board';p.append(closed);
    for(const q of b.questions || []) {const a=document.createElement('a');a.id=questionId(b.project,q.number);a.href=q.url;a.textContent=' · #'+q.number+' question';a.onclick=e=>{e.preventDefault();taskWorkspace.receive({open:{root:b.project,n:q.number,kind:'ask'}});};p.append(a);}
    coverage.append(p);
  }
  root.querySelector('.board-fallback').hidden=true;
  fetch('/pm/request',{signal:AbortSignal.timeout(10000)}).then(r=>r.json()).then(v=>{
    pmAccess={...v,targets:v.targets || []};
    const select=document.getElementById('pm-target'), prior=select.options[select.selectedIndex]?.text;
    if(!pmDraft.request) {
      select.replaceChildren(...pmAccess.targets.map((t,i)=>new Option(t.target.installation_id+' / '+t.target.project_id+' / '+t.target.runtime_id,String(i))));
      const index=[...select.options].findIndex(o=>o.text===prior);pmSelected=index>=0?index:0;select.value=String(pmSelected);
    }
    pmComposer.paint();
  }).catch(()=>{pmAccess={targets:[],reason:'PM access unknown; draft kept'};pmComposer.paint();});
}
if (typeof module === 'object') module.exports={workspaceState};
