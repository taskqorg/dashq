// D31: actual server-rendered pages stay inert, usable and within the viewport at large text and narrow widths.
const {chromium}=require('playwright'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const dir=process.env.DASHQ_ARCHIVE_EVIDENCE;
(async()=>{
 const browser=await chromium.launch({headless:true,...(process.env.DASHQ_CHROME?{executablePath:process.env.DASHQ_CHROME}:{})});
 try {
  for(const [width,size] of [[1440,16],[360,20]])for(const theme of ['light','dark']) {
   const page=await browser.newPage({viewport:{width,height:1100},colorScheme:theme});
   const errors=[];page.on('pageerror',e=>errors.push(e.message));
   for(const file of ['list','detail','step','search']) {
    await page.setContent(fs.readFileSync(path.join(dir,file+'.html'),'utf8'));
    await page.addStyleTag({content:`html{font-size:${size}px}`});
    assert(await page.getByRole('heading',{name:'Session archive',exact:true}).isVisible());
    assert(await page.getByLabel('Search all sessions').isVisible());
    assert.equal(await page.locator('script,img,iframe').count(),0,'archive text never creates active content');
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'outer document must not clip or scroll horizontally');
    if(file==='list') {
     await page.locator('.archive-table summary').first().click();
     assert(await page.locator('.archive-conditions').isVisible());
     assert(await page.getByRole('link',{name:'s',exact:true}).isVisible());
    }
    if(file==='detail') {
     assert(await page.getByRole('button',{name:'Save name'}).isVisible());
     await page.getByText('Delete session content',{exact:true}).click();
     assert(await page.getByRole('button',{name:'Delete content',exact:true}).isVisible());
    }
    if(file==='step') {
     assert((await page.locator('pre').first().innerText()).includes('<script>alert(1)</script>'));
     await page.getByText('Exact raw record and source range',{exact:true}).click();
     assert(await page.locator('details pre').isVisible());
    }
    await page.screenshot({path:path.join(dir,`${file}-${width}-${size}-${theme}.png`),fullPage:true});
   }
   assert.deepEqual(errors,[]);
   await page.close();
  }
 } finally {await browser.close();}
 console.log('archive: inert output, forms, raw disclosure and narrow/large-text layout passed');
})().catch(e=>{console.error(e);process.exitCode=1});
