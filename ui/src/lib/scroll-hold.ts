/** Keeps every scrolled panel where the user left it when its content
 *  shrinks for a moment, as while a preview is worked out again: the
 *  browser then pulls the panel up to fit, and when the content comes back
 *  the panel is put back. A panel the user scrolls, or that the page
 *  scrolls on purpose, keeps its new place. */
export function holdScrollPositions(root: Document = document, window_ms = 2000): () => void {
  /** Where each panel was last scrolled to, by the user or the page. */
  const placed = new Map<Element, number>();
  /** Panels pulled up by shrinking content, and where they belong. */
  const held = new Map<Element, { top: number; until: number }>();

  const onScroll = (event: Event) => {
    const el = event.target;
    if (!(el instanceof Element) || held.has(el)) return;
    placed.set(el, el.scrollTop);
  };
  const userMoved = (event: Event) => {
    for (const el of [...held.keys()]) if (event.target instanceof Node && el.contains(event.target)) held.delete(el);
  };
  const check = () => {
    const now = performance.now();
    for (const [el, top] of placed) {
      if (!el.isConnected) { placed.delete(el); held.delete(el); continue; }
      const hold = held.get(el);
      if (hold) {
        if (now > hold.until) { held.delete(el); placed.set(el, el.scrollTop); }
        else if (el.scrollHeight - el.clientHeight >= hold.top) { el.scrollTop = hold.top; held.delete(el); }
      } else if (el.scrollTop < top && el.scrollHeight - el.clientHeight < top) {
        // Pulled up because the content no longer reaches where it was.
        held.set(el, { top, until: now + window_ms });
      }
    }
  };
  const observer = new MutationObserver(() => { if (placed.size) check(); });
  observer.observe(root.documentElement, { childList: true, subtree: true, characterData: true });
  root.addEventListener('scroll', onScroll, true);
  const inputs = ['wheel', 'touchmove', 'keydown', 'pointerdown'];
  for (const type of inputs) root.addEventListener(type, userMoved, true);
  return () => {
    observer.disconnect();
    root.removeEventListener('scroll', onScroll, true);
    for (const type of inputs) root.removeEventListener(type, userMoved, true);
  };
}
