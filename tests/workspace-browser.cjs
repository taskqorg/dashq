// #186 A2-A6: the exact shipped bundles and DashQ adapter; lost replies, local state and navigation.
const fs=require('node:fs'),assert=require('node:assert/strict'),{chromium}=require('playwright');
const read=f=>fs.readFileSync(f,'utf8');
const js=['assets/taskq/resource-controls.js','assets/resources.js','assets/taskq/project-blocks.js','assets/taskq/answer-card.js','assets/taskq/task-controls.js','assets/taskq/task-workspace.js','assets/taskq/pm-composer.js','assets/workspace.js','assets/boards.js','assets/app.js'].map(read).join('\n');
const css=['assets/style.css','assets/taskq/resource-controls.css','assets/resources.css','assets/taskq/project-blocks.css','assets/taskq/answer-card.css','assets/taskq/pm-composer.css','assets/workspace.css'].map(read).join('\n')+'\n@scope (#taskq-workspace) {'+read('assets/taskq/task-workspace.css')+'}';
const escape=s=>s.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
(async()=>{
 const browser=await chromium.launch({headless:true});
 try {
  // Replay TaskQ's published journeys unchanged before exercising DashQ-specific behavior.
  const fixture=JSON.parse(read('assets/taskq/task-workspace-fixtures.json'));
  for(const journey of fixture.journeys){
   const page=await browser.newPage({viewport:{width:journey.viewport[0],height:journey.viewport[1]}}),sent=[],errors=[];
   page.on('pageerror',e=>errors.push(e.message));await page.exposeFunction('sent',m=>sent.push(m));
   await page.goto('about:blank');await page.setContent('<!doctype html><html><body><div class="menu"></div></body></html>');await page.addStyleTag({content:read('assets/taskq/task-workspace.css')});
   await page.addScriptTag({content:read('assets/taskq/task-workspace.js')});
   await page.evaluate(s=>{window.ipc={postMessage:m=>window.sent(JSON.parse(m))};window.testApp=TaskqTaskWorkspace.create(TaskqTaskWorkspace.createState(),{});testApp.mount();taskq.receive(s);},{...fixture.state,font:journey.font});
   for(const step of journey.steps){
    if(step.click)await page.locator(step.click).first().click();
    if(step.press){for(let i=0;i<(step.max||1);i++){await page.keyboard.press(step.press);if(!step.until||await page.locator(step.until).evaluateAll(es=>es.includes(document.activeElement)))break;}}
    if(step.expect){const e=step.expect;
     for(const [selector,count] of Object.entries(e.count||{}))assert.equal(await page.locator(selector).count(),count,journey.id+' '+selector);
     if(e.open!=null)assert.equal(await page.locator('#taskq-popover:visible').count()>0,e.open,journey.id+' open');
     if(e.focus)assert(await page.locator(e.focus).evaluateAll(es=>es.some(e=>e===document.activeElement||e.contains(document.activeElement))),journey.id+' focus');
     for(const [selector,text] of Object.entries(e.text||{}))assert((await page.locator(selector).innerText()).includes(text));
     if(e.fits)assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'journey horizontal fit');
     if(e.sent)assert(sent.some(m=>m.a===e.sent),journey.id+' send');
    }
   }
   assert.deepEqual(errors,[]);await page.close();console.log('PASS pinned journey',journey.id);
  }
  for(const [width,font,theme] of [[1024,16,'light'],[360,20,'dark']]){
   const page=await browser.newPage({viewport:{width,height:850},colorScheme:theme,reducedMotion:'reduce'}),errors=[],posts=[];
   page.on('pageerror',e=>errors.push(e.message));let offline=false,answer='lost';const answers=[];
   const doc=JSON.parse(read('fixture/data.json')),b=doc.boards[0];
   b.source={state:'ok',at:Math.floor(Date.now()/1000),age_s:0,incomplete:false};
   // The board reports every state but held; held must stay unknown, never zero.
   delete b.counts.held;
   b.tasks=[{n:7,state:'doing',title:'One shared task workspace',url:b.url+'/7',revision:20,controls:['stop','later']}];b.questions=[];b.failed=[];b.attempts=[];doc.boards=[b];
   const target=JSON.parse(read('assets/taskq/pm-composer-fixtures.json')).request;
   const html=()=>read('fixture/page.html').replace(/data-boards="[^"]*"/,'data-boards="'+escape(JSON.stringify(doc.boards))+'"');
   await page.route('http://fixture/**',async route=>{const req=route.request(),p=new URL(req.url()).pathname;
    if(offline&&(p==='/data'||p==='/'))return route.abort();
    if(p==='/pm/request'){
     if(req.method()==='GET')return route.fulfill({json:{csrf:'invented-csrf',actor_id:target.actor_id,targets:[target]}});
     const r=JSON.parse(req.postData());posts.push(r);
     if(answer==='lost')return route.fulfill({status:502,body:'lost reply'});
     return route.fulfill({json:{schema:r.schema,actor_id:r.actor_id,op_id:r.op_id,revision:r.revision,outcome:'accepted',request_id:'a'.repeat(64),native:{state:'launched',host:'invented-host',runtime:'claude',session:'invented-session',url:'https://claude.ai/code/session_invented'}}});
    }
    if(p==='/answer'&&req.method()==='GET')return route.fulfill({json:{csrf:'invented-answer-csrf'}});
    if(p==='/answer'){const f=new URLSearchParams(req.postData());answers.push(Object.fromEntries(f));return route.fulfill({json:{outcome:'accepted',task:Number(f.get('task')),op_id:'dashq.'+f.get('op_id')}});}
    if(p==='/task/control')return route.fulfill({status:503,json:{reason:'fixture: action off'}});
    const body=p==='/app.js'?js:p==='/style.css'?css:p==='/data'?JSON.stringify(doc):html();
    return route.fulfill({contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body});
   });
   await page.goto('http://fixture/projects');await page.waitForFunction(()=>!busy&&!!taskWorkspace);
   await page.addStyleTag({content:`html{font-size:${font}px}`});await page.evaluate(()=>renderBoards());
   assert.equal(await page.locator('[data-a=task-detail]').count(),1);
   assert(await page.evaluate(()=>taskWorkspaceView.S.display.all.counts.some(c=>c.n===null)),'unreported state count stays unknown');
   await page.locator('#pm-draft').fill('Read the invented board. Do not change tasks.');
   await page.locator('#pm-draft').focus();await page.evaluate(()=>document.getElementById('pm-draft').setSelectionRange(4,9));
   await page.evaluate(()=>refresh());
   assert.equal(await page.locator('#pm-draft').inputValue(),'Read the invented board. Do not change tasks.');
   assert(await page.locator('#pm-draft').evaluate(e=>e===document.activeElement),'draft focus survives refresh');
   assert.deepEqual(await page.locator('#pm-draft').evaluate(e=>[e.selectionStart,e.selectionEnd]),[4,9]);
   await page.locator('#view-home').click();await page.goBack();
   assert.equal(await page.locator('#pm-draft').inputValue(),'Read the invented board. Do not change tasks.');
   await page.locator('#pm-composer button:has-text("Send")').click();await page.waitForFunction(()=>pmDraft.receipt?.outcome==='unknown',{},{timeout:5000}).catch(async e=>{console.error(errors,posts,await page.evaluate(()=>({draft:pmDraft,access:pmAccess,text:document.getElementById('pm-composer').innerText})));throw e;});
   const original=posts[0];assert(original.op_id);assert.equal(posts.length,1);
   offline=true;await page.evaluate(()=>refresh());assert.equal(await page.locator('#pm-draft').inputValue(),original.text);
   offline=false;answer='accepted';await page.locator('#pm-composer button:has-text("Check delivery")').click();
   await page.waitForFunction(()=>pmDraft.receipt?.native?.session==='invented-session');assert.deepEqual(posts,[original,original]);
   assert.equal(await page.locator('.native-session').getAttribute('href'),'https://claude.ai/code/session_invented');
   await page.evaluate(()=>refresh());
   await page.locator('[data-a=task-detail]').focus();await page.keyboard.press('Enter');
   assert(await page.locator('#taskq-popover').isVisible());await page.locator('#taskq-popover [data-control=stop]').focus();await page.evaluate(()=>refresh());
   assert(await page.locator('#taskq-popover').isVisible(),'detail selection survives refresh');
   assert(await page.locator('#taskq-popover [data-control=stop]').evaluate(e=>e===document.activeElement),'detail focus survives refresh');
   assert(await page.locator('#taskq-popover [data-control=stop]').evaluate(e=>getComputedStyle(e).outlineStyle!=='none'),'keyboard focus stays visible');
   await page.keyboard.press('Escape');
   // Native-created tasks become visible through the same subsequent board observation.
   b.tasks.push({n:8,state:'held',title:'PM-created task read back from its board',url:b.url+'/8',revision:21,controls:['start']});
   await page.evaluate(()=>refresh());assert.equal(await page.locator('[data-a=task-detail]').count(),2);
   // #187 DQ-06: the workspace's "Answer in workspace…" reached no card (only 'conversations' was handled).
   // Risk: the question is visible but unanswerable in DashQ; it must reach the pinned card and guarded /answer once.
   b.tasks.push({n:9,state:'ask',title:'Keep the old API?',url:b.url+'/9',revision:22,controls:[],question:{parsed:true,text:'Problem: Keep the old API?',done:null,problem:'Keep the old API?',evidence:[],choices:[{id:1,text:'Keep it',recommended:true,consequential:false},{id:2,text:'Remove it',recommended:false,consequential:false}],limits:{lines:4,chars:240}}});
   await page.evaluate(()=>refresh());
   await page.locator('[data-a=task-detail]:has-text("#9")').click();
   await page.locator('#taskq-popover [data-page=answer]').click();
   await page.locator('#workspace-answer button[data-choice="2"]:not([disabled])').click();
   await page.waitForFunction(()=>/accepted/i.test(document.getElementById('workspace-answer').innerText));
   assert.deepEqual(answers.map(a=>[a.board,a.task,a.revision,a.choice,a.csrf]),[[b.project,'9','22','2','invented-answer-csrf']]);
   await page.evaluate(()=>refresh());assert.equal(answers.length,1,'refresh does not resend the answer');
   await page.keyboard.press('Escape');
   const instance=await page.evaluateHandle(()=>taskWorkspace);
   for(const path of ['/hosts','/agents','/services','/checks','/integrations','/projects']){
    await page.evaluate(path=>navigateView(path),path);assert.equal(new URL(page.url()).pathname,path);
    assert(await page.evaluate(prior=>prior===taskWorkspace,instance),'navigation keeps the workspace instance');
   }
   assert.deepEqual(errors,[]);if(!await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)){console.error('overflow',await page.evaluate(()=>[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>innerWidth&&e.getBoundingClientRect().width>0).slice(0,15).map(e=>({tag:e.tagName,id:e.id,cls:e.className,width:e.getBoundingClientRect().width,right:e.getBoundingClientRect().right,text:e.textContent.slice(0,100)}))));fs.mkdirSync('/tmp/dashq-186-evidence',{recursive:true});await page.screenshot({path:'/tmp/dashq-186-evidence/overflow.png',fullPage:true});}assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'DashQ horizontal fit');
   if(process.argv.includes('--capture')&&theme==='light'){fs.mkdirSync('/tmp/dashq-186-evidence',{recursive:true});await page.screenshot({path:'/tmp/dashq-186-evidence/workspace.png',fullPage:true});}
   await page.close();console.log('PASS DashQ adapter',width,font,theme);
  }
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
