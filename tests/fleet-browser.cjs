// D26 / R2 / D18 / F3,F7: Fleet navigation, coverage, layout and refresh preserve observations without writes.
const {chromium}=require('playwright'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const dir=process.env.DASHQ_FLEET_EVIDENCE;
const read=f=>fs.readFileSync(f,'utf8');
const html=read(path.join(dir,'page.html'));
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
    const p=new URL(route.request().url()).pathname;
    if(route.request().method()!=='GET')writes.push(p);
    return route.fulfill({status:offline&&p==='/data'?500:200,contentType:p.endsWith('.js')?'text/javascript':p.endsWith('.css')?'text/css':p==='/data'?'application/json':'text/html',body:p==='/app.js'?js:p==='/style.css'?css:p==='/data'?'{}':html});
   });
   await page.goto('http://fixture/fleet?host=pds18.ufobe.com');
   await page.waitForFunction(()=>!busy);
   await page.addStyleTag({content:`html{font-size:${size}px}`});
   assert(await page.locator('#fleet').isVisible());
   assert.equal(await page.locator('#view-fleet').getAttribute('aria-current'),'page');
   const text=await page.locator('#fleet').innerText();
   for(const s of ['24 h','7 d','Tokens per project','Tokens per task attempt','invented-account','Input: 100','not-reported','completions/hour','workers started: 1','behind upstream latest','comparison unknown'])assert(text.includes(s),s);
   assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'horizontal overflow '+JSON.stringify(await page.evaluate(()=>[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>innerWidth+1).map(e=>[e.tagName,e.id,e.className,e.getBoundingClientRect().right]).slice(0,12))));
   const disclosure=page.locator('#fleet details').first();const id=await disclosure.getAttribute('id');
   await disclosure.locator('summary').click();
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert.equal(await page.locator(`[id="${id}"]`).getAttribute('open'),null,'refresh reopened disclosure');
   const before=await page.locator('#fleet').innerText();offline=true;
   await page.evaluate(()=>refresh());await page.waitForFunction(()=>!busy);
   assert(await page.locator('#banner').isVisible());
   assert.equal((await page.locator('#fleet').innerText()).replaceAll('Unconfirmed after refresh failure. Retained: ',''),before,'failed refresh lost values');
   assert((await page.locator('[data-fleet-state]').first().innerText()).includes('Unconfirmed')); 
   assert.deepEqual(writes,[]);assert.deepEqual(errors,[]);
   cells.push({width,theme,size,result:'pass'});
   if(width===1280&&theme==='light'&&size===16){offline=false;await page.evaluate(()=>refresh());await page.locator(`[id="${id}"] summary`).click();await page.screenshot({path:path.join(dir,'fleet.png'),fullPage:true});}
   const staticPage=await browser.newPage({javaScriptEnabled:false,viewport:{width,height:1000},colorScheme:theme});
   await staticPage.route('**/*',route=>route.abort());
   await staticPage.setContent(html.replace(/<script[^>]*>[\s\S]*?<\/script>/g,'').replace(/<link[^>]*>/g,'').replace('</head>',`<style>${css} html{font-size:${size}px}</style></head>`));
   assert(await staticPage.locator('#fleet').isVisible(),'server-rendered Fleet missing without JavaScript');
   await staticPage.close();
   await page.close();
  }
  fs.writeFileSync(path.join(dir,'browser.json'),JSON.stringify({platform:process.platform,browser:browser.version(),cells},null,2)+'\n');
  console.log(`Fleet browser: ${cells.length} cells passed; failed refresh, disclosure preservation, no writes`);
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
