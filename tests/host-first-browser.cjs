// #170 A1/A2/A5, D8/D22: actual renderer, readable reflow, truthful gaps, direct question and scoped tasks.
// Existing startup/resources chains retain draft, outage, receipt and automatic-refresh protection.
const {chromium} = require('playwright');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const dir = process.env.DASHQ_HOST_FIRST_EVIDENCE, capture = process.argv.includes('--capture');
const html = fs.readFileSync(path.join(dir, 'page.html'), 'utf8');
const jsFiles = ['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js', 'assets/boards.js','assets/app.js'];
const cssFiles = ['assets/style.css','assets/taskq/resource-controls.css','assets/resources.css','assets/taskq/project-blocks.css','assets/taskq/answer-card.css'];
const out = 'docs/design/evidence/host-first-runtime';
const report = {kind:'runtime-fixture-evidence',rules:['D8/D10/D22','R12/R13','specq F1–F7'],reference:'docs/design/evidence/host-first/manifest.json',
  before:'host_first_actions_and_unknowns FAIL: hosts must precede Attention',after:'PASS: real renderer hosts precede human actions; history and shared controls retain their chains',
  matrix:[],screens:[],hashes:{},notTested:['production deployment','live host/login/assigned-queue probes','unreleased full TaskQ workspace API','owner acceptance']};
(async () => {
  const browser = await chromium.launch({headless:true});
  report.browser = browser.version(); report.platform = process.platform;
  if (capture) fs.mkdirSync(out, {recursive:true});
  try {
    for (const width of [1280,360]) for (const size of [16,20]) for (const theme of ['light','dark']) {
      const page = await browser.newPage({viewport:{width,height:900},colorScheme:theme,reducedMotion:'reduce'});
      const errors = [], writes = [];
      page.on('pageerror',e => errors.push(e.message));
      await page.route('http://fixture/**', route => {
        const req = route.request(), p = new URL(req.url()).pathname;
        if (req.method() !== 'GET') writes.push(req.method()+' '+p);
        const body = p === '/app.js' ? jsFiles.map(f=>fs.readFileSync(f,'utf8')).join('\n') : p === '/style.css' ? cssFiles.map(f=>fs.readFileSync(f,'utf8')).join('\n') : p === '/data' ? '{}' : p === '/answer' ? '{"reason":"fixture: no answer authority"}' : html;
        return route.fulfill({contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'||p==='/answer'?'application/json':'text/html',body});
      });
      await page.goto('http://fixture/');
      await page.waitForFunction(()=>!busy);
      await page.addStyleTag({content:`html{font-size:${size}px}`});
      assert.equal(await page.locator('body').evaluate(e=>getComputedStyle(e).fontSize),size+'px','keep requested text size');
      assert(await page.locator('#host-grid').isVisible());
      assert(!await page.locator('#boards').isVisible());
      assert(!await page.locator('#servers').isVisible());
      assert(!await page.locator('#installations').isVisible());
      assert.equal(await page.locator('.first').count(),0);
      assert.equal(await page.locator('#host-grid polyline').count(),0,'trends belong to detail');
      assert((await page.locator('#host-grid').innerText()).includes('Unknown or unavailable'));
      await page.evaluate(()=>navigateView(new URL('/?period=86400&host=fra%2Fnative&agent=codex&session=fixture',location.origin)));
      await page.waitForFunction(()=>!busy);
      const inherited = new URL(await page.locator('#attention-link').getAttribute('href'),page.url());
      assert.equal(inherited.search,'?period=86400&host=fra%2Fnative&agent=codex&session=fixture','Attention keeps the current filter');
      await page.evaluate(()=>navigateView(new URL('/',location.origin)));
      await page.waitForFunction(()=>!busy);
      const grid = await page.locator('#host-grid').boundingBox(), attention = await page.locator('#attention').boundingBox();
      assert.equal(await page.locator('#main > section:visible').first().getAttribute('id'),'host-grid','hosts are the first visible section');
      assert(grid.y < page.viewportSize().height && grid.y < attention.y,'hosts start in the first viewport before Attention: '+JSON.stringify({width,size,theme,grid,attention}));
      for (const view of ['/', '/hosts', '/projects']) {
        await page.evaluate(url=>navigateView(new URL(url,location.origin)), view);
        await page.waitForFunction(()=>!busy);
        if(view==='/projects')assert(await page.locator('#boards .tqp-task').first().isVisible(),'Tasks opens its first board rows');
        const geometry = await page.evaluate(()=>({scrollWidth:document.documentElement.scrollWidth,viewport:innerWidth,
          overflow:[...document.querySelectorAll('#host-grid .cell,#attention a,#boards .tqp-task')].filter(e=>e.getClientRects().length).filter(e=>{const r=e.getBoundingClientRect();return r.left<-.5||r.right>innerWidth+.5;}).map(e=>e.textContent.slice(0,50))}));
        assert(geometry.scrollWidth<=width,JSON.stringify({width,size,theme,view,...geometry}));
        assert.deepEqual(geometry.overflow,[]);
        report.matrix.push({width,size,theme,view,result:'PASS'});
        const shot = size===16 && view==='/' && (width===1280||theme==='dark') ? `hosts-${width}-${theme}.png`
          : width===1280&&size===16&&view==='/hosts'&&theme==='dark' ? 'host-detail.png'
          : width===1280&&size===16&&view==='/projects'&&theme==='light' ? 'tasks.png' : null;
        if(capture&&shot) { await page.screenshot({path:path.join(out,shot),fullPage:true});report.screens.push(shot); }
      }
      await page.evaluate(()=>navigateView(new URL('/',location.origin)));
      await page.locator('#attention [data-board-question]').first().click();
      await page.waitForFunction(()=>!busy);
      assert.equal(new URL(page.url()).pathname,'/projects');
      assert.equal(await page.locator('[id="board-fixture-dashq-q-304"]').count(),1);
      assert(await page.locator('#boards').isVisible());
      await page.goBack(); assert.equal(new URL(page.url()).pathname,'/');
      assert(await page.locator('#host-grid').isVisible());
      await page.locator('#host-grid .count-link').first().click();
      await page.waitForFunction(()=>!busy);
      assert.equal(new URL(page.url()).pathname,'/projects');
      assert(new URL(page.url()).searchParams.get('host'));
      assert((await page.locator('#boards').innerText()).includes('assigned queue unknown'));
      assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
      await page.close();
    }
    if(capture) {
      for(const f of ['src/page.rs','src/data.rs','tests/host-first-browser.cjs','docs/design/host-first-fixture.json',...jsFiles,...cssFiles]) report.hashes[f]=crypto.createHash('sha256').update(fs.readFileSync(f,'utf8').replace(/\r\n/g,'\n')).digest('hex');
      report.hashes['rendered-page.html']=crypto.createHash('sha256').update(html.replace(/\r\n/g,'\n')).digest('hex');
      fs.writeFileSync(path.join(out,'manifest.json'),JSON.stringify(report,null,2)+'\n');
    }
    console.log(`PASS host-first runtime: ${report.matrix.length} matrix cells; navigation, direct answer, honest gaps, no writes`);
  } finally { await browser.close(); }
})().catch(e=>{console.error(e);process.exitCode=1;});
