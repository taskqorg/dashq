// #220 A1–A4 / F3: extends #219's pinned reference, matrix and fixture browser boundary.
const {chromium}=require('playwright'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const read=f=>fs.readFileSync(f,'utf8'),dir=process.env.DASHQ_AGENTS_EVIDENCE;
const digest=s=>crypto.createHash('sha256').update(s.replace(/\r\n/g,'\n')).digest('hex');
const reference=read('docs/design/operator-redesign/overview-candidate.html');
assert.equal(digest(reference),JSON.parse(read('docs/design/operator-redesign/approval.json')).public_reference_sha256);
const html=read(path.join(dir,'page.html'));
const js=['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/taskq/task-workspace.js','assets/taskq/pm-composer.js','assets/workspace.js','assets/boards.js','assets/app.js'].map(read).join('\n');
const css=['assets/style.css','assets/taskq/resource-controls.css','assets/resources.css','assets/taskq/project-blocks.css','assets/taskq/answer-card.css','assets/taskq/pm-composer.css','assets/workspace.css'].map(read).join('\n')+'\n@scope (#taskq-workspace) {'+read('assets/taskq/task-workspace.css')+'}';
(async()=>{
 const browser=await chromium.launch({headless:true});
 const report={reference:digest(reference),browser:browser.version(),platform:process.platform,inputs:Object.fromEntries(['src/agent_work.rs','src/page.rs','assets/app.js','assets/style.css','tests/agents-browser.cjs'].map(f=>[f,digest(read(f))])),rendered:digest(html),cells:[]};
 try {
  for(const [width,size] of [[1280,16],[360,20],[1280,24]])for(const theme of ['light','dark']){
   const page=await browser.newPage({viewport:{width,height:1000},colorScheme:theme,reducedMotion:'reduce'});
   const ref=await browser.newPage({viewport:{width,height:1000},colorScheme:theme});
   let offline=false,body=html;const errors=[],writes=[];
   page.on('pageerror',e=>errors.push(e.message));
   await page.route('http://fixture/**',route=>{
    const p=new URL(route.request().url()).pathname;
    if(route.request().method()!=='GET')writes.push(p);
    return route.fulfill({status:offline&&p==='/data'?500:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':body});
   });
   await page.clock.install();await page.goto('http://fixture/agents');await page.waitForFunction(()=>!busy);
   await page.addStyleTag({content:`html{font-size:${size}px}`});
   assert.equal(await page.locator('.attempt-select').count(),5,'A1: Agents must list attempts before client/accounting tables');
   assert(await page.locator('#agentwork').isVisible());assert.equal(await page.locator('#agents:visible,#modeluse:visible,#aqresults:visible,#agenttotals:visible').count(),0);
   await ref.setContent(reference);
   await ref.locator('[data-page=agents]').evaluate(e=>e.hidden=false);
   await ref.addStyleTag({content:`#dq-overview{font-size:${size}px}#dq-overview [data-page=agents]{display:block!important}`});
   const geometry=(p,layout,panel)=>p.evaluate(({layout,panel})=>{const l=document.querySelector(layout),r=document.querySelector(panel);return {columns:getComputedStyle(l).gridTemplateColumns.split(' ').length,gap:parseFloat(getComputedStyle(l).columnGap),padding:parseFloat(getComputedStyle(r).paddingLeft)};},{layout,panel});
   const actual=await geometry(page,'.attempt-layout','.attempt-inspectors'),expected=await geometry(ref,'.dq-agent-layout','.dq-agent-panel');
   assert.deepEqual(actual,expected,'approved list/inspector column count, gap and padding');
   const row=page.locator('#select-attempt-220-3');await row.click();
   assert.equal(await row.getAttribute('aria-pressed'),'true');assert(await page.locator('#attempt-220-3').isVisible());
   assert((await page.locator('#attempt-220-3').innerText()).includes('adopted'));
   assert((await page.locator('#attempt-220-3').innerText()).includes('Unknown: stage boundaries'));
   assert.equal(await page.locator('#heading-attempt-220-3').evaluate(e=>e===document.activeElement),true,'new detail starts at heading');
   await page.locator('#metrics-attempt-220-3 summary').click();await page.locator('#metrics-attempt-220-3 summary').focus();
   const y=await page.evaluate(()=>scrollY);
   await page.evaluate(()=>refresh());
   assert.equal(await page.locator('#select-attempt-220-3').getAttribute('aria-pressed'),'true');
   assert(await page.locator('#metrics-attempt-220-3').evaluate(e=>e.open),'refresh keeps disclosure');
   assert(await page.locator('#metrics-attempt-220-3 summary').evaluate(e=>e===document.activeElement),'refresh keeps focus');
   assert(Math.abs(await page.evaluate(()=>scrollY)-y)<2,'refresh keeps reading position');
   offline=true;await page.clock.fastForward(2000);await page.evaluate(()=>refresh());assert(await page.locator('#banner').isVisible());assert(await page.locator('#attempt-220-3').isVisible());
   assert((await page.locator('#attempt-220-3 [data-admission-state]').innerText()).includes('unconfirmed'));
   assert((await page.locator('#attempt-220-3 .attempt-admission [data-age-s]').innerText()).includes('unconfirmed'));
   assert((await page.locator('.attempt-source').innerText()).includes('Retained: Stage source: incomplete'),'outage keeps stage coverage reason');
   offline=false;await page.evaluate(()=>refresh());
   for(const id of [1,2,4,5]){
    await page.locator('#select-attempt-220-'+id).click();
    const panel=page.locator('#attempt-220-'+id);
    assert(await panel.isVisible());assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'ordinary attempt text reflows');
    if(id===2)assert((await panel.innerText()).includes('open · end unknown'));
    if(id===4)assert((await panel.innerText()).includes('untimed'));
    if(id===5)assert((await panel.innerText()).includes('partial'));
   }
   await page.goBack();assert(await page.locator('#attempt-220-4').isVisible(),'Back restores previous attempt');
   assert(await page.locator('#select-attempt-220-4').evaluate(e=>e===document.activeElement),'Back restores focus to the prior attempt');
   await page.locator('#agentwork a[href$="#modeluse"]').click();assert(await page.locator('#modeluse').isVisible());assert.equal(await page.locator('#agentwork:visible').count(),0);
   await page.locator('#attempts-home').click();assert(await page.locator('#attempt-220-4').isVisible(),'return keeps selection');
   await page.locator('#agentwork a[href$="#aqresults"]').click();assert(await page.locator('#aqresults').isVisible());await page.goBack();
   await page.locator('#agentwork a[href$="#agenttotals"]').click();assert(await page.locator('#agenttotals').isVisible());await page.goBack();
   const taskLink=page.locator('#attempt-220-4 a[href*="/projects#board-"]');
   assert((await page.locator('#attempt-220-4').innerText()).includes("Current task admission (not this attempt's wait)"));
   assert((await page.locator('#attempt-220-4 .attempt-admission').innerText()).includes('stale'),'current admission retains collection state');
   assert((await taskLink.getAttribute('href')).includes('taskqorg%2Fdashq'),'reuse encoded workspace identity');
   await taskLink.click();assert.equal(new URL(page.url()).pathname,'/projects');assert(await page.locator('#taskq-workspace').isVisible());assert(await page.evaluate(()=>!!document.getElementById(location.hash.slice(1))),'workspace link resolves to its real board');await page.goBack();assert(await page.locator('#attempt-220-4').isVisible());
   if(width===360){await page.locator('#attempt-220-4 .attempt-back').click();assert(await page.locator('#select-attempt-220-4').evaluate(e=>e===document.activeElement));}
   // A vanished selected attempt must not silently switch to another row.
   body=html.replace(/<article class="attempt-inspector" id="attempt-220-4"[\s\S]*?<\/article>/,'');
   await page.evaluate(()=>refresh());assert(await page.locator('#attempt-missing').isVisible());assert.equal(await page.locator('.attempt-inspector:visible').count(),0);
   body=html;await page.evaluate(()=>refresh());assert(await page.locator('#attempt-220-4').isVisible());
   // F3: a list longer than the viewport still has a keyboard return at every width.
   await page.evaluate(()=>{
    const row=document.querySelector('.attempt-select'),panel=document.querySelector('.attempt-inspector');
    for(let i=100;i<125;i++){
     const b=row.cloneNode(true),p=panel.cloneNode(true),id='attempt-220-'+i;
     b.id='select-'+id;b.dataset.attempt=id;b.setAttribute('aria-controls',id);b.setAttribute('aria-pressed','false');
     p.id=id;p.querySelector('h3').id='heading-'+id;p.querySelector('.attempt-back').href='#select-'+id;
     p.querySelectorAll('details').forEach(d=>d.remove());p.hidden=true;
     document.querySelector('.attempt-list').append(b);document.querySelector('.attempt-inspectors').append(p);
    }
   });
   await page.locator('#select-attempt-220-124').click();await page.locator('#attempt-220-124 .attempt-back').click();
   assert(await page.locator('#select-attempt-220-124').evaluate(e=>e===document.activeElement),'long-list return restores row focus');
   await page.locator('#select-attempt-220-1').click();await page.goBack();
   assert(await page.locator('#select-attempt-220-124').evaluate(e=>e===document.activeElement),'Back restores long-list reading target');
   body=html;await page.evaluate(()=>refresh());await page.locator('#select-attempt-220-1').click();
   await page.evaluate(()=>{const next=document.createElement('a');next.id='attempts-next';next.href='/agents?cursor=200';next.textContent='Next attempts';document.getElementById('agentwork').prepend(next);next.focus();});
   await page.evaluate(()=>refresh());assert(await page.locator('#attempts-status').evaluate(e=>e===document.activeElement),'last-page refresh keeps a focus target when Next disappears');
   assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
   report.cells.push({width,size,theme,result:'PASS',actual,expected});
   if(process.argv.includes('--capture')&&width===1280&&size===16&&theme==='light'){
    await page.locator('#select-attempt-220-1').click();await page.evaluate(()=>scrollTo(0,0));await page.screenshot({path:path.join(dir,'agents.png'),fullPage:true});
   }
   const staticPage=await browser.newPage({javaScriptEnabled:false,viewport:{width,height:1000}});
   await staticPage.route('**/*',route=>route.abort());
   const staticHtml=html.replace(/<script[^>]*>[\s\S]*?<\/script>/g,'').replace(/<link[^>]*>/g,'').replace('</head>','<style>'+css+'</style></head>');
   await staticPage.setContent(staticHtml,{waitUntil:'domcontentloaded'});assert(await staticPage.locator('#agentwork').isVisible(),'no-script Agents remains readable');
   for(const id of ['agents','agenttotals','aqresults','modeluse'])assert(await staticPage.locator('#'+id).isVisible(),'no-script evidence destination '+id);await staticPage.close();
   await page.close();await ref.close();
  }
  fs.writeFileSync(path.join(dir,'agents-fidelity.json'),JSON.stringify(report,null,2)+'\n');
  console.log(`PASS Agents: ${report.cells.length} fidelity cells; intervals, selection, Back, refresh/outage, separate accounting and no writes`);
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
