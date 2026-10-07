<script lang="ts">
  import { api } from '../../api/client';
  import { t } from '../../i18n/index';
  import { orderRows, moveOrderRow } from '../../lib/order-tree';
  import { server } from '../../stores/server.svelte';
  import { featureEdits } from '../../stores/feature-edits';
  import { ui } from '../../stores/ui.svelte';
  import { explain } from '../../lib/format';
  import type { Features, Preview } from '../../api';
  import type { SetFeatures } from './features/form';
  let { preview, features, set, onfocus }: { preview: Preview|null; features: Features; set: SetFeatures; onfocus: (source:number)=>void } = $props();
  const draft=$derived(server.doc!.draft!);
  const rows=$derived(orderRows(preview));
  const layers=$derived([...new Set(rows.map(r=>r.layer))]);
  const active=$derived(new Set(rows.flatMap(r=>r.sources)));
  const inactive=$derived(draft.placed.map((_,i)=>i).filter(i=>!active.has(i)));
  const blocked=$derived(featureEdits.busy || !!draft.error || !!ui.picking);
  let layerBusy=$state(false);
  let pointer=$state<{id:number;from:number;x:number;y:number;revision:number}|null>(null);
  let pointerMoved=false;
  function startPointer(e:PointerEvent,from:number){
    if(blocked || query.trim() || e.button!==0)return;
    e.preventDefault();e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    pointer={id:e.pointerId,from,x:e.clientX,y:e.clientY,revision:draft.revision};pointerMoved=false;dragging=from;
  }
  function pointerMove(e:PointerEvent){
    if(!pointer || pointer.id!==e.pointerId)return;
    if(Math.hypot(e.clientX-pointer.x,e.clientY-pointer.y)<4)return;
    pointerMoved=true;
    const target=document.elementFromPoint(e.clientX,e.clientY)?.closest<HTMLElement>('[data-order-row]');
    const i=target ? Number(target.dataset.orderRow) : -1;
    over=i>=0 && rows[i]?.layer===rows[pointer.from]?.layer ? i : null;
  }
  function pointerEnd(e:PointerEvent,cancel=false){
    if(!pointer || pointer.id!==e.pointerId)return;
    const from=pointer.from,to=over,revision=pointer.revision;
    pointer=null;dragging=null;over=null;
    if(!cancel && pointerMoved && to!==null && revision===draft.revision)move(from,to);
  }
  async function machiningOrder(markFirst:boolean){
    if(blocked || layerBusy)return;
    const revision=draft.revision;
    const ordered=[...draft.layers].sort((a,b)=>Number((a.mode==='mark')!==markFirst)-Number((b.mode==='mark')!==markFirst)).map(l=>l.name);
    layerBusy=true;
    try{await api.changeLayers({kind:'order',layers:ordered},revision);}catch(e){ui.say(explain(e),true);}finally{layerBusy=false;}
  }
  let dragRevision=0;
  let dragging=$state<number|null>(null), over=$state<number|null>(null), chosen=$state<number|null>(null), query=$state('');
  const matches=(ids:number[])=>!query.trim() || ids.some(s=>String(s+1).includes(query.trim()));
  function move(from:number,to:number) {
    const manual=moveOrderRow(rows,from,to); if(blocked || !manual || from===to) return;
    set(f=>{ f.order.strategy={manual}; f.order.inner_first=false; f.order.circles_first=false; f.order.spread_heat=false; });
  }
  function drop(event:DragEvent,to:number) { event.preventDefault(); if(dragging!==null && dragRevision===draft.revision) move(dragging,to); dragging=null; over=null; }
  function key(event:KeyboardEvent,i:number) {
    if(!event.altKey || !['ArrowUp','ArrowDown'].includes(event.key)) return;
    event.preventDefault(); move(i,i+(event.key==='ArrowUp'?-1:1));
  }
  function name(source:number) {
    const placed=draft.placed[source]; const part=draft.parts.find(p=>placed && placed.source>=p.first && placed.source<p.first+p.contours);
    return server.doc?.library.parts.find(p=>p.id===part?.id)?.name ?? draft.name;
  }
</script>
<svelte:window onpointermove={pointerMove} onpointerup={e=>pointerEnd(e)} onpointercancel={e=>pointerEnd(e,true)} />
<section class="order-tree">
  <h3>{t('Contour tree')}</h3>
  <div class="machining-order"><button class="btn btn-soft" disabled={blocked || layerBusy} onclick={()=>machiningOrder(true)}>{t('Engrave before cutting')}</button><button class="btn btn-ghost" disabled={blocked || layerBusy} onclick={()=>machiningOrder(false)}>{t('Cut before engraving')}</button></div>
  <p class="muted">{t('Layer order applies to simulation and machine output. Drag the handle to move a contour within its layer.')}</p>
  <p class="muted">{t('Drag contours within their layer to change the real machining order. Click a contour to show it. Alt + arrow keys also move it.')}</p>
  <p class="muted">{t('A manual move disables automatic priorities. Inside to outside can be switched on again below. Layer order stays unchanged.')}</p>
  <input type="search" placeholder={t('Find contour number')} aria-label={t('Find contour number')} bind:value={query} />
  {#if draft.error}<p role="alert" class="muted">{t('Fix geometry errors before changing the machining order.')}</p>{/if}
  {#each layers as layer}
    <details open><summary>{layer} · {rows.filter(r=>r.layer===layer).length}</summary>
      <ol>
      {#each rows as row,i (row.sources.join(','))}
        {#if row.layer===layer && matches(row.sources)}
          <li data-order-row={i} class:target={over===i} class:selected={chosen===row.sources[0]}>
            <button class="drag-handle" aria-label={t('Move contour {0}',row.sources.map(s=>s+1).join(', '))} disabled={blocked || !!query.trim()} onpointerdown={e=>startPointer(e,i)} onkeydown={e=>key(e,i)}>⠿</button>
            <button class="entry" draggable={!blocked && !query.trim()} ondragstart={e=>{dragging=i;dragRevision=draft.revision; if(e.dataTransfer){e.dataTransfer.effectAllowed='move';e.dataTransfer.setData('text/plain',row.sources.join(','));}}} ondragend={()=>{dragging=null;over=null;}} ondragover={e=>{if(dragging!==null && rows[dragging]?.layer===row.layer){e.preventDefault();over=i;}}} ondrop={e=>drop(e,i)} onkeydown={e=>key(e,i)} onclick={()=>{chosen=row.sources[0]!;onfocus(row.sources[0]!);}}>
              <strong>{i+1}.</strong><span>{t('Contour {0}',row.sources.map(s=>s+1).join(', '))}<small>{name(row.sources[0]!)} · {t('Copy {0}',(draft.placed[row.sources[0]!]?.copy ?? 0)+1)} · {row.depth ? t('Inner contour') : t('Outer contour')}</small></span>
            </button>
            <div class="moves"><button class="btn btn-ghost" aria-label={t('Move up')} disabled={blocked || i===0 || rows[i-1]?.layer!==layer} onclick={()=>move(i,i-1)}>↑</button><button class="btn btn-ghost" aria-label={t('Move down')} disabled={blocked || rows[i+1]?.layer!==layer} onclick={()=>move(i,i+1)}>↓</button></div>
          </li>
        {/if}
      {/each}
      </ol>
    </details>
  {/each}
  {#if inactive.length}<details><summary>{t('Not in machining order')} · {inactive.length}</summary><ol>{#each inactive.filter(s=>matches([s])) as source}<li><button class="entry" onclick={()=>onfocus(source)}>{t('Contour {0}',source+1)} · {t('Hidden, excluded or invalid')}</button></li>{/each}</ol></details>{/if}
  {#if !rows.length}<p class="muted">{t('No active contours.')}</p>{/if}
</section>
<style>
.drag-handle{touch-action:none;min-width:44px;min-height:48px;background:transparent;color:var(--ink);border:0;cursor:grab}.machining-order{display:flex;flex-wrap:wrap;gap:8px}.order-tree{display:grid;gap:10px}.order-tree h3{margin:0}.order-tree input{width:100%;min-height:44px;padding:8px;background:var(--panel-2);color:var(--ink);border:1px solid var(--line);border-radius:8px}summary{cursor:pointer;padding:12px 0;font-weight:600}ol{list-style:none;padding:0;margin:0;display:grid;gap:6px;max-height:440px;overflow:auto}li{display:flex;border:1px solid var(--line);border-radius:8px;background:var(--panel-2)}li.target{border-top:3px solid var(--accent)}li.selected{border-color:var(--accent)}.entry{display:flex;gap:8px;align-items:center;text-align:left;flex:1;min-width:0;min-height:48px;padding:8px;color:var(--ink);background:transparent;border:0;cursor:pointer}.entry[draggable=true]{cursor:grab}.entry small{display:block;color:var(--ink-3);overflow-wrap:anywhere}.moves{display:flex;align-items:center}.moves button{padding:8px;min-width:36px}
</style>
