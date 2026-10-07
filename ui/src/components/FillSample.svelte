<script lang="ts">
  // A few millimetres of a scan fill, drawn at scale, so the line spacing,
  // the angle and each pass show: what the fill numbers mean on the work.
  import { t } from '../i18n/index';
  import type { Fill } from '../api';

  let { fill, size = 4 }: { fill: Fill; size?: number } = $props();
  const lines = $derived.by(() => {
    const out: Array<{ d: string; pass: number }> = [];
    const step = Math.max(fill.spacing, size / 120);
    for (let pass = 0; pass < Math.min(fill.passes, 4); pass++) {
      const angle = ((fill.angle + pass * fill.pass_angle) * Math.PI) / 180;
      const shift = (pass * fill.pass_shift) % 1;
      const ux = Math.cos(angle), uy = Math.sin(angle), nx = -uy, ny = ux;
      const reach = size * 1.5;
      for (let k = -Math.ceil(reach / step); k <= Math.ceil(reach / step); k++) {
        const o = (k + shift) * step;
        const cx = size / 2 + nx * o, cy = size / 2 + ny * o;
        out.push({ d: `M${cx - ux * reach} ${size - (cy - uy * reach)}L${cx + ux * reach} ${size - (cy + uy * reach)}`, pass });
      }
    }
    return out;
  });
  const id = `fill-sample-${Math.random().toString(36).slice(2)}`;
</script>

<figure class="sample">
  <svg viewBox="0 0 {size} {size}" role="img" aria-label="A {size} mm square of the fill">
    <defs><clipPath {id}><rect width={size} height={size} /></clipPath></defs>
    <rect class="sheet" width={size} height={size} />
    <g clip-path="url(#{id})">
      {#each lines as line, i (i)}<path class="line pass{line.pass}" d={line.d} />{/each}
    </g>
  </svg>
  <figcaption>{t("{0} × {0} mm at scale", size)} · {fill.passes > 1 ? t("{0} passes in colours", fill.passes) : t("one pass")}{fill.spacing < size / 120 ? ` · ${t("lines closer than shown")}` : ''}</figcaption>
</figure>

<style>
  .sample { margin:0; display:grid; gap:6px; justify-items:start; }
  svg { width:160px; height:160px; border-radius:10px; border:1px solid var(--line); background:var(--panel-2); }
  .sheet { fill:var(--panel-2); }
  .line { stroke-width:0.012; fill:none; }
  .pass0 { stroke:var(--accent); }
  .pass1 { stroke:#2f8fd8; }
  .pass2 { stroke:#2fa86b; }
  .pass3 { stroke:#c05ad6; }
  figcaption { font-size:var(--t-sm); color:var(--ink-3); }
</style>
