// Chain D13 live (#44 gate, answers.md § Gates): deployed DashQ /answer -> pinned taskq -> the real board -> readback.
// Risk: an answer is duplicated, misdirected or accepted without board readback, or a denied actor/request writes.
// Not in `cargo test`: it writes one answer to an owner-authorized test task. Run on the dashboard host as the
// host administrator, against the loopback listener (the proxy strips X-Dashq-Subject from clients):
//   DASHQ_PUBLIC_HOST=dash.example BOARD=owner/repo TASK=N SUBJECT='github|1' DASHQ_BIN=/path/dashq TASKQ_BIN=/path/taskq
//   [CHOICE=K | TEXT='..'] [OTHER_SUBJECT=..] [OTHER_BOARD=owner/repo2] [WORKSPACE=w] [GH=1] node tests/answer-live.cjs
// The task must be `ask` with that choice (or free text); `review` takes choices only. Prints one JSON record.
// Safety: versions, the served component pin and the task are checked before any POST; every refusal probe carries a
// stale revision and its own op ID, so even a broken guard cannot write; the first failed check stops the run.
// Exit 0: every check ran and passed. 1: a check failed (nothing after it ran). 2: passed, but some checks did not run.
// Regression: tests/answer-live-stub.cjs (in `cargo test`) runs this against broken and correct stub servers.
const crypto = require('node:crypto'), fs = require('node:fs'), path = require('node:path'), {execFileSync} = require('node:child_process');
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
class Stop extends Error {}

async function run(env, {exec = (bin, args) => execFileSync(bin, args).toString(), pollMs = 10000, polls = 18} = {}) {
  const base = env.DASHQ_URL || 'http://127.0.0.1:18882', origin = 'https://' + env.DASHQ_PUBLIC_HOST;
  const {BOARD: board, TASK: task, SUBJECT: subject} = env;
  const record = {at: new Date().toISOString(), board, task: Number(task), posts: 0, checks: {}};
  const check = (name, ok, got) => {
    record.checks[name] = ok ? 'pass' : 'FAIL ' + JSON.stringify(got);
    if (!ok) throw new Stop(name);
  };
  const skip = (name, why) => { record.checks[name] = 'not run: ' + why; };
  const call = async (method, p, {who = subject, body, from = origin} = {}) => {
    const headers = {origin: from, ...(who ? {'x-dashq-subject': who} : {})};
    if (body) { headers['content-type'] = 'application/x-www-form-urlencoded'; record.posts++; }
    const r = await fetch(base + p, {method, headers, body: body && new URLSearchParams(body)});
    return [r.status, await r.json().catch(() => null)];
  };
  const text = async p => { const r = await fetch(base + p); return r.ok ? r.text() : ''; };
  const data = async () => (await call('GET', '/data'))[1];
  const row = async () => data().then(d => d?.boards?.find(b => b.project === board)?.tasks?.find(t => t.n === Number(task)));
  const csrf = async (who = subject, b = board) => (await call('GET', '/answer?board=' + encodeURIComponent(b), {who}))[1]?.csrf;
  const post = (form, opts) => call('POST', '/answer', {...opts, body: {board, task, ...form}});
  try {
    check('inputs', env.DASHQ_PUBLIC_HOST && board && task && subject && env.DASHQ_BIN && env.TASKQ_BIN && !env.CHOICE !== !env.TEXT,
      'set DASHQ_PUBLIC_HOST, BOARD, TASK, SUBJECT, DASHQ_BIN, TASKQ_BIN and one of CHOICE or TEXT');
    const answer = env.CHOICE ? {choice: env.CHOICE} : {text: env.TEXT};
    // A1 versions: the deployed binaries, this checkout and the component the deployment serves; all before any POST
    const root = path.join(__dirname, '..');
    record.dashq = {version: exec(env.DASHQ_BIN, ['version']).trim(), binary_sha256: sha(fs.readFileSync(env.DASHQ_BIN)),
      checkout: exec('git', ['-C', root, 'rev-parse', 'HEAD']).trim()};
    const versions = ((await data())?.versions?.rows || []).filter(r => r.product === 'taskq' && r.workspace === board);
    record.taskq = {binary_sha256: sha(fs.readFileSync(env.TASKQ_BIN)), heartbeat: versions.map(r => r.running ?? null)};
    const pin = JSON.parse(fs.readFileSync(path.join(root, 'assets/taskq/provenance.json')));
    const served = (await text('/app.js')) + (await text('/style.css'));
    record.component = {repository: pin.repository, commit: pin.commit, release: pin.release, files: pin.files};
    const off = Object.entries(pin.files).filter(([f]) => /\.(js|css)$/.test(f)).filter(([f, h]) => {
      const local = fs.readFileSync(path.join(root, 'assets/taskq', f));
      return sha(local) !== h || !served.includes(local.toString());
    }).map(([f]) => f);
    check('served component matches pin', off.length === 0, off);
    const before = await row(), token = await csrf();
    record.revision = before?.revision; record.state = before?.state;
    check('answerable task and answer access', token && before && ['ask', 'review'].includes(before.state), before);
    const op = 'live-' + crypto.randomUUID(), form = {csrf: token, revision: before.revision, op_id: op, ...answer};
    // A3 guard/CSRF and A2 identity/workspace. Each probe is stale with its own op ID: a broken guard yields 409, no write
    let n = 0;
    const probe = extra => ({...form, revision: before.revision - 1, op_id: `${op}-probe${++n}`, ...extra});
    check('no subject 401', (await post(probe(), {who: null}))[0] === 401);
    if (env.OTHER_SUBJECT) check('unmapped subject 403', (await post(probe({csrf: await csrf(env.OTHER_SUBJECT) || token}), {who: env.OTHER_SUBJECT}))[0] === 403);
    else skip('unmapped subject 403', 'OTHER_SUBJECT unset');
    check('cross-site origin 403', (await post(probe(), {from: 'https://evil.example'}))[0] === 403);
    check('wrong csrf 403', (await post(probe({csrf: 'x'})))[0] === 403);
    check('unconfigured board 404', (await post(probe({board: 'nobody/none'})))[0] === 404);
    if (env.OTHER_BOARD) {
      const r = await post(probe({board: env.OTHER_BOARD, csrf: await csrf(subject, env.OTHER_BOARD) || token}));
      check('other workspace refused', r[0] === 403 || r[0] === 404, r);
    } else skip('other workspace refused', 'OTHER_BOARD unset');
    if (env.WORKSPACE) {
      exec(env.DASHQ_BIN, ['answerer', env.WORKSPACE, subject, 'off']);
      let r;
      try { r = await post(probe()); } finally { exec(env.DASHQ_BIN, ['answerer', env.WORKSPACE, subject, 'on']); }
      check('revoked identity 403', r[0] === 403, r);
    } else skip('revoked identity 403', 'WORKSPACE unset');
    const stale = await post(probe());
    check('stale revision 409', stale[0] === 409 && stale[1]?.outcome === 'stale', stale);
    // A1 + A2: a double click sends the same operation twice at once; one write, one receipt
    const [a, b] = await Promise.all([post(form), post(form)]);
    record.receipts = [a, b];
    check('double click: same op, at most one in flight', [a, b].every(([s, v]) => (s === 200 && v?.op_id === 'dashq.' + op) || (s === 202 && v?.outcome === 'unknown')) && [a, b].some(([s]) => s === 200), [a, b]);
    // lost reply: the retry with the same op ID returns the first receipt, no second write
    const first = [a, b].find(([s]) => s === 200)[1], again = await post(form);
    check('lost reply: same receipt', again[0] === 200 && first.outcome === 'accepted' && JSON.stringify(again[1]) === JSON.stringify(first), again);
    const other = await post({...form, ...(answer.choice ? {choice: String(Number(answer.choice) % 9 + 1)} : {text: answer.text + ' changed'})});
    check('same op ID, other payload refused', other[0] === 409 && other[1]?.outcome === 'rejected', other);
    // readback: the board moved past the answered revision (DashQ's board cache may lag up to its TTL)
    let after = await row();
    for (let i = 0; i < polls && after?.revision === before.revision; i++) { await new Promise(r => setTimeout(r, pollMs)); after = await row(); }
    record.after = {revision: after?.revision, state: after?.state};
    check('board readback moved', after && after.revision !== before.revision, record.after);
    if (env.GH) {
      const c = JSON.parse(exec('gh', ['api', '--paginate', `repos/${board}/issues/${task}/comments`, '--jq', '[.[].body]'])).flat();
      const hits = c.filter(t => t.includes('dashq.' + op)).length;
      check('one canonical entry for the op ID', hits === 1, hits);
    } else skip('one canonical entry for the op ID', 'GH unset');
    record.op_id = 'dashq.' + op;
  } catch (e) {
    if (!(e instanceof Stop)) record.checks.error = 'FAIL ' + String(e && e.stack || e);
  }
  const v = Object.values(record.checks);
  record.result = v.some(x => x.startsWith('FAIL')) ? 'failed' : v.some(x => x.startsWith('not run')) ? 'incomplete' : 'qualified';
  return record;
}

module.exports = {run};
if (require.main === module) run(process.env).then(r => {
  console.log(JSON.stringify(r, null, 2));
  process.exit({qualified: 0, failed: 1, incomplete: 2}[r.result]);
});
