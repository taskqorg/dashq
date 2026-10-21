// H1/H2, D5: shared host names keep separate episodes; disclosure/focus survive automatic refresh.
const {chromium} = require('playwright');
const fs = require('node:fs'), assert = require('node:assert/strict');
const dir = process.env.DASHQ_FINDINGS_EVIDENCE;
const html = fs.readFileSync(dir + '/page.html', 'utf8');
const js = ['assets/taskq/resource-controls.js', 'assets/resources.js', 'assets/taskq/project-blocks.js', 'assets/taskq/answer-card.js', 'assets/boards.js', 'assets/app.js'].map(f => fs.readFileSync(f, 'utf8')).join('\n');
(async () => {
  const browser = await chromium.launch({headless: true, ...(process.env.DASHQ_CHROME ? {executablePath: process.env.DASHQ_CHROME} : {})});
  try {
    for (const [width, size, theme] of [[1024, 16, 'light'], [360, 20, 'dark']]) {
      const page = await browser.newPage({viewport: {width, height: 900}, colorScheme: theme, reducedMotion: 'reduce'});
      await page.clock.install();
      let reads = 0;
      const writes = [], errors = [];
      page.on('pageerror', e => errors.push(e.message));
      await page.route('http://fixture/**', route => {
        const r = route.request(), path = new URL(r.url()).pathname;
        if (r.method() !== 'GET') writes.push(r.method());
        if (path === '/data') reads++;
        return route.fulfill({contentType: path === '/app.js' ? 'text/javascript' : path === '/data' ? 'application/json' : 'text/html', body: path === '/app.js' ? js : path === '/data' ? '{}' : html.replace('</head>', '<script src="/app.js" defer></script></head>')});
      });
      await page.goto('http://fixture/checks');
      await page.waitForFunction(() => !busy);
      await page.addStyleTag({content: `html{font-size:${size}px}`});
      assert.equal(await page.locator('.finding').count(), 3);
      const summaries = await page.locator('.finding summary').allTextContents();
      assert.equal(summaries.filter(s => s.includes('verified')).length, 1);
      assert.equal(summaries.filter(s => s.includes('open')).length, 2);
      const ids = await page.locator('.finding').evaluateAll(ds => ds.map(d => d.id));
      assert.equal(new Set(ids).size, 3);
      const d = page.locator('#' + ids[0]), summary = d.locator('summary');
      await summary.click();
      await summary.focus();
      const focus = await summary.getAttribute('id');
      for (let i = 0; i < 2; i++) {
        await page.clock.fastForward(60000);
        await page.waitForFunction(() => !busy);
        assert(await d.evaluate(d => d.open));
        assert.equal(await page.evaluate(() => document.activeElement.id), focus);
      }
      assert(reads >= 3);
      await page.locator('.finding').evaluateAll(ds => ds.forEach(d => d.open = true));
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await page.locator('.finding').first().scrollIntoViewIfNeeded();
      await page.screenshot({path: `${dir}/${width}-${theme}.png`});
      assert.deepEqual(writes, []);
      assert.deepEqual(errors, []);
      await page.close();
    }
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exit(1); });
