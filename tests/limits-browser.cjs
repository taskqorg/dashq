// Chain #159 A1-A3: enrolled taskq-resources/1 groups -> production adapter -> exact submit -> lookup -> applied only
// from a fresh projection carrying the saved revision. Risk: an edit offered without a covering project-limits grant,
// an uncovered runtime editable, a changed or second submit, a refusal without its reason, or saved read as applied.
// Input: DASHQ_LIMITS_EVIDENCE from `cargo test --locked taskq_resources_record`. HTTP is stubbed; no host is written.
const {chromium} = require('playwright');
const fs = require('node:fs'), assert = require('node:assert/strict');
const T0 = 1800000000, SAVED = '2'.repeat(64);
const js = ['assets/taskq/resource-controls.js', 'assets/resources.js', 'assets/taskq/project-blocks.js', 'assets/taskq/answer-card.js', 'assets/taskq/task-controls.js', 'assets/boards.js', 'assets/app.js'].map(p => fs.readFileSync(p, 'utf8')).join('\n');
const css = ['assets/style.css', 'assets/taskq/resource-controls.css', 'assets/resources.css', 'assets/taskq/project-blocks.css', 'assets/taskq/answer-card.css'].map(p => fs.readFileSync(p, 'utf8')).join('\n');
const html = fs.readFileSync(process.env.DASHQ_LIMITS_EVIDENCE, 'utf8');

async function open(browser, {grants, submit, lookup}) {
  const page = await browser.newPage({viewport: {width: 1024, height: 900}});
  const errors = [], posts = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.clock.install({time: T0 * 1000});
  await page.route('http://fixture/**', async route => {
    const req = route.request(), path = new URL(req.url()).pathname;
    const json = (status, body) => route.fulfill({status, contentType: 'application/json', body: JSON.stringify(body)});
    if (path === '/app.js') return route.fulfill({contentType: 'text/javascript', body: js});
    if (path === '/style.css') return route.fulfill({contentType: 'text/css', body: css});
    if (path === '/data') return json(200, {});
    if (path === '/api/taskq/resources/v1/grants') return json(...grants);
    if (path === '/api/taskq/resources/v1/operations' && req.method() === 'POST') {
      posts.push({body: req.postDataJSON(), csrf: req.headers()['x-dashq-csrf'], type: req.headers()['content-type']});
      return json(...submit(posts.length));
    }
    if (path.startsWith('/api/taskq/resources/v1/operations/')) return json(...lookup());
    return route.fulfill({contentType: 'text/html', body: html});
  });
  await page.goto('http://fixture/integrations');
  await page.waitForFunction(() => !busy);
  const inst = await page.locator('.node-policy').evaluate(el => JSON.parse(el.dataset.taskqGroups)[0].targets[0].target.installation_id);
  // a second registered runtime the grant below does not cover
  await page.locator('.node-policy').evaluate(el => {
    const gs = JSON.parse(el.dataset.taskqGroups), t = gs[0].targets[0];
    gs[0].targets.push({...t, target: {...t.target, runtime_id: 'claude'}});
    el.dataset.taskqGroups = JSON.stringify(gs);
    DashqResources.bind();
  });
  await page.locator('.node-policy summary').click();
  return {page, errors, posts, inst};
}

const grant = inst => ({grant_id: 'g-1', actor_id: 'actor-1', installation_id: inst, generation: 1, project_ids: ['project-1'], runtime_ids: ['codex'],
  host_policy: false, project_limits: true, expires_at: T0 + 3600, revoked: false});
const receipt = (body, state, extra = {}) => ({schema: 'taskq.resources/1', op_id: body.op_id, target: body.target, state, reason: null, saved_revision: null, effective_revision: null, observed_at: null, ...extra});

(async () => {
  const browser = await chromium.launch({headless: true});
  try {
    // covered codex: review 1 -> 0, one exact submit, pending -> saved by lookup, then applied from the projection
    let last = null;
    const inst = require('../fixture/taskq-resources/resource-policy-v1.json').enrollment_record.payload.targets[0].target.installation_id;
    const s = await open(browser, {grants: [200, {schema: 'taskq.resources/1', csrf: 'csrf-1', grants: [grant(inst)]}],
      submit: () => [202, (last = receipt(s.posts[0].body, 'pending'))], lookup: () => [200, {...last, state: 'saved', saved_revision: SAVED, effective_revision: '1'.repeat(64), observed_at: T0}]});
    const {page} = s;
    assert.equal(await page.locator('[data-rsc-field="rt:claude"]').count(), 0, 'an uncovered runtime has no input');
    await page.getByRole('button', {name: 'Edit limits'}).click();
    assert.equal(await page.locator('[data-rsc-field="rt:codex"]').count(), 1);
    assert.equal(await page.locator('[data-rsc-field="rt:claude"]').count(), 0);
    await page.locator('[data-rsc-field="rt:codex"]').fill('0');
    await page.getByRole('button', {name: 'Apply…'}).click();
    assert((await page.locator('.rsc-review').textContent()).includes('Codex 1 → 0'));
    await page.getByRole('button', {name: 'Confirm'}).click();
    await page.getByText('Saved; waiting for service').waitFor({timeout: 15000});
    assert.equal(s.posts.length, 1, 'one submit, no resend');
    const b = s.posts[0].body;
    assert.deepEqual(Object.keys(b).sort(), ['actor_id', 'changes', 'expected_revision', 'expires_at', 'grant_id', 'issued_at', 'op_id', 'policy_schema', 'schema', 'target'].sort());
    assert.deepEqual([b.policy_schema, b.actor_id, b.grant_id, b.expected_revision, b.changes, b.expires_at - b.issued_at, s.posts[0].csrf, s.posts[0].type],
      ['taskq.project-limits/1', 'actor-1', 'g-1', '1'.repeat(64), {runtime_limit: 0}, 300, 'csrf-1', 'application/json']);
    assert.deepEqual(b.target, {installation_id: inst, generation: 1, project_id: 'project-1', runtime_id: 'codex'});
    // the next projection: another revision is superseded; the saved one, current and effective, is applied
    const refresh = (rev) => page.locator('.node-policy').evaluate((el, [rev, at]) => {
      const gs = JSON.parse(el.dataset.taskqGroups);
      for (const t of gs[0].targets) Object.assign(t, {current_revision: rev, effective_revision: rev, current_policy: {runtime_limit: 0}, effective_policy: {runtime_limit: 0}});
      gs[0].observed_at = at;
      el.dataset.taskqGroups = JSON.stringify(gs);
      DashqResources.bind();
    }, [rev, T0 + 1]);
    await refresh('3'.repeat(64));
    await page.getByText('Saved; replaced by a newer command').waitFor();
    await refresh(SAVED);
    await page.getByText('Applied: read back from the service').waitFor();
    assert.deepEqual(s.errors, []);
    await page.close();

    // refusals keep their reason; no grant, or resource writes off, is read-only with its reason and no input
    const r = await open(browser, {grants: [200, {schema: 'taskq.resources/1', csrf: 'csrf-1', grants: [grant(inst)]}], submit: () => [403, {schema: 'taskq.resources/1', reason: 'revoked'}], lookup: () => [404, {}]});
    await r.page.getByRole('button', {name: 'Edit limits'}).click();
    await r.page.locator('[data-rsc-field="rt:codex"]').fill('4');
    await r.page.getByRole('button', {name: 'Apply…'}).click();
    await r.page.getByRole('button', {name: 'Confirm'}).click();
    await r.page.getByText('Not saved: grant revoked or expired').waitFor();
    await r.page.close();
    for (const [grants, why] of [[[200, {schema: 'taskq.resources/1', csrf: 'c', grants: []}], 'No active project-limits grant for this project; read-only.'],
      [[503, {schema: 'taskq.resources/1', reason: 'unavailable'}], 'Project limits read-only: resource writes are off on this dashboard.']]) {
      const x = await open(browser, {grants, submit: () => [500, {}], lookup: () => [404, {}]});
      await x.page.getByText(why).waitFor();
      assert.equal(await x.page.getByRole('button', {name: 'Edit limits'}).count(), 0, why);
      assert.equal(x.posts.length, 0);
      // Work mode never edits this profile
      await x.page.getByRole('button', {name: '◎ Work mode…'}).click();
      await x.page.locator('[data-rsc-select]').first().check();
      assert(await x.page.getByRole('button', {name: 'Edit work mode'}).isDisabled());
      await x.page.close();
    }
    console.log('PASS project limits: grant scope, exact submit, lookup, superseded/applied readback, refusal reasons');
  } finally {
    await browser.close();
  }
})().catch(e => { console.error(e); process.exit(1); });
