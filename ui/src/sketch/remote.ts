// Sketches kept on the server, so every screen sees the same ones. The
// browser keeps a copy to open at once; the server's is the one that lasts.
import { isSketch, placedText, point, type Sketch } from './model';
import { sampled } from './curves';
import type { SketchDocument, SketchTarget } from './documents';

/** A sketch in the server's list, without its editing model. */
export interface RemoteSummary {
  id: string;
  name: string;
  target: string;
  source_part?: string;
  outline: [number, number][][];
  updated: number;
}

/** Only a served page talks to the server; tests and files do not. */
const online = (): boolean => typeof fetch === 'function' && typeof location !== 'undefined' && /^https?:$/.test(location.protocol);

async function call<T>(method: string, path: string, body?: unknown): Promise<T | null> {
  if (!online()) return null;
  try {
    const r = await fetch(path, { method, headers: body === undefined ? {} : { 'content-type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
    if (!r.ok) return null;
    return (await r.json()) as T;
  } catch { return null; }
}

const targetOf = (t: string): SketchTarget => (t === 'add' || t === 'replace' ? t : 'new');

/** The server's sketches, latest first; null when the server cannot be reached. */
export async function listRemote(): Promise<RemoteSummary[] | null> {
  return (await call<{ sketches: RemoteSummary[] }>('GET', '/api/sketches'))?.sketches ?? null;
}

/** The sketches removed on the server, latest first. */
export async function removedRemote(): Promise<RemoteSummary[] | null> {
  return (await call<{ sketches: RemoteSummary[] }>('GET', '/api/sketches/removed'))?.sketches ?? null;
}

type Saved = { id: string; name: string; sketch: unknown; target: string; source_part?: string; job_key?: string; updated: number };
const fromSaved = (d: Saved): SketchDocument | null => isSketch(d.sketch)
  ? { id: d.id, name: d.name, sketch: d.sketch, target: targetOf(d.target), sourcePart: d.source_part, jobKey: d.job_key, updated: d.updated }
  : null;

/** One sketch from the server. */
export async function getRemote(id: string): Promise<SketchDocument | null> {
  const reply = await call<{ sketch: Saved }>('GET', `/api/sketches/${encodeURIComponent(id)}`);
  return reply ? fromSaved(reply.sketch) : null;
}

/** Keeps a sketch on the server; the time it was saved, or null. */
export async function saveRemote(doc: SketchDocument): Promise<number | null> {
  const reply = await call<{ updated: number }>('POST', `/api/sketches/${encodeURIComponent(doc.id)}`, {
    id: doc.id, name: doc.name, sketch: doc.sketch, target: doc.target, source_part: doc.sourcePart, job_key: doc.jobKey, outline: outline(doc.sketch), updated: 0,
  });
  return reply?.updated ?? null;
}

export const removeRemote = (id: string) => call('DELETE', `/api/sketches/${encodeURIComponent(id)}`);
export async function restoreRemote(id: string): Promise<SketchDocument | null> {
  const reply = await call<{ sketch: Saved }>('POST', `/api/sketches/${encodeURIComponent(id)}/restore`, {});
  return reply ? fromSaved(reply.sketch) : null;
}

/** The sketch's drawn curves as polylines, for a preview. */
export function outline(sketch: Sketch): [number, number][][] {
  const out: [number, number][][] = [];
  const at = (id: number): [number, number] => { const p = point(sketch, id); return [p.x, p.y]; };
  for (const e of sketch.entities) {
    if (e.construction) continue;
    if (e.kind === 'line') out.push([at(e.a), at(e.b)]);
    else if (e.kind === 'ellipse' || e.kind === 'ellipse-arc' || e.kind === 'spline') out.push(sampled(sketch, e).map((p) => [p.x, p.y] as [number, number]));
    else {
      const c = point(sketch, e.c);
      const r = e.kind === 'circle' ? e.r : Math.hypot(point(sketch, e.a).x - c.x, point(sketch, e.a).y - c.y);
      let start = 0, sweep = Math.PI * 2;
      if (e.kind === 'arc') {
        const a = point(sketch, e.a), b = point(sketch, e.b);
        start = Math.atan2(a.y - c.y, a.x - c.x);
        sweep = Math.atan2(b.y - c.y, b.x - c.x) - start;
        if (sweep <= 0) sweep += Math.PI * 2;
      }
      const n = Math.max(8, Math.ceil(sweep / (Math.PI / 24)));
      out.push(Array.from({ length: n + 1 }, (_, i) => [c.x + r * Math.cos(start + (sweep * i) / n), c.y + r * Math.sin(start + (sweep * i) / n)] as [number, number]));
    }
  }
  for (const text of sketch.texts ?? []) out.push(...placedText(text));
  return out;
}
