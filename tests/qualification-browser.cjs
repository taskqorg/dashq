// #223 A1/A2: one fidelity pass over Overview, Agents, Reports, Telemetry, Tasks and System detail against the
// hash-pinned public reference, at 1280/16, 360/20 and 1280/24 in light and dark. Fixture mode reads the exported
// renderer (DASHQ_QUALIFY_EVIDENCE); live mode (DASHQ_QUALIFY_URL, a loopback tunnel) reads real scoped data and
// aborts every non-GET, so neither mode can write. UI counts are compared with the same /data document.
const {chromium}=require('playwright'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const read=f=>fs.readFileSync(f,'utf8'),digest=s=>crypto.createHash('sha256').update(s.replace(/\r\n/g,'\n')).digest('hex');
const reference=read('docs/design/operator-redesign/overview-candidate.html');
assert.equal(digest(reference),JSON.parse(read('docs/design/operator-redesign/approval.json')).public_reference_sha256,'F1 public reference pin');
const live=process.env.DASHQ_QUALIFY_URL,dir=process.env.DASHQ_QUALIFY_EVIDENCE;
const jsFiles=['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/taskq/task-workspace.js','assets/taskq/pm-composer.js','assets/workspace.js','assets/boards.js','assets/app.js'];
const cssFiles=['assets/style.css','assets/taskq/resource-controls.css','assets/resources.css','assets/taskq/project-blocks.css','assets/taskq/answer-card.css','assets/taskq/pm-composer.css','assets/workspace.css'];
const js=jsFiles.map(read).join('\n'),css=cssFiles.map(read).join('\n')+'\n@scope (#taskq-workspace) {'+read('assets/taskq/task-workspace.css')+'}';
const html=live?null:read(path.join(dir,'page.html')),fixtureDoc=live?null:read(path.join(dir,'data.json'));
// Reference System destinations and the D22 view that preserves each one (preservation ledger).
const SYSTEM={'Services':'/services','Checks & findings':'/checks','Versions & updates':'/services','Integrations & enrollment':'/integrations','Resources & configuration':'/hosts','Optimization & stage totals':'/agents#agenttotals'};
const VIEW={'/services':'#qversions','/checks':'#checks','/integrations':'#installations','/hosts':'#servers','/agents#agenttotals':'#agenttotals'};
(async()=>{
 const browser=await chromium.launch({headless:true,...(process.env.DASHQ_CHROME?{executablePath:process.env.DASHQ_CHROME}:{})});
 const report={rules:['#223 A1/A2','D8/D10/D22','R12/R13','F1–F3/F7'],mode:live?'live':'fixture',browser:browser.version(),platform:process.platform,reference:digest(reference),
  inputs:Object.fromEntries(['src/page.rs','assets/app.js','assets/style.css','tests/qualification-browser.cjs'].map(f=>[f,digest(read(f))])),cells:[]};
 try{
  for(const [width,size] of [[1280,16],[360,20],[1280,24]])for(const theme of ['light','dark']){
   // bypassCSP only admits the test's own text-size style; the live CSP header is otherwise unchanged.
   const page=await browser.newPage({viewport:{width,height:1000},colorScheme:theme,reducedMotion:'reduce',bypassCSP:true});
   const ref=await browser.newPage({viewport:{width,height:1000},colorScheme:theme});
   const errors=[],writes=[];page.on('pageerror',e=>errors.push(e.message));
   await page.route('http://dash.test/**',async route=>{
    const req=route.request(),u=new URL(req.url());
    if(req.method()!=='GET'){writes.push(req.method()+' '+u.pathname);return route.abort();}
    // Live: server-rendered HTML and /data from the deployed release, candidate /app.js and /style.css.
    if(live&&u.pathname!=='/app.js'&&u.pathname!=='/style.css'){try{return await route.fulfill({response:await route.fetch({url:live+u.pathname+u.search,headers:{...req.headers(),host:'127.0.0.1:18882'}})});}catch(e){if(!/disposed|closed/.test(e.message))throw e;return;}}
    const p=u.pathname;
    return route.fulfill({contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',
     body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?fixtureDoc:p.startsWith('/answer')||p.startsWith('/pm/')||p.startsWith('/task/')?'{}':html});
   });
   const idle=()=>page.waitForFunction(()=>typeof busy==='undefined'||!busy,{},{timeout:30000});
   const open=async view=>{await page.goto('http://dash.test'+view);await idle();await page.addStyleTag({content:`html{font-size:${size}px}`});
    assert.equal(await page.locator('body').evaluate(e=>getComputedStyle(e).fontSize),size+'px',view+': requested text size is never reduced');};
   const reflows=async view=>assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),view+': ordinary text reflows');
   await ref.setContent(reference);
   const refPage=async name=>{await ref.evaluate(n=>document.querySelectorAll('[data-page]').forEach(e=>e.hidden=e.dataset.page!==n),name);
    await ref.addStyleTag({content:`#dq-overview{font-size:${size}px}#dq-overview [data-page=${name}]{display:block!important}`});};
   const grid=(p,s)=>p.locator(s).first().evaluate(e=>({columns:new Set([...e.children].filter(c=>c.getBoundingClientRect().width).map(c=>Math.round(c.getBoundingClientRect().left))).size,gap:parseFloat(getComputedStyle(e).columnGap)}));
   const cell={width,size,theme,views:{}};

   await open('/');await refPage('overview');
   const doc=await page.evaluate(()=>fetch('/data').then(r=>r.json()));
   const cards=await page.locator('#host-grid .overview-host').count();
   assert.equal(cards,doc.hosts.length,'Overview: one host card per /data host');
   const card=(p,s)=>p.locator(s).nth(0).evaluate(e=>{const c=getComputedStyle(e);return {padding:parseFloat(c.paddingLeft),radius:parseFloat(c.borderRadius)};});
   assert.deepEqual(await card(page,'#host-grid .overview-host'),await card(ref,'[data-cards]>article'),'Overview: approved card padding/radius');
   for(const order of await page.locator('#host-grid .overview-host').evaluateAll(es=>es.map(e=>['.overview-name','.overview-state','.overview-metrics','.overview-open'].map(s=>e.querySelector(s)?.getBoundingClientRect().top).filter(v=>v!==undefined))))
    assert(order.every((t,i)=>i===0||t>order[i-1]),'Overview: identity → state → facts → action '+JSON.stringify(order));
   await reflows('/');cell.views.overview={hosts:cards};

   await open('/agents');await refPage('agents');
   const attempts=await page.locator('.attempt-select').count();
   assert.equal(attempts,Math.min(doc.agent_work?.attempts?.length??0,200),'Agents: one row per /data attempt');
   if(attempts){assert.deepEqual(await page.locator('.attempt-layout').evaluate(e=>{const l=getComputedStyle(e);return {columns:l.gridTemplateColumns.split(' ').length,gap:parseFloat(l.columnGap)};}),
    await ref.locator('.dq-agent-layout').evaluate(e=>{const l=getComputedStyle(e);return {columns:l.gridTemplateColumns.split(' ').length,gap:parseFloat(l.columnGap)};}),'Agents: approved list/inspector columns');
    await page.locator('.attempt-select').first().click();assert.equal(await page.locator('.attempt-inspector:visible').count(),1,'Agents: selected attempt opens its inspector');}
   await reflows('/agents');cell.views.agents={attempts,total:doc.agent_work?.attempts_total??null};

   await open('/reports');await refPage('reports');
   assert.deepEqual(await grid(page,'.report-grid'),await grid(ref,'.dq-report-grid'),'Reports: approved columns/gap');
   const wall=doc.agent_work?.totals?.elapsed_ms;
   assert((await page.locator('#report-elapsed_ms').innerText()).includes(`${wall?.known??0} of`),'Reports: wall-time known/of matches /data');
   assert(!(await page.locator('#reports').innerText()).includes('Actual expenses'),'Reports: no actual expense inferred');
   await reflows('/reports');cell.views.reports={elapsed_known:wall?.known??null,of:wall?.of??null};

   await open('/telemetry');await refPage('telemetry');
   const senders=page.locator('.telemetry-sender'),n=await senders.count();
   for(let i=0;i<n;i++)assert.deepEqual(await senders.nth(i).locator('[data-stage]').evaluateAll(es=>es.map(e=>e.dataset.stage)),['emitted','queued','accepted','projected','exported','queryable'],'Telemetry: six ordered stages');
   if(n)assert.equal((await grid(page,'.telemetry-coverage')).columns,(await grid(ref,'[data-page=telemetry] .dq-diag-grid')).columns,'Telemetry: coverage reflows with the approved diagnosis grid');
   assert.equal(await page.locator('#telemetry .ok,#telemetry .good').count(),0,'Telemetry: no implied pass');
   await reflows('/telemetry');cell.views.telemetry={senders:n};

   await open('/projects');await refPage('tasks');
   await page.waitForFunction(()=>!!document.querySelector('#taskq-workspace'),{},{timeout:30000});
   // Boards without open tasks appear only in DashQ's coverage list, never as an empty TaskQ block.
   const coverage=await page.locator('#workspace-coverage').innerText();
   for(const b of doc.boards)assert(coverage.includes(b.project+' · '+b.source.state),'Tasks: configured board '+b.project+' keeps its coverage');
   const rows=doc.boards.reduce((n,b)=>n+b.tasks.length,0);
   assert.equal(await page.locator('#taskq-workspace').evaluate(e=>e.getBoundingClientRect().width<=innerWidth+1),true,'Tasks: workspace fits the viewport');
   await reflows('/projects');cell.views.tasks={boards:doc.boards.length,tasks:rows};

   await open('/');await refPage('system');
   const destinations=await ref.locator('[data-system]').evaluateAll(es=>es.map(e=>e.dataset.system));
   assert.deepEqual(destinations,Object.keys(SYSTEM),'System: reference destinations unchanged');
   for(const [name,view] of Object.entries(SYSTEM)){
    const [p,hash]=view.split('#');await page.locator('#more').evaluate(e=>e.open=true);
    await page.locator(`#more a[href^="${p}?"],#more a[href="${p}"]`).first().click();await idle();
    await page.locator('#more').evaluate(e=>e.open=false);
    if(hash)await page.locator(`#agentwork a[href$="#${hash}"]`).click();
    assert.equal(new URL(page.url()).pathname,p,name+': More navigation');
    assert(await page.locator(VIEW[view]).isVisible(),name+': destination retained');
    assert(!await page.locator('#view-unavailable').isVisible(),name+': not silently unavailable');
    // Real installation IDs and evidence hashes are long unbroken tokens (#223 live /checks at 360/20).
    await page.locator(VIEW[view]).evaluate(e=>{const c=document.createElement('code');c.className='qualify-probe';c.textContent='f'.repeat(64);(e.querySelector('h3')||e).append(c);});
    await reflows(name);await page.locator('.qualify-probe').evaluate(e=>e.remove());
   }
   cell.views.system={destinations:destinations.length};
   assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
   report.cells.push({...cell,result:'PASS'});await page.close();await ref.close();
  }
  if(process.env.DASHQ_QUALIFY_OUT)fs.writeFileSync(process.env.DASHQ_QUALIFY_OUT,JSON.stringify(report,null,2)+'\n');
  console.log(`PASS qualification (${report.mode}): ${report.cells.length} cells × Overview, Agents, Reports, Telemetry, Tasks, System; ${JSON.stringify(report.cells[0].views)}`);
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
