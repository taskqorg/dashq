 const reportLanes=agents.filter(a=>a.host==='Mac'&&a.segments);
 const reportTime={work:0,wait:0,unknown:0};
 for(const attempt of reportLanes)for(const [kind,minutes] of attempt.segments)reportTime[kind]+=minutes;
 const timeNames={work:'Work',wait:'Wait',unknown:'Unknown'};
 root.querySelector('[data-report-time-chart]').innerHTML=`<div class="dq-report-bar" aria-label="Example: 60 agent-minutes, work ${reportTime.work}, waiting ${reportTime.wait}, unknown ${reportTime.unknown}">${Object.entries(reportTime).map(([kind,minutes])=>`<button data-report-detail="${kind}" style="flex-grow:${minutes}" aria-label="${timeNames[kind]}: ${minutes} agent-minutes" data-tooltip="${timeNames[kind]} · ${minutes} agent-min"></button>`).join('')}</div><div class="dq-report-labels">${Object.entries(reportTime).map(([kind,minutes])=>`<button data-report-detail="${kind}"><i class="dq-report-key ${kind==='wait'?'dq-wait':kind==='unknown'?'dq-unknown':''}" aria-hidden="true"></i>${timeNames[kind]} ${minutes}m</button>`).join('')}</div>`;
 let reportSelection=null,reportOrigin=null;
 const reportKeys=['work','wait','unknown','coverage'];
 function reportDetail(key){
  if(!reportKeys.includes(key))return;
  reportSelection=key;
  root.querySelector('[data-report-inspector]').hidden=false;
  root.querySelector('[data-report-detail-title]').textContent=timeNames[key]?timeNames[key]+' · '+reportTime[key]+' agent-min':'Sources & cost basis';
  let body='';
  if(timeNames[key]){
   body=`<p class="dq-report-note">Example attempt intervals, not a live time report. ${key==='unknown'?'Unobserved intervals remain unknown.':'Explicit stage boundaries; parallel attempts are summed and nested spans counted once.'}</p><div class="dq-report-list">${reportLanes.map(a=>`<div class="dq-report-record"><span>${a.name} · TaskQ #${a.taskId} · attempt 1<small>${a.segments.filter(s=>s[0]===key).map(s=>s[2]+' '+s[1]+'m').join(' · ')}</small></span><button class="dq-control" data-agent-select="${a.id}">${a.segments.filter(s=>s[0]===key).reduce((n,s)=>n+s[1],0)}m ${mark('arrow-up-right')}</button></div>`).join('')}</div>`;
  }else{
   body='<dl class="dq-report-list dq-report-facts"><div class="dq-report-record"><dt>Actual expenses</dt><dd>Billing source not connected.<small>API charges, subscriptions and infrastructure remain separate.</small></dd></div><div class="dq-report-record"><dt>API estimates</dt><dd>TaskQ attempt accounting not connected.<small>Price version, currency, model and known/of required.</small></dd></div><div class="dq-report-record"><dt>Existing evidence</dt><dd>Owner-private accounting audit.<small>Operational counts and amounts are withheld. Require verified currency, price basis and field coverage before a cost claim.</small></dd></div><div class="dq-report-record"><dt>Time</dt><dd>Example: 39 work +8 wait +13 unknown =60 agent-min.<small>Two attempt windows, not 60 minutes of wall time. Requires qualified stage/transition projection.</small></dd></div><div class="dq-report-record"><dt>Savings</dt><dd>Unknown.<small>Repeat identity and paired outcome evidence not established.</small></dd></div></dl>';
  }
  root.querySelector('[data-report-detail-body]').innerHTML=body;
  root.querySelector('[data-report-announcement]').textContent='Opened '+root.querySelector('[data-report-detail-title]').textContent;
  if(globalThis.lucide)lucide.createIcons({attrs:{width:18,height:18}});
 }
 function saveReport(){window.openai?.setWidgetState({modelContent:{reportDetail:reportSelection},privateContent:{...(window.openai?.widgetState?.privateContent||{}),reportStudy:{v:2,detail:reportSelection}}})?.catch(()=>{});}
 root.addEventListener('click',event=>{
  const trigger=event.target.closest('[data-report-detail]');
  if(trigger){reportOrigin=trigger;reportDetail(trigger.dataset.reportDetail);root.querySelector('[data-report-close]').focus({preventScroll:true});requestAnimationFrame(()=>requestAnimationFrame(()=>root.querySelector('[data-report-inspector]').scrollIntoView({block:'nearest'})));saveReport();return;}
  if(event.target.closest('[data-report-close]')){root.querySelector('[data-report-inspector]').hidden=true;const origin=reportOrigin||root.querySelector(`[data-report-detail="${reportSelection}"]`);reportSelection=null;origin?.focus({preventScroll:true});origin?.scrollIntoView({block:'nearest'});saveReport();}
 });
 const savedReport=window.openai?.widgetState?.privateContent?.reportStudy;
 if(savedReport?.v===2&&reportKeys.includes(savedReport.detail))reportDetail(savedReport.detail);
 window.addEventListener('openai:set_globals',event=>{const next=event.detail?.globals?.widgetState?.privateContent?.reportStudy;if(next?.v!==2||next.detail===reportSelection)return;if(reportKeys.includes(next.detail))reportDetail(next.detail);else if(next.detail===null){reportSelection=null;root.querySelector('[data-report-inspector]').hidden=true;}});
 console.assert(Object.values(reportTime).reduce((a,b)=>a+b,0)===60&&reportTime.wait===8&&reportTime.unknown===13,'Attempt time and unknown intervals must reconcile');
 console.assert(!root.querySelector('[data-report-cost-total]')&&!root.querySelector('[data-report-repeat-cost]'),'No unsupported cost attribution or repeat subtotal');
