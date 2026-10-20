// Chain D14/#46: server fixture -> shared summary -> Work mode -> refresh/review -> no remote write.
// The editable adapter below is a fixture only. Production writes only taskq.project-limits/1 (tests/limits-browser.cjs).
const {chromium} = require('playwright');
const fs = require('node:fs'), assert = require('node:assert/strict');
const fx = require('../fixture/taskq/resource-controls.json');
(async () => {
  const browser = await chromium.launch({headless: true});
  try {
    const html = fs.readFileSync(process.env.DASHQ_RESOURCE_EVIDENCE, 'utf8');
    if (process.env.DASHQ_RESOURCE_SHOTS) fs.mkdirSync(process.env.DASHQ_RESOURCE_SHOTS, {recursive: true});
    for (const width of [1024, 360]) for (const size of [16, 20]) for (const theme of ['light', 'dark']) {
      const page = await browser.newPage({viewport: {width, height: 900}, colorScheme: theme});
      const errors = [];
      page.on('pageerror', e => errors.push(e.message));
      await page.clock.install({time: 1000000});
      await page.route('http://fixture/**', route => {
        const path = new URL(route.request().url()).pathname;
        const body = path === '/app.js' ? ['assets/taskq/resource-controls.js', 'assets/resources.js', 'assets/taskq/project-blocks.js', 'assets/taskq/answer-card.js', 'assets/boards.js', 'assets/app.js'].map(p => fs.readFileSync(p, 'utf8')).join('\n') :
          path === '/style.css' ? ['assets/style.css', 'assets/taskq/resource-controls.css', 'assets/resources.css', 'assets/taskq/project-blocks.css', 'assets/taskq/answer-card.css'].map(p => fs.readFileSync(p, 'utf8')).join('\n') :
          path === '/data' ? '{}' : html;
        return route.fulfill({contentType: path.endsWith('.js') ? 'text/javascript' : path.endsWith('.css') ? 'text/css' : path === '/data' ? 'application/json' : 'text/html', body});
      });
      await page.goto('http://fixture/');
      await page.waitForFunction(() => !busy);
      await page.addStyleTag({content: 'html{font-size:' + size + 'px}'});
      assert.equal(await page.locator('.node').count(), 4);
      assert.equal(await page.locator('.node-policy').count(), 1);
      assert(await page.locator('.node-policy').textContent().then(t => t.includes('installation ' + fx.work[0].groups[0].targets[0].target.installation_id)));
      if (process.env.DASHQ_RESOURCE_SHOTS && ((width === 1024 && size === 16 && theme === 'light') || (width === 360 && size === 20 && theme === 'dark'))) {
        if (width === 360) await page.locator('.node-policy summary').click();
        await page.locator(width === 1024 ? '.first' : '.node-policy').screenshot({path: `${process.env.DASHQ_RESOURCE_SHOTS}/overview-${width}-${size}-${theme}.png`});
      }
      await page.getByRole('button', {name: '◎ Work mode…'}).click();
      await page.locator('[data-rsc-select]').first().check();
      assert(await page.getByRole('button', {name: 'Edit work mode'}).isDisabled());
      await page.getByRole('button', {name: 'Close', exact: true}).click();
      // Same shell, canonical editable fixture. No HTTP mutation handler exists.
      await page.evaluate(groups => {
        window.sent = [];
        window.clockNow = 1000;
        const el = document.querySelector('.node-policy');
        window.fixtureBinding = DashqResources.mount(el, groups, {now: () => clockNow, capability: () => ({edit: true}), send: async (g, op) => { sent.push({key: g.key, op}); return {state: 'saved'}; }});
      }, fx.work[0].groups);
      await page.getByRole('button', {name: '◎ Work mode…'}).click();
      await page.locator('[data-rsc-select="mac/taskq"]').check();
      await page.locator('[data-rsc-select="mac/agentq"]').check();
      await page.getByRole('button', {name: 'Edit work mode'}).click();
      const max = page.locator('[data-rsc-field="host.maximum"]');
      assert.equal(await max.getAttribute('placeholder'), 'Mixed');
      if (process.env.DASHQ_RESOURCE_SHOTS && ((width === 1024 && size === 16 && theme === 'light') || (width === 360 && size === 20 && theme === 'dark'))) {
        await page.screenshot({path: `${process.env.DASHQ_RESOURCE_SHOTS}/editor-${width}-${size}-${theme}.png`});
      }
      await max.fill('4');
      await page.evaluate(() => refresh());
      assert.equal(await max.inputValue(), '4');
      assert(await max.evaluate(el => el === document.activeElement));
      await page.getByRole('button', {name: 'Review changes'}).click();
      assert(await page.locator('.rsc-review').textContent().then(t => t.includes('taskq: Host maximum no maximum → 4') && t.includes('agentq: Host maximum 2 → 4') && !t.includes('legacy:')));
      const box = await page.locator('dialog').boundingBox();
      assert(box.x >= 0 && box.x + box.width <= width, 'dialog fits');
      assert(await page.locator('dialog').evaluate(d => d.scrollWidth <= d.clientWidth + 1), 'no horizontal dialog overflow');
      if (process.env.DASHQ_RESOURCE_SHOTS) {
        await page.screenshot({path: `${process.env.DASHQ_RESOURCE_SHOTS}/resources-${width}-${size}-${theme}.png`});
      }
      // An unselected stale project must not hold fresh selected targets or lose their draft.
      await page.evaluate(groups => { const next = structuredClone(groups); next[2].observed_at = 0; fixtureBinding.update(next); }, fx.work[0].groups);
      assert.equal(await page.locator('dialog .rsc-work').evaluate(el => el.parentElement.inert), false);
      assert(await page.locator('.rsc-review').textContent().then(t => t.includes('Host maximum')));
      // Freshness loss during review cannot submit; draft and exact target remain visible.
      await page.evaluate(groups => { clockNow = 1100; fixtureBinding.update(groups); }, fx.work[0].groups);
      assert(await page.locator('dialog .rsc-work').evaluate(el => el.parentElement.inert));
      assert.equal(await page.evaluate(() => sent.length), 0);
      await page.getByRole('button', {name: 'Close', exact: true}).click();
      assert.deepEqual(errors, []);
      console.log(`PASS browser ${width}/${size}/${theme}: read-only, Mixed, review, draft/focus, refresh, stale hold`);
      await page.close();
    }
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
