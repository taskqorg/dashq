// #186 A2-A4: release identity, request fencing and receipt correlation; no native or forge effects.
const fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const pin=JSON.parse(fs.readFileSync('assets/taskq/workspace-provenance.json'));
for(const [file,sha] of Object.entries(pin.files)) assert.equal(crypto.createHash('sha256').update(fs.readFileSync('assets/taskq/'+file)).digest('hex'),sha,file);
const pm=vm.runInThisContext(fs.readFileSync('assets/taskq/pm-composer.js','utf8')+';TaskqPmComposer');
const fixture=JSON.parse(fs.readFileSync('assets/taskq/pm-composer-fixtures.json'));
const r=fixture.request;assert.deepEqual(pm.request(r,r.text,r.op_id,r.issued_at),r);
for(const c of fixture.cases){const v={schema:r.schema,actor_id:r.actor_id,op_id:r.op_id,revision:r.revision,outcome:c.outcome};
 if(c.outcome==='accepted')Object.assign(v,{request_id:'a'.repeat(64),native:{host:'invented-host',runtime:'claude',session:'invented-session'}});
 assert.equal(pm.receipt(v,r).outcome,c.outcome,c.id);
 assert.equal(pm.receipt({...v,op_id:'foreign'},r).outcome,'unknown');
}
assert.throws(()=>pm.request(r,'',r.op_id,100));assert.throws(()=>pm.request(r,'x'.repeat(65537),r.op_id,100));
console.log('PASS #186 release hashes and PM fixtures: typed request, denial, stale, duplicate, unknown, correlated native receipt');
