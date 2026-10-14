// Chain D13 browser (release/visual check, needs Chrome + Playwright): fixture page -> pinned TaskQ blocks and card in
// Boards. Risk: counters/icons unreadable in a theme or size, a refresh drops focus/scroll/disclosures/card instances,
// the card sends another form or a new operation on retry than the answer route expects,
// Attention cannot reach the card, or long text overflows. Run: PLAYWRIGHT_MODULE=<path>/playwright node tests/boards-browser.cjs
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const fs = require('fs'), http = require('http'), assert = require('assert');
const out = process.env.DASHQ_BOARDS_EVIDENCE || 'docs/design/evidence/boards';
const read = f => fs.readFileSync(f, 'utf8');
// Same concatenation as src/main.rs app_js()/style_css().
const js = ['assets/taskq/resource-controls.js', 'assets/resources.js', 'assets/taskq/project-blocks.js', 'assets/taskq/answer-card.js', 'assets/boards.js', 'assets/app.js'].map(read).join('\n');
const css = ['assets/style.css', 'assets/taskq/resource-controls.css', 'assets/resources.css', 'assets/taskq/project-blocks.css', 'assets/taskq/answer-card.css'].map(read).join('\n');
// `snapshot` rewrites the served board rows, so a refresh can add or remove tasks.
let snapshot = boards => boards;
// `answers`: null = the deployment has no `answer` config (route 503); else {csrf, posts, reply(form) -> [status, receipt] | 'drop'}.
let answers = null;
const disabled = {outcome: 'rejected', reason: 'Answers are not enabled on this dashboard', next: null};
const unescape = s => s.replace(/&(quot|#39|lt|gt|amp);/g, (_, e) => ({quot: '"', '#39': "'", lt: '<', gt: '>', amp: '&'}[e]));
const escape = s => s.replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
const page = () => read('fixture/page.html').replace(/data-boards="([^"]*)"/, (_, b) => 'data-boards="' + escape(JSON.stringify(snapshot(JSON.parse(unescape(b))))) + '"');
const server = http.createServer((req, res) => {
  const p = new URL(req.url, 'http://x').pathname;
  if (p === '/answer') {
    let body = '';
    req.on('data', d => body += d).on('end', () => {
      const r = !answers ? [503, disabled] : req.method === 'GET' ? [200, {csrf: answers.csrf}]
        : (answers.posts.push({origin: req.headers.origin, type: req.headers['content-type'], form: Object.fromEntries(new URLSearchParams(body))}), answers.reply(answers.posts.at(-1).form));
      // a lost reply as the browser sees it behind the proxy: an error page, not a receipt (a reset socket makes
      // Chrome resend the POST itself)
      if (r === 'drop') { res.writeHead(502, {'Content-Type': 'text/html'}); return res.end('<h1>502 Bad Gateway</h1>'); }
      const [status, v] = r;
      res.writeHead(status, {'Content-Type': 'application/json'}); res.end(JSON.stringify(v));
    });
    return;
  }
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
    await page.waitForFunction(() => !document.getElementById('board-taskqorg%2Ftaskq-q-12').innerText.includes('Checking answer access'));
    const card = await page.locator('[id="board-taskqorg%2Ftaskq-q-12"]').innerText();
    assert(card.includes('Answers are not enabled on this dashboard') && card.includes('SQLite'), at + ' card: ' + card);
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
  // counter tint (overview.md): a refresh tints changed and newly nonzero counters only; reduced motion drops the animation
  {
    const page = await browser.newPage({viewport: {width: 1024, height: 900}, reducedMotion: 'no-preference'});
    await page.goto(url); await page.waitForSelector('#boards .tqp-counts');
    const tinted = () => page.$$eval('#boards .tqp-counts button.tint', bs => bs.map(b => [b.parentElement.closest('[id]').id, b.dataset.state, getComputedStyle(b).animationName]));
    const board = 'board-taskqorg%2Ftaskq';
    // Doing 1 -> 0 (hidden), Ready 1 -> 2, Later 0 -> 1; Review, Ask, Failed and Closed `?` unchanged
    snapshot = boards => boards.map(b => b.project !== 'taskqorg/taskq' ? b : {...b, tasks: [...b.tasks.filter(t => t.state !== 'doing'),
      {...b.tasks.find(t => t.state === 'ready'), n: 15}, {...b.tasks.find(t => t.state === 'ready'), n: 16, state: 'later'}]});
    await page.evaluate(() => refresh());
    assert.deepEqual(await tinted(), [[board, 'ready', 'tint'], [board, 'later', 'tint']], 'changed and newly nonzero counters tint');
    await page.evaluate(() => refresh());
    assert.deepEqual(await tinted(), [], 'unchanged values do not tint');
    await page.emulateMedia({reducedMotion: 'reduce'});
    snapshot = boards => boards;
    await page.evaluate(() => refresh());
    assert.deepEqual(await tinted(), [[board, 'doing', 'none'], [board, 'ready', 'none']], 'reduced motion: marked, no animation');
    await page.close();
  }
  // answers enabled (#44): choice and free text post the route's form; a lost reply shows unknown and its retry resends
  // the same op_id and payload; stale keeps the draft. Live board qualification: tests/answer-live.cjs.
  {
    const receipt = (f, outcome, reason) => ({task: Number(f.task), op_id: 'dashq.' + f.op_id, outcome, state: outcome === 'accepted' ? 'doing' : 'ask', revision: Number(f.revision), reason, next: null});
    const plan = [f => [200, receipt(f, 'accepted', null)], () => 'drop', f => [200, receipt(f, 'accepted', null)], f => [409, receipt(f, 'stale', 'The question changed')]];
    answers = {csrf: 'c1', posts: [], reply: f => plan[answers.posts.length - 1](f)};
    const page = await browser.newPage({viewport: {width: 1024, height: 900}});
    const errors = []; page.on('pageerror', e => errors.push(e.message));
    await page.goto(url);
    const q = '[id="board-taskqorg%2Ftaskq-q-12"]';
    await page.waitForSelector(q + ' button[data-choice="1"]:not([disabled])');
    // D6/D13: route access must not enable a retained or incomplete board observation after refresh.
    for (const source of [{state: 'stale', incomplete: false}, {state: 'ok', incomplete: true}]) {
      snapshot = boards => boards.map(b => b.project !== 'taskqorg/taskq' ? b : {...b, source: {...b.source, ...source}});
      await page.evaluate(() => refresh());
      assert(await page.locator(q + ' button[data-choice="1"]').isDisabled(), 'incomplete observation stays read-only');
      assert.equal(answers.posts.length, 0);
    }
    snapshot = boards => boards;
    await page.evaluate(() => refresh());
    await page.waitForSelector(q + ' button[data-choice="1"]:not([disabled])');
    await page.locator(q + ' button[data-choice="1"]').click();
    await page.waitForFunction(q => document.querySelector(q).innerText.includes('Answer accepted on the board'), q);
    const [first] = answers.posts;
    assert.deepEqual({...first.form, op_id: 'x'}, {board: 'taskqorg/taskq', csrf: 'c1', task: '12', revision: '6139994000', op_id: 'x', choice: '1'});
    assert(first.type.startsWith('application/x-www-form-urlencoded') && first.origin === url.slice(0, -1), 'form and Origin ' + JSON.stringify(first));
    // the next question (new revision) answered with free text; the reply is lost
    snapshot = boards => boards.map(b => b.project !== 'taskqorg/taskq' ? b : {...b, tasks: b.tasks.map(t => t.n === 12 ? {...t, revision: t.revision + 1} : t)});
    await page.evaluate(() => refresh());
    // the answered card stays bound to its old question until the owner reads the new one
    await page.locator(q + ' button[data-qac="review"]').click();
    await page.$eval(q + ' details.qac-other', d => { d.open = true; });
    await page.locator(q + ' textarea').fill('Use SQLite with WAL');
    await page.locator(q + ' button[type=submit]').click();
    await page.waitForFunction(q => document.querySelector(q).innerText.includes('Delivery unknown'), q);
    await page.locator(q + ' button[data-qac="retry"]').click();
    await page.waitForFunction(q => document.querySelector(q).innerText.includes('Answer accepted on the board'), q);
    assert.deepEqual(answers.posts[2].form, answers.posts[1].form, 'retry keeps op_id and payload');
    assert.equal(answers.posts[1].form.text, 'Use SQLite with WAL');
    assert.notEqual(answers.posts[1].form.op_id, first.form.op_id);
    // stale: the server re-read a moved revision; the draft stays for review
    snapshot = boards => boards.map(b => b.project !== 'taskqorg/taskq' ? b : {...b, tasks: b.tasks.map(t => t.n === 12 ? {...t, revision: t.revision + 2} : t)});
    await page.evaluate(() => refresh());
    // the answered card stays bound to its old question until the owner reads the new one
    await page.locator(q + ' button[data-qac="review"]').click();
    await page.$eval(q + ' details.qac-other', d => { d.open = true; });
    await page.locator(q + ' textarea').fill('Files after all');
    await page.locator(q + ' button[type=submit]').click();
    await page.waitForFunction(q => document.querySelector(q).innerText.includes('The question changed'), q);
    assert.equal(await page.locator(q + ' textarea').inputValue(), 'Files after all', 'stale keeps the draft');
    assert.equal(answers.posts.length, 4);
    assert.deepEqual(errors, []);
    snapshot = boards => boards; answers = null;
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
  console.log('PASS boards browser (' + browser.version() + '): 4 size/theme layouts, long text, counters, icon contrast, Attention, 3 refreshes, changing-snapshot focus, counter tint, answer route (choice, lost reply retry, free text, stale)');
})().catch(e => { console.error(e); server.close(); process.exit(1); });
