// #179 A2-A5: real browser bundles -> adapter -> receipts. Risks: unoffered/unguarded writes,
// fresh IDs after uncertain delivery, lost pending state on refresh, or inaccessible controls/Closed coverage.
// Run with the existing Playwright install: node tests/task-controls-browser.cjs [--capture].
const {chromium} = require('playwright');
const fs = require('node:fs'), assert = require('node:assert/strict');
const jsFiles = ['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/boards.js','assets/app.js'];
const cssFiles = ['assets/style.css','assets/taskq/resource-controls.css','assets/resources.css','assets/taskq/project-blocks.css','assets/taskq/answer-card.css'];
const read = f => fs.readFileSync(f,'utf8');
const js = jsFiles.map(read).join('\n'), css = cssFiles.map(read).join('\n');
const capture = process.argv.includes('--capture'), out = 'docs/design/evidence/task-controls';
const escape = s => s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
(async()=>{
 const browser = await chromium.launch({headless:true});
 try {
  for(const [width,size,theme] of [[1024,16,'light'],[360,20,'dark']]) {
   const page = await browser.newPage({viewport:{width,height:850},colorScheme:theme,reducedMotion:'reduce'});
   const errors=[],posts=[];
   page.on('pageerror',e=>errors.push(e.message));
   let access=false, reply='lost';
   const doc=JSON.parse(read('fixture/data.json')), b=doc.boards[0];
   b.source={state:'ok',at:Math.floor(Date.now()/1000),age_s:0,incomplete:false};
   b.tasks=[{n:7,title:'Control one task with a long title that must remain readable on a narrow screen',url:'https://github.com/example/test/issues/7',state:'doing',revision:20,controls:['stop','later']}];
   b.questions=[];b.failed=[];b.attempts=[];doc.boards=[b];
   const html=()=>read('fixture/page.html').replace(/data-boards="[^"]*"/, 'data-boards="'+escape(JSON.stringify(doc.boards))+'"');
   await page.route('http://fixture/**',async route=>{
    const req=route.request(),p=new URL(req.url()).pathname;
    if(p==='/task/control') {
     if(req.method()==='GET') return route.fulfill({status:access?200:503,json:access?{csrf:'fixture-csrf'}:{outcome:'rejected',reason:'Task controls are not enabled on this dashboard'}});
     const form=Object.fromEntries(new URLSearchParams(req.postData())); posts.push(form);
     if(reply==='lost') return route.fulfill({status:502,contentType:'text/html',body:'<h1>Lost reply</h1>'});
     if(reply==='denied') return route.fulfill({status:403,json:{task:null,op_id:null,outcome:'rejected',reason:'Actor grant revoked'}});
     return route.fulfill({status:reply==='stale'?409:200,json:{task:7,op_id:'dashq.control.'+form.op_id,outcome:reply,revision:21,state:'failed',reason:reply==='stale'?'task changed':null}});
    }
    const body=p==='/app.js'?js:p==='/style.css'?css:p==='/data'?JSON.stringify(doc):p==='/answer'?'{"reason":"fixture: answers off"}':html();
    return route.fulfill({contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'||p==='/answer'?'application/json':'text/html',body});
   });
   await page.goto('http://fixture/projects'); await page.waitForFunction(()=>!busy);
   await page.addStyleTag({content:`html{font-size:${size}px}`});
   const field=page.locator('.task-control-adapter fieldset'), status=page.locator('.task-control-adapter [role=status]');
   await page.waitForFunction(()=>document.querySelector('[data-control-reason]').textContent.includes('not enabled'));
   assert(await field.evaluate(e=>e.disabled));
   assert.equal(await page.locator('[data-control]').count(),2,'only listed offers');
   assert.equal(await page.locator('[aria-label="Close #7 on the board"]').getAttribute('href'),b.tasks[0].url);
   assert.equal(await page.locator('a:has-text("Closed on board")').count(),1);
   assert.equal(posts.length,0);
   access=true;
   await page.evaluate(()=>{taskControlAccess.clear();taskControlAccessLoad(JSON.parse(document.getElementById('boards').dataset.boards)[0].project);});
   await page.waitForFunction(()=>!document.querySelector('.task-control-adapter fieldset').disabled);
   if(capture && theme==='light'){fs.mkdirSync(out,{recursive:true});await page.screenshot({path:`${out}/${width}-${size}-${theme}.png`,fullPage:true});}
   await page.locator('[data-control=stop]').dblclick();
   await page.waitForFunction(()=>document.querySelector('.task-control-adapter [role=status]').textContent.startsWith('unknown'));
   assert.equal(posts.length,1,'double click sends only one operation');
   const original={...posts[0]};
   assert.equal(original.revision,'20');assert.equal(original.action,'stop');assert.equal(original.task,'7');assert(original.op_id);
   await page.evaluate(()=>{const e=document.getElementById('boards'),bs=JSON.parse(e.dataset.boards);bs[0].source.state='unconfirmed';e.dataset.boards=JSON.stringify(bs);renderBoards();});
   assert(await field.evaluate(e=>e.disabled));
   assert((await status.innerText()).startsWith('unknown'),'refresh keeps uncertain delivery');
   if(capture && theme==='dark'){fs.mkdirSync(out,{recursive:true});await page.screenshot({path:`${out}/${width}-${size}-${theme}.png`,fullPage:true});}
   reply='denied';await page.locator('[data-control-retry]').click();
   await page.waitForFunction(()=>document.querySelector('.task-control-adapter [role=status]').textContent.includes('Actor grant revoked'));
   assert((await status.innerText()).startsWith('unknown'),'refused reconciliation cannot authorize a fresh operation');
   reply='accepted';await page.locator('[data-control-retry]').click();
   await page.waitForFunction(()=>document.querySelector('.task-control-adapter [role=status]').textContent.startsWith('accepted'));
   assert.deepEqual(posts,[original,original,original],'retry keeps board, revision, op-id and payload');
   await page.evaluate(()=>{const e=document.getElementById('boards'),bs=JSON.parse(e.dataset.boards);bs[0].source.state='ok';bs[0].tasks[0].revision=21;bs[0].tasks[0].state='failed';bs[0].tasks[0].controls=['retry'];e.dataset.boards=JSON.stringify(bs);renderBoards();});
   reply='stale';await page.locator('[data-control=retry]').click();
   await page.waitForFunction(()=>document.querySelector('.task-control-adapter [role=status]').textContent.startsWith('stale'));
   assert.notEqual(posts.at(-1).op_id,original.op_id);
   assert.equal(posts.at(-1).revision,'21');
   assert(await field.evaluate(e=>e.disabled),'stale receipt waits for current revision');
   // Missing canonical revision and a refused actor each keep a listed offer read-only.
   await page.evaluate(()=>{const e=document.getElementById('boards'),bs=JSON.parse(e.dataset.boards);bs[0].tasks[0].revision=null;e.dataset.boards=JSON.stringify(bs);renderBoards();});
   assert(await field.evaluate(e=>e.disabled));assert((await page.locator('[data-control-reason]').innerText()).includes('revision unavailable'));
   await page.evaluate(()=>{const e=document.getElementById('boards'),bs=JSON.parse(e.dataset.boards);bs[0].tasks[0].revision=22;e.dataset.boards=JSON.stringify(bs);renderBoards();});
   reply='denied';await page.locator('[data-control=retry]').click();
   await page.waitForFunction(()=>document.querySelector('.task-control-adapter [role=status]').textContent.includes('Actor grant revoked'));
   assert(await field.evaluate(e=>e.disabled),'revoked actor stays read-only after its rejected receipt');
   assert.equal(await page.locator('[data-control-reason]').innerText(),'Actor grant revoked');
   await page.evaluate(()=>{const e=document.getElementById('boards'),bs=JSON.parse(e.dataset.boards);bs[0].tasks[0].controls={invalid:true};e.dataset.boards=JSON.stringify(bs);renderBoards();});
   assert.equal(await page.locator('[data-control]').count(),0,'invalid producer offers cannot crash or enable controls');
   assert.deepEqual(errors,[]);
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no horizontal overflow');

   await page.close();
  }
  console.log('PASS task controls browser: gate, listed offers, revision, same-ID retry, uncertain/denied/stale receipts, refresh and narrow reflow; '+browser.version());
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
