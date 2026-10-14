// Chain D13 live gate (#44): tests/answer-live.cjs against a stub DashQ /answer + board. Risk: the live run writes
// after a failed guard or prerequisite, misses a duplicate write, or reports a run with skipped checks as qualified.
// Run: node tests/answer-live-stub.cjs (in `cargo test`). The stub enforces revision like TaskQ; `guard: false` breaks
// only DashQ's guard, as a misconfigured deployment would.
const assert = require('node:assert/strict'), fs = require('node:fs'), http = require('node:http');
const {run} = require('./answer-live.cjs');
const BOARD = 'o/test', OTHER = 'o/other', TASK = 7, S = 'github|1', HOST = 'dash.example';
const asset = f => fs.readFileSync(__dirname + '/../assets/taskq/' + f, 'utf8');

function stub({guard = true, app = ['project-blocks.js', 'answer-card.js'].map(asset).join('\n')} = {}) {
  const st = {rev: 100, writes: 0, comments: [], ops: new Map(), revoked: false};
  const json = (res, code, v) => { res.writeHead(code, {'content-type': 'application/json'}); res.end(JSON.stringify(v)); };
  const refuse = (res, code) => json(res, code, {outcome: 'rejected', op_id: null});
  const who = (req, board) => !req.headers['x-dashq-subject'] ? 401 : board === OTHER ? 403 : board !== BOARD ? 404
    : req.headers['x-dashq-subject'] !== S || st.revoked ? 403 : 0;
  const server = http.createServer((req, res) => {
    const u = new URL(req.url, 'http://x');
    if (u.pathname === '/data') return json(res, 200, {boards: [{project: BOARD, tasks: [{n: TASK, revision: st.rev, state: st.writes ? 'doing' : 'ask'}]}],
      versions: {rows: [{product: 'taskq', workspace: BOARD, running: '4.1.0'}]}});
    if (u.pathname === '/app.js') return res.end(app);
    if (u.pathname === '/style.css') return res.end(['project-blocks.css', 'answer-card.css'].map(asset).join('\n'));
    if (req.method === 'GET') { const g = guard && who(req, u.searchParams.get('board')); return g ? refuse(res, g) : json(res, 200, {csrf: 't'}); }
    let body = '';
    req.on('data', c => body += c).on('end', () => {
      const f = Object.fromEntries(new URLSearchParams(body)), op = 'dashq.' + f.op_id, payload = f.choice ?? 'text:' + f.text;
      if (guard && req.headers.origin !== 'https://' + HOST) return refuse(res, 403);
      const g = guard && who(req, f.board);
      if (g) return refuse(res, g);
      if (guard && f.csrf !== 't') return refuse(res, 403);
      const seen = st.ops.get(op);
      if (seen) return seen.payload === payload ? json(res, ...seen.reply) : refuse(res, 409);
      const stale = Number(f.revision) !== st.rev;
      if (!stale) { st.writes++; st.rev++; st.comments.push('answer ' + op); }
      const reply = stale ? [409, {outcome: 'stale', op_id: op}] : [200, {outcome: 'accepted', op_id: op, revision: st.rev}];
      st.ops.set(op, {payload, reply});
      json(res, ...reply);
    });
  });
  const exec = (bin, args) => args[0] === 'version' ? 'dashq 0.0.0-stub' : args[0] === 'answerer' ? (st.revoked = args[3] === 'off', '')
    : bin === 'gh' ? JSON.stringify(st.comments) : 'stubsha';
  return new Promise(r => server.listen(0, '127.0.0.1', () => r({st, server, exec, url: 'http://127.0.0.1:' + server.address().port})));
}

const ENV = {DASHQ_PUBLIC_HOST: HOST, BOARD, TASK: String(TASK), SUBJECT: S, DASHQ_BIN: __filename, TASKQ_BIN: __filename, CHOICE: '2'};
const FULL = {OTHER_SUBJECT: 'github|2', OTHER_BOARD: OTHER, WORKSPACE: 'w', GH: '1'};
async function go(name, env, opts, want) {
  const s = await stub(opts);
  const r = await run({...ENV, DASHQ_URL: s.url, ...env}, {exec: s.exec, pollMs: 5, polls: 3});
  s.server.close();
  try { want(r, s.st); } catch (e) { console.error(JSON.stringify(r, null, 2)); throw e; }
  console.log('PASS', name);
}

(async () => {
  await go('broken guard: stops at the first probe, nothing written', FULL, {guard: false}, (r, st) => {
    assert.equal(r.result, 'failed');
    assert.match(r.checks['no subject 401'], /^FAIL/);
    assert.equal(r.posts, 1); assert.equal(st.writes, 0);
  });
  await go('missing TASKQ_BIN: no POST', {TASKQ_BIN: ''}, {}, (r, st) => { assert.equal(r.result, 'failed'); assert.equal(r.posts, 0); assert.equal(st.writes, 0); });
  await go('served component differs from the pin: no POST', FULL, {app: 'old'}, (r, st) => {
    assert.match(r.checks['served component matches pin'], /^FAIL.*answer-card\.js/); assert.equal(r.posts, 0); assert.equal(st.writes, 0);
  });
  for (const answer of [{}, {CHOICE: '', TEXT: 'Use the stub'}])
    await go('correct deployment, every check: one write ' + (answer.TEXT ? '(text)' : '(choice)'), {...FULL, ...answer}, {}, (r, st) => {
      assert.equal(r.result, 'qualified', JSON.stringify(r.checks));
      assert.equal(st.writes, 1); assert.equal(st.comments.length, 1); assert.equal(st.revoked, false);
      assert.equal(r.dashq.version, 'dashq 0.0.0-stub'); assert.deepEqual(r.taskq.heartbeat, ['4.1.0']); assert.equal(r.component.files['answer-card.js'].length, 64);
    });
  await go('optional checks unset: incomplete, never qualified', {}, {}, (r, st) => {
    assert.equal(r.result, 'incomplete'); assert.equal(st.writes, 1);
    for (const k of ['unmapped subject 403', 'other workspace refused', 'revoked identity 403', 'one canonical entry for the op ID']) assert.match(r.checks[k], /^not run/);
  });
})().catch(e => { console.error(e); process.exit(1); });
