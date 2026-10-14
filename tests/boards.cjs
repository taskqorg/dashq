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
