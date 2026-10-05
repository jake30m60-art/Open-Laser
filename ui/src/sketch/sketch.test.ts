import { describe, expect, it } from 'vitest';
import { evaluate } from './expression';
import { angleOf, circleThrough, dist, intersections, midOf, shapeOf, sweep, type Vec } from './geometry';
import { clone, emptySketch, merge, point, remove, type Sketch } from './model';
import { createTool, type Locked } from './tools';
import { snap, gridStep, type Snap } from './snap';
import { repairArcs, translate, chain, originsOf } from './edit';
import { toDxf } from './dxf';

const free = (x: number, y: number): Snap => ({ x, y, kind: 'free' });

/** Clicks a tool through positions, resolving typed values as the canvas does. */
function draw(sketch: Sketch, id: Parameters<typeof createTool>[0], clicks: Array<Vec | [Vec, Locked]>): string[] {
  const tool = createTool(id);
  return clicks.map((c) => {
    const [at, locked] = Array.isArray(c) ? c : [c, {}];
    const s = snap(sketch, at, { reach: 0.01, grid: 0, anchor: tool.anchor() });
    tool.move(s);
    return tool.click(sketch, tool.resolve(s, locked), locked);
  });
}

describe('typed values', () => {
  it('accepts arithmetic and decimal commas', () => {
    expect(evaluate('50/2+3')).toBe(28);
    expect(evaluate('(80-2*4)')).toBe(72);
    expect(evaluate('1,5')).toBe(1.5);
    expect(evaluate('-3*-2')).toBe(6);
  });
  it('refuses anything else', () => {
    expect(evaluate('')).toBeNull();
    expect(evaluate('2+')).toBeNull();
    expect(evaluate('alert(1)')).toBeNull();
    expect(evaluate('1/0')).toBeNull();
  });
});

describe('geometry', () => {
  it('finds the circle through three points', () => {
    const c = circleThrough({ x: 10, y: 0 }, { x: 0, y: 10 }, { x: -10, y: 0 })!;
    expect(c.c.x).toBeCloseTo(0);
    expect(c.c.y).toBeCloseTo(0);
    expect(c.r).toBeCloseTo(10);
    expect(circleThrough({ x: 0, y: 0 }, { x: 1, y: 1 }, { x: 2, y: 2 })).toBeNull();
  });
  it('crosses lines, circles and arcs only where both exist', () => {
    const l = { kind: 'line' as const, a: { x: -20, y: 0 }, b: { x: 20, y: 0 } };
    expect(intersections(l, { kind: 'circle', c: { x: 0, y: 0 }, r: 10 })).toHaveLength(2);
    // The upper half arc touches the x axis only at its ends.
    const upper = { kind: 'arc' as const, c: { x: 0, y: 0 }, r: 10, start: 0, end: Math.PI };
    const vertical = { kind: 'line' as const, a: { x: 5, y: -20 }, b: { x: 5, y: 20 } };
    const hits = intersections(vertical, upper);
    expect(hits).toHaveLength(1);
    expect(hits[0]!.y).toBeGreaterThan(0);
  });
  it('picks grid steps of 1, 2 and 5', () => {
    expect(gridStep(0.1, 10)).toBe(1);
    expect(gridStep(0.15, 10)).toBe(2);
    expect(gridStep(0.4, 10)).toBe(5);
    expect(gridStep(1, 10)).toBe(10);
  });
});

describe('line tool', () => {
  it('chains lines through shared points and closes on the start', () => {
    const s = emptySketch();
    const outcomes = draw(s, 'line', [{ x: 0, y: 0 }, { x: 40, y: 0 }, { x: 40, y: 20 }, { x: 0, y: 20 }, { x: 0, y: 0 }]);
    expect(outcomes).toEqual(['more', 'more', 'more', 'more', 'done']);
    expect(s.entities).toHaveLength(4);
    expect(s.points).toHaveLength(4);
    expect(s.constraints.filter((c) => c.kind === 'horizontal')).toHaveLength(2);
    expect(s.constraints.filter((c) => c.kind === 'vertical')).toHaveLength(2);
  });
  it('places a typed length and angle', () => {
    const s = emptySketch();
    draw(s, 'line', [{ x: 0, y: 0 }, [{ x: 3, y: 7 }, { length: 50, angle: 30 }]]);
    const e = s.entities[0]!;
    if (e.kind !== 'line') throw new Error('line expected');
    const b = point(s, e.b);
    expect(b.x).toBeCloseTo(50 * Math.cos(Math.PI / 6));
    expect(b.y).toBeCloseTo(25);
  });
  it('snaps onto an existing end, sharing its point', () => {
    const s = emptySketch();
    draw(s, 'line', [{ x: 0, y: 0 }, { x: 10, y: 0 }]);
    const tool = createTool('line');
    const start = snap(s, { x: 10.3, y: 0.2 }, { reach: 1, grid: 0 });
    expect(start.kind).toBe('end');
    tool.click(s, start, {});
    tool.click(s, free(10, 10), {});
    expect(s.points).toHaveLength(3);
  });
});

describe('shape tools', () => {
  it('draws a rectangle of typed size from the first corner toward the cursor', () => {
    const s = emptySketch();
    draw(s, 'rect2', [{ x: 10, y: 10 }, [{ x: -5, y: 50 }, { width: 30, height: 20 }]]);
    expect(s.entities).toHaveLength(4);
    const xs = s.points.map((p) => p.x).sort((a, b) => a - b);
    const ys = s.points.map((p) => p.y).sort((a, b) => a - b);
    expect(xs[0]).toBeCloseTo(-20);
    expect(xs[3]).toBeCloseTo(10);
    expect(ys[0]).toBeCloseTo(10);
    expect(ys[3]).toBeCloseTo(30);
  });
  it('keeps a centre rectangle centred with construction diagonals', () => {
    const s = emptySketch();
    draw(s, 'rectc', [{ x: 0, y: 0 }, [{ x: 1, y: 1 }, { width: 80, height: 40 }]]);
    expect(s.entities.filter((e) => !e.construction)).toHaveLength(4);
    expect(s.entities.filter((e) => e.construction)).toHaveLength(2);
    expect(s.constraints.some((c) => c.kind === 'midpoint')).toBe(true);
  });
  it('draws circles by diameter', () => {
    const s = emptySketch();
    draw(s, 'circle', [{ x: 5, y: 5 }, [{ x: 9, y: 5 }, { diameter: 12 }]]);
    const c = s.entities[0]!;
    expect(c.kind === 'circle' && c.r).toBeCloseTo(6);
    draw(s, 'circle2', [{ x: 0, y: 0 }, { x: 0, y: 20 }]);
    const c2 = s.entities[1]!;
    expect(c2.kind === 'circle' && c2.r).toBeCloseTo(10);
  });
  it('bends a 3-point arc through the third point', () => {
    const s = emptySketch();
    draw(s, 'arc3', [{ x: -10, y: 0 }, { x: 10, y: 0 }, { x: 0, y: -10 }]);
    const shape = shapeOf(s, s.entities[0]!);
    const m = midOf(shape)!;
    expect(m.x).toBeCloseTo(0);
    expect(m.y).toBeCloseTo(-10);
  });
  it('makes a 3-point arc of a typed radius on the cursor side', () => {
    const s = emptySketch();
    draw(s, 'arc3', [{ x: 0, y: 0 }, { x: 20, y: 0 }, [{ x: 10, y: 3 }, { radius: 25 }]]);
    const shape = shapeOf(s, s.entities[0]!);
    expect(shape.kind === 'arc' && shape.r).toBeCloseTo(25);
    expect(midOf(shape)!.y).toBeGreaterThan(0);
  });
  it('sweeps a centre point arc the way the cursor turned', () => {
    const s = emptySketch();
    const tool = createTool('arcc');
    tool.click(s, free(0, 0), {});
    tool.click(s, free(10, 0), {});
    // Turn clockwise a quarter.
    for (let a = 0; a >= -Math.PI / 2; a -= 0.1) tool.move({ x: 10 * Math.cos(a), y: 10 * Math.sin(a) });
    tool.move({ x: 0, y: -10 });
    expect(tool.click(s, tool.resolve(free(0, -10), {}), {})).toBe('done');
    const shape = shapeOf(s, s.entities[0]!);
    if (shape.kind !== 'arc') throw new Error('arc expected');
    expect(sweep(shape.start, shape.end)).toBeCloseTo(Math.PI / 2);
    expect(midOf(shape)!.y).toBeLessThan(0);
  });
  it('builds a closed slot with its caps outside', () => {
    const s = emptySketch();
    draw(s, 'slot', [{ x: 0, y: 0 }, [{ x: 9, y: 0 }, { length: 40 }], [{ x: 20, y: 3 }, { width: 10 }]]);
    const arcs = s.entities.filter((e) => e.kind === 'arc');
    expect(arcs).toHaveLength(2);
    const outer = arcs.map((a) => midOf(shapeOf(s, a))!.x).sort((a, b) => a - b);
    expect(outer[0]).toBeCloseTo(-5);
    expect(outer[1]).toBeCloseTo(45);
    // Every outline point is shared by exactly two curves: the profile is closed.
    const uses = new Map<number, number>();
    for (const e of s.entities.filter((x) => !x.construction)) {
      for (const p of e.kind === 'line' ? [e.a, e.b] : e.kind === 'arc' ? [e.a, e.b] : []) uses.set(p, (uses.get(p) ?? 0) + 1);
    }
    expect([...uses.values()].every((n) => n === 2)).toBe(true);
  });
  it('draws a polygon with the typed side count', () => {
    const s = emptySketch();
    draw(s, 'polygon', [{ x: 0, y: 0 }, [{ x: 10, y: 0 }, { sides: 8, diameter: 50 }]]);
    expect(s.entities).toHaveLength(8);
    for (const p of s.points) expect(dist(p, { x: 0, y: 0 })).toBeCloseTo(25);
  });
});

describe('editing', () => {
  it('removes curves and the points nothing else uses', () => {
    const s = emptySketch();
    draw(s, 'line', [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }]);
    remove(s, [s.entities[0]!.id]);
    expect(s.entities).toHaveLength(1);
    expect(s.points).toHaveLength(2);
  });
  it('keeps an arc whole when one of its ends is dragged', () => {
    const s = emptySketch();
    draw(s, 'arc3', [{ x: -10, y: 0 }, { x: 10, y: 0 }, { x: 0, y: 10 }]);
    const before = clone(s);
    const arc = s.entities[0]!;
    if (arc.kind !== 'arc') throw new Error('arc expected');
    translate(s, new Map([[arc.b, { ...point(before, arc.b) }]]), { x: 5, y: 0 });
    repairArcs(s, before);
    const c = point(s, arc.c), a = point(s, arc.a), b = point(s, arc.b);
    expect(dist(c, a)).toBeCloseTo(dist(c, b));
    expect(sweep(angleOf(c, a), angleOf(c, b))).toBeCloseTo(Math.PI);
  });
  it('moves curves with their shared points and merges dropped ends', () => {
    const s = emptySketch();
    draw(s, 'line', [{ x: 0, y: 0 }, { x: 10, y: 0 }]);
    draw(s, 'line', [{ x: 20, y: 0 }, { x: 30, y: 0 }]);
    const [first, second] = s.entities as [Sketch['entities'][number], Sketch['entities'][number]];
    if (first.kind !== 'line' || second.kind !== 'line') throw new Error('lines expected');
    translate(s, originsOf(s, [second.id]), { x: -10, y: 0 });
    merge(s, second.a, first.b);
    expect(s.points).toHaveLength(3);
    expect(chain(s, first.id).sort()).toEqual([first.id, second.id].sort());
  });
});

describe('DXF', () => {
  it('writes lines, circles and arcs in millimetres, without construction', () => {
    const s = emptySketch();
    draw(s, 'rectc', [{ x: 0, y: 0 }, [{ x: 1, y: 1 }, { width: 80, height: 40 }]]);
    draw(s, 'circle', [{ x: 0, y: 0 }, [{ x: 1, y: 0 }, { diameter: 10 }]]);
    draw(s, 'arc3', [{ x: -10, y: 30 }, { x: 10, y: 30 }, { x: 0, y: 40 }]);
    const dxf = toDxf(s);
    expect(dxf.match(/\r\nLINE\r\n/g)).toHaveLength(4);
    expect(dxf.match(/\r\nCIRCLE\r\n/g)).toHaveLength(1);
    expect(dxf.match(/\r\nARC\r\n/g)).toHaveLength(1);
    expect(dxf).toContain('$INSUNITS\r\n70\r\n4');
    expect(dxf.trimEnd().endsWith('EOF')).toBe(true);
  });
});
