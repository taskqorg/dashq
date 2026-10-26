// #219 A1–A4 / F1,F2,F6: actual renderer versus the hash-pinned public derivative.
// Protect hierarchy/geometry, honest trends and disclosure/refresh/return; synthetic sources only.
const {chromium}=require('playwright'), fs=require('node:fs'), path=require('node:path'), crypto=require('node:crypto'), assert=require('node:assert/strict');
const read=f=>fs.readFileSync(f,'utf8'), dir=process.env.DASHQ_OVERVIEW_EVIDENCE;
const reference=read('docs/design/operator-redesign/overview-candidate.html');
const digest=s=>crypto.createHash('sha256').update(s.replace(/\r\n/g,'\n')).digest('hex');
assert.equal(digest(reference),JSON.parse(read('docs/design/operator-redesign/approval.json')).public_reference_sha256,'F1 public reference pin');
const html=read(process.env.DASHQ_OVERVIEW_PAGE||path.join(dir,'page.html'));
const js=['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/boards.js','assets/app.js'].map(read).join('\n');
const css=['assets/style.css','assets/taskq/resource-controls.css','assets/resources.css','assets/taskq/project-blocks.css','assets/taskq/answer-card.css'].map(read).join('\n');
(async()=>{
 const browser=await chromium.launch({headless:true});
 const report={reference:digest(reference),browser:browser.version(),platform:process.platform,inputs:Object.fromEntries(['src/page.rs','assets/app.js','assets/style.css','assets/overview-icons.svg','tests/overview-browser.cjs'].map(f=>[f,digest(read(f))])),rendered:digest(html),cells:[],checks:[]};
 try {
  for(const [width,size] of [[1280,16],[360,20],[1280,24]]) for(const theme of ['light','dark']) {
   const page=await browser.newPage({viewport:{width,height:1000},colorScheme:theme,reducedMotion:'reduce'});
   const ref=await browser.newPage({viewport:{width,height:1000},colorScheme:theme});
   let offline=false; const errors=[],writes=[];
   page.on('pageerror',e=>errors.push(e.message));
   await page.route('http://fixture/**',route=>{
    const p=new URL(route.request().url()).pathname;
    if(route.request().method()!=='GET')writes.push(p);
    return route.fulfill({status:offline&&p==='/data'?500:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':html});
   });
   await ref.setContent(reference);
   await ref.addStyleTag({content:`body{margin:0}#dq-overview{margin:auto;font-size:${size}px}#dq-server-study{--ds-font:${size}px!important}`});
   await page.clock.install();
   await page.goto('http://fixture/');await page.waitForFunction(()=>!busy);
   await page.addStyleTag({content:`html{font-size:${size}px}`});
   assert.equal(await page.locator('#host-grid .overview-host').count(),3,'A1: minimal host cards replace the #170 seven-column layout');
   assert.deepEqual(await page.locator('.overview-name').allTextContents(),await ref.locator('.ds-name').allTextContents(),'identical host scenarios');
   const measures=async(p,card,name,metric,chart)=>p.evaluate(({card,name,metric,chart})=>{
    const cards=[...document.querySelectorAll(card)],r=cards[1].getBoundingClientRect(),m=document.querySelector(metric)?.getBoundingClientRect(),c=document.querySelector(chart)?.getBoundingClientRect();
    const style=getComputedStyle(cards[1]);return {width:r.width,padding:parseFloat(style.paddingLeft),radius:parseFloat(style.borderRadius),name:parseFloat(getComputedStyle(document.querySelector(name)).fontSize),metricWidth:m?.width,chartWidth:c?.width,chartHeight:c?.height,chartRight:m?.right-c?.right,columns:new Set(cards.map(e=>Math.round(e.getBoundingClientRect().x))).size};
   },{card,name,metric,chart});
   const expected=await measures(ref,'[data-cards]>article','.ds-name','[data-scenario="warning"] .ds-metric','.ds-spark');
   const actual=await measures(page,'#host-grid .overview-host','.overview-name','#host-FRA .overview-metric','#host-FRA .overview-metric svg.trend');
   for(const key of ['width','padding','radius','name','metricWidth','chartWidth','chartHeight','chartRight','columns'])assert(Math.abs(actual[key]-expected[key])<=1,`${width}/${size}/${theme} reference ${key}: ${JSON.stringify({actual,expected})}`);
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'ordinary text reflows');
   assert.equal(await page.locator('#host-grid .products:visible').count(),0,'versions stay in detail');
   assert.equal(await page.locator('#host-grid .overview-summary').evaluateAll(es=>es.some(e=>e.textContent.includes('?'))),false,'no repeated question-mark cells');
   assert.equal(await page.locator('#host-FRA .overview-metric svg.trend').count(),3);
   assert(await page.locator('#host-FRA .overview-metric svg.trend').evaluateAll(es=>es.every(e=>e.checkVisibility())),'trends are visible');
   assert((await page.locator('#host-FRA .overview-number').allTextContents()).join(' ').includes('6.2 / 16.0 GB'),'real current values with units');
   assert.equal(await page.locator('#host-Mac .overview-summary polyline,#host-Desktop .overview-summary polyline').count(),0,'unavailable has no invented history');
   assert((await page.locator('#host-grid').innerText()).includes('Last 30 minutes · 0–100%'));
   assert((await page.locator('#host-Desktop .overview-summary').innerText()).includes('No recent data'));
   const details=page.locator('#host-FRA > details');await details.locator(':scope > summary').click();await details.locator(':scope > summary').focus();
   offline=true;await page.evaluate(()=>refresh());
   assert((await page.locator('#host-FRA .overview-state').innerText()).includes('Unconfirmed resource data'),'outage marks retained resources: '+await page.locator('#host-FRA .overview-state').innerText());
   assert(await details.evaluate(e=>e.open),'failed refresh retains disclosure');
   assert(await details.locator(':scope > summary').evaluate(e=>e===document.activeElement),'failed refresh retains focus');
   await page.evaluate(()=>document.querySelector('#host-FRA .overview-summary').dataset.resourceValidFor='1');
   await page.clock.fastForward(2000);
   assert((await page.locator('#host-FRA .overview-state').innerText()).includes('Stale resource data'),'sample expires during outage without a fetch');
   offline=false;await page.evaluate(()=>refresh());
   assert(await page.locator('#host-FRA > details').evaluate(e=>e.open),'successful refresh retains disclosure');
   await page.locator('#host-FRA .overview-open').click();await page.waitForFunction(()=>!busy);
   assert.equal(new URL(page.url()).pathname,'/checks','producer finding opens its action');
   await page.goBack();assert(await page.locator('#host-grid').isVisible(),'Back restores Overview');
   assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
   report.cells.push({width,size,theme,result:'PASS',actual,expected});
   if(process.argv.includes('--capture')&&width===1280&&size===16&&theme==='light') {
    await page.locator('#host-FRA > details > summary').click();
    await page.screenshot({path:path.join(dir,'overview.png'),fullPage:true});
   }
   await page.close();await ref.close();
  }
  report.checks=['hierarchy','reference geometry','honest missing history','disclosure/focus across outage/recovery','direct finding and Back','no writes'];
  fs.writeFileSync(path.join(dir,'fidelity.json'),JSON.stringify(report,null,2)+'\n');
  console.log(`PASS Overview fidelity: ${report.cells.length} size/text/theme cells; pinned reference, trends, detail, refresh and return`);
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
