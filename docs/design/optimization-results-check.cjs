// Development proof for P3; consumes HTML from DASHQ_RESULTS_PROOF cargo test optimization::tests.
const {chromium}=require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const fs=require('fs'),path=require('path'),http=require('http'),assert=require('assert');
const root=process.cwd(),dir=process.env.DASHQ_RESULTS_PROOF || '/tmp/dashq-67-browser/proof';
let scenario='mixed_versions_and_recipes';
const server=http.createServer((req,res)=>{
 const requested=new URL(req.url,'http://localhost'); let pathname=requested.pathname;
 if(pathname==='/data'){res.setHeader('Content-Type','application/json');return res.end('{}');}
 const file=pathname==='/style.css'||pathname==='/app.js'?path.join(root,'assets',pathname):path.join(dir,(requested.searchParams.has('period')?'production-'+requested.searchParams.get('period')+'-'+(requested.searchParams.get('agent')||''):scenario)+'.html');
 res.setHeader('Content-Type',pathname.endsWith('.js')?'application/javascript':pathname.endsWith('.css')?'text/css':'text/html');res.end(fs.readFileSync(file));
});
(async()=>{await new Promise(r=>server.listen(0,'127.0.0.1',r));const url='http://127.0.0.1:'+server.address().port;
 const browser=await chromium.launch({channel:'chrome',headless:true});
 for(const [width,font,colorScheme] of [[1024,16,'light'],[360,20,'dark']]){
 const page=await browser.newPage({viewport:{width,height:1000},colorScheme,reducedMotion:'reduce'});
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(url);await page.waitForFunction(()=>!busy);await page.addStyleTag({content:`html{font-size:${font}px}`});
 await page.screenshot({path:`${dir}/${width}-${font}-${colorScheme}.png`,fullPage:true});
 assert(await page.locator('#aqresults').isVisible());
 assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'horizontal overflow');
 await page.locator('#results-compressed').click();await page.locator('#results-evidence-summary').click();await page.locator('#results-effect-summary').click();
 await page.locator('#results-compressed').focus();
 scenario='pass_through_and_failure_are_distinct';await page.evaluate(()=>scrollTo(0,100));const scroll=await page.evaluate(()=>scrollY);for(let n=0;n<3;n++)await page.evaluate(()=>refresh());assert.equal(await page.evaluate(()=>scrollY),scroll);assert((await page.locator('#aqresults .stats').innerText()).includes('4 Calls'));scenario='mixed_versions_and_recipes';
 assert.equal(await page.locator('#results-compressed').getAttribute('aria-expanded'),'true');
 assert(await page.locator('#reasons-compressed').isVisible());assert(await page.locator('#results-evidence').evaluate(e=>e.open));assert(await page.locator('#results-effect').evaluate(e=>e.open));
 assert.equal(await page.evaluate(()=>document.activeElement.id),'results-compressed');
 await page.evaluate(()=>{const r=document.createRange();r.selectNodeContents(document.querySelector('#reasons-compressed strong'));getSelection().addRange(r)});
 const selected=await page.evaluate(()=>getSelection().toString());await page.evaluate(()=>refresh());assert.equal(await page.evaluate(()=>getSelection().toString()),selected);await page.evaluate(()=>getSelection().removeAllRanges());
 await page.locator('#results-period').selectOption('7200');await page.waitForURL('**period=7200**');
 await page.locator('#results-client').selectOption('claude-code');await page.waitForURL('**agent=claude-code**');assert((await page.locator('#aqresults .stats').innerText()).includes('2 Calls'));await page.locator('#results-client').selectOption('hermes');await page.waitForURL('**agent=hermes**');await page.waitForFunction(()=>!busy);assert.equal(await page.locator('#results-period').inputValue(),'7200');assert.equal(await page.locator('#results-client').inputValue(),'hermes');assert((await page.locator('#aqresults').innerText()).includes('hermes · —'));await page.evaluate(()=>refresh());assert.equal(await page.locator('#results-client').inputValue(),'hermes');await page.screenshot({path:`${dir}/${width}-selected-missing.png`,fullPage:true});
 for(const name of ['valid_zero','missing_source_is_unknown','partial_source_is_lower_bound','late_and_duplicate_delivery','pass_through_and_failure_are_distinct']){
 scenario=name;await page.goto(url);await page.waitForFunction(()=>!busy);await page.addStyleTag({content:`html{font-size:${font}px}`});
 assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));if(name==='pass_through_and_failure_are_distinct'){await page.locator('#results-failed').click();assert((await page.locator('#reasons-failed').innerText()).includes('timeout'));await page.locator('#results-passed').click();assert((await page.locator('#reasons-passed').innerText()).includes('not_qualified'));}
 await page.screenshot({path:`${dir}/${width}-${name}.png`,fullPage:true});
 }
 assert.deepEqual(errors,[]);scenario='mixed_versions_and_recipes';await page.close();
 }
 await browser.close();server.close();console.log('Browser: '+browser.version()+'; both sizes/themes, 3 refreshes, focus/disclosures/outcome/text selection, filter URLs and four coverage states passed');
})().catch(e=>{console.error(e);server.close();process.exit(1)});
