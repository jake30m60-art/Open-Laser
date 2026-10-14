<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../i18n/index';
  import Modal from './Modal.svelte';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { access } from '../lib/access.svelte';
  import { explain } from '../lib/format';
  import { defaultTemplate, outerFirst, referenceDrawing, nearestPick, pointOn, curvePoints, mapped, type AlignmentView, type Template, type Plan, type SimulationReply, type XY } from '../lib/workpiece-alignment';
  let {remnantId = null}: {remnantId?: string | null} = $props();
  let open=$state(false), busy=$state(false), error=$state(''), dirty=$state(false);
  let view=$state<AlignmentView|null>(null), template=$state<Template>(defaultTemplate());
  let plan=$state<Plan|null>(null), reply=$state<SimulationReply|null>(null);
  let associate=$state(false), selected=$state<number|null>(null), part=$state('');
  let dx=$state(25),dy=$state(-20),angle=$state(2),fault=$state('none');
  let associationConfirmed=$state(false);
  const doc=$derived(server.doc!);
  const disabled=$derived(busy||!access.canControl||!!doc.machine.operation||doc.machine.program?.state==='held');
  const drawing=$derived(referenceDrawing(template.shape));
  const curves=$derived(drawing.contours.flatMap((c,ci)=>c.curves.map((curve,ei)=>({curve,ci,ei,points:curvePoints(curve)}))));
  const picks=$derived(template.shape.kind==='disc'?Array.from({length:8},(_,i)=>({contour:0,curve:0,t:i/8})):template.picks);
  const points=$derived(picks.map(p=>{const c=drawing.contours[p.contour]?.curves[p.curve];return c?pointOn(c,p.t):null;}));
  const bounds=$derived.by(()=>{
    const p=curves.flatMap(c=>c.points).filter(p=>p.every(Number.isFinite));
    if(!p.length)return {x:-100,y:-100,w:200,h:200};
    const xs=p.map(p=>p[0]),ys=p.map(p=>p[1]);const pad=template.search_mm*2+20;
    const x=Math.min(...xs)-pad,y=Math.min(...ys)-pad;
    return {x,y,w:Math.max(10,Math.max(...xs)+pad-x),h:Math.max(10,Math.max(...ys)+pad-y)};
  });
  const mark=$derived(Math.max(bounds.w,bounds.h)/90);
  let resultRevision=$state(0),resultHomed=$state(false),resultConfiguration=$state('');
  const stale=$derived(!!reply && (doc.draft_revision!==resultRevision || doc.machine.session.homed!==resultHomed || JSON.stringify(doc.bindings)!==resultConfiguration));
  const machinePaths=$derived(reply?.compiled?.moves.map(m=>m.points)??[]);
  const actualOutline=$derived(reply?.view.result?curves.map(c=>c.points.map(p=>mapped(p,reply!.view.result!.pose))):[]);
  const machineBounds=$derived.by(()=>{
    const p=[...actualOutline.flat(),...machinePaths.flat(),...(reply?.plan.searches.flatMap(s=>[s.start,s.end])??[])];
    if(!p.length)return '0 0 100 100';const xs=p.map(p=>p[0]),ys=p.map(p=>p[1]);const x=Math.min(...xs)-20,y=Math.min(...ys)-20;
    return `${x} ${y} ${Math.max(...xs)-x+20} ${Math.max(...ys)-y+20}`;
  });
  const poly=(p:XY[])=>p.map(p=>p.join(',')).join(' ');
  async function load(){try{view=await api.alignment();}catch(e){error=explain(e);}}
  onMount(()=>{void load();});
  async function show(){await load();if(view?.template){template=structuredClone($state.snapshot(view.template));associationConfirmed=true;}open=true;dirty=false;reply=null;error='';if(remnantId){try{template=await api.alignmentRemnant(remnantId);dirty=true;}catch(e){error=explain(e);return;}}await check();}
  function edited(){dirty=true;reply=null;plan=null;error='';}
  async function check(){const current=JSON.stringify(template);try{const checked=await api.alignmentPlan($state.snapshot(template));if(current===JSON.stringify(template)){plan=checked;error='';}}catch(e){if(current===JSON.stringify(template)){plan=null;error=explain(e);}}}
  async function save(enabled=view?.enabled??false){
    if(!view)return;busy=true;error='';
    try{view=await api.saveAlignment({revision:view.revision,enabled,template:$state.snapshot(template)});dirty=false;reply=null;await check();}catch(e){error=explain(e);}finally{busy=false;}
  }
  function kind(value:string){
    template.remnant=null;
    template.shape=value==='disc'?{kind:'disc',diameter:600}:value==='special'?{kind:'special',drawing:{contours:[]}}:{kind:'rectangle',width:600,height:500};
    template.picks=[];template.drawing_point=value==='disc'?[0,0]:[300,250];selected=null;associationConfirmed=false;edited();
  }
  async function source(){busy=true;try{const d=part?(await api.partDrawing(part)).drawing:await api.alignmentReference();template.remnant=null;template.shape={kind:'special',drawing:outerFirst(d)};template.picks=[];selected=null;associationConfirmed=false;edited();}catch(e){error=explain(e);}finally{busy=false;}}
  async function choose(p:XY){
    if(disabled)return;
    if(associate){
      template.drawing_point=p;edited();await save();
      if(!view||dirty)return;
      busy=true;try{view=await api.associateAlignment(view.revision,p);if(view.template)template=structuredClone($state.snapshot(view.template));associationConfirmed=true;associate=false;await check();}catch(e){error=explain(e);}finally{busy=false;}return;
    }
    if(template.shape.kind==='disc')return;
    const hit=nearestPick(drawing,p);if(!hit||hit.distance>mark*3)return;
    if(selected!==null){template.picks[selected]=hit.pick;selected=null;}else if(template.picks.length<32)template.picks.push(hit.pick);
    edited();await check();
  }
  function click(e:MouseEvent){const svg=e.currentTarget as SVGSVGElement;const m=svg.getScreenCTM();if(!m)return;const p=new DOMPoint(e.clientX,e.clientY).matrixTransform(m.inverse());void choose([p.x,-p.y]);}
  async function simulate(){
    if(dirty||!view?.enabled)await save(true);if(dirty||!view?.enabled)return;
    busy=true;reply=null;error='';
    const revision=doc.draft_revision,homed=doc.machine.session.homed,configuration=JSON.stringify(doc.bindings);
    try{reply=await api.simulateAlignment({revision:view.revision,delta:[dx,dy],angle_deg:angle,fault});view=reply.view;plan=reply.plan;resultRevision=revision;resultHomed=homed;resultConfiguration=configuration;}catch(e){error=explain(e);await load();}finally{busy=false;}
  }
  async function cancel(){try{view=await api.cancelAlignment();reply=null;error='';}catch(e){error=explain(e);}}
</script>

<button class="btn" disabled={!doc.draft && !remnantId} onclick={show}>{t('Align workpiece')}{#if view?.enabled} · {t('Enabled')}{/if}</button>
{#if open}
<Modal title={t('Align workpiece')} wide onclose={()=>open=false}>
  <div class="alignment">
    <p class="notice">{t('Offline development mode. No real measurement or machine motion. Simulated results never enable cutting.')}</p>
    <label class="toggle"><input type="checkbox" checked={view?.enabled??false} disabled={disabled} onchange={e=>save(e.currentTarget.checked)} />{t('Enable workpiece alignment (locks machine Start)')}</label>
    <div class="fields"><button class="btn" disabled={disabled||!doc.draft} onclick={async()=>{try{template=await api.alignmentStock();edited();await check();}catch(e){error=explain(e);}}}>{t('Use current remnant stock as reference')}</button></div>
    {#if template.remnant}<p>{t('Remnant reference revision {0}',template.remnant.revision)}</p>{/if}
    <div class="fields">
      <label>{t('Reference shape')}<select value={template.shape.kind} disabled={disabled} onchange={e=>kind(e.currentTarget.value)}><option value="rectangle">{t('Sheet')}</option><option value="disc">{t('Disc')}</option><option value="special">{t('Special part')}</option></select></label>
      {#if template.shape.kind==='rectangle'}
        <label>{t('Width')} (mm)<input type="number" min="0.1" max="10000" bind:value={template.shape.width} oninput={()=>{associationConfirmed=false;edited();}} /></label>
        <label>{t('Height')} (mm)<input type="number" min="0.1" max="10000" bind:value={template.shape.height} oninput={()=>{associationConfirmed=false;edited();}} /></label>
      {:else if template.shape.kind==='disc'}
        <label>{t('Outside diameter')} (mm)<input type="number" min="0.1" max="10000" bind:value={template.shape.diameter} oninput={edited} /></label>
      {:else}
        <label>{t('Reference source')}<select bind:value={part}><option value="">{t('Current drawing')}</option>{#each doc.library.parts as p}<option value={p.id}>{p.name}</option>{/each}</select></label>
        <button class="btn" disabled={disabled} onclick={source}>{t('Load reference')}</button>
      {/if}
    </div>
    <p>{t('Reference only; never added to the cut. Click distributed edges to add measurements. Select a point below to move it.')}</p>
    <div class="fields">
      <label>{t('Placement uncertainty per axis')} ± mm<input type="number" min="0.1" max="50" bind:value={template.search_mm} oninput={edited} onchange={check} /></label>
      {#if template.shape.kind!=='disc'}<label>{t('Maximum rotation')} ± °<input type="number" min="0" max="10" bind:value={template.angle_deg} oninput={edited} onchange={check} /></label>{/if}
      <label>{t('Maximum residual')} mm<input type="number" min="0.005" max="1" step="0.01" bind:value={template.tolerance_mm} oninput={edited} onchange={check} /></label>
    </div>
    <svg class="reference" viewBox={`${bounds.x} ${-bounds.y-bounds.h} ${bounds.w} ${bounds.h}`} role="button" tabindex="0" aria-label={t('Choose measurement points on the reference contour')} onclick={click} onkeydown={e=>{if(e.key==='Escape'){selected=null;associate=false;}}}>
      <g transform="scale(1,-1)">
        {#each curves as c}<polyline points={poly(c.points)} fill="none" stroke="currentColor" stroke-width={mark/4} />{/each}
        {#if plan}{#each plan.searches as s}{@const off=plan.nominal.translation}<line x1={s.start[0]-off[0]} y1={s.start[1]-off[1]} x2={s.end[0]-off[0]} y2={s.end[1]-off[1]} stroke="#db8b20" stroke-width={mark/3} /><circle cx={s.start[0]-off[0]} cy={s.start[1]-off[1]} r={mark/2} fill="#db8b20" />{/each}{/if}
        {#each points as p,i}{#if p}<circle cx={p[0]} cy={p[1]} r={mark} fill={selected===i?'#ef9d30':'#25a4bc'} /><text x={p[0]+mark*1.4} y={-p[1]-mark} transform="scale(1,-1)" font-size={mark*2.2} fill="currentColor">{i+1}</text>{/if}{/each}
        <path d={`M ${template.drawing_point[0]-mark*2} ${template.drawing_point[1]} h ${mark*4} M ${template.drawing_point[0]} ${template.drawing_point[1]-mark*2} v ${mark*4}`} stroke="#cc4c86" stroke-width={mark/2} />
      </g>
    </svg>
    <small>{t('Orange: bounded search from the solid dot toward the edge. Pink cross: approximate head association. Drawing origin stays unchanged.')}</small>
    {#if template.shape.kind!=='disc'}<div class="points">{#each template.picks as p,i}<span><button class="btn" aria-pressed={selected===i} onclick={()=>selected=selected===i?null:i}>{i+1}</button><button class="btn" aria-label={t('Remove measurement {0}',i+1)} onclick={()=>{template.picks.splice(i,1);selected=null;edited();void check();}}>×</button></span>{/each}<button class="btn" onclick={()=>{template.picks=[];selected=null;edited();}}>{t('Clear points')}</button></div>{:else}<p>{t('Eight automatic edge measurements; center only, no rotation correction.')}</p>{/if}
    <details><summary>{t('Keyboard measurement selection')}</summary><div class="points">{#each curves as c}<button class="btn" disabled={disabled||template.shape.kind==='disc'} onclick={()=>choose(pointOn(c.curve,0.3))}>{t('Contour {0}, edge {1}',c.ci+1,c.ei+1)} · 30%</button><button class="btn" disabled={disabled||template.shape.kind==='disc'} onclick={()=>choose(pointOn(c.curve,0.7))}>{t('Contour {0}, edge {1}',c.ci+1,c.ei+1)} · 70%</button>{/each}</div></details>
    <div class="fields">
      <button class="btn" aria-pressed={associate} disabled={disabled||!doc.readiness.set_origin.ok} onclick={()=>associate=!associate}>{t('Reassociate head position')}</button>
      <span>{associate?t('Click the drawing point approximately under the stationary head.'):t('Saved search position: X {0}, Y {1} mm',template.machine_point[0].toFixed(2),template.machine_point[1].toFixed(2))}</span>
    </div>
    <details><summary>{t('Offline search position')}</summary><p>{t('Numeric coordinates are a simulation assumption, not a captured machine position.')}</p><div class="fields"><label>X (mm)<input type="number" bind:value={template.machine_point[0]} oninput={edited} /></label><label>Y (mm)<input type="number" bind:value={template.machine_point[1]} oninput={edited} /></label><label>{t('Drawing reference X')}<input type="number" bind:value={template.drawing_point[0]} oninput={edited} /></label><label>{t('Drawing reference Y')}<input type="number" bind:value={template.drawing_point[1]} oninput={edited} /></label></div></details>
    {#if !associationConfirmed}<small>{t('No head association captured in this dialog. Offline values are only a search assumption.')}</small>{/if}
    <label class="toggle"><input type="checkbox" bind:checked={template.every_job} onchange={edited} />{t('Measure before every new job')}</label>
    <div class="fields"><button class="btn" disabled={disabled} onclick={()=>save()}>{t('Save measurement template')}</button><button class="btn" disabled={busy} onclick={check}>{t('Check measurement plan')}</button><button class="btn" disabled title={t('Real edge protocol and verified lid input are still missing')}>{t('Start measurement')}</button></div>
    <p>{t('After a real measurement: Workpiece aligned. Close the lid and press Start. Closing the lid alone never starts cutting.')}</p>
    {#if error}<p class="problem" role="alert">{t(error)}</p>{:else if plan}<p role="status">{t('Measurement plan is geometrically usable. This does not verify machine travel, supports or sensor behavior.')}</p>{/if}
    <details open><summary>{t('Offline measurement simulation')}</summary>
      <div class="fields"><label>ΔX (mm)<input type="number" bind:value={dx} /></label><label>ΔY (mm)<input type="number" bind:value={dy} /></label>{#if template.shape.kind!=='disc'}<label>Δ°<input type="number" bind:value={angle} /></label>{/if}<label>{t('Test case')}<select bind:value={fault}><option value="none">{t('Valid workpiece')}</option><option value="missing">{t('Missing material')}</option><option value="hole">{t('Hole in search path')}</option><option value="wrong_size">{t('Wrong dimensions')}</option><option value="outside">{t('Search range exceeded')}</option><option value="outlier">{t('Measurement outlier')}</option></select></label></div>
      <div class="fields"><button class="btn" disabled={disabled||!plan} onclick={simulate}>{t('Simulate measurement')}</button><button class="btn" onclick={cancel}>{t('Discard measurement')}</button></div>
      {#if reply?.view.result && !stale}
        <p class="notice">{t('SIMULATED — cutting remains locked.')}</p>
        <p>{t('Translation')} X {reply.view.result.pose.translation[0].toFixed(3)} / Y {reply.view.result.pose.translation[1].toFixed(3)} mm · {t('Angle')} {(reply.view.result.pose.angle*180/Math.PI).toFixed(4)}° · {t('Maximum residual')} {reply.view.result.max_error_mm.toFixed(4)} mm</p>
        {#if reply.compile_problem}<p class="problem">{t('Aligned machining preview unavailable')}: {t(reply.compile_problem)}</p>{/if}
        <svg class="machine-preview" viewBox={machineBounds} role="img" aria-label={t('Simulated reference and compiled machining paths in machine coordinates')}>
          {#each actualOutline as p}<polyline points={poly(p)} fill="none" stroke="#888" stroke-width="1.5" stroke-dasharray="6 4" />{/each}
          {#each machinePaths as p}<polyline points={poly(p)} fill="none" stroke="#25a4bc" stroke-width="1.5" />{/each}
          {#each reply.plan.searches as s}<line x1={s.start[0]} y1={s.start[1]} x2={s.end[0]} y2={s.end[1]} stroke="#db8b20" stroke-width="2" />{/each}
          {#each reply.view.result.hits as p}<circle cx={p[0]} cy={p[1]} r="3" fill="#cc4c86" />{/each}
        </svg><small>{t('Dashed: reference only. Blue: separately compiled machining paths. Orange: search. Pink: simulated edge hits.')}</small>
      {:else if stale}<p class="problem">{t('Measurement is stale. Simulate again after the change.')}</p>{/if}
    </details>
  </div>
</Modal>
{/if}
<style>
  .alignment{display:grid;gap:12px;max-width:100%;}.alignment p{margin:0}.fields,.points{display:flex;gap:10px;align-items:end;flex-wrap:wrap}.fields label{display:grid;gap:4px;flex:1;min-width:110px}.fields input,.fields select{width:100%;min-width:0}.toggle{display:flex;gap:8px;align-items:center}.notice{padding:12px;border:1px solid #bc842a;border-radius:6px;background:#bc842a15}.problem{color:var(--danger,#db5959)}.reference,.machine-preview{width:100%;height:330px;border:1px solid var(--border,#8885);border-radius:6px;background:var(--canvas-bg,#8881)}.reference{cursor:crosshair}.machine-preview{transform:scaleY(-1)}small{opacity:.8}.points span{display:flex;gap:2px}details>div{margin-top:10px}
</style>
