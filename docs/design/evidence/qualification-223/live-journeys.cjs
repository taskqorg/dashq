// Usage: ssh -N -L 18883:127.0.0.1:18882 fra; NODE_PATH=<playwright 1.56.1>/node_modules node live-journeys.cjs
// #223 A2 read-only journeys on the deployed release through the FRA loopback (D11 host-local scope, like `dashq report`).
// Every non-GET is aborted and counted, so this run cannot write; the PM draft is never sent.
const {chromium}=require('playwright'),assert=require('node:assert/strict');
const tunnel='http://127.0.0.1:18883',out={};
(async()=>{
 const browser=await chromium.launch({headless:true});
 const page=await browser.newPage({viewport:{width:1280,height:900}});
 const errors=[],blocked=[];let offline=false;
 page.on('pageerror',e=>errors.push(e.message));
 await page.route('http://dash.live/**',async route=>{try{
  const req=route.request(),u=new URL(req.url());
  if(req.method()!=='GET'){blocked.push(req.method()+' '+u.pathname);return route.abort();}
  if(offline&&u.pathname==='/data')return route.abort();
  return await route.fulfill({response:await route.fetch({url:tunnel+u.pathname+u.search,headers:{...req.headers(),host:'127.0.0.1:18882'}})});
 }catch(e){if(!/disposed|closed/.test(e.message))throw e;}});
 const idle=()=>page.waitForFunction(()=>typeof busy==='undefined'||!busy,{},{timeout:30000});
 const go=async p=>{await page.goto('http://dash.live'+p);await idle();};
 const doc=async q=>page.evaluate(q=>fetch('/data'+q).then(r=>r.json()),q);

 await go('/?period=7200');const d=await doc('?period=7200');
 assert(await page.locator('#host-grid').isVisible());
 assert.equal(await page.locator('#host-grid .overview-host').count(),d.hosts.length);
 out['DQ-01']={hosts:d.hosts.length,attention:d.attention.requests.length,non_ok_sources:Object.entries(d.sources).filter(([,s])=>s.state!=='ok').map(([k,s])=>k+': '+s.state)};
 assert.equal(await page.locator('#attention .action-row').count(),d.attention.requests.length,'Attention lists exactly the /data requests');
 for(const p of ['/hosts','/agents','/services','/checks','/integrations','/telemetry','/reports','/projects']){await page.locator('#more').evaluate(e=>e.open=true);await page.locator(`header a[href^="${p}?"]`).first().click();await idle();
  assert.equal(new URL(page.url()).searchParams.get('period'),'7200',p+' keeps the period');}

 await go('/?period=7200');
 const action=page.locator('#attention a[href^="/checks"][href*="#finding-"]').first();const target=new URL(await action.getAttribute('href'),'http://dash.live').hash.slice(1);
 await action.click();await idle();assert.equal(new URL(page.url()).pathname,'/checks');
 assert(await page.evaluate(id=>!!document.getElementById(id),target),'DQ-03 finding target exists');
 await page.goBack();await idle();assert(await page.locator('#host-grid').isVisible());
 out['DQ-03']={action:'finding opens /checks target; Back returns',resource_rows:d.resources.rows.length};

 await go('/agents?period=7200');
 const rows=await page.locator('.attempt-select').count();assert.equal(rows,Math.min(d.agent_work.attempts.length,200));
 await page.locator('.attempt-select').first().click();const insp=await page.locator('.attempt-inspector:visible').innerText();
 out['DQ-04']={attempts:rows,inspector_has_coverage:/coverage|open|complete|adopted|partial/i.test(insp)};

 await go('/projects?period=7200');await page.waitForSelector('#taskq-workspace');
 const waiting=d.boards.flatMap(b=>b.tasks.filter(t=>t.admission).map(t=>({b:b.project,n:t.n,code:t.admission.code})));
 const ws=await page.locator('#boards').innerText();
 for(const w of waiting)assert(ws.includes('#'+w.n),'DQ-05 waiting task shown');
 out['DQ-05']={admission:waiting.map(w=>w.code),coverage_rows:await page.locator('#workspace-coverage p').count()};
 out['DQ-06']={offered:d.boards.flatMap(b=>b.tasks.map(t=>(t.controls||[]).length)).reduce((a,b)=>a+b,0),
  answer_get:await page.evaluate(()=>fetch('/answer?board=taskqorg%2Fdashq').then(r=>r.status)),
  control_get:await page.evaluate(()=>fetch('/task/control?board=taskqorg%2Fdashq').then(r=>r.status))};
 await page.locator('#pm-draft').fill('Invented draft; never sent.');
 await page.locator('#view-home').click();await idle();await page.goBack();await idle();
 assert.equal(await page.locator('#pm-draft').inputValue(),'Invented draft; never sent.','DQ-07 draft survives navigation');
 offline=true;await page.evaluate(()=>refresh());assert(await page.locator('#banner').isVisible(),'DQ-10 failed fetch is explicit');
 assert.equal(await page.locator('#pm-draft').inputValue(),'Invented draft; never sent.','DQ-10 draft survives outage');
 const banner=await page.locator('#banner').innerText();
 offline=false;await page.evaluate(()=>refresh());assert(!await page.locator('#banner').isVisible(),'DQ-10 recovery clears the banner');
 out['DQ-07']={draft:'kept across navigation and outage; not sent'};out['DQ-10']={banner:banner.split(',')[0]};

 await go('/reports?period=86400');const r=await doc('?period=86400');
 const wall=await page.locator('#report-elapsed_ms').innerText();assert(wall.includes(r.agent_work.totals.elapsed_ms.known+' of'));
 const drill=page.locator('#reports a[href*="/agents"][href*="#attempt-"]').first();await drill.click();await idle();
 assert.equal(new URL(page.url()).pathname,'/agents');assert.equal(await page.locator('.attempt-inspector:visible').count(),1,'DQ-08 report row opens its attempt inspector');
 out['DQ-08']={wall_known:r.agent_work.totals.elapsed_ms.known,of:r.agent_work.totals.elapsed_ms.of,queue_known:r.agent_work.totals.queue_wait_s.known,model_use_rows:r.model_use.rows.length};

 await go('/telemetry?period=1800');const t=await doc('?period=1800');
 const sender=page.locator('.telemetry-sender').filter({has:page.locator('details a[href*="block="]')}).first();
 await sender.locator('details summary').first().click();const link=new URL(await sender.locator('details a[href*="block="]').first().getAttribute('href'),'http://dash.live');
 const block=await doc(link.search);assert.equal(block.telemetry_block.state,'ok','DQ-09 listed block is queryable');
 out['DQ-09']={senders:await page.locator('.telemetry-sender').count(),rows:t.installations.rows.length,block_state:block.telemetry_block.state};

 assert.deepEqual(errors,[]);out.blocked_writes=blocked;
 console.log(JSON.stringify(out,null,1));
 await browser.close();
})().catch(e=>{console.error(e);process.exitCode=1;});
