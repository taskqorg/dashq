// D30/A2-A6/R2: table, scoped link payload, native detail, refresh and honest outage states; invented inputs only.
const {chromium}=require('playwright'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const dir=process.env.DASHQ_SERVERS_EVIDENCE||path.join(process.env.DASHQ_HOST_FIRST_EVIDENCE,'servers'),read=f=>fs.readFileSync(f,'utf8');
const html=read(path.join(dir,'page.html'));
const js=['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/taskq/task-workspace.js','assets/taskq/pm-composer.js','assets/workspace.js','assets/boards.js','assets/app.js'].map(read).join('\n');
(async()=>{
 const browser=await chromium.launch({headless:true,...(process.env.DASHQ_CHROME?{executablePath:process.env.DASHQ_CHROME}:{})});
 const cells=[];
 try {
  for(const width of [1280,360])for(const theme of ['light','dark'])for(const size of [16,24]){
   const page=await browser.newPage({viewport:{width,height:1000},colorScheme:theme,reducedMotion:'reduce'});
   let offline=false;const errors=[],writes=[];
   page.on('pageerror',e=>errors.push(e.message));
   await page.route('http://fixture/**',r=>{
    const req=r.request(),p=new URL(req.url()).pathname;if(req.method()!=='GET')writes.push(p);
    return r.fulfill({status:offline&&p==='/data'?500:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?read('assets/style.css'):p==='/data'?'{}':html});
   });
   await page.clock.install({time:new Date(JSON.parse(read('fixture/servers.json')).now*1000)});await page.goto('http://fixture/hosts');await page.waitForFunction(()=>!busy);
   await page.addStyleTag({content:`html{font-size:${size}px}`});
   const table=page.locator('#server-table');assert(await table.isVisible());
   assert.equal(await table.locator('tbody > tr').count(),3);
   const partial=table.locator('tbody > tr').filter({hasText:'invented-partial/native'}).locator('.server-source').first();
   assert.equal(await partial.getAttribute('data-server-state'),'incomplete','partial inventory must retain its initial coverage state');
   const text=await table.innerText();for(const s of ['invented-fresh/native','invented-stale/native','invented-partial/native','stale','collection-failed','unknown','estimated disk full'])assert(text.includes(s),s);
   assert.equal(await table.locator('form,button,input').count(),0);
   const contrast=await table.locator('span.unknown').first().evaluate(e=>{
    const luminance=s=>s.match(/[0-9.]+/g).slice(0,3).map(Number).map(n=>n/255).map(n=>n<=0.04045?n/12.92:((n+0.055)/1.055)**2.4).reduce((s,n,i)=>s+n*[0.2126,0.7152,0.0722][i],0);
    const a=luminance(getComputedStyle(e).color),b=luminance(getComputedStyle(document.body).backgroundColor);
    return (Math.max(a,b)+0.05)/(Math.min(a,b)+0.05);
   });
   assert(contrast>=4.5,'D7 unknown text contrast '+contrast);
   const detail=table.locator('details').filter({hasText:'invented-fresh/native'});await table.locator('tbody > tr').first().locator('th a').click();assert(await detail.evaluate(e=>e.open),'server link did not open native detail');await detail.locator('summary').focus();
   for(const s of ['Projects','Services','Largest folders','/srv/invented-<cache>'])assert((await detail.innerText()).includes(s),s);
   // #272 A2/A3: every row opens the Telegram chat with its server selected; a lost payload or a stale pending label fails here.
   const links=table.locator('a[href^="https://t.me/"]');assert.equal(await links.count(),3,'one Telegram link per server row');
   for(const href of await links.evaluateAll(es=>es.map(e=>e.href)))assert.match(href,/^https:\/\/t\.me\/invented_pilot_bot\?start=server-[0-9a-f]{16}$/);
   assert.equal(await table.locator('tbody > tr').first().locator('a[href^="https://t.me/"]').getAttribute('href'),'https://t.me/invented_pilot_bot?start=server-0ad79b58009edbd1');
   assert(!text.includes('chat selection pending'),'supported link still reads pending');
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'page overflow outside table '+JSON.stringify(await page.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,items:[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>innerWidth+1&&getComputedStyle(e).position!=='absolute').slice(0,12).map(e=>[e.tagName,e.id,e.className,e.getBoundingClientRect().width])}))));
   const comparison=table.locator('table');await comparison.evaluate(e=>e.scrollLeft=80);const x=await comparison.evaluate(e=>e.scrollLeft);
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert(await detail.evaluate(e=>e.open),'refresh closed detail');
   assert(await detail.locator('summary').evaluate(e=>e===document.activeElement),'refresh lost focus');
   assert.equal(await comparison.evaluate(e=>e.scrollLeft),x,'refresh lost table scroll');
   if(process.env.DASHQ_SERVERS_SHOTS&&width===1280&&size===16&&theme==='light')await table.screenshot({path:path.join(dir,'servers.png')});
   await partial.evaluate(e=>e.dataset.serverValidFor='1');
   await page.clock.fastForward(2000);await page.evaluate(()=>expireHostObservations());
   assert.equal(await partial.getAttribute('data-server-state'),'stale','partial inventory must expire while refresh is deferred');
   assert((await partial.innerText()).includes('producer reports partial inventory'),'expiry lost the partial collection reason');
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   offline=true;await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert((await table.innerText()).includes('Unconfirmed inventory'),'failed refresh left fresh status');
   assert.equal(await partial.getAttribute('data-server-state'),'unconfirmed','failed refresh must also mark partial inventory unconfirmed');
   assert((await partial.innerText()).includes('producer reports partial inventory'),'outage lost the partial collection reason');
   assert(!(await table.innerText()).includes('estimated disk full'),'outage kept a current-looking forecast');
   assert(await detail.evaluate(e=>e.open));
   offline=false;await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert((await table.innerText()).includes('estimated disk full'),'recovery did not restore observed estimate');
   assert.equal(await partial.getAttribute('data-server-state'),'incomplete','recovery must restore partial coverage, never healthy');
   await page.evaluate(()=>{document.querySelectorAll('[data-server-valid-for]').forEach(e=>e.dataset.serverValidFor='1')});
   await page.clock.fastForward(2000);await page.evaluate(()=>expireHostObservations());
   assert((await table.innerText()).includes('Stale inventory'),'time expiry left fresh status');
   assert.equal(await partial.getAttribute('data-server-state'),'stale','partial inventory must expire while refresh is deferred');
   assert((await partial.innerText()).includes('producer reports partial inventory'),'expiry lost the partial collection reason');
   assert(!(await table.innerText()).includes('estimated disk full'));
   assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);cells.push({width,theme,size,result:'PASS'});
   await page.close();
  }
  const page=await browser.newPage({javaScriptEnabled:false});await page.setContent(html);assert(await page.locator('#server-table').isVisible());assert.equal(await page.locator('#server-table tbody > tr').count(),3);await page.locator('#server-table tbody > tr').first().locator('th a').click();assert(await page.locator('#server-table details').first().evaluate(e=>e.open),'no-JS native detail did not open');await page.close();
  fs.writeFileSync(path.join(dir,'browser.json'),JSON.stringify({cells,checks:['fresh/stale/partial','details','link selector','scope metadata only','focus/disclosure/table scroll across refresh','outage/expiry/recovery','no writes','no-JS']},null,2)+'\n');
  console.log(`PASS Servers: ${cells.length} width/text/theme cells; detail, selectors, refresh, outage, expiry, no writes`);
 }finally{await browser.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
