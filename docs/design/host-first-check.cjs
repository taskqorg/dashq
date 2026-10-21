// #169 A1–A5: protect host-first placement, honest missing data, shared UI and readable reflow.
// Design-fixture evidence only; no live producer, refresh, native tray or write qualification.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict');
const {chromium} = require('playwright');
const root = path.resolve(__dirname, '../..'), capture = process.argv.includes('--capture');
const out = path.join(__dirname, 'evidence/host-first');
const htmlPath = 'docs/design/host-first-preview.html';
const fixture = JSON.parse(fs.readFileSync(path.join(__dirname, 'host-first-fixture.json')));
const hash = p => crypto.createHash('sha256').update(fs.readFileSync(path.join(root,p))).digest('hex');
const canonicalHash = p => crypto.createHash('sha256').update(fs.readFileSync(path.join(root,p),'utf8').replace(/\r\n/g,'\n')).digest('hex');
const manifest = {schema:1,kind:'accepted-design-fixture-evidence',base:'e7d0fe8f7c001a2f641604859a22a0617da8b9e9',hashEncoding:'Source: UTF-8 LF (checkout CRLF normalized); PNG: raw bytes',
  rules:['TaskQ R12/R13, § 7','DashQ D6/D8–D22','specq W1–W15/F1–F7'],
  defect:'Baseline fixture places Attention, connections and integrations before hosts; owner A1 requires hosts first.',
  retainedProtection:'Unknown data never becomes healthy or zero; shared TaskQ assets/authority stay unchanged.',
  baseline:{firstSection:'attention',requestedA1:'FAIL (fixture/page.html; not a new live capture)'},
  ownerApproval:{state:'approved',answer:'https://github.com/taskqorg/dashq/issues/169#issuecomment-6318252313',reviewedCommit:'74ff807fcb0a0129624f0cc84312c318afa94f84',scope:'Specification and five fixture screens; implementation and qualification gaps retained'},screens:[],matrix:[],hashes:{},notTested:['live online/login/host-queue producers','full tray workspace/control release','production automatic refresh and draft recovery','native tray conformance','authorized writes','deployment']};
manifest.reviewFixes=[
  {defect:'Host summary was global and project navigation dropped host',before:'Read-only review: fra rows Doing 1/Review 1/Ready 1 but counters Doing 2/Ready 2/Ask 1',after:'Scoped counter and project-navigation assertions PASS'},
  {defect:'Product/login observations had no separate fixture times',before:'Read-only review: host age reused, product times omitted',after:'Independent producer/probe/release times in fixtures and accessible cells PASS'},
  {defect:'Attention question opened generic task list',before:'Read-only review: no host/question anchor in target',after:'Direct win question #304 anchor and matching card assertions PASS'}
];
const docs=['dashq.md','docs/design/first-screen.md','docs/design/overview.md','docs/design/boards.md','docs/design/connections.md','docs/design/optimization-results.md'];
const previewFiles=[htmlPath,'docs/design/host-first-fixture.json','docs/design/host-first-check.cjs'];
for(const p of [...docs,...previewFiles,'fixture/page.html','assets/taskq/project-blocks.js','assets/taskq/project-blocks.css','assets/taskq/answer-card.js','assets/taskq/answer-card.css'])manifest.hashes[p]=canonicalHash(p);
const provenance=JSON.parse(fs.readFileSync(path.join(root,'assets/taskq/provenance.json')));
for(const f of ['project-blocks.js','project-blocks.css','answer-card.js','answer-card.css'])assert.equal(canonicalHash('assets/taskq/'+f),provenance.files[f],'Vendor changed: '+f);
assert.equal(fixture.kind,'invented-design-fixture');
assert.equal(fixture.hosts[2].cpu,null);
assert.equal(fixture.hosts[2].queued,null);
assert.equal(fixture.project.closed,null);
const doc=fs.readFileSync(path.join(root,'dashq.md'),'utf8');
assert.ok(doc.split('\n').length-1<200,'Contract must shrink from 200 lines');
assert.ok(Buffer.byteLength(doc)<58653,'Contract must shrink in bytes');
for(const p of docs){
  const text=fs.readFileSync(path.join(root,p),'utf8');
  for(const m of text.matchAll(/\]\(([^)]+)\)/g)){
    const target=m[1].split('#')[0];
    if(!target||/^[a-z]+:/i.test(target)||target==='evidence/host-first/manifest.json')continue;
    assert.ok(fs.existsSync(path.resolve(path.dirname(path.join(root,p)),target)),p+' broken link: '+m[1]);
  }
}
// Historical baseline is pinned in the accepted manifest; #170 now protects actual host-first placement.
assert.ok(fs.readFileSync(path.join(root,'fixture/page.html'),'utf8').includes('id="host-grid"'));
manifest.documentation={lines:doc.split('\n').length-1,bytes:Buffer.byteLength(doc),baseline:{lines:200,bytes:58653},links:'PASS',releasedAssets:'PASS (canonical LF hashes)'};
const shots=[['hosts',1280,'light','hosts-1280-light.png'],['hosts',1280,'dark','hosts-1280-dark.png'],['hosts',360,'dark','hosts-360-dark.png'],['host',1280,'dark','host-detail.png'],['tasks',1280,'light','tasks.png']];
(async()=>{
  fs.mkdirSync(out,{recursive:true});
  const browser=await chromium.launch({headless:true,...(process.env.DASHQ_CHROME?{executablePath:process.env.DASHQ_CHROME}:{})});
  manifest.environment={browser:browser.version(),platform:process.platform,font:'system-ui / Segoe UI fallback',deviceScaleFactor:1,height:900};
  const errors=[],writes=[],external=[];
  try{
    for(const width of [1280,360])for(const font of [16,20])for(const theme of ['light','dark'])for(const view of ['hosts','host','tasks']){
      const page=await browser.newPage({viewport:{width,height:900},colorScheme:theme,reducedMotion:'reduce',deviceScaleFactor:1});
      page.on('pageerror',e=>errors.push(e.message));
      await page.route('**/*',async route=>{
        const req=route.request(),u=new URL(req.url());
        if(req.method()!=='GET')writes.push(req.method()+' '+u.pathname);
        if(u.hostname!=='fixture.test'){external.push(u.href);return route.abort();}
        const file=path.resolve(root,'.'+decodeURIComponent(u.pathname));
        if(!file.startsWith(root+path.sep))return route.abort();
        const contentType=file.endsWith('.js')?'application/javascript':file.endsWith('.css')?'text/css':file.endsWith('.json')?'application/json':'text/html';
        await route.fulfill({contentType,body:fs.readFileSync(file)});
      });
      await page.goto('http://fixture.test/'+htmlPath+'?view='+view);
      await page.waitForSelector('html[data-ready=true]');
      await page.evaluate(size=>document.documentElement.style.fontSize=size+'px',font);
      const geometry=await page.evaluate(()=>{
        const visible=el=>el.getClientRects().length;
        const overflow=[...document.querySelectorAll('main *,header *')].filter(visible).filter(el=>{
          const r=el.getBoundingClientRect();return r.left<-.5||r.right>innerWidth+.5;
        }).map(el=>({tag:el.tagName,cls:el.className,text:el.textContent.slice(0,60)}));
        const hosts=document.querySelector('[data-view=hosts]');
        return {scrollWidth:document.documentElement.scrollWidth,viewport:innerWidth,overflow,hostsTop:visible(hosts)?hosts.getBoundingClientRect().top:null,firstHeading:document.querySelector('[data-view]:not([hidden]) h1').textContent,font:getComputedStyle(document.body).fontSize};
      });
      assert.deepEqual(geometry.overflow,[],JSON.stringify({width,font,theme,view,...geometry}));
      assert.ok(geometry.scrollWidth<=width,'Horizontal overflow');
      assert.equal(geometry.font,font+'px');
      if(view==='hosts'){
        assert.equal(geometry.firstHeading,'Hosts');
        assert.ok(geometry.hostsTop<180,'Hosts displaced from first viewport');
        assert.equal(await page.locator('#host-grid article').count(),3);
        assert.equal(await page.locator('#host-grid .identity').last().innerText().then(s=>s.includes('Unknown')),true);
        assert.equal(await page.locator('#host-grid article').last().locator('.meter').count(),0,'Unknown must not get bars');
        assert.equal(await page.locator('#action-list .action-row a').count(),4);
        assert.match(await page.locator('#host-grid .service').first().getAttribute('aria-label'),/observed 2026-10-21T11:59:48Z; release checked/);
        assert.match(await page.locator('#host-grid .login').first().getAttribute('aria-label'),/observed 2026-10-21T11:59:40Z/);
        assert.equal(await page.locator('.source').first().getAttribute('open'),null,'Diagnostics start closed');
      }
      if(view==='tasks'){
        assert.equal(await page.locator('#task-list .tqp-task').count(),fixture.project.tasks.length);
        assert.equal(await page.locator('[data-qac-state="read-only"]').count(),1);
        assert.equal(await page.locator('[data-qac=choose]:enabled').count(),0);
        assert.equal((await page.locator('[data-state=closed]').innerText()).trim(),'?');
      }
      manifest.matrix.push({width,font,theme,view,result:'PASS',...geometry});
      const shot=shots.find(s=>s[0]===view&&s[1]===width&&s[2]===theme&&font===16);
      if(shot&&capture){await page.screenshot({path:path.join(out,shot[3]),fullPage:true});manifest.screens.push({file:shot[3],view,width,font,theme,viewportHeight:900,fullPage:true});}
      if(width===1280&&font===16&&theme==='light'&&view==='hosts'){
        await page.locator('#host-grid article').first().locator('.count-link').first().click();
        await page.waitForSelector('html[data-ready=true]');
        assert.equal(new URL(page.url()).searchParams.get('host'),'fra');
        assert.equal(await page.locator('#task-list .tqp-task').count(),3);
        assert.equal((await page.locator('[data-state=ready]').innerText()).trim(),'1','Host counters must match scoped rows');
        assert.equal(await page.locator('[data-state=ask]').count(),0,'Other host question must not enter scoped counts');
        await page.locator('[data-state=ready]').click();await page.waitForSelector('html[data-ready=true]');
        assert.equal(await page.locator('#task-list .tqp-task').count(),1);
        await page.locator('[data-tqp=project]').click();await page.waitForSelector('html[data-ready=true]');
        assert.equal(new URL(page.url()).searchParams.get('host'),'fra','Project navigation retains host');
        await page.reload();await page.waitForSelector('html[data-ready=true]');
        assert.ok((await page.locator('#scope-label').innerText()).includes('Host: fra'));
        await page.goBack();await page.goBack();await page.goBack();await page.waitForSelector('html[data-ready=true]');
        assert.equal(await page.locator('[data-view=hosts]').isVisible(),true);
        await page.getByRole('link',{name:'Answer question →'}).click();await page.waitForSelector('html[data-ready=true]');
        assert.equal(new URL(page.url()).hash,'#answer-304');
        assert.equal(new URL(page.url()).searchParams.get('host'),'win');
        assert.equal(await page.locator('#answer-304 .qac').isVisible(),true);
        await page.goto('http://fixture.test/'+htmlPath+'?view=unsupported');await page.waitForSelector('html[data-ready=true]');
        assert.match(await page.locator('#gap-reason').innerText(),/Unknown preview view/);
      }
      await page.close();
    }
    assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);assert.deepEqual(external,[]);
    manifest.interaction={routesBackReload:'PASS',scopedCounters:'PASS',directQuestion:'PASS',separateFieldTimes:'PASS',unknownRoute:'PASS',consoleErrors:errors,writes,externalRequests:external};
    if(!capture&&fs.existsSync(path.join(out,'manifest.json')))manifest.screens=JSON.parse(fs.readFileSync(path.join(out,'manifest.json'))).screens;
    for(const s of manifest.screens)s.sha256=hash('docs/design/evidence/host-first/'+s.file);
    if(capture)fs.writeFileSync(path.join(out,'manifest.json'),JSON.stringify(manifest,null,2)+'\n');
    assert.ok(fs.existsSync(path.join(out,'manifest.json')),'Generated evidence link resolves');
    console.log(JSON.stringify({matrix:manifest.matrix.length,result:'PASS',screens:manifest.screens.length,documentation:manifest.documentation,interaction:manifest.interaction,notTested:manifest.notTested},null,2));
  }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
