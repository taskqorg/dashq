// Usage: ssh -f -N -L 18883:127.0.0.1:18882 fra; NODE_PATH=<playwright 1.56.1>/node_modules node operator-redesign-live-187.cjs <file holding the guard subject>
// #187 live browser run: FRA dashq loopback through an SSH tunnel, guard subject header added by the route (D11 host-local trust).
// Every non-GET is aborted, so this run cannot write.
const {chromium} = require('playwright');
const fs = require('node:fs'), assert = require('node:assert/strict');
const tunnel = 'http://127.0.0.1:18883', subject = fs.readFileSync(process.argv[2], 'utf8').trim();
const log = (...a) => console.log(...a);
(async () => {
  const browser = await chromium.launch({headless: true});
  log('browser', browser.version());
  const page = await browser.newPage({viewport: {width: 1280, height: 900}});
  const errors = [], blocked = [];
  let offline = false, gets = 0;
  page.on('pageerror', e => errors.push(e.message));
  await page.route('http://dash.live/**', async route => { try {
    const req = route.request(), u = new URL(req.url());
    if (req.method() !== 'GET') { blocked.push(req.method() + ' ' + u.pathname); return route.abort(); }
    if (offline && (u.pathname === '/data' || u.pathname === '/' || !u.pathname.includes('.'))) return route.abort();
    gets++;
    const r = await route.fetch({url: tunnel + u.pathname + u.search, headers: {...req.headers(), host: '127.0.0.1:18882', 'x-dashq-subject': subject}});
    return await route.fulfill({response: r});
  } catch (e) { if (!/disposed|closed/.test(e.message)) throw e; } });
  const idle = () => page.waitForFunction(() => typeof busy === 'undefined' || !busy, {}, {timeout: 30000});
  await page.goto('http://dash.live/'); await idle();
  assert(await page.locator('#host-grid').isVisible(), 'host grid first');
  const hosts = await page.locator('#host-grid').innerText();
  log('DQ-01 overview: host grid visible,', (hosts.match(/\n/g) || []).length, 'lines; Unknown shown:', /unknown/i.test(hosts));
  const attention = await page.locator('#attention').innerText().catch(() => '');
  log('Attention:', attention.replace(/\s+/g, ' ').slice(0, 160));
  // Attention action link opens its target in /checks.
  const link = page.locator('.action-row a[href*="/checks"]').first();
  if (await link.count()) {
    const href = await link.getAttribute('href'); await link.click(); await idle();
    const id = decodeURIComponent(new URL(page.url()).hash.slice(1)) || 'checks';
    log('DQ-03 attention action:', new URL(page.url()).pathname, 'target present:', await page.locator(`[id="${id}"]`).count() === 1, href.split('#')[0]);
    await page.goBack(); await idle(); log('return:', new URL(page.url()).pathname);
  }
  // Filters survive navigation; detail selection survives refresh; back returns.
  await page.evaluate(() => navigateView(new URL('/?period=7200', location.origin))); await idle();
  const open = async p => {
    const a = page.locator('#view-' + p.slice(1));
    if (!await a.isVisible()) await page.locator(`details:has(#view-${p.slice(1)}) > summary`).click();
    await a.click(); await idle();
  };
  // DQ list > detail: a host's task count opens its tasks.
  const count = page.locator('#host-grid a.count-link').first();
  await count.click(); await idle();
  log('host count link:', new URL(page.url()).pathname + new URL(page.url()).search, 'rows:', await page.locator('[data-a=task-detail]').count());
  await page.goBack(); await idle(); log('return:', new URL(page.url()).pathname + new URL(page.url()).search, 'grid visible:', await page.locator('#host-grid').isVisible());
  for (const p of ['/hosts', '/agents', '/services', '/checks', '/integrations', '/projects']) {
    await open(p);
    const u = new URL(page.url());
    assert.equal(u.pathname, p); assert.equal(u.searchParams.get('period'), '7200', p + ' keeps period');
  }
  log('D22 views: six views via nav links, period=7200 kept');
  const detail = page.locator('[data-a=task-detail]').first();
  log('Tasks rows:', await page.locator('[data-a=task-detail]').count());
  await detail.focus(); await page.keyboard.press('Enter');
  assert(await page.locator('#taskq-popover').isVisible(), 'task detail opens');
  await page.evaluate(() => refresh()); await idle();
  assert(await page.locator('#taskq-popover').isVisible(), 'detail survives refresh');
  await page.keyboard.press('Escape');
  log('Tasks list > detail > refresh > close: pass');
  await page.goBack(); await idle(); log('history back:', new URL(page.url()).pathname + new URL(page.url()).search);
  // PM draft across navigation, refresh and outage; never sent.
  await page.evaluate(() => navigateView(new URL('/projects?period=7200', location.origin))); await idle();
  const draft = page.locator('#pm-draft');
  log('PM composer present:', await draft.count());
  if (await draft.count()) {
    const text = 'dq187 browser draft, not sent';
    await draft.fill(text);
    await open('/hosts');
    await page.goBack(); await idle();
    assert.equal(await draft.inputValue(), text, 'draft across navigation');
    await page.evaluate(() => refresh()); await idle();
    assert.equal(await draft.inputValue(), text, 'draft across refresh');
    offline = true; await page.evaluate(() => refresh()).catch(() => {}); await page.waitForTimeout(1500);
    const banner = await page.locator('#banner').isVisible() ? await page.locator('#banner').innerText() : '';
    assert(banner, 'outage banner'); assert.equal(await draft.inputValue(), text, 'draft across outage');
    assert((await page.locator('#boards-all, #taskq-workspace').first().innerText()).length > 0, 'board rows kept');
    log('DQ-10 page outage: banner', JSON.stringify(banner.replace(/\s+/g, ' ').slice(0, 120)), '; draft and rows kept');
    offline = false; await page.evaluate(() => refresh()); await idle();
    assert.equal(await page.locator('#banner').isVisible(), false, 'recovery clears banner');
    assert.equal(await draft.inputValue(), text, 'draft after recovery');
    log('recovery: banner cleared, draft kept');
    await draft.fill('');
  }
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no horizontal scroll');
  log('page errors:', JSON.stringify(errors), 'blocked writes:', JSON.stringify(blocked), 'GETs:', gets);
  assert.deepEqual(errors, []); assert.deepEqual(blocked, []);
  await browser.close(); log('PASS');
})().catch(e => { console.error('FAIL', e.message); process.exitCode = 1; });
