// D19 U1-U4 read-only chain: server-rendered states -> source outage -> recovery.
// Risk: fake empty/healthy state, lost retained evidence or UI state, or a recovery write.
const {chromium} = require('playwright');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const dir = process.env.DASHQ_STARTUP_EVIDENCE;
const js = ['assets/taskq/resource-controls.js', 'assets/resources.js', 'assets/taskq/project-blocks.js', 'assets/taskq/answer-card.js', 'assets/boards.js', 'assets/app.js'].map(f => fs.readFileSync(f, 'utf8')).join('\n');
const css = ['assets/style.css', 'assets/taskq/resource-controls.css', 'assets/resources.css', 'assets/taskq/project-blocks.css', 'assets/taskq/answer-card.css'].map(f => fs.readFileSync(f, 'utf8')).join('\n');
const states = ['first-launch', 'first-failure', 'taskq-only', 'empty-board', 'pending', 'warmup', 'partial', 'all-failed', 'recovery'];
(async () => {
  const browser = await chromium.launch({headless: true});
  try {
    for (const [width, size, theme] of [[1024, 16, 'light'], [360, 20, 'dark']]) {
      const page = await browser.newPage({viewport: {width, height: 768}, colorScheme: theme, reducedMotion: 'reduce'});
      await page.clock.install({time: new Date()});
      const errors = [], writes = [];
      page.on('pageerror', e => errors.push(e.message));
      let state = 'first-launch', offline = false, unavailable = false, recoveryTest = false;
      await page.route('http://fixture/**', async route => {
        const req = route.request(), url = new URL(req.url()), endpoint = url.pathname;
        const scenario = recoveryTest ? state : url.searchParams.get('case') || state;
        if (req.method() !== 'GET') writes.push(req.method() + ' ' + endpoint);
        if (offline && (endpoint === '/' || endpoint === '/data')) return route.abort();
        const asset = endpoint === '/app.js' || endpoint === '/style.css';
        const body = endpoint === '/app.js' ? js : endpoint === '/style.css' ? css : fs.readFileSync(path.join(dir, scenario + (endpoint === '/data' ? '.json' : '.html')), 'utf8');
        return route.fulfill({status: endpoint === '/data' && unavailable ? 503 : 200,
          contentType: asset ? endpoint.endsWith('.js') ? 'text/javascript' : 'text/css' : endpoint === '/data' ? 'application/json' : 'text/html', body});
      });
      for (state of states) {
        unavailable = state === 'first-failure';
        await page.goto('http://fixture/?case=' + state);
        await page.waitForFunction(() => !busy);
        await page.addStyleTag({content: 'html{font-size:' + size + 'px}'});
        const at = state + ' ' + width + '/' + size + '/' + theme;
        assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), at + ' overflow');
        if (state === 'first-launch') {
          assert.equal(await page.locator('#startup a[href="/connect"]').count(), 1, at);
          assert.equal(await page.locator('#servers,#boards,#agents,#checks,svg').count(), 0, at);
        }
        if (state === 'first-failure') {
          assert((await page.locator('#boards').textContent()).includes('No board observation received yet'), at);
          assert(await page.locator('#banner').isVisible(), at + ' 503 banner');
        }
        if (state === 'taskq-only' || state === 'empty-board') {
          assert.equal(await page.locator('#servers,#agents,#checks,#resources').count(), 0, at);
          assert((await page.locator('#boards-all').innerText()).includes('1 of 1 boards fresh'), at);
        }
        if (state === 'empty-board') {
          assert((await page.locator('#boards').textContent()).includes('No matching tasks observed'), at);
          assert.equal(await page.locator('#boards [data-state="doing"]').count(), 0, at + ' no fake zero');
        }
        if (state === 'pending') assert((await page.locator('#installations').innerText()).includes('Pending · first snapshot readback'), at);
        if (state === 'warmup') assert.equal(await page.locator('#servers polyline').count(), 0, at);
        if (state === 'partial' || state === 'all-failed') {
          assert((await page.locator('#boards-all').innerText()).includes('Current coverage'), at);
          assert((await page.locator('#boards').innerText()).includes('Last-known counts · not current'), at);
        }
        assert.equal(await page.locator('#boards button[type=submit]:enabled,#boards button[data-qac=choose]:enabled').count(), 0, at + ' read-only');
        if (process.env.DASHQ_STARTUP_SHOTS) {
          fs.mkdirSync(process.env.DASHQ_STARTUP_SHOTS, {recursive: true});
          await page.screenshot({path: path.join(process.env.DASHQ_STARTUP_SHOTS, state + '-' + width + '-' + size + '-' + theme + '.png')});
        }
      }
      // A total transport failure preserves the same card, focus, disclosure and scroll while counts become unknown.
      state = 'recovery'; unavailable = false; recoveryTest = true;
      await page.goto('http://fixture/'); await page.waitForFunction(() => !busy);
      const card = '[id="board-taskqorg%2Ftaskq-q-12"]';
      await page.evaluate(() => { document.getElementById('board-taskqorg%2Ftaskq-q-12').marked = 1; document.getElementById('attention-gaps').open = true; });
      // Test-only in-memory capability, like the shared Work mode fixture; never a production fallback or transport.
      await page.evaluate(() => {
        const c = boardCards.get('board-taskqorg%2Ftaskq-q-12');
        const previous = c.el;
        c.el = previous.cloneNode(false); previous.replaceWith(c.el); c.el.marked = 1;
        window.startupSent = [];
        c.widget = TaskqAnswerCard.mount(c.el, {capability: (_, row) => { row.question.limits = {lines: 4, chars: 240}; return {answer: JSON.parse(document.getElementById('boards').dataset.boards)[0].source.state === 'ok'}; },
          send: (_, op) => { startupSent.push(op); return Promise.reject(new Error('Fixture has no transport')); }});
        const row = JSON.parse(document.getElementById('boards').dataset.boards)[0].tasks.find(t => t.n === 12);
        c.widget.update([{board: 'taskqorg/taskq', row}]);
      });
      await page.locator(card + ' summary').filter({hasText: 'Other answer'}).click();
      await page.locator(card + ' textarea').fill('Retained draft');
      await page.locator(card + ' a').first().focus();
      await page.evaluate(() => scrollTo(0, 200));
      const before = await page.evaluate(() => [document.activeElement.id, scrollY]);
      const ageBefore = await page.$eval('#servers [data-age-s]', e => Number(e.dataset.ageS));
      offline = true;
      await page.clock.fastForward(61000);
      await page.waitForFunction(() => !busy);
      assert.deepEqual(await page.evaluate(() => [document.activeElement.id, scrollY]), before, 'offline UI retention');
      assert((await page.locator('#boards-all').innerText()).includes('0 of'), 'offline coverage');
      assert((await page.locator('#banner').innerText()).includes('Current source state unknown'), 'offline state');
      assert(await page.$eval('#servers [data-age-s]', e => Number(e.dataset.ageS)) >= ageBefore + 60, 'producer age advances through failed automatic refresh');
      state = 'partial'; offline = false;
      await page.evaluate(() => refresh());
      state = 'recovery';
      for (let i = 0; i < 3; i++) { await page.clock.fastForward(60000); await page.waitForFunction(() => !busy); }
      assert.deepEqual(await page.evaluate(() => [document.activeElement.id, scrollY]), before, 'recovery UI retention');
      assert.deepEqual(await page.evaluate(() => [document.getElementById('board-taskqorg%2Ftaskq-q-12').marked, document.getElementById('attention-gaps').open]), [1, true], 'card and disclosure retained');
      assert.equal(await page.locator(card + ' textarea').inputValue(), 'Retained draft', 'revision-bound draft retained');
      assert.deepEqual(await page.evaluate(() => startupSent), [], 'draft never replays on reconnect');
      assert.equal(await page.locator('#banner').isVisible(), false, 'recovery clears banner');
      assert.deepEqual(writes, [], 'no reconnect writes');
      assert.deepEqual(errors, [], 'no browser errors');
      await page.close();
    }
    console.log('PASS D19 read-only startup/outage/recovery at 1024/16 light and 360/20 dark');
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
