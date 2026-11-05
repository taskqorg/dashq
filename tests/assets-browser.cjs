// D29 / R1 / R2 / F3,F7: read-only AssetQ view coverage, layout, links and refresh.
const {chromium}=require('playwright'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const dir=process.env.DASHQ_ASSETS_EVIDENCE,shots=process.env.DASHQ_ASSETS_SHOTS;
const read=f=>fs.readFileSync(f,'utf8');
const html=read(path.join(dir,'page.html'));
const unconfigured=read(path.join(dir,'unconfigured.html'));
const healthy=read(path.join(dir,'page-ok.html'));
const js=['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/taskq/task-workspace.js','assets/taskq/pm-composer.js','assets/workspace.js','assets/boards.js','assets/app.js'].map(read).join('\n');
const css=read('assets/style.css');
(async()=>{
 const browser=await chromium.launch({headless:true});
 const cells=[];
 try{
  for(const width of [1280,360])for(const theme of ['light','dark'])for(const size of [16,24]){
   const page=await browser.newPage({viewport:{width,height:1000},colorScheme:theme,reducedMotion:'reduce'});
   let offline=false;const writes=[],errors=[];
   page.on('pageerror',e=>errors.push(e.message));
   await page.route('http://fixture/**',route=>{
    const req=route.request(),p=new URL(req.url()).pathname;
    if(req.method()!=='GET')writes.push(p);
    return route.fulfill({status:offline&&p==='/data'?500:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':html});
   });
   await page.goto('http://fixture/assets');
   await page.waitForFunction(()=>!busy);
   await page.addStyleTag({content:`html{font-size:${size}px}`});
   const root=page.locator('#assets');
   assert(await root.isVisible());
   assert.equal(await page.locator('#view-assets').getAttribute('aria-current'),'page');
   const text=await root.innerText();
   for(const s of ['invented-project','Uncertain submissions 1','job-active','— · unknown: provider did not report usage','Cancellation requested','Backup','no fresh backup or restore observation recorded','Integrity','Failed jobs · ≥1 (capped)','Retained records · ≥1 (capped)','All jobs and new requests on AssetQ','this page only reads'])assert(text.includes(s),s);
   // D6: producer times use the shared relative/exact formatter, including without JavaScript below.
   const updated=root.locator('time[data-at]').first();
   assert((await updated.innerText()).endsWith(' ago'));
   assert(await updated.getAttribute('title'),'viewer-local exact time is available on hover');
   assert((await page.locator('[data-assets-state]').innerText()).includes('<1m old'));
   // D29: no write control, and every link opens the AssetQ site, never a machine path.
   assert.equal(await root.locator('form,button,input,select,textarea').count(),0,'the view holds a write control');
   const hrefs=await root.locator('a').evaluateAll(a=>a.map(x=>x.getAttribute('href')));
   assert(hrefs.length>0);
   for(const h of hrefs){assert(h.startsWith('https://assets.example.com'),h);assert(!/^https:\/\/[^/]+\/v1\//.test(h),h);}
   // A failed job shows AssetQ's reason and links only its job page.
   await page.locator('[id="assetq-invented-project-failed"]').evaluate(d=>d.open=true);
   const failed=await page.locator('[id="assetq-invented-project-failed"]').innerText();
   for(const s of ['job-failed','validation refused the import','a verified manifest reports validation'])assert(failed.includes(s),s);
   assert(!text.includes('SECRET'),'content leaked');
   const noOverflow=()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1);
   assert(await noOverflow(),'horizontal overflow '+JSON.stringify(await page.evaluate(()=>[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>innerWidth+1).map(e=>[e.tagName,e.id,e.className]).slice(0,8))));
   // An open disclosure survives refresh (D10).
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert.equal(await page.locator('[id="assetq-invented-project-failed"]').evaluate(d=>d.open),true,'refresh closed a disclosure');
   assert(await noOverflow(),'overflow with an open disclosure');
   // A failed refresh keeps values and marks them unconfirmed.
   offline=true;
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert(await page.locator('#banner').isVisible());
   assert((await page.locator('[data-assets-state]').first().innerText()).includes('Unconfirmed'));
   assert((await root.innerText()).includes('job-active'),'failed refresh lost values');
   assert.deepEqual(writes,[],'the view sent a write');
   assert.deepEqual(errors,[]);
   cells.push({width,theme,size,result:'pass'});
   await page.close();
   const staticPage=await browser.newPage({javaScriptEnabled:false,viewport:{width,height:1000},colorScheme:theme});
   await staticPage.route('**/*',route=>route.abort());
   await staticPage.setContent(html.replace(/<script[^>]*>[\s\S]*?<\/script>/g,'').replace(/<link[^>]*>/g,'').replace('</head>',`<style>${css} html{font-size:${size}px}</style></head>`));
   assert(await staticPage.locator('#assets').count(),'server-rendered AssetQ missing without JavaScript');
   assert((await staticPage.locator('#assets time[data-at]').first().innerText()).includes('ago (2026-11-05 10:05 UTC)'));
   await staticPage.close();
   // Not configured: no navigation entry and no panel; /assets names the missing configuration, also after a refresh.
   const bare=await browser.newPage({viewport:{width,height:1000},colorScheme:theme,reducedMotion:'reduce'});
   await bare.route('http://fixture/**',route=>{const p=new URL(route.request().url()).pathname;return route.fulfill({status:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':unconfigured});});
   await bare.goto('http://fixture/assets');await bare.waitForFunction(()=>!busy);
   for(const round of ['load','refresh']){
    if(round==='refresh'){await bare.evaluate(()=>refresh());await bare.waitForFunction(()=>!busy);}
    assert((await bare.locator('#view-unavailable').innerText()).includes('AssetQ is not configured'),round);
    assert(await bare.locator('#view-unavailable').isVisible(),round);
    assert.equal(await bare.locator('#assets').count(),0,'an unconfigured product created a panel');
    assert.equal(await bare.locator('#view-assets').count(),0,'an unconfigured product kept its navigation entry');
   }
   await bare.close();
  }
  // D6/R2: green needs an ok source. Defect guarded: a retained health value stayed green or read as confirmed.
  for(const [body,green,after] of [[html,0,'healthy · source incomplete'],[healthy,5,'healthy · source unconfirmed']]){
   const page=await browser.newPage({viewport:{width:1280,height:1000},reducedMotion:'reduce'});
   let offline=false;
   await page.route('http://fixture/**',route=>{const p=new URL(route.request().url()).pathname;return route.fulfill({status:offline&&p==='/data'?500:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':body});});
   await page.goto('http://fixture/assets');await page.waitForFunction(()=>!busy);
   assert.equal(await page.locator('#assets .s.ok').count(),green);
   offline=true;
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert.equal(await page.locator('#assets .s.ok').count(),0,'green after a failed refresh');
   assert((await page.locator('#assets').innerText()).includes(after),after);
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert(!(await page.locator('#assets').innerText()).includes('source unconfirmed · source unconfirmed'),'a second failure repeats the label');
   await page.close();
  }
  if(shots){
   fs.mkdirSync(shots,{recursive:true});
   for(const [name,width,open] of [['assets-desktop',1280,false],['assets-narrow',360,true]]){
    const page=await browser.newPage({viewport:{width,height:1000},colorScheme:'light',reducedMotion:'reduce'});
    await page.route('http://fixture/**',route=>{const p=new URL(route.request().url()).pathname;return route.fulfill({status:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':html});});
    await page.goto('http://fixture/assets');await page.waitForFunction(()=>!busy);
    await page.evaluate(open=>document.querySelectorAll('#assets .asset-group').forEach(d=>d.open=!open?d.id.endsWith('-methods'):!d.id.endsWith('-records')),open);
    await page.screenshot({path:path.join(shots,name+'.png'),fullPage:true});
    await page.close();
   }
  }
  fs.writeFileSync(path.join(dir,'browser.json'),JSON.stringify({platform:process.platform,browser:browser.version(),cells},null,2)+'\n');
  console.log(`AssetQ browser: ${cells.length} cells passed; read-only, links, failed refresh, failed jobs, not configured, no green under a non-ok source`);
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
