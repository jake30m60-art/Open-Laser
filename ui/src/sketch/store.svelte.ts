// The open sketch, its undo history and the tool in hand. The sketch is
// kept in this browser between visits; sending it to Parts makes a library
// part the server keeps.
import { clone, emptySketch, isSketch, remove, type Id, type Sketch } from './model';
import { createTool, type Tool, type ToolId } from './tools';

const KEY = 'ol-sketch';
const LIMIT = 200;

function restored(): { name: string; sketch: Sketch } {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? 'null') as { name?: unknown; sketch?: unknown } | null;
    if (raw && isSketch(raw.sketch)) return { name: typeof raw.name === 'string' ? raw.name : 'Sketch', sketch: raw.sketch };
  } catch { /* A fresh sketch needs no storage. */ }
  return { name: 'Sketch', sketch: emptySketch() };
}

class SketchStore {
  /** The sketch as shown; replaced, never mutated in place, so views follow it. */
  sketch = $state.raw<Sketch>(emptySketch());
  name = $state('Sketch');
  selection = $state.raw<ReadonlySet<Id>>(new Set());
  /** The drawing tool in hand, or null for selecting. */
  tool = $state.raw<Tool | null>(null);
  /** Bumped when the tool's own progress changes, so its ghost and hint redraw. */
  toolRevision = $state(0);
  gridSnap = $state(true);
  /** The canvas view, kept while other pages are open. */
  view: { cx: number; cy: number; s: number } | null = null;
  private past: Sketch[] = [];
  private future: Sketch[] = [];
  canUndo = $state(false);
  canRedo = $state(false);

  constructor() {
    const { name, sketch } = restored();
    this.name = name;
    this.sketch = sketch;
  }

  private save(): void {
    try { localStorage.setItem(KEY, JSON.stringify({ name: this.name, sketch: this.sketch })); } catch { /* storage full or private */ }
  }

  private history(): void {
    this.canUndo = this.past.length > 0;
    this.canRedo = this.future.length > 0;
  }

  /** Applies a change to a copy of the sketch as one undo step. */
  change(edit: (draft: Sketch) => void): void {
    const draft = clone(this.sketch);
    edit(draft);
    this.commit(draft, this.sketch);
  }

  /** Keeps `next` as one undo step that `previous` returns to. */
  commit(next: Sketch, previous: Sketch = this.sketch): void {
    this.past.push(previous);
    if (this.past.length > LIMIT) this.past.shift();
    this.future = [];
    this.sketch = next;
    this.prune();
    this.history();
    this.save();
  }

  /** Shows a sketch during a drag without making an undo step. */
  preview(next: Sketch): void { this.sketch = next; }

  rename(name: string): void { this.name = name.trim() || 'Sketch'; this.save(); }

  undo(): void {
    const previous = this.past.pop();
    if (!previous) return;
    this.future.push(this.sketch);
    this.sketch = previous;
    this.prune();
    this.history();
    this.save();
  }

  redo(): void {
    const next = this.future.pop();
    if (!next) return;
    this.past.push(this.sketch);
    this.sketch = next;
    this.prune();
    this.history();
    this.save();
  }

  /** Drops selected ids that no longer exist. */
  private prune(): void {
    const ids = new Set(this.sketch.entities.map((e) => e.id));
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
    this.change((s) => remove(s, ids));
    this.selection = new Set();
  }

  toggleConstruction(): void {
    if (!this.selection.size) return;
    const ids = this.selection;
    const all = this.sketch.entities.filter((e) => ids.has(e.id)).every((e) => e.construction);
    this.change((s) => {
      for (const e of s.entities) {
        if (!ids.has(e.id)) continue;
        if (all) delete e.construction;
        else e.construction = true;
      }
    });
  }

  use(id: ToolId | null): void {
    this.tool = id ? createTool(id) : null;
    this.toolRevision++;
    if (id) this.selection = new Set();
  }

  clear(): void {
    if (!this.sketch.entities.length) return;
    this.commit(emptySketch());
    this.selection = new Set();
  }
}

export const sketchStore = new SketchStore();
