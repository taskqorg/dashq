// #222 A1–A4 / D11,D22,D23: the Rust renderer's route/coverage, links and refresh preserve the same evidence.
const {chromium} = require('playwright');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const input = process.env.DASHQ_TELEMETRY_EVIDENCE, capture = process.argv.includes('--capture');
const read = file => fs.readFileSync(file, 'utf8');
const html = read(path.join(input, 'page.html')), doc = JSON.parse(read(path.join(input, 'data.json')));
const jsFiles = ['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/taskq/task-workspace.js','assets/taskq/pm-composer.js','assets/workspace.js','assets/boards.js','assets/app.js'];
const cssFiles = ['assets/style.css','assets/taskq/resource-controls.css','assets/resources.css','assets/taskq/project-blocks.css','assets/taskq/answer-card.css','assets/taskq/pm-composer.css','assets/workspace.css','assets/taskq/task-workspace.css'];
const js = jsFiles.map(read).join('\n'), css = cssFiles.map(f=>f.endsWith('/task-workspace.css') ? `@scope (#taskq-workspace) {${read(f)}}` : read(f)).join('\n');
const digest = s => crypto.createHash('sha256').update(s.replace(/\r\n/g,'\n')).digest('hex');
const out = 'docs/design/evidence/telemetry';
(async () => {
  const browser = await chromium.launch({headless:true, ...(process.env.DASHQ_CHROME ? {executablePath:process.env.DASHQ_CHROME} : {})});
  const report = {kind:'runtime-fixture-evidence', rules:['R12/R13','D11/D22/D23','#222 A1–A4'], browser:browser.version(), platform:process.platform,
    before:'cargo test --locked telemetry_view: FAIL #222: missing dedicated Telemetry view',
    after:'PASS: six stage states and independent coverage from the scoped document',
    inputs:Object.fromEntries(['dashq.md','src/page.rs','src/enroll.rs','src/store.rs',...jsFiles,...cssFiles,'tests/telemetry-browser.cjs'].map(f=>[f,digest(read(f))])),
    rendered:digest(html), cells:[], notTested:['production telemetry','deployment','owner visual acceptance']};
  try {
    for (const [width,size] of [[1280,16],[360,20],[1280,24]]) for (const theme of ['light','dark']) {
      const page = await browser.newPage({viewport:{width,height:1000}, colorScheme:theme, reducedMotion:'reduce'});
      let failed = false, scenario = 'page';
      const errors = [], writes = [];
      page.on('pageerror', e=>errors.push(e.message));
      await page.clock.install();
      await page.route('http://fixture/**', route=> {
        const req = route.request(), u = new URL(req.url()), p = u.pathname;
        if (req.method() !== 'GET') writes.push(req.method()+' '+p);
        const body = p==='/app.js' ? js : p==='/style.css' ? css : p==='/data' ? JSON.stringify(doc) : read(path.join(input, scenario+'.html'));
        return route.fulfill({status:failed&&p==='/data'?500:200,
          contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html', body});
      });
      const query = '?period=1800&host=win&agent=codex&session=example';
      await page.goto('http://fixture/telemetry'+query); await page.waitForFunction(()=>!busy);
      await page.addStyleTag({content:`html{font-size:${size}px}`});
      assert(await page.locator('#telemetry').isVisible(), 'direct entry');
      assert(!await page.locator('#host-grid').isVisible());
      await page.locator('#view-home').click();
      await page.locator('#more > summary').click();
      await page.locator('#view-telemetry').click();
      assert.equal(new URL(page.url()).pathname,'/telemetry','More navigation');
      assert.equal(new URL(page.url()).search,query,'More preserves filters');
      await page.locator('#more > summary').click();
      assert.equal(await page.locator('body').evaluate(e=>getComputedStyle(e).fontSize), size+'px');
      const sender = page.locator('#telemetry-inst-tq-w');
      const stages = Object.keys(doc.installations.rows[0].route);
      assert.deepEqual(await sender.locator('[data-stage]').evaluateAll(es=>es.map(e=>e.dataset.stage)), stages);
      for (const [index,row] of doc.installations.rows.entries()) {
        const article = page.locator('.telemetry-sender').nth(index);
        for (const key of stages) {
          const step = article.locator(`[data-stage=${key}]`), s = row.route[key];
          assert.equal(await step.getAttribute('data-state'), s.state);
          const value = s.state!=='known' ? '—' : s.value===true ? 'Yes' : s.value===false ? 'No' : String(s.value);
          const text = await step.innerText();
          assert(text.includes(value) && text.includes('Evidence time:'));
          if (s.at===null) assert(text.includes('Evidence time: —'));
          if (s.reason) assert(text.includes(s.reason));
          assert.equal(await step.locator('.ok,.good').count(),0,'no implied pass from a stage state');
        }
      }
      const text = await sender.innerText();
      for (const expected of ['seq 10–4','seq 20–22','40 holes','90 rows','Restarts: 1','capacity: 5 rows','retention: 8 rows','allowlist: 1 rows','status not reported by taskq']) assert(text.includes(expected),expected);
      assert((await page.locator('#telemetry-inst-counted-w').innerText()).includes('Backlog: 0 rows'), 'known zero stays zero');
      const details = sender.locator('details'); await details.locator('summary').click();
      assert((await details.innerText()).includes('200 of 202 (newest 200)'), 'capped list names total');
      const evidence = new URL(await details.locator('a').first().getAttribute('href'), 'http://fixture');
      assert.equal(evidence.pathname,'/data'); assert.equal(evidence.searchParams.get('block'),'block-2');
      for (const [key,value] of new URLSearchParams(query)) assert.equal(evidence.searchParams.get(key),value);
      assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'ordinary text never overflows');
      const layout = await sender.locator('[data-stage]').evaluateAll(es=>es.map(e=>({x:e.getBoundingClientRect().x,y:e.getBoundingClientRect().y})));
      const columns = new Set(layout.map(p=>p.x)).size;
      if (width===360) assert.equal(columns,1,'narrow steps reflow');
      else if (size===24) assert(columns<6,'large text reflows without smaller type');
      else assert.equal(columns,6,'desktop route keeps all stages together');
      if (capture && width===1280 && size===16 && theme==='light') {
        fs.mkdirSync(out,{recursive:true});
        await details.evaluate(e=>e.open=false);
        await page.screenshot({path:path.join(out,'delivery.png'),fullPage:true});
        await details.evaluate(e=>e.open=true);
      }
      await details.locator('summary').focus();
      const scroll = await page.evaluate(()=>scrollY);
      await page.clock.fastForward(60000); await page.waitForFunction(()=>!busy);
      assert(await details.evaluate(e=>e.open),'disclosure survives automatic refresh');
      assert(await details.locator('summary').evaluate(e=>e===document.activeElement),'focus survives automatic refresh');
      assert.equal(await page.evaluate(()=>scrollY),scroll);
      failed = true; await page.evaluate(()=>refresh());
      assert(await page.locator('#banner').isVisible(),'outage is explicit');
      assert((await sender.innerText()).includes('seq 10–4'),'outage keeps retained evidence');
      failed = false; await page.evaluate(()=>refresh());
      assert(!await page.locator('#banner').isVisible(),'recovery clears warning');
      await sender.locator('a[data-view-link]').first().click();
      assert.equal(new URL(page.url()).pathname,'/integrations');
      assert(await page.locator('#inst-tq-w').isVisible(),'stale installation detail is revealed');
      assert(await page.locator('#show-inactive').isChecked());
      await page.goBack();
      assert(await page.locator('#telemetry').isVisible()); assert(await details.evaluate(e=>e.open));
      assert.equal(new URL(page.url()).search,query);
      await page.reload(); await page.waitForFunction(()=>!busy);
      assert(await page.locator('#telemetry').isVisible(),'reload restores encoded view');
      for (scenario of ['empty','unreceived','pruned']) {
        await page.evaluate(()=>refresh());
        assert(await page.locator('#telemetry').isVisible());
        const text = await page.locator('#telemetry').innerText();
        assert(text.includes(scenario==='empty'?'no sender observations':scenario==='unreceived'?'no telemetry block yet':'every stored block was pruned'));
        if (scenario!=='empty') assert(text.includes('holes and restarts unavailable') && !text.includes('0 holes') && !text.includes('Restarts: 0'));
        assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
      }
      assert.deepEqual(errors,[]); assert.deepEqual(writes,[]);
      report.cells.push({width,text_px:size,theme,result:'PASS'}); await page.close();
    }
    if (capture) fs.writeFileSync(path.join(out,'manifest.json'),JSON.stringify(report,null,2)+'\n');
    console.log(`PASS Telemetry: ${report.cells.length} width/text/theme cells; stage states, coverage, evidence links, navigation, refresh, outage/recovery, empty/pruned and no writes; Chromium ${browser.version()}`);
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
