// The open sketch, its undo history and the tool in hand. The sketch is
// kept in this browser between visits; sending it to Parts makes a library
// part the server keeps.
import { clone, emptySketch, isDimension, isSketch, remove, type Id, type Sketch } from './model';
import { createTool, type Tool, type ToolId } from './tools';
import { createAction, type Action, type ActionId } from './actions';
import { freedom, solveEdit, solve, solverReady } from './solver';

import { primaryRelations } from './primary-relations';
import { readDocuments, type SketchDocument, type SketchTarget } from './documents';
import { getRemote, listRemote, removeRemote, restoreRemote, saveRemote, type RemoteSummary } from './remote';
const ARCHIVE='ol-sketch-documents';
const REMOVED='ol-sketch-removed-documents';
const KEY = 'ol-sketch';
const LIMIT = 200;

function restored(): { name: string; sketch: Sketch; target: 'new' | 'add' } {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? 'null') as { name?: unknown; sketch?: unknown; target?: unknown } | null;
    if (raw && isSketch(raw.sketch)) return { name: typeof raw.name === 'string' ? raw.name : 'Sketch', sketch: raw.sketch, target: raw.target === 'new' ? 'new' : 'add' };
  } catch { /* A fresh sketch needs no storage. */ }
  return { name: 'Sketch', sketch: emptySketch(), target: 'add' };
}

export class SketchStore {
  /** The sketch as shown; replaced, never mutated in place, so views follow it. */
  sketch = $state.raw<Sketch>(emptySketch());
  name = $state('Sketch');
  target = $state<SketchTarget>('add');
  documents = $state.raw<SketchDocument[]>([]);
  activeDocument = $state('');
  sourcePart = $state<string | undefined>();
  jobKey = $state<string | undefined>();
  selection = $state.raw<ReadonlySet<Id>>(new Set());
  /** The drawing tool in hand, or null for selecting. */
  tool = $state.raw<Tool | null>(null);
  /** A modify, constraint or dimension tool in hand, instead of a drawing tool. */
  action = $state.raw<Action | null>(null);
  /** Bumped when the tool's own progress changes, so its ghost and hint redraw. */
  toolRevision = $state(0);
  gridSnap = $state(true);
  /** Grid spacing in millimetres, or null to follow the zoom. */
  gridStep = $state<number | null>(null);
  showConstraints = $state(true);
  showDimensions = $state(true);
  /** What can still move, for colouring defined geometry; null while unknown. */
  free = $state.raw<{ points: Set<Id>; circles: Set<Id> } | null>(null);
  /** Bumped when the solver becomes ready. */
  solverRevision = $state(0);
  /** Existing constraint IDs involved in the last rejected edit. */
  conflicts = $state<Set<Id>>(new Set());
  /** The canvas view, kept while other pages are open. */
  view: { cx: number; cy: number; s: number } | null = null;
  private past: Sketch[] = [];
  private future: Sketch[] = [];
  canUndo = $state(false);
  canRedo = $state(false);

  constructor() {
    const { name, sketch, target } = restored();
    this.target = target;
    this.name = name;
    this.sketch = primaryRelations(sketch);
    try {this.documents=readDocuments(localStorage.getItem(ARCHIVE));this.removedDocuments=readDocuments(localStorage.getItem(REMOVED));}catch{}
    const current=this.documents.at(-1);
    this.activeDocument=current?.id ?? 'sketch-'+Date.now().toString(36);
    if(current){this.sketch=primaryRelations(current.sketch);this.name=current.name;this.target=current.target;this.sourcePart=current.sourcePart;this.jobKey=current.jobKey;}
    void this.sync();
  }

  /** Whether the server's list has been read: until then a removal or a
   *  restore only acts on this browser's copy. */
  synced = $state(false);
  private pending: ReturnType<typeof setTimeout> | null = null;

  /** Takes in the server's sketches. Sketches only this browser has go up
   *  once; ones another screen saved later replace this browser's copy,
   *  and the open one is reloaded when it has no unsaved steps here. */
  async sync(): Promise<void> {
    const remote = await listRemote();
    if (!remote) return;
    const byId = new Map(remote.map((r) => [r.id, r]));
    const local = new Map(this.documents.map((d) => [d.id, d]));
    for (const d of this.documents) if (!byId.has(d.id) && !d.remote) void saveRemote(d);
    const merged: SketchDocument[] = this.documents.map((d) => {
      const r = byId.get(d.id);
      return r && r.updated > (d.updated ?? 0) ? { ...summaryDocument(r), sketch: d.sketch } : d;
    });
    for (const r of [...remote].reverse()) if (!local.has(r.id)) merged.unshift(summaryDocument(r));
    this.documents = merged;
    this.synced = true;
    const active = merged.find((d) => d.id === this.activeDocument);
    if (active?.remote && !this.past.length) await this.load(active.id);
  }

  /** Fetches a sketch only the server has in full. */
  private async load(id: string): Promise<SketchDocument | null> {
    const full = await getRemote(id);
    if (!full) return null;
    this.documents = this.documents.map((d) => (d.id === id ? full : d));
    if (id === this.activeDocument && !this.past.length) {
      this.name = full.name; this.target = full.target; this.sourcePart = full.sourcePart; this.jobKey = full.jobKey;
      this.sketch = primaryRelations(full.sketch); this.refreshFreedom();
    }
    return full;
  }

  private save(): void {
    const document:SketchDocument={id:this.activeDocument,name:this.name,sketch:clone(this.sketch),target:this.target,sourcePart:this.sourcePart,jobKey:this.jobKey,updated:Math.floor(Date.now()/1000)};
    this.documents=[...this.documents.filter(d=>d.id!==this.activeDocument),document];
    try { localStorage.setItem(ARCHIVE,JSON.stringify(this.documents.filter(d=>!d.remote))); localStorage.setItem(KEY, JSON.stringify({ name: this.name, sketch: this.sketch, target: this.target })); } catch { /* storage full or private */ }
    // A burst of edits goes to the server once, shortly after the last.
    if (this.pending) clearTimeout(this.pending);
    this.pending = setTimeout(() => { this.pending = null; void saveRemote(document); }, 600);
  }

  private history(): void {
    this.canUndo = this.past.length > 0;
    this.canRedo = this.future.length > 0;
  }

  /** Applies a change to a copy of the sketch as one undo step; the reason
   *  when the constraints cannot all hold. */
  change(edit: (draft: Sketch) => void): string | null {
    const draft = clone(this.sketch);
    edit(draft);
    return this.settle(draft, this.sketch);
  }

  /** Solves `next` and keeps it as one undo step, or says why it cannot hold. */
  settle(next: Sketch, previous: Sketch = this.sketch, strict = false): string | null {
    if (!solverReady()) { this.commit(next, previous); return null; }
    // The rest of the sketch keeps to where it was; should that not solve, solve plainly.
    let solved = solveEdit(previous, next);
    if (!solved.ok && !solved.reason) solved = solve(next);
    if (!solved.ok) {
      this.conflicts = new Set(solved.conflicting);
      return solved.reason ?? (solved.conflicting.length
        ? 'Over-constrained: that conflicts with constraints or dimensions already there.'
        : 'The constraints cannot all hold.');
    }
    if (strict && solved.redundant.length) {
      // Only what this edit says twice counts, not what the sketch already did.
      const before = new Set(solve(previous).redundant);
      if (solved.redundant.some((id) => !before.has(id))) {
        const known = new Set(previous.constraints.map((c) => c.id));
        const added = next.constraints.filter((c) => !known.has(c.id));
        if (!added.length || !added.every(isDimension)) {
          return 'Over-constrained: that is already fixed by the constraints and dimensions there.';
        }
        // As Fusion offers: keep the dimension as a driven one, which only shows.
        const driven = clone(next);
        for (const c of driven.constraints) if (!known.has(c.id) && isDimension(c)) c.driven = true;
        const kept = solve(driven);
        if (!kept.ok) return 'The constraints cannot all hold.';
        this.conflicts = new Set();
        this.commit(kept.sketch, previous);
        return 'That is already fixed by other constraints, so it was added as a driven dimension (in brackets).';
      }
    }
    this.conflicts = new Set();
    this.commit(solved.sketch, previous);
    return null;
  }

  private freeTimer: ReturnType<typeof setTimeout> | null = null;

  /** Works out, a moment after the sketch settles, what can still move. */
  refreshFreedom(): void {
    if (this.freeTimer) clearTimeout(this.freeTimer);
    this.freeTimer = setTimeout(() => {
      this.freeTimer = null;
      this.free = solverReady() ? freedom(this.sketch) : null;
    }, 60);
  }

  /** Keeps `next` as one undo step that `previous` returns to. */
  commit(next: Sketch, previous: Sketch = this.sketch): void {
    this.conflicts = new Set();
    this.past.push(previous);
    if (this.past.length > LIMIT) this.past.shift();
    this.future = [];
    this.sketch = next; this.conflicts = new Set();
    this.prune();
    this.history();
    this.save();
    this.refreshFreedom();
  }

  /** Shows a sketch during a drag without making an undo step. */
  preview(next: Sketch): void { this.sketch = next; this.conflicts = new Set(); }

  rename(name: string): void { this.name = name.trim() || 'Sketch'; this.save(); }

  undo(): void {
    const previous = this.past.pop();
    if (!previous) return;
    this.future.push(this.sketch);
    this.sketch = previous; this.conflicts = new Set();
    this.prune();
    this.history();
    this.save();
    this.refreshFreedom();
  }

  redo(): void {
    const next = this.future.pop();
    if (!next) return;
    this.past.push(this.sketch);
    this.sketch = next; this.conflicts = new Set();
    this.prune();
    this.history();
    this.save();
    this.refreshFreedom();
  }

  /** Drops selected ids that no longer exist. */
  private prune(): void {
    const ids = new Set([...this.sketch.entities.map((e) => e.id), ...this.sketch.constraints.map((c) => c.id), ...(this.sketch.texts ?? []).map((t) => t.id)]);
    if ([...this.selection].some((id) => !ids.has(id))) this.selection = new Set([...this.selection].filter((id) => ids.has(id)));
  }

  select(ids: Iterable<Id>, mode: 'replace' | 'add' | 'toggle' = 'replace'): void {
    const next = new Set(mode === 'replace' ? [] : this.selection);
    for (const id of ids) {
      if (mode === 'toggle' && next.has(id)) next.delete(id);
      else next.add(id);
    }
    this.selection = next;
  }

  deleteSelection(): void {
    if (!this.selection.size) return;
    const ids = [...this.selection];
    // Deleting only removes: the rest always still holds.
    const next = clone(this.sketch);
    remove(next, ids);
    this.commit(next);
    this.selection = new Set();
  }

  toggleConstruction(): string | null {
    if (!this.selection.size) return null;
    const ids = this.selection;
    const all = this.sketch.entities.filter((e) => ids.has(e.id)).every((e) => e.construction);
    return this.change((s) => {
      for (const e of s.entities) {
        if (!ids.has(e.id)) continue;
        if (all) delete e.construction;
        else e.construction = true;
      }
    });
  }

  automaticConstraints = $state(false);
  use(id: ToolId | null): void {
    this.tool = id ? createTool(id, this.automaticConstraints) : null;
    this.action = null;
    this.toolRevision++;
    if (id) this.selection = new Set();
  }

  /** Takes up a modify, constraint or dimension tool; the selection stays
   *  for the tools that work on it. */
  act(id: ActionId | null): void {
    this.tool = null;
    this.action = id ? createAction(id) : null;
    this.toolRevision++;
  }

  bindPart(id:string): void {this.sourcePart=id;this.save();}

  removedDocuments = $state<SketchDocument[]>([]);
  removeDocument(): void {
    this.save();
    const removed = this.documents.find(d=>d.id===this.activeDocument);
    if (!removed) return;
    this.removedDocuments = [...this.removedDocuments, structuredClone($state.snapshot(removed))];
    this.documents = this.documents.filter(d=>d.id!==removed.id);
    if (this.pending) { clearTimeout(this.pending); this.pending = null; }
    void saveRemote(removed).then(() => removeRemote(removed.id));
    const next = this.documents.at(-1);
    this.activeDocument = next?.id ?? 'sketch-'+Date.now().toString(36)+'-'+Math.random().toString(36).slice(2,8);
    this.name=next?.name ?? 'Neue Skizze'; this.target=next?.target ?? 'new'; this.sourcePart=next?.sourcePart; this.jobKey=next?.jobKey;
    this.sketch=next ? clone(next.sketch) : emptySketch();
    this.use(null);this.past=[];this.future=[];this.history();this.selection=new Set();this.view=null;
    try { localStorage.setItem(REMOVED,JSON.stringify(this.removedDocuments)); } catch {}
    this.save();this.refreshFreedom();
  }
  restoreDocument(): void {
    const removed=this.removedDocuments.at(-1);if(!removed)return;
    this.removedDocuments=this.removedDocuments.slice(0,-1);
    this.documents=[...this.documents.filter(d=>d.id!==removed.id),removed];
    void restoreRemote(removed.id);
    this.openDocument(removed.id);
    try { localStorage.setItem(REMOVED,JSON.stringify(this.removedDocuments)); } catch {}
  }
  openDocument(id:string): void {
    const wanted=this.documents.find(d=>d.id===id);
    // Another screen's sketch comes from the server first.
    if(wanted?.remote){ void this.load(id).then(full=>{ if(full)this.openDocument(id); }); return; }
    this.save(); const doc=this.documents.find(d=>d.id===id); if(!doc)return;
    this.activeDocument=doc.id;this.name=doc.name;this.target=doc.target;this.sourcePart=doc.sourcePart;this.jobKey=doc.jobKey;
    this.use(null);this.sketch=primaryRelations(doc.sketch);this.past=[];this.future=[];this.history();this.selection=new Set();this.view=null;this.save();this.refreshFreedom();
  }

  begin(sketch: Sketch, name: string, target: SketchTarget, sourcePart?:string, jobKey?:string): void {
    this.save();
    this.activeDocument='sketch-'+Date.now().toString(36)+'-'+Math.random().toString(36).slice(2,8);
    this.sourcePart=sourcePart; this.jobKey=jobKey;
    this.past=[];this.future=[];this.history();
    this.target = target;
    this.name = name;
    this.use(null);
    this.view = null;
    this.sketch=primaryRelations(sketch);this.save();this.refreshFreedom();
    this.selection = new Set();
  }

  clear(): void {
    if (!this.sketch.entities.length) return;
    this.commit(emptySketch());
    this.selection = new Set();
  }
}

/** A server sketch in the list until it is opened. */
function summaryDocument(r: RemoteSummary): SketchDocument {
  return { id: r.id, name: r.name, sketch: emptySketch(), target: r.target === 'add' || r.target === 'replace' ? r.target : 'new', sourcePart: r.source_part, updated: r.updated, remote: true };
}

export const sketchStore = new SketchStore();
