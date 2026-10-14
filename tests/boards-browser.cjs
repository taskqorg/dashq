// Chain D13 browser (release/visual check, needs Chrome + Playwright): fixture page -> pinned TaskQ blocks and card in
// Boards. Risk: counters/icons unreadable in a theme or size, a refresh drops focus/scroll/disclosures/card instances,
// Attention cannot reach the card, or long text overflows. Run: PLAYWRIGHT_MODULE=<path>/playwright node tests/boards-browser.cjs
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const fs = require('fs'), http = require('http'), assert = require('assert');
const out = process.env.DASHQ_BOARDS_EVIDENCE || 'docs/design/evidence/boards';
const read = f => fs.readFileSync(f, 'utf8');
// Same concatenation as src/main.rs app_js()/style_css().
const js = ['assets/taskq/project-blocks.js', 'assets/taskq/answer-card.js', 'assets/boards.js', 'assets/app.js'].map(read).join('\n');
const css = ['assets/style.css', 'assets/taskq/project-blocks.css', 'assets/taskq/answer-card.css'].map(read).join('\n');
// `snapshot` rewrites the served board rows, so a refresh can add or remove tasks.
let snapshot = boards => boards;
const unescape = s => s.replace(/&(quot|#39|lt|gt|amp);/g, (_, e) => ({quot: '"', '#39': "'", lt: '<', gt: '>', amp: '&'}[e]));
const escape = s => s.replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
const page = () => read('fixture/page.html').replace(/data-boards="([^"]*)"/, (_, b) => 'data-boards="' + escape(JSON.stringify(snapshot(JSON.parse(unescape(b))))) + '"');
const server = http.createServer((req, res) => {
  const p = new URL(req.url, 'http://x').pathname;
  const [type, body] = p === '/app.js' ? ['text/javascript', js] : p === '/style.css' ? ['text/css', css]
    : p === '/data' ? ['application/json', read('fixture/data.json')] : ['text/html', page()];
  res.setHeader('Content-Type', type + '; charset=utf-8'); res.end(body);
});
const long = 'taskqorg/' + 'a-very-long-project-name-'.repeat(4);
const lum = c => { const [r, g, b] = c.match(/[\d.]+/g).map(Number); return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255; };
(async () => {
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  const url = 'http://127.0.0.1:' + server.address().port + '/';
  fs.mkdirSync(out, {recursive: true});
  const browser = await chromium.launch({channel: 'chrome', headless: true});
  for (const [width, font, colorScheme] of [[1024, 16, 'light'], [1024, 20, 'dark'], [360, 16, 'dark'], [360, 20, 'light']]) {
    const page = await browser.newPage({viewport: {width, height: 900}, colorScheme, reducedMotion: 'reduce'});
    const errors = []; page.on('pageerror', e => errors.push(e.message));
    await page.goto(url); await page.addStyleTag({content: `html{font-size:${font}px}`});
    await page.waitForSelector('#boards .tqp-counts');
    const at = `${width}/${font}/${colorScheme}`;
    // counters: TaskQ order, zero hidden, unknown `?`, Closed quiet
    const labels = await page.$$eval('[id="board-taskqorg%2Ftaskq"] .tqp-counts button', bs => bs.map(b => b.getAttribute('aria-label').split(' · ')[1]));
    assert.deepEqual(labels, ['Doing: 1', 'Review: 1', 'Ask: 1', 'Failed: 1', 'Ready: 1', 'Closed: unknown'], at);
    assert.equal(await page.$eval('[id="board-taskqorg%2Ftaskq"] [data-state=closed]', b => b.className), 'quiet');
    assert.equal(await page.$$eval('[id="board-taskqorg%2Fagentq"] .tqp-counts button', bs => bs.filter(b => b.textContent.trim() === '?').length), 8, at + ' unread board unknown');
    // provider icon and counter text readable against the page (the #1041 dark-theme defect)
    const contrast = await page.$eval('[id="board-taskqorg%2Ftaskq"] [data-icon=github]', s => [getComputedStyle(s).fill, getComputedStyle(document.body).backgroundColor]);
    assert(Math.abs(lum(contrast[0]) - lum(contrast[1])) > 0.5, at + ' provider icon contrast ' + contrast);
    // one card per board/task, read-only with reason and issue link
    assert.equal(await page.locator('[id="board-taskqorg%2Ftaskq-q-12"]').count(), 1);
    const card = await page.locator('[id="board-taskqorg%2Ftaskq-q-12"]').innerText();
    assert(card.includes('Answer adapter has not been qualified') && card.includes('SQLite'), at + ' card: ' + card);
    assert.equal(await page.locator('[id="board-taskqorg%2Ftaskq-q-12"] a[href="https://github.com/taskqorg/taskq/issues/12"]').count() > 0, true);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), at + ' horizontal overflow');
    await page.locator('.first').screenshot({path: `${out}/${width}-${font}-${colorScheme}.png`});
    // Attention opens the single Boards card
    await page.locator('a[data-board-question="taskqorg/taskq"][data-task="12"]').click();
    assert(await page.evaluate(() => document.activeElement.closest('[id$="-q-12"]') !== null), at + ' Attention focus');
    // counter navigation opens the filtered task disclosure
    await page.locator('[id="board-taskqorg%2Ftaskq"] [data-state=ask]').click();
    assert(await page.$eval('[id="board-taskqorg%2Ftaskq-tasks"]', d => d.open && d.querySelector('summary').textContent === 'Tasks · ask'));
    // refresh keeps focus, scroll, disclosures and the card instance (its drafts live in the component)
    await page.evaluate(() => { document.getElementById('board-taskqorg%2Ftaskq-q-12').marked = 1; });
    await page.locator('[id="board-taskqorg%2Ftaskq"] [data-state=failed]').focus();
    await page.evaluate(() => scrollTo(0, 200));
    const before = await page.evaluate(() => [scrollY, document.activeElement.id]);
    for (let n = 0; n < 3; n++) await page.evaluate(() => refresh());
    assert.deepEqual(await page.evaluate(() => [scrollY, document.activeElement.id]), before, at + ' focus/scroll');
    assert(await page.$eval('[id="board-taskqorg%2Ftaskq-tasks"]', d => d.open), at + ' disclosure');
    assert(await page.evaluate(() => document.getElementById('board-taskqorg%2Ftaskq-q-12').marked === 1), at + ' card instance');
    assert.deepEqual(errors, [], at);
    await page.close();
  }
  // changing snapshot: a refresh that removes Doing keeps focus on the same Ask counter and card control, not a shifted one
  {
    const page = await browser.newPage({viewport: {width: 1024, height: 900}});
    await page.goto(url); await page.waitForSelector('#boards .tqp-counts');
    const ask = '[id="board-taskqorg%2Ftaskq"] [data-state=ask]';
    await page.locator(ask).focus();
    snapshot = boards => boards.map(b => ({...b, tasks: b.tasks && b.tasks.filter(t => !(b.project === 'taskqorg/taskq' && t.state === 'doing'))}));
    await page.evaluate(() => refresh());
    assert.equal(await page.$$eval('[id="board-taskqorg%2Ftaskq"] [data-state=doing]', bs => bs.length), 0, 'Doing removed');
    assert(await page.$eval(ask, b => b === document.activeElement), 'focus after Doing removed: ' + await page.evaluate(() => document.activeElement.id));
    await page.locator('[id="board-taskqorg%2Ftaskq-q-12"] a').first().focus();
    const link = await page.evaluate(() => document.activeElement.getAttribute('href'));
    snapshot = boards => boards;
    await page.evaluate(() => refresh());
    assert.equal(await page.evaluate(() => document.activeElement.closest('[id$="-q-12"]') && document.activeElement.getAttribute('href')), link, 'card focus after Doing restored');
    await page.close();
  }
  // narrow long-text fixture: long project, title and question wrap inside the column
  const page = await browser.newPage({viewport: {width: 360, height: 900}, colorScheme: 'dark'});
  await page.goto(url); await page.addStyleTag({content: 'html{font-size:20px}'});
  await page.evaluate(long => {
    const root = document.getElementById('boards'), b = JSON.parse(root.dataset.boards);
    b[0].project = long; b[0].tasks[2].title = 'Decide '.repeat(20); b[0].tasks[2].question.problem = 'Which-store-should-we-pick-'.repeat(8);
    root.dataset.boards = JSON.stringify(b); renderBoards();
  }, long);
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'long text overflow');
  await page.locator('#boards').screenshot({path: `${out}/360-20-dark-long.png`});
  await browser.close(); server.close();
  console.log('PASS boards browser (' + browser.version() + '): 4 size/theme layouts, long text, counters, icon contrast, Attention, 3 refreshes, changing-snapshot focus');
})().catch(e => { console.error(e); server.close(); process.exit(1); });
