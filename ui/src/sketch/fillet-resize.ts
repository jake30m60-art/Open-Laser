// Seed a changed sketch fillet from the unchanged intersection of its supporting
// lines. This preserves the same tangent branch and rejects radii beyond either side.
import { add, dist, dot, len, scale, sub } from './geometry';
import { clone, point, type Id, type Sketch } from './model';
export function resizedFillets(previous: Sketch, next: Sketch): { sketch: Sketch; moving: Set<Id>; curves: Set<Id>; error?: string } {
  const sketch=clone(next), moving=new Set<Id>(), curves=new Set<Id>();
  const result={sketch,moving,curves};
  for(const dimension of next.constraints) {
    if(dimension.kind!=='radius' || dimension.driven) continue;
    const old=previous.constraints.find(c=>c.id===dimension.id);
    if(old?.kind!=='radius' || old.value===dimension.value) continue;
    const arc=previous.entities.find(e=>e.id===dimension.curve);
    if(arc?.kind!=='arc') continue;
    const tangents=previous.constraints.filter(c=>c.kind==='tangent' && (c.a===arc.id || c.b===arc.id));
    const sides=tangents.flatMap(c=>{
      if(c.kind!=='tangent') return [];
      const line=previous.entities.find(e=>e.id===(c.a===arc.id?c.b:c.a));
      if(line?.kind!=='line') return [];
      const end=[line.a,line.b].find(id=>id===arc.a || id===arc.b);
      return end===undefined?[]:[{end,far:line.a===end?line.b:line.a}];
    });
    if(sides.length!==2 || sides[0]!.end===sides[1]!.end) continue;
    const [first,second]=sides;
    const p=point(previous,first!.end),q=point(previous,second!.end);
    const d1=sub(point(previous,first!.far),p),d2=sub(point(previous,second!.far),q);
    const cross=(a:{x:number;y:number},b:{x:number;y:number})=>a.x*b.y-a.y*b.x;
    const den=cross(d1,d2);
    if(Math.abs(den)<1e-10) return {...result,error:'Die Verrundung hat keine gültige Ecke mehr.'};
    const vertex=add(p,scale(d1,cross(sub(q,p),d2)/den));
    const f1=point(previous,first!.far),f2=point(previous,second!.far);
    const u1=scale(sub(f1,vertex),1/dist(f1,vertex)),u2=scale(sub(f2,vertex),1/dist(f2,vertex));
    const half=Math.acos(Math.max(-1,Math.min(1,dot(u1,u2))))/2;
    const back=dimension.value/Math.tan(half);
    if(!Number.isFinite(back) || back<=0 || back>=dist(f1,vertex)-1e-6 || back>=dist(f2,vertex)-1e-6)
      return {...result,error:'Der Radius ist zu groß für die angrenzenden Linien. Die Kontur bleibt unverändert.'};
    const bis=scale(add(u1,u2),1/len(add(u1,u2)));
    const positions=[[first!.end,add(vertex,scale(u1,back))],[second!.end,add(vertex,scale(u2,back))],[arc.c,add(vertex,scale(bis,dimension.value/Math.sin(half)))]] as const;
    for(const [id,at] of positions){Object.assign(point(sketch,id),at);moving.add(id);}
    curves.add(arc.id);
  }
  return result;
}
