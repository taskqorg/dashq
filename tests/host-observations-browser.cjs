// #178 A2/A3/A6, D6: stored TaskQ observations, expiry and refresh failure in the actual renderer.
const {chromium} = require('playwright');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const input = process.env.DASHQ_HOST_OBSERVATIONS_EVIDENCE;
const html = fs.readFileSync(path.join(input, 'page.html'), 'utf8');
const js = ['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/boards.js','assets/app.js'].map(f=>fs.readFileSync(f,'utf8')).join('\n');
const css = ['assets/style.css','assets/taskq/resource-controls.css','assets/resources.css','assets/taskq/project-blocks.css','assets/taskq/answer-card.css'].map(f=>fs.readFileSync(f,'utf8')).join('\n');
(async()=>{
  const browser = await chromium.launch({headless:true, ...(process.env.DASHQ_CHROME ? {executablePath:process.env.DASHQ_CHROME} : {})});
  let cells = 0;
  try {
    for(const width of [1280,360]) for(const size of [16,20]) for(const theme of ['light','dark']) {
      const page = await browser.newPage({viewport:{width,height:1000},colorScheme:theme,reducedMotion:'reduce'});
      let failed = false;
      const errors = [], writes = [];
      page.on('pageerror',e=>errors.push(e.message));
      await page.clock.install();
      await page.route('http://fixture/**',route=>{
        const req = route.request(), p = new URL(req.url()).pathname;
        if(req.method()!=='GET')writes.push(req.method()+' '+p);
        return route.fulfill({status:failed&&p==='/data'?500:200,
          contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',
          body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':html});
      });
      await page.goto('http://fixture/');await page.waitForFunction(()=>!busy);
      await page.addStyleTag({content:`html{font-size:${size}px}`});
      const host = page.locator('#host-win');
      assert((await host.innerText()).includes('logged in'));
      assert((await host.innerText()).includes('2 queued'));
      const evidence = host.locator('details');await evidence.locator('summary').click();
      assert((await evidence.innerText()).includes('aaaaaaaaaaaa'));
      assert((await evidence.innerText()).includes('valid until'));
      assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no horizontal overflow');
      if(process.argv.includes('--capture')&&width===1280&&size===16) {
        fs.mkdirSync('docs/design/evidence/host-observations',{recursive:true});
        await page.screenshot({path:`docs/design/evidence/host-observations/stored-${theme}.png`,fullPage:true});
      }
      await evidence.locator('summary').focus();
      await page.evaluate(()=>{const e=document.querySelector('#host-win .host-observation');e.dataset.validFor='1';});
      await page.clock.fastForward(2000);
      assert.equal(await host.locator('.host-observation').first().innerText(),'unknown','expiry without a new fetch');
      await page.evaluate(()=>document.querySelectorAll('#host-win .host-observation[data-valid-for]').forEach(e=>{e.dataset.validFor='5';}));
      const claims = await host.locator('.host-observation').allTextContents();
      const titles = await host.locator('.host-observation').evaluateAll(es=>es.map(e=>e.title));
      failed = true;await page.evaluate(()=>refresh());
      const retained = claims.map(x=>x==='unknown'?x:x+' · unconfirmed');
      assert.deepEqual(await host.locator('.host-observation').allTextContents(),retained,'D5/D6: failed refresh retains unexpired values as unconfirmed');
      assert(await host.locator('.host-observation[data-valid-for]').evaluateAll(es=>es.every(e=>e.classList.contains('unconfirmed'))),'retained values have non-ok state');
      assert(await host.locator('.host-observation').evaluateAll((es,titles)=>es.every((e,i)=>e.title.includes(titles[i])),titles),'original observation time remains in title');
      await page.clock.fastForward(2000);
      await page.evaluate(()=>refresh());
      assert.deepEqual(await host.locator('.host-observation').allTextContents(),retained,'repeated failure does not duplicate the state or reset expiry');
      assert(await evidence.evaluate(e=>e.open),'disclosure preserved');
      assert(await evidence.locator('summary').evaluate(e=>e===document.activeElement),'focus preserved');
      await page.clock.fastForward(2000);
      assert((await host.locator('.host-observation').allTextContents()).every(x=>x==='unknown'),'unconfirmed values still expire without a fetch');
      await page.evaluate(()=>refresh());
      assert((await host.locator('.host-observation').allTextContents()).every(x=>x==='unknown'),'failure cannot restore expired values');
      failed = false;await page.evaluate(()=>refresh());
      assert((await page.locator('#host-win').innerText()).includes('logged in'),'recovery reads producer evidence');
      assert.equal(await page.locator('#host-win .host-observation.unconfirmed').count(),0,'recovery clears unconfirmed state');
      assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
      await page.close();cells++;
    }
    console.log(`PASS stored host observations: ${cells} width/text/theme cells; producer/expiry, outage, recovery, focus and no writes; Chromium ${browser.version()}`);
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
