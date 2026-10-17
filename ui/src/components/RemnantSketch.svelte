<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../i18n/index';
  import Modal from './Modal.svelte';
  import SheetShape from './SheetShape.svelte';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { access } from '../lib/access.svelte';
  import { explain } from '../lib/format';
  import { curvePoints, outerFirst } from '../lib/workpiece-alignment';
  import { sketchStore } from '../sketch/store.svelte';
  import { fromDrawing, type SourceDrawing } from '../sketch/from-drawing';
  import { toDxf } from '../sketch/dxf';
  import type { SheetView, LaserMode } from '../api';
  let { id = null, onclose }: {id?: string | null; onclose:()=>void}=$props();
  let record=$state<SheetView|null>(null),drawing=$state<SourceDrawing|null>(null);
  let name=$state(''),material=$state(''),thickness=$state(1),mode=$state<LaserMode>('fiber');
  let part=$state(''),busy=$state(false),error=$state(''),inspected=$state(false);
  const contours=$derived(drawing?.contours.map(c=>c.curves.flatMap(curvePoints))??[]);
  onMount(()=>{if(id)void (async()=>{try{record=await api.sheet(id!);drawing=outerFirst(await api.remnantMaterial(id!));name=record.name;material=record.material;thickness=record.thickness_mm;mode=record.mode;}catch(e){error=explain(e);}})();});
  async function load(){busy=true;error='';try{drawing=outerFirst(part?(await api.partDrawing(part)).drawing:await api.alignmentSketch(toDxf(sketchStore.sketch)));inspected=false;if(!name)name=part?server.doc!.library.parts.find(p=>p.id===part)?.name??'':sketchStore.name;}catch(e){error=explain(e);}finally{busy=false;}}
  function edit(){if(!drawing)return;sketchStore.begin(fromDrawing($state.snapshot(drawing)),name||t('Remnant'),'new');ui.tab='sketch';ui.say('Edit the remnant sketch, then return to Stock and load the current sketch. No cut is created.');onclose();}
  async function save(){if(!drawing)return;busy=true;error='';try{await api.saveSketchRemnant({id,revision:record?.revision??null,name,material,thickness_mm:thickness,mode,drawing:$state.snapshot(drawing),inspected});ui.say('Remnant geometry saved. Previous measurements are invalid.');onclose();}catch(e){error=explain(e);}finally{busy=false;}}
</script>
<Modal title={id?t('Update remnant sketch'):t('Create remnant from sketch')} wide {onclose}>
  <div class="remnant-form">
    <p>{t('The sketch describes the actual remaining sheet: one outer contour and all known holes. It is nesting stock and a measurement reference, never a cutting contour.')}</p>
    <div class="fields"><label>{t('Reference source')}<select bind:value={part}><option value="">{t('Current sketch')}</option>{#each server.doc!.library.parts as p}<option value={p.id}>{p.name}</option>{/each}</select></label><button class="btn" disabled={busy} onclick={load}>{t('Load sketch geometry')}</button></div>
    {#if drawing}<div class="shape"><SheetShape outline={contours[0]??[]} cutouts={contours.slice(1)} /></div><button class="btn" disabled={busy} onclick={edit}>{t('Edit contour in sketch')}</button>{/if}
    <div class="fields"><label>{t('Name')}<input bind:value={name} maxlength="120" /></label><label>{t('Material')}<input bind:value={material} maxlength="120" /></label><label>{t('Thickness')} (mm)<input type="number" min="0.01" max="200" step="0.1" bind:value={thickness} /></label><label>{t('Laser')}<select bind:value={mode}><option value="fiber">{t('Fiber')}</option><option value="co2">CO₂</option></select></label></div>
    <label class="confirm"><input type="checkbox" bind:checked={inspected} />{t('I checked that this sketch matches the actual remaining sheet, including all holes and removed areas.')}</label>
    {#if id}<p>{t('Updating preserves the previous database revision and invalidates the measurement template. Reload the remnant in any prepared nesting job and nest again.')}</p>{/if}
    {#if error}<p class="error" role="alert">{t(error)}</p>{/if}
    <button class="btn btn-primary" disabled={busy||!access.canControl||!drawing||!inspected||!name.trim()||!material.trim()} onclick={save}>{id?t('Save revised remnant'):t('Save inspected remnant')}</button>
  </div>
</Modal>
<style>.remnant-form{display:grid;gap:14px}.fields{display:flex;gap:12px;flex-wrap:wrap;align-items:end}.fields label{display:grid;gap:5px;min-width:120px;flex:1}.shape{height:280px;border:1px solid var(--line);padding:12px}.shape :global(svg){height:100%;width:100%}.confirm{display:flex;gap:10px;align-items:center}.error{color:var(--warn)}p{line-height:1.5}</style>
