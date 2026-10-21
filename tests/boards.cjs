// Chain: canonical question (cargo test) -> shared fixtures -> src/answer-card.js states. Risk: a duplicate click sends
// twice, a stale draft sends to a new question, or a consequential choice is answerable outside the board.
// Run: node tests/answer-card.test.cjs. Consumers (tray, DashQ) run the same replay over the same fixture file.
const fs = require('node:fs'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const card = require('../assets/taskq/answer-card.js');
const fx = JSON.parse(fs.readFileSync('assets/taskq/answer-card.json', 'utf8'));
for (const c of fx.cases) {
  const checks = c.events.filter(e => e.type === 'check');
  const got = card.replay(c, (_, row) => row.capability || c.capability);
  assert.equal(got.length, checks.length, c.name);
  checks.forEach((want, i) => {
    const g = got[i], at = c.name + ' check ' + (i + 1);
    if (want.status) assert.equal(g.status, want.status, at);
    if (want.sent != null) assert.equal(g.sent.length, want.sent, at);
    if (want.draft != null) assert.equal(g.draft, want.draft, at);
    for (const s of want.has || []) assert(g.html.includes(s), at + ' lacks ' + s + '\n' + g.html);
    for (const s of want.not || []) assert(!g.html.includes(s), at + ' has ' + s + '\n' + g.html);
    const last = g.sent[g.sent.length - 1];
    if (want.op) for (const [k, v] of Object.entries(want.op)) assert.equal(last[k], v, at + ' op.' + k);
    if (want.same_op) assert.deepEqual(last, g.sent[g.sent.length - 2], at + ': a retry keeps the id and payload');
    if (want.new_op) assert.notEqual(last.id, g.sent[g.sent.length - 2].id, at + ': a changed answer is a new operation');
  });
  assert(card.STATES.includes(got[got.length - 1].status));
  console.log('PASS', c.name);
}
// A1: immutable source pin, every consumed asset and conformance fixture.
const pin = JSON.parse(fs.readFileSync('assets/taskq/provenance.json'));
for (const [name, hash] of Object.entries(pin.files))
  assert.equal(crypto.createHash('sha256').update(fs.readFileSync('assets/taskq/' + name)).digest('hex'), hash, name);
console.log('PASS immutable asset checksums');
// Chain: fixture /data boards -> boardProjects -> pinned TaskQ counters. Risk: DashQ loses TaskQ order, shows a zero,
// reports an unread board as 0, or makes Closed loud; the old server renderer checked these before #43.
const blocks = require('../assets/taskq/project-blocks.js'), {boardProjects} = require('../assets/boards.js');
const caps = html => [...html.matchAll(/class="([^"]*)" data-tqp="tasks"[^>]*data-state="(\w+)" aria-label="([^"]*)"/g)].map(m => [m[2], m[1], m[3]]);
const [taskq, agentq] = boardProjects(require('../fixture/data.json').boards);
assert.deepEqual(caps(blocks.summary(taskq)).map(c => c[2]), ['Doing', 'Review', 'Ask', 'Failed', 'Ready', 'Closed'].map(s => 'taskqorg/taskq · ' + s + ': ' + (s === 'Closed' ? 'unknown' : 1)));
assert(blocks.summary(taskq).includes('href="https://github.com/taskqorg/taskq/issues"') && blocks.summary(taskq).includes('data-icon="github"'), 'board link and provider icon');
assert.deepEqual(caps(blocks.summary(agentq)).map(c => c[2].split(' · ')[1]), blocks.states.map(([, n]) => n + ': unknown'), 'unread board: every state unknown, none 0');
const all = blocks.aggregate([taskq, agentq]);
assert(all.includes('Partial: taskqorg/agentq') && caps(all).every(c => c[2].endsWith('unknown')), 'aggregate of a partial set is unknown');
const done = {...taskq, closed: 7, tasks: taskq.tasks.filter(t => t.state === 'doing')};
assert.deepEqual(caps(blocks.summary(done)).map(c => c.slice(0, 2)), [['doing', 'good'], ['closed', 'quiet']], 'zero hidden, Closed quiet');
console.log('PASS counters: order, zero hidden, unknown, Closed quiet, partial aggregate');

// #170/D8: only canonical, current attempt evidence scopes a host; Ready is never its assigned queue.
const scoped = boardProjects([{project:'p',url:'https://example.invalid',source:{state:'ok'},counts:{closed:7},
  tasks:[{n:1,state:'doing'},{n:2,state:'ready'},{n:3,state:'doing'}],attempts:[
    {task:1,host_id:'machine/native',current:true},{task:2,host_id:'machine/native',current:false},
    {task:3,host_id:'machine/wsl',current:true}]}], 'MACHINE/native')[0];
assert.deepEqual(scoped.tasks.map(t=>t.n),[1]);
assert.equal(scoped.closed,null); assert.equal(scoped.complete,false,'host assignment coverage remains unknown');
console.log('PASS host scope: canonical identity, fresh attempts, no shared-queue multiplication');

// B4: released long-title/count fixtures reach the same renderer as dashboard rows; catch dropped identity or counts.
const shared = require('../assets/taskq/project-blocks.json');
for (const count of shared.counts) {
  const project = {...shared.project, closed: count};
  assert(blocks.summary(project).includes(project.name), 'released project identity');
  assert.equal(caps(blocks.summary(project)).length, count === 0 ? 0 : 1, 'released count ' + count);
}
assert(blocks.task(shared.project, shared.row).includes(shared.row.title), 'released long task title');
console.log('PASS released project-block fixtures');
