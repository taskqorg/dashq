// #221 A1–A4 / F3,F7: Reports uses the Rust document, preserves scope and reaches real stage evidence.
const {chromium}=require('playwright'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const dir=process.env.DASHQ_REPORTS_EVIDENCE,read=f=>fs.readFileSync(f,'utf8');
const html=read(process.env.DASHQ_REPORTS_PAGE||path.join(dir,'page.html'));
assert(html.includes('id="reports"'),'#221: scoped Reports view missing');
const doc=JSON.parse(read(path.join(dir,'page.json'))),page2=read(path.join(dir,'page2.html')),empty=read(path.join(dir,'empty.html'));
const retained=Object.fromEntries(['incomplete','stale','unconfirmed'].map(state=>[state,read(path.join(dir,state+'.html'))]));
const jsFiles=['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/taskq/task-workspace.js','assets/taskq/pm-composer.js','assets/workspace.js','assets/boards.js','assets/app.js'];
const cssFiles=['assets/style.css','assets/taskq/resource-controls.css','assets/resources.css','assets/taskq/project-blocks.css','assets/taskq/answer-card.css','assets/taskq/pm-composer.css','assets/workspace.css','assets/taskq/task-workspace.css'];
const js=jsFiles.map(read).join('\n'),css=cssFiles.map(f=>f.endsWith('/task-workspace.css')?`@scope (#taskq-workspace){${read(f)}}`:read(f)).join('\n');
const digest=s=>crypto.createHash('sha256').update(s.replace(/\r\n/g,'\n')).digest('hex');
const reference=read('docs/design/operator-redesign/overview-candidate.html');
assert.equal(digest(reference),JSON.parse(read('docs/design/operator-redesign/approval.json')).public_reference_sha256);
(async()=>{
 const browser=await chromium.launch({headless:true});
 const report={rules:['#221 A1–A4','D21/D22/D24','R12/R13','F3/F7'],platform:process.platform,browser:browser.version(),reference:digest(reference),rendered:digest(html),inputs:Object.fromEntries(['src/page.rs','src/agent_work.rs','src/model_use.rs','assets/app.js','assets/style.css','tests/reports-browser.cjs'].map(f=>[f,digest(read(f))])),cells:[]};
 try{
  for(const [width,size] of [[1280,16],[360,20],[1280,24]])for(const theme of ['light','dark']){
   const page=await browser.newPage({viewport:{width,height:1000},colorScheme:theme,reducedMotion:'reduce'}),ref=await browser.newPage({viewport:{width,height:1000}});
   let offline=false,state=null,hideAttempt=false;const writes=[],errors=[];
   page.on('pageerror',e=>errors.push(e.message));
   await page.route('http://fixture/**',route=>{
    const u=new URL(route.request().url()),p=u.pathname;
    if(route.request().method()!=='GET')writes.push(p);
    let body=u.searchParams.get('host')==='absent'?empty:u.searchParams.get('cursor')==='200'?page2:state?retained[state]:html;
    if(hideAttempt)body=body.replace(/<a id="report-attempt-221-1"[^>]*>[\s\S]*?<\/a>/,'');
    return route.fulfill({status:offline&&p==='/data'?500:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':body});
   });
   const url='http://fixture/reports?period=1800&host=mac&agent=claude&session=s1';
   await page.goto(url);await page.waitForFunction(()=>!busy);await page.addStyleTag({content:`html{font-size:${size}px}`});
   assert(await page.locator('#reports').isVisible());assert.equal(await page.locator('#agentwork:visible,#host-grid:visible,#modeluse:visible').count(),0);
   assert.equal(await page.locator('#view-reports').getAttribute('aria-current'),'page');
   for(const [field,divisor] of [['elapsed_ms',1000],['execution_ms',1000],['queue_wait_s',1]]){
    const t=doc.agent_work.totals[field],text=await page.locator('#report-'+field).innerText();
    assert(text.includes((t.sum/divisor).toFixed(3)+' s'));assert(text.includes(`${t.known} of ${t.of}`));
    for(const why of t.reasons)assert(text.includes(why));
   }
   const estimate=await page.locator('#report-estimate').innerText();assert(estimate.includes('2.00 EUR + 5.00 USD')&&estimate.includes('2 of 3')&&estimate.includes('output_tokens not reported'));
   assert(!(await page.locator('#reports').innerText()).includes('Actual expenses'));
   assert((await page.locator('#reports').innerText()).includes('agent/session filters apply to model use'));
   await ref.setContent(reference);await ref.locator('[data-page=reports]').evaluate(e=>e.hidden=false);
   await ref.addStyleTag({content:`#dq-overview{font-size:${size}px}`});
   const geometry=(p,selector)=>p.locator(selector).evaluate(e=>({columns:getComputedStyle(e).gridTemplateColumns.split(' ').length,gap:parseFloat(getComputedStyle(e).columnGap)}));
   const actual=await geometry(page,'.report-grid'),expected=await geometry(ref,'.dq-report-grid');assert.deepEqual(actual,expected,'approved Report columns/gap');
   await page.locator('#report-prices-summary').click();assert((await page.locator('#report-prices').innerText()).includes('effective 2026-01-01'));assert.equal(await page.locator('#report-prices a[href="https://example.com/invented-usd"]').count(),1);
   await page.locator('#report-retention-summary').click();assert((await page.locator('#report-retention').innerText()).includes('retained from 2026-'));assert((await page.locator('#report-retention').innerText()).includes('end watermark unknown'));
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'ordinary report text reflows');
   await page.locator('#report-prices-summary').focus();const y=await page.evaluate(()=>scrollY);await page.evaluate(()=>refresh());
   assert(await page.locator('#report-prices').evaluate(e=>e.open));assert(await page.locator('#report-prices-summary').evaluate(e=>e===document.activeElement));assert(Math.abs(await page.evaluate(()=>scrollY)-y)<2);
   offline=true;await page.evaluate(()=>refresh());assert(await page.locator('#banner').isVisible());assert((await page.locator('.report-source').first().innerText()).includes('Unconfirmed'));assert.equal(await page.locator('#report-estimate').innerText(),estimate);
   offline=false;await page.evaluate(()=>refresh());assert(!(await page.locator('.report-source').first().innerText()).includes('Unconfirmed'));
   for(const next of ['incomplete','stale','unconfirmed']){
    state=next;await page.evaluate(()=>refresh());assert((await page.locator('.report-source').first().innerText()).includes(next));
    assert((await page.locator('#report-elapsed_ms .report-number').innerText()).startsWith('≥'));
    if(next==='incomplete')assert((await page.locator('#reports-status').innerText()).includes('at least 202'));
    assert((await page.locator('#report-elapsed_ms').innerText()).includes('24321.000 s'));
   }
   state=null;await page.evaluate(()=>refresh());await page.locator('#report-attempt-221-1').focus();hideAttempt=true;await page.evaluate(()=>refresh());
   assert(await page.locator('#reports-status').evaluate(e=>e===document.activeElement),'vanished attempt link retains a report focus target');
   hideAttempt=false;await page.evaluate(()=>refresh());
   await page.locator('#report-attempt-221-1').click();assert.equal(new URL(page.url()).pathname,'/agents');assert(await page.locator('#attempt-221-1').isVisible());assert((await page.locator('#attempt-221-1 .attempt-timeline').innerText()).includes('checks'));assert.equal(await page.locator('#select-attempt-221-1').getAttribute('aria-pressed'),'true');
   assert.equal(new URL(page.url()).searchParams.get('session'),'s1');await page.waitForFunction(()=>!busy);await page.goBack();await page.waitForFunction(()=>!busy);assert(await page.locator('#reports').isVisible());assert(await page.locator('#report-attempt-221-1').evaluate(e=>e===document.activeElement),'Back focus: '+await page.evaluate(()=>document.activeElement?.id));
   await page.locator('#reports-next').click();await page.waitForFunction(()=>!busy);assert((await page.locator('#report-elapsed_ms').innerText()).includes('24321.000 s'));assert.equal(await page.locator('.report-record').count(),2);
   await page.locator('#report-attempt-221-201').click();assert(await page.locator('#attempt-221-201').isVisible());assert((await page.locator('#attempt-221-201 .attempt-timeline').innerText()).includes('checks'));await page.reload();await page.waitForFunction(()=>!busy);assert(await page.locator('#attempt-221-201').isVisible(),'direct shared attempt link restores exact inspector');await page.goBack();
   await page.goto('http://fixture/reports?period=1800&host=absent&agent=claude&session=s1');await page.waitForFunction(()=>!busy);assert((await page.locator('#report-elapsed_ms').innerText()).includes('unknown: no attempt measurements'));assert(!(await page.locator('#report-elapsed_ms').innerText()).includes('0.000 s'));assert((await page.locator('#report-estimate').innerText()).includes('—'));
   assert((await page.locator('#reports').innerText()).includes('Estimate unknown: no model calls read in this scope.'));
   const staticPage=await browser.newPage({javaScriptEnabled:false,viewport:{width,height:1000},colorScheme:theme});
   await staticPage.route('**/*',route=>route.abort());
   await staticPage.setContent(html.replace(/<script[^>]*>[\s\S]*?<\/script>/g,'').replace(/<link[^>]*>/g,'').replace('</head>',`<style>${css} html{font-size:${size}px}</style></head>`));
   assert(await staticPage.locator('#reports').isVisible(),'no-script Reports stays readable');await staticPage.close();
   assert.deepEqual(writes,[]);assert.deepEqual(errors,[]);report.cells.push({width,size,theme,result:'PASS',actual,expected});
   if(process.argv.includes('--capture')&&width===1280&&size===16&&theme==='light'){
    await page.goto(url);await page.waitForFunction(()=>!busy);await page.locator('#report-prices-summary').click();await page.screenshot({path:path.join(dir,'reports.png')});
   }
   await page.close();await ref.close();
  }
  fs.writeFileSync(path.join(dir,'reports-fidelity.json'),JSON.stringify(report,null,2)+'\n');
  console.log(`PASS Reports: ${report.cells.length} cells; shared totals, coverage, price basis, paging, attempt intervals, Back, refresh/outage and no writes`);
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
