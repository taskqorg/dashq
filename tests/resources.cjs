// Chain: taskq.resources/1 projection -> shared fixtures -> src/resource-controls.js states. Risk: unknown reads as
// zero or Ready, a legacy guard reads as Auto, an untouched field travels, a retry mints a new operation.
// Run: node tests/resource-controls.test.cjs. Consumers (tray, DashQ) replay the same fixture file.
const fs = require('node:fs'), crypto = require('node:crypto'), assert = require('node:assert/strict');
// the release bundle defines the browser global `TaskqResources`, no CommonJS export
const rc = new Function(fs.readFileSync('assets/taskq/resource-controls.js', 'utf8') + ';return TaskqResources;')();
const fx = JSON.parse(fs.readFileSync('fixture/taskq/resource-controls.json', 'utf8'));
for (const c of fx.cases) {
  const checks = c.events.filter(e => e.type === 'check');
  const got = rc.replay(c, () => null);
  assert.equal(got.length, checks.length, c.name);
  checks.forEach((want, i) => {
    const g = got[i], at = c.name + ' check ' + (i + 1);
    if (want.status) assert.equal(g.status, want.status, at);
    if (want.headline) assert.equal(g.headline, want.headline, at);
    if (want.sent != null) assert.equal(g.sent.length, want.sent, at);
    for (const s of want.has || []) assert(g.html.includes(s), at + ' lacks ' + s + '\n' + g.html);
    for (const s of want.not || []) assert(!g.html.includes(s), at + ' has ' + s + '\n' + g.html);
    const last = g.sent[g.sent.length - 1];
    if (want.ops) assert.deepEqual(last.ops, want.ops, at + ': one contract-shaped change per target, untouched fields stay');
    if (want.same_op) assert.deepEqual(last, g.sent[g.sent.length - 2], at + ': a retry keeps the id and payload');
  });
  assert(rc.STATES.includes(got[got.length - 1].status));
  console.log('PASS', c.name);
}
// #954 Work mode (menu M12): selection, Mixed, one operation and receipt per selected project
for (const c of fx.work) {
  const checks = c.events.filter(e => e.type === 'check');
  const got = rc.replayWork(c, () => null);
  assert.equal(got.length, checks.length, c.name);
  checks.forEach((want, i) => {
    const g = got[i], at = c.name + ' check ' + (i + 1);
    if (want.status) assert.equal(g.status, want.status, at);
    if (want.each) assert.deepEqual(g.each, want.each, at + ': per-project outcome');
    if (want.sent != null) assert.equal(g.sent.length, want.sent, at);
    for (const s of want.has || []) assert(g.html.includes(s), at + ' lacks ' + s + '\n' + g.html);
    for (const s of want.not || []) assert(!g.html.includes(s), at + ' has ' + s + '\n' + g.html);
    if (want.order) { const at2 = want.order.map(s => g.html.indexOf('>' + s + '<')); assert(at2.every((x, k) => x >= 0 && (!k || x > at2[k - 1])), at + ': parent order ' + want.order + ' ' + at2); }
    if (want.ops) assert.deepEqual(Object.fromEntries(g.sent.map(x => [x.key, x.op.ops])), want.ops, at + ': selected projects only, untouched fields stay');
    if (want.same_op) { const s = g.sent.filter(x => x.key === want.same_op); assert.deepEqual(s[s.length - 1], s[s.length - 2], at + ': a retry keeps the id and payload'); }
  });
  console.log('PASS', c.name);
}

const pin = JSON.parse(fs.readFileSync('assets/taskq/sources.json'));
for (const [path, info] of Object.entries(pin.files)) {
  assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path)).digest('hex'), info.sha256, path);
}
console.log('PASS immutable asset hashes');
