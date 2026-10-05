// Typed values may be arithmetic, as in Fusion: "50/2+3", "(80-2*4)", "1,5".
// Numbers, + - * /, unary minus and parentheses; anything else is refused.

export function evaluate(text: string): number | null {
  const source = text.replace(/,/g, '.').replace(/\s+/g, '');
  if (!source) return null;
  let at = 0;
  const peek = (): string => source[at] ?? '';

  function primary(): number {
    if (peek() === '(') {
      at++;
      const v = sum();
      if (peek() !== ')') throw new Error('unclosed');
      at++;
      return v;
    }
    if (peek() === '-') { at++; return -primary(); }
    if (peek() === '+') { at++; return primary(); }
    const match = /^(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/i.exec(source.slice(at));
    if (!match) throw new Error('number expected');
    at += match[0].length;
    return Number(match[0]);
  }

  function product(): number {
    let v = primary();
    while (peek() === '*' || peek() === '/') {
      const op = source[at++];
      const r = primary();
      v = op === '*' ? v * r : v / r;
    }
    return v;
  }

  function sum(): number {
    let v = product();
    while (peek() === '+' || peek() === '-') {
      const op = source[at++];
      const r = product();
      v = op === '+' ? v + r : v - r;
    }
    return v;
  }

  try {
    const v = sum();
    return at === source.length && Number.isFinite(v) ? v : null;
  } catch {
    return null;
  }
}
