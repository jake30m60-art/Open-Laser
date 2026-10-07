<script lang="ts">
 import { api } from '../../api/client';
 import { explain } from '../../lib/format';
 import { t } from '../../i18n/index';
 import { server } from '../../stores/server.svelte';
 import { ui } from '../../stores/ui.svelte';
 import { orderRows } from '../../lib/order-tree';
 let { onselectpart }: { onselectpart:(first:number,count:number)=>void }=$props();
 const doc=$derived(server.doc!); const draft=$derived(doc.draft!);
 const rows=$derived(orderRows(draft.preview));
 const cutting=$derived(new Set(rows.filter(r=>draft.layers.find(l=>l.name===r.layer)?.mode!=='mark').flatMap(r=>r.sources)).size);
 const engraving=$derived(new Set(rows.filter(r=>draft.layers.find(l=>l.name===r.layer)?.mode==='mark').flatMap(r=>r.sources)).size);
 let busy=$state(false);
 async function moveLayer(index:number,delta:number){ if(busy)return; const layers=draft.layers.map(l=>l.name); const target=index+delta; if(target<0||target>=layers.length)return; [layers[index],layers[target]]=[layers[target]!,layers[index]!]; busy=true; try{await api.changeLayers({kind:'order',layers});}catch(e){ui.say(explain(e),true);}finally{busy=false;} }
 const excluded=$derived(Math.max(0,draft.placed.length-new Set(rows.flatMap(r=>r.sources)).size));
</script>
<details class="job-overview" open>
 <summary>{t('Job overview')} · {draft.name}</summary>
 <div class="counts"><span>{t('{0} cut contours',cutting)}</span><span>{t('{0} engraving contours',engraving)}</span><span>{t('{0} excluded contours',excluded)}</span></div>
 {#if draft.error}<p class="warning" role="status">{t('Output blocked')}: {draft.error}</p>{:else if draft.layers.some(l=>l.output&&!l.chosen)}<p class="warning">{t('Recipe missing on an active layer')}</p>{:else}<p class="muted">{t('Geometry prepared')}</p>{/if}
 {#if draft.features.only_contours}<p class="warning">{t('Only selected contours are processed')}</p>{/if}
 <details><summary>{t('Parts in this job')} · {draft.parts.length}</summary><ul>{#each draft.parts as part}<li><button class="btn btn-ghost" onclick={()=>onselectpart(part.first,part.contours)}>{doc.library.parts.find(p=>p.id===part.id)?.name ?? draft.name} · {part.contours}</button></li>{/each}</ul></details>
 <ol>{#each draft.layers as layer,i}<li><button class="operation" onclick={()=>{ui.machiningLayer=layer.name; ui.setupPanel=layer.mode==='mark'?'engrave':'order';}}><strong>{i+1}. {layer.mode==='mark'?t('Engraving'):t('Cutting')} · {layer.name}</strong><small>{layer.output ? layer.chosen ? t('Active') : t('Recipe missing') : t('Output off')} · {layer.contours} {t('Contours')}</small></button><div class="layer-moves"><button class="btn btn-ghost" aria-label={t('Move up')} disabled={busy||i===0} onclick={()=>moveLayer(i,-1)}>↑</button><button class="btn btn-ghost" aria-label={t('Move down')} disabled={busy||i===draft.layers.length-1} onclick={()=>moveLayer(i,1)}>↓</button></div></li>{/each}</ol>
 <div class="actions"><button class="btn btn-soft" onclick={()=>ui.setupPanel='simulation'}>{t('Simulation')}</button><button class="btn btn-ghost" onclick={()=>ui.tab='parts'}>{t('Parts and saved jobs')}</button></div>
</details>
<style>
.job-overview{border-bottom:1px solid var(--line);padding-bottom:14px;margin-bottom:14px}summary{cursor:pointer;font-weight:600;padding:8px 0;overflow-wrap:anywhere}.counts{display:flex;flex-wrap:wrap;gap:8px;font-size:var(--t-sm);padding:8px 0}.counts span{background:var(--panel-2);border-radius:6px;padding:4px 6px}.warning{color:var(--warn);font-size:var(--t-sm);overflow-wrap:anywhere}ol,ul{list-style:none;padding:0;margin:8px 0;display:grid;gap:6px}.operation{display:block;text-align:left;width:100%;padding:8px;border:1px solid var(--line);border-radius:8px;background:var(--panel-2);color:var(--ink);cursor:pointer}.operation small{display:block;color:var(--ink-3);padding-top:4px}.layer-moves{display:flex;gap:4px}.actions{display:flex;flex-wrap:wrap;gap:6px}
</style>
