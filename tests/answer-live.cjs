// Chain D13 live (#44 gate, answers.md § Gates): deployed DashQ /answer -> pinned taskq -> the real board -> readback.
// Risk: an answer is duplicated, misdirected or accepted without board readback, or a denied actor/request writes.
// Not in `cargo test`: it writes one answer to an owner-authorized test task. Run on the dashboard host as the
// host administrator, against the loopback listener (the proxy strips X-Dashq-Subject from clients):
//   DASHQ_PUBLIC_HOST=dash.example BOARD=owner/repo TASK=N SUBJECT='github|1' [OTHER_SUBJECT=..] [OTHER_BOARD=owner/repo2]
//   [CHOICE=K | TEXT='..'] [WORKSPACE=w DASHQ_BIN=/path/dashq] [GH=1] node tests/answer-live.cjs
// The task must be `ask` with that choice (or free text); `review` takes choices only. Prints one JSON record.
const assert = require('node:assert/strict'), crypto = require('node:crypto'), {execFileSync} = require('node:child_process');
const env = process.env, base = env.DASHQ_URL || 'http://127.0.0.1:18882', origin = 'https://' + env.DASHQ_PUBLIC_HOST;
const {BOARD: board, TASK: task, SUBJECT: subject} = env;
assert(env.DASHQ_PUBLIC_HOST && board && task && subject && (env.CHOICE || env.TEXT), 'set DASHQ_PUBLIC_HOST, BOARD, TASK, SUBJECT and CHOICE or TEXT');
const answer = env.CHOICE ? {choice: env.CHOICE} : {text: env.TEXT};
const record = {at: new Date().toISOString(), board, task: Number(task), checks: {}};
const check = (name, ok, got) => { record.checks[name] = ok ? 'pass' : 'FAIL ' + JSON.stringify(got); };
const call = async (method, path, {who = subject, body, from = origin} = {}) => {
  const headers = {origin: from, ...(who ? {'x-dashq-subject': who} : {})};
  if (body) headers['content-type'] = 'application/x-www-form-urlencoded';
  const r = await fetch(base + path, {method, headers, body: body && new URLSearchParams(body)});
  return [r.status, await r.json().catch(() => null)];
};
const row = async () => {
  const [, d] = await call('GET', '/data');
  return d.boards.find(b => b.project === board).tasks.find(t => t.n === Number(task));
};
const csrf = async (who = subject, b = board) => (await call('GET', '/answer?board=' + encodeURIComponent(b), {who}))[1].csrf;
const comments = () => env.GH ? JSON.parse(execFileSync('gh', ['api', '--paginate', `repos/${board}/issues/${task}/comments`, '--jq', '[.[].body]'])).flat() : null;
const post = (form, opts) => call('POST', '/answer', {...opts, body: {board, task, ...form}});
(async () => {
  if (env.DASHQ_BIN) record.dashq = execFileSync(env.DASHQ_BIN, ['version']).toString().trim();
  const before = await row(), token = await csrf();
  record.revision = before.revision; record.state = before.state;
  assert(token && ['ask', 'review'].includes(before.state), 'answer access or answerable task missing: ' + JSON.stringify(before));
  const op = 'live-' + crypto.randomUUID(), form = {csrf: token, revision: before.revision, op_id: op, ...answer};
  // A3 guard/CSRF and A2 identity/workspace: each refusal happens before any write
  check('no subject 401', (await post(form, {who: null}))[0] === 401);
  if (env.OTHER_SUBJECT) check('unmapped subject 403', (await post({...form, csrf: await csrf(env.OTHER_SUBJECT) || token}, {who: env.OTHER_SUBJECT}))[0] === 403);
  check('cross-site origin 403', (await post(form, {from: 'https://evil.example'}))[0] === 403);
  check('wrong csrf 403', (await post({...form, csrf: 'x'}))[0] === 403);
  check('unconfigured board 404', (await call('POST', '/answer', {body: {...form, board: 'nobody/none', task}}))[0] === 404);
  if (env.OTHER_BOARD) {
    const [s, v] = await call('POST', '/answer', {body: {...form, board: env.OTHER_BOARD, csrf: await csrf(subject, env.OTHER_BOARD) || token}});
    check('other workspace refused', s === 403 || s === 404, [s, v]);
  }
  if (env.DASHQ_BIN && env.WORKSPACE) {
    execFileSync(env.DASHQ_BIN, ['answerer', env.WORKSPACE, subject, 'off']);
    try { check('revoked identity 403', (await post(form))[0] === 403); }
    finally { execFileSync(env.DASHQ_BIN, ['answerer', env.WORKSPACE, subject, 'on']); }
  }
  const stale = await post({...form, op_id: op + '-stale', revision: before.revision - 1});
  check('stale revision 409', stale[0] === 409 && stale[1].outcome === 'stale', stale);
  // A1 + A2: a double click sends the same operation twice at once; one write, one receipt
  const [a, b] = await Promise.all([post(form), post(form)]);
  record.receipts = [a, b];
  const accepted = [a, b].filter(([s, v]) => s === 200 && v.outcome === 'accepted');
  check('double click: one accepted, other same or in flight', accepted.length >= 1 && [a, b].every(([s, v]) => (s === 200 && v.op_id === 'dashq.' + op) || (s === 202 && v.outcome === 'unknown')), [a, b]);
  // lost reply: the retry with the same op ID returns the first receipt, no second write
  const again = await post(form);
  check('lost reply: same receipt', again[0] === 200 && JSON.stringify(again[1]) === JSON.stringify(accepted[0][1]), again);
  const other = await post({...form, ...(answer.choice ? {choice: String(Number(answer.choice) % 9 + 1)} : {text: answer.text + ' changed'})});
  check('same op ID, other payload refused', other[0] !== 200 || other[1].outcome !== 'accepted', other);
  // readback: the board moved past the answered revision (DashQ's board cache may lag up to its TTL)
  let after = before;
  for (let i = 0; i < 18 && after.revision === before.revision; i++) { await new Promise(r => setTimeout(r, 10000)); after = await row(); }
  record.after = {revision: after.revision, state: after.state};
  check('board readback moved', after.revision !== before.revision, record.after);
  const c = comments();
  if (c) check('one canonical entry for the op ID', c.filter(t => t.includes('dashq.' + op)).length === 1, c.filter(t => t.includes('dashq.' + op)).length);
  record.op_id = 'dashq.' + op;
  console.log(JSON.stringify(record, null, 2));
  process.exit(Object.values(record.checks).every(v => v === 'pass') ? 0 : 1);
})().catch(e => { console.error(e); process.exit(1); });
