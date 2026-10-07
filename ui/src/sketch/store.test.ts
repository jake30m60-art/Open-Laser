import {it,expect,vi} from 'vitest';import {SketchStore} from './store.svelte';import {emptySketch,addPoint} from './model';
vi.mock('./solver',()=>({solverReady:()=>false,freedom:vi.fn(),solveEdit:vi.fn(),solve:vi.fn()}));
it('keeps named sketches, source bindings and constraints across switches and reloads',()=>{vi.useFakeTimers();const data=new Map<string,string>();vi.stubGlobal('localStorage',{getItem:(key:string)=>data.get(key)??null,setItem:(key:string,value:string)=>data.set(key,value)});const a=emptySketch();const p=addPoint(a,3,4);a.constraints.push({id:a.next++,kind:'fix',point:p});const store=new SketchStore();store.begin(a,'A','replace','part','job');const id=store.activeDocument;store.begin(emptySketch(),'B','new');store.openDocument(id);expect(store.sketch.constraints).toEqual(a.constraints);expect(store.sourcePart).toBe('part');expect(store.canUndo).toBe(false);const loaded=new SketchStore();expect(loaded.name).toBe('A');expect(loaded.target).toBe('replace');expect(loaded.jobKey).toBe('job');expect(loaded.documents.some(d=>d.name==='B')).toBe(true);vi.useRealTimers();vi.unstubAllGlobals();});

it('removes only the selected saved sketch and restores it after reload',()=>{
 vi.useFakeTimers();const data=new Map<string,string>();vi.stubGlobal('localStorage',{getItem:(k:string)=>data.get(k)??null,setItem:(k:string,v:string)=>data.set(k,v)});
 const store=new SketchStore();store.begin(emptySketch(),'A','replace','part','job');const id=store.activeDocument;store.begin(emptySketch(),'B','new');store.openDocument(id);store.removeDocument();
 expect(store.documents.some(d=>d.id===id)).toBe(false);expect(store.documents.some(d=>d.name==='B')).toBe(true);
 const reopened=new SketchStore();expect(reopened.documents.some(d=>d.id===id)).toBe(false);reopened.restoreDocument();
 expect(reopened.activeDocument).toBe(id);expect(reopened.name).toBe('A');expect(reopened.sourcePart).toBe('part');expect(reopened.jobKey).toBe('job');
 vi.useRealTimers();vi.unstubAllGlobals();
});
