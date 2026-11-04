// D27 / R1 / R2 / F3,F7: AssetQ view coverage, layout, drafts and refresh; the actions and their receipts are stubbed.
const {chromium}=require('playwright'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const dir=process.env.DASHQ_ASSETS_EVIDENCE,shots=process.env.DASHQ_ASSETS_SHOTS;
const read=f=>fs.readFileSync(f,'utf8');
const html=read(path.join(dir,'page.html'));
const unconfigured=read(path.join(dir,'unconfigured.html'));
const js=['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/taskq/task-workspace.js','assets/taskq/pm-composer.js','assets/workspace.js','assets/assetq.js','assets/boards.js','assets/app.js'].map(read).join('\n');
const css=read('assets/style.css');
(async()=>{
 const browser=await chromium.launch({headless:true});
 const cells=[];
 try{
  for(const width of [1280,360])for(const theme of ['light','dark'])for(const size of [16,24]){
   const page=await browser.newPage({viewport:{width,height:1000},colorScheme:theme,reducedMotion:'reduce'});
   let offline=false,reply={status:200,body:{state:'accepted',job:'job-new'}};const posts=[],errors=[];
   page.on('pageerror',e=>errors.push(e.message));
   await page.route('http://fixture/**',route=>{
    const req=route.request(),p=new URL(req.url()).pathname;
    if(p==='/assetq/control'){
     if(req.method()==='GET')return route.fulfill({status:200,contentType:'application/json',body:JSON.stringify({csrf:'invented-csrf'})});
     posts.push(JSON.parse(req.postData()));
     if(reply.abort)return route.abort();
     return route.fulfill({status:reply.status,contentType:'application/json',body:JSON.stringify(reply.body)});
    }
    return route.fulfill({status:offline&&p==='/data'?500:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':html});
   });
   await page.goto('http://fixture/assets');
   await page.waitForFunction(()=>!busy);
   await page.addStyleTag({content:`html{font-size:${size}px}`});
   const root=page.locator('#assets');
   assert(await root.isVisible());
   assert.equal(await page.locator('#view-assets').getAttribute('aria-current'),'page');
   const text=await root.innerText();
   for(const s of ['invented-project','Uncertain submissions 1','job-active','— · unknown: provider did not report usage','Cancellation requested','Backup','no fresh backup or restore observation recorded','Integrity','New job','Failed jobs · ≥1 (capped)','Retained records · ≥1 (capped)'])assert(text.includes(s),s);
   // A failed job shows AssetQ's reason and offers nothing to act on.
   await page.locator('[id="assetq-invented-project-failed"]').evaluate(d=>d.open=true);
   const failed=await page.locator('[id="assetq-invented-project-failed"]').innerText();
   for(const s of ['job-failed','validation refused the import','a verified manifest reports validation'])assert(failed.includes(s),s);
   assert.equal(await page.locator('[id="assetq-invented-project-failed"] form').count(),0,'a failed job offers an action');
   await page.locator('[id="assetq-invented-project-failed"]').evaluate(d=>d.open=false);
   assert(!text.includes('SECRET'),'content leaked');
   const noOverflow=()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1);
   assert(await noOverflow(),'horizontal overflow '+JSON.stringify(await page.evaluate(()=>[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>innerWidth+1).map(e=>[e.tagName,e.id,e.className]).slice(0,8))));
   // A draft keeps its text and the revision it started on across refresh (D10, D27).
   const id='assetq-invented-project-new-job';
   await page.locator(`[id="${id}"]`).evaluate(f=>f.closest('details').open=true);
   await page.locator(`[id="${id}-text"]`).fill('draft text');
   await page.locator(`[id="${id}-max-credits"]`).fill('1');
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert.equal(await page.locator(`[id="${id}-text"]`).inputValue(),'draft text','refresh lost draft');
   assert(await page.locator(`[id="${id}"]`).isVisible(),'refresh closed the form');
   assert(await noOverflow(),'overflow with an open form');
   // Unknown delivery keeps one operation; a repeat carries the same ID (D27).
   reply={abort:true};
   await page.locator(`[id="${id}"] button[type=submit]`).click();
   await page.locator(`[id="${id}"] .asset-receipt`,{hasText:'Unknown'}).waitFor();
   assert(await page.locator(`[id="${id}"] button[type=submit]`).isDisabled(),'unknown allowed a fresh operation');
   reply={status:200,body:{state:'accepted',job:'job-new',effect:'queued'}};
   await page.locator(`[id="${id}"] .asset-receipt button`).click();
   await page.locator(`[id="${id}"] .asset-receipt`,{hasText:'Accepted · job job-new'}).waitFor();
   await page.waitForFunction(()=>!busy);
   await page.locator(`[id="${id}"] .asset-receipt`,{hasText:'Accepted · job job-new'}).waitFor();
   assert.equal(posts.length,2);assert.equal(posts[0].op_id,posts[1].op_id,'repeat changed the operation');
   assert.equal(posts[0].action,'job.submit');assert.equal(posts[0].revision,'3:17');assert.equal(posts[0].method,'synthetic.fake@1');
   await page.waitForFunction(()=>!busy);
   // Refused and stale receipts keep the draft; invalid JSON sends nothing.
   const add='assetq-invented-project-add-method';
   await page.locator(`[id="${add}"]`).evaluate(f=>{let d=f.closest('details');while(d){d.open=true;d=d.parentElement.closest('details')}});
   const count=posts.length;
   await page.locator(`[id="${add}-id"]`).fill('invented.method');
   await page.locator(`[id="${add}-spec"]`).fill('not json');
   await page.locator(`[id="${add}"] button[type=submit]`).click();
   await page.locator(`[id="${add}"] .asset-receipt`,{hasText:'Nothing was sent'}).waitFor();
   assert.equal(posts.length,count,'invalid JSON was sent');
   reply={status:409,body:{state:'stale',reason:'revision_changed'}};
   await page.locator(`[id="${add}-spec"]`).fill('{"operation":"synthetic.generate"}');
   await page.locator(`[id="${add}"] button[type=submit]`).click();
   await page.locator(`[id="${add}"] .asset-receipt`,{hasText:'Stale · revision_changed'}).waitFor();
   // The stale receipt fetches the new revision; the receipt must survive that swap (a field named `id` once broke its key).
   await page.waitForFunction(()=>!busy);
   await page.locator(`[id="${add}"] .asset-receipt`,{hasText:'Stale · revision_changed'}).waitFor();
   assert.equal(await page.locator(`[id="${add}-id"]`).inputValue(),'invented.method','stale lost the draft');
   // A failed refresh keeps values and marks them unconfirmed.
   offline=true;
   const before=await root.innerText();
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert(await page.locator('#banner').isVisible());
   assert((await page.locator('[data-assets-state]').first().innerText()).includes('Unconfirmed'));
   assert((await root.innerText()).includes('job-active'),'failed refresh lost values:'+before.length);
   assert.deepEqual(errors,[]);
   cells.push({width,theme,size,result:'pass'});
   await page.close();
   const staticPage=await browser.newPage({javaScriptEnabled:false,viewport:{width,height:1000},colorScheme:theme});
   await staticPage.route('**/*',route=>route.abort());
   await staticPage.setContent(html.replace(/<script[^>]*>[\s\S]*?<\/script>/g,'').replace(/<link[^>]*>/g,'').replace('</head>',`<style>${css} html{font-size:${size}px}</style></head>`));
   assert(await staticPage.locator('#assets').count(),'server-rendered AssetQ missing without JavaScript');
   assert.equal(await staticPage.locator('#assets fieldset:not([disabled])').count(),0,'an action is enabled without the adapter');
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
  if(shots){
   fs.mkdirSync(shots,{recursive:true});
   for(const [name,width,open] of [['assets-desktop',1280,false],['assets-narrow',360,true]]){
    const page=await browser.newPage({viewport:{width,height:1000},colorScheme:'light',reducedMotion:'reduce'});
    await page.route('http://fixture/**',route=>{const p=new URL(route.request().url()).pathname;return route.fulfill({status:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':html});});
    await page.goto('http://fixture/assets');await page.waitForFunction(()=>!busy);
    await page.evaluate(open=>document.querySelectorAll('#assets .asset-group').forEach(d=>d.open=!open?d.id.endsWith('-methods'):!d.id.endsWith('-records')),open);
    await page.evaluate(open=>{if(open)document.querySelector('[id$="-submit"]').open=true},open);
    await page.screenshot({path:path.join(shots,name+'.png'),fullPage:true});
    await page.close();
   }
  }
  fs.writeFileSync(path.join(dir,'browser.json'),JSON.stringify({platform:process.platform,browser:browser.version(),cells},null,2)+'\n');
  console.log(`AssetQ browser: ${cells.length} cells passed; drafts, delivery, refusal, failed refresh, failed jobs, not configured`);
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
