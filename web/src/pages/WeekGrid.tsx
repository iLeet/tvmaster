import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import { DAY_NAMES, daysLabel, sourceLabel } from '../lib';
import type { Block } from '../types';
import { Btn } from '../ui';
import {
  clockLabel,
  DAY_MINUTES,
  daysOf,
  detachDay,
  duplicate,
  hueFor,
  moveOccurrence,
  newBlock,
  resizeEnd,
  resizeStart,
  segments,
  shadows,
  shift,
  snap,
  span,
  type Seg,
} from '../weekgrid';

const PX_PER_HOUR = 44;
const PX_PER_MIN = PX_PER_HOUR / 60;
/** Pointer travel (px) before a press on a block becomes a drag instead of a click. */
const DRAG_SLOP = 4;
const UNDO_LIMIT = 50;

type Drag =
  | { kind: 'create'; day: number; anchor: number; cur: number }
  | { kind: 'move'; block: number; origin: number; grabCol: number; grabMin: number; orig: Block; x: number; y: number; active: boolean }
  | { kind: 'resize'; edge: 'start' | 'end'; block: number; orig: Block; offset: number };

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

export function WeekGrid({
  blocks,
  onChange,
  step,
  renderEditor,
}: {
  blocks: Block[];
  onChange: (b: Block[]) => void;
  /** Snap size in minutes: the channel's time grid. */
  step: number;
  /** The form for the selected block (supplied by the parent so this file stays free of the editor). */
  renderEditor: (block: Block, index: number, set: (b: Block) => void, remove: () => void) => ReactNode;
}) {
  const [sel, setSel] = useState<{ block: number; day: number } | null>(null);
  const [draft, setDraft] = useState<Block[] | null>(null);
  const [preview, setPreview] = useState<{ day: number; start: number; end: number } | null>(null);
  const [undo, setUndo] = useState<Block[][]>([]);

  // Handlers registered on `window` during a drag must see current props.
  const live = useRef({ blocks, onChange, step });
  live.current = { blocks, onChange, step };
  const drag = useRef<Drag | null>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Open scrolled to where the lineup starts rather than to midnight.
  useEffect(() => {
    const starts = segments(live.current.blocks).filter((s) => !s.contFrom).map((s) => s.start);
    const first = starts.length ? Math.min(...starts) : 8 * 60;
    if (scrollRef.current) scrollRef.current.scrollTop = Math.max(0, (first - 60) * PX_PER_MIN);
  }, []);

  const shown = draft ?? blocks;
  const segs = useMemo(() => segments(shown), [shown]);
  const shade = useMemo(() => shadows(shown), [shown]);

  /** Every change from this grid goes through here, so it can be undone. */
  const commit = useCallback((next: Block[]) => {
    const { blocks: cur, onChange: out } = live.current;
    setUndo((u) => [...u.slice(-(UNDO_LIMIT - 1)), cur]);
    out(next);
  }, []);

  // Keep the selection pointing at a block that exists.
  useEffect(() => {
    if (sel && sel.block >= blocks.length) setSel(null);
  }, [blocks.length, sel]);

  const where = (e: { clientX: number; clientY: number }) => {
    const r = bodyRef.current!.getBoundingClientRect();
    return {
      col: clamp(Math.floor(((e.clientX - r.left) / r.width) * 7), 0, 6),
      min: clamp(((e.clientY - r.top) / r.height) * DAY_MINUTES, 0, DAY_MINUTES),
    };
  };

  // Window listeners must be the same function objects when added and removed,
  // while the logic they run must see this render's props: hence the indirection.
  const handlers = useRef<{ move: (e: PointerEvent) => void; up: (e: PointerEvent) => void; cancel: () => void }>({
    move: () => {},
    up: () => {},
    cancel: () => {},
  });
  const stable = useRef({
    move: (e: PointerEvent) => handlers.current.move(e),
    up: (e: PointerEvent) => handlers.current.up(e),
    cancel: () => handlers.current.cancel(),
  }).current;

  const finish = () => {
    window.removeEventListener('pointermove', stable.move);
    window.removeEventListener('pointerup', stable.up);
    window.removeEventListener('pointercancel', stable.cancel);
    drag.current = null;
    setDraft(null);
    setPreview(null);
  };

  const apply = (d: Drag, e: { clientX: number; clientY: number }): Block[] | null => {
    const { blocks: cur, step: st } = live.current;
    const p = where(e);
    if (d.kind === 'move') {
      if (!d.active) return null;
      let b = shift(d.orig, p.min - d.grabMin, st);
      b = moveOccurrence(b, d.origin, d.origin + (p.col - d.grabCol));
      return cur.map((x, i) => (i === d.block ? b : x));
    }
    if (d.kind === 'resize') {
      const b = d.edge === 'end' ? resizeEnd(d.orig, p.min + d.offset, st) : resizeStart(d.orig, p.min + d.offset, st);
      return cur.map((x, i) => (i === d.block ? b : x));
    }
    return null;
  };

  function onMove(e: PointerEvent) {
    const d = drag.current;
    if (!d) return;
    if (d.kind === 'create') {
      const p = where(e);
      d.cur = p.min;
      const { step: st } = live.current;
      setPreview({ day: d.day, start: snap(Math.min(d.anchor, d.cur), st), end: Math.max(snap(Math.max(d.anchor, d.cur), st), snap(Math.min(d.anchor, d.cur), st) + st) });
      return;
    }
    if (d.kind === 'move' && !d.active) {
      if (Math.hypot(e.clientX - d.x, e.clientY - d.y) < DRAG_SLOP) return;
      d.active = true;
    }
    const next = apply(d, e);
    if (next) setDraft(next);
  }

  function onUp(e: PointerEvent) {
    const d = drag.current;
    if (!d) return;
    const { blocks: cur, step: st } = live.current;
    if (d.kind === 'create') {
      const moved = Math.abs(d.cur - d.anchor) >= st / 2;
      const a = moved ? d.anchor : Math.floor(d.anchor / st) * st;
      const b = moved ? d.cur : a + Math.max(st, 60);
      const nb = newBlock(d.day, a, b, st);
      commit([...cur, nb]);
      setSel({ block: cur.length, day: d.day });
    } else {
      const next = apply(d, e);
      if (next && JSON.stringify(next) !== JSON.stringify(cur)) {
        commit(next);
        // Follow the occurrence the user dragged to its new day.
        if (d.kind === 'move') {
          const days = daysOf(next[d.block]);
          const dest = clamp(d.origin + (where(e).col - d.grabCol), 0, 6);
          setSel({ block: d.block, day: days.includes(dest) ? dest : days.includes(d.origin) ? d.origin : days[0] });
        }
      }
    }
    finish();
  }

  function onCancel() {
    finish();
  }

  handlers.current = { move: onMove, up: onUp, cancel: onCancel };
  const listen = () => {
    window.addEventListener('pointermove', stable.move);
    window.addEventListener('pointerup', stable.up);
    window.addEventListener('pointercancel', stable.cancel);
  };

  const startCreate = (e: ReactPointerEvent<HTMLDivElement>, day: number) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('.wg-seg')) return;
    const p = where(e);
    drag.current = { kind: 'create', day, anchor: snap(p.min, live.current.step), cur: p.min };
    setSel(null);
    listen();
  };

  const startMove = (e: ReactPointerEvent<HTMLDivElement>, s: Seg) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    const p = where(e);
    setSel({ block: s.block, day: s.origin });
    drag.current = { kind: 'move', block: s.block, origin: s.origin, grabCol: p.col, grabMin: p.min, orig: live.current.blocks[s.block], x: e.clientX, y: e.clientY, active: false };
    listen();
  };

  const startResize = (e: ReactPointerEvent<HTMLDivElement>, s: Seg, edge: 'start' | 'end') => {
    if (e.button !== 0) return;
    e.stopPropagation();
    e.preventDefault();
    setSel({ block: s.block, day: s.origin });
    // The end handle of an overnight block's tail is measured from the start day's midnight.
    drag.current = { kind: 'resize', edge, block: s.block, orig: live.current.blocks[s.block], offset: edge === 'end' && s.contFrom ? DAY_MINUTES : 0 };
    listen();
  };

  useEffect(() => () => {
    window.removeEventListener('pointermove', stable.move);
    window.removeEventListener('pointerup', stable.up);
    window.removeEventListener('pointercancel', stable.cancel);
  }, [stable]);

  const replace = (i: number, b: Block) => commit(blocks.map((x, j) => (j === i ? b : x)));
  const remove = (i: number) => {
    commit(blocks.filter((_, j) => j !== i));
    setSel(null);
  };

  const onKey = (e: ReactKeyboardEvent, s: Seg) => {
    const b = blocks[s.block];
    const k = e.key;
    if (k === 'Delete' || k === 'Backspace') return e.preventDefault(), remove(s.block);
    if (k === 'Escape') return setSel(null);
    if (k === 'Enter' || k === ' ') return e.preventDefault(), setSel({ block: s.block, day: s.origin });
    if (k === 'ArrowUp' || k === 'ArrowDown') {
      e.preventDefault();
      const dir = k === 'ArrowUp' ? -1 : 1;
      if (e.shiftKey) {
        const sp = segs.find((x) => x.block === s.block && !x.contTo && x.origin === s.origin);
        const end = (sp ? (sp.contFrom ? sp.end + DAY_MINUTES : sp.end) : 0) + dir * live.current.step;
        replace(s.block, resizeEnd(b, end, live.current.step));
      } else {
        replace(s.block, shift(b, dir * live.current.step, live.current.step));
      }
      setSel({ block: s.block, day: s.origin });
    } else if (k === 'ArrowLeft' || k === 'ArrowRight') {
      e.preventDefault();
      replace(s.block, moveOccurrence(b, s.origin, s.origin + (k === 'ArrowLeft' ? -1 : 1)));
      const moved = moveOccurrence(b, s.origin, s.origin + (k === 'ArrowLeft' ? -1 : 1));
      if (moved !== b) setSel({ block: s.block, day: clamp(s.origin + (k === 'ArrowLeft' ? -1 : 1), 0, 6) });
    }
  };

  // The selected occurrence's day, falling back to the block's first day if its days were edited.
  const selected = sel && blocks[sel.block] ? { block: sel.block, day: daysOf(blocks[sel.block]).includes(sel.day) ? sel.day : daysOf(blocks[sel.block])[0] } : null;
  const label = (b: Block) => b.name.trim() || sourceLabel(b.source);
  /** The block's real times, even on the part of an overnight block that sits in the next column. */
  const full = (b: Block) => {
    const sp = span(b);
    return sp ? `${clockLabel(sp.start)} – ${clockLabel(sp.end)}` : '';
  };

  return (
    <div className="stack" style={{ gap: 10 }}>
      <div className="row wrap between">
        <p className="small dim" style={{ margin: 0 }}>
          Drag on a day to add a block. Drag a block to move it, its top or bottom edge to resize. Blocks snap to {step} minutes. With a block focused: arrow keys move it, Shift+↑/↓ resizes, Delete removes.
        </p>
        <div className="row">
          <Btn small disabled={undo.length === 0} onClick={() => { const prev = undo[undo.length - 1]; setUndo(undo.slice(0, -1)); live.current.onChange(prev); }}>Undo</Btn>
        </div>
      </div>

      <div className="wg" role="group" aria-label="Weekly schedule grid">
        <div className="wg-scroll" ref={scrollRef}>
          <div className="wg-head">
            <div />
            {DAY_NAMES.map((d) => <div key={d}>{d}</div>)}
          </div>
          <div className="wg-body" style={{ height: 24 * PX_PER_HOUR }}>
            <div className="wg-times" aria-hidden="true">
              {Array.from({ length: 24 }, (_, h) => (
                <div key={h} style={{ top: h * PX_PER_HOUR }}>{h === 0 ? '' : clockLabel(h * 60)}</div>
              ))}
            </div>
            <div className="wg-cols" ref={bodyRef}>
              {DAY_NAMES.map((name, day) => (
                <div key={name} className="wg-col" onPointerDown={(e) => startCreate(e, day)} aria-label={name}>
                  {segs.filter((s) => s.day === day).map((s, i) => {
                    const b = shown[s.block];
                    const isSel = selected?.block === s.block;
                    const h = (s.end - s.start) * PX_PER_MIN;
                    const style = { top: s.start * PX_PER_MIN, height: Math.max(h, 6), '--h': hueFor(b.source.type === 'show' ? b.source.show : b.source.type === 'pool' ? b.source.shows.join() : b.source.type) } as CSSProperties;
                    return (
                      <div
                        key={`${s.block}-${s.origin}-${i}`}
                        className={`wg-seg${isSel ? ' sel' : ''}${isSel && selected?.day === s.origin ? ' main' : ''}${s.contFrom ? ' cont-from' : ''}${s.contTo ? ' cont-to' : ''}`}
                        style={{ ...style, zIndex: 10 - Math.min(9, s.block) }}
                        role="button"
                        tabIndex={0}
                        aria-label={`${label(b)}, ${DAY_NAMES[s.origin]} ${clockLabel(s.start)} to ${clockLabel(s.end)}`}
                        onPointerDown={(e) => startMove(e, s)}
                        onKeyDown={(e) => onKey(e, s)}
                        onFocus={() => setSel({ block: s.block, day: s.origin })}
                      >
                        {!s.contFrom && <div className="wg-h top" onPointerDown={(e) => startResize(e, s, 'start')} />}
                        <div className="wg-t ellipsis">{label(b)}</div>
                        {h >= 34 && <div className="wg-s ellipsis">{full(b)}</div>}
                        {shade.filter((x) => x.block === s.block && x.day === s.day).map((x, j) => (
                          <div key={j} className="wg-shadow" title="An earlier block in the list covers this time, so it airs instead" style={{ top: (x.start - s.start) * PX_PER_MIN, height: (x.end - x.start) * PX_PER_MIN }} />
                        ))}
                        {!s.contTo && <div className="wg-h bottom" onPointerDown={(e) => startResize(e, s, 'end')} />}
                      </div>
                    );
                  })}
                  {preview?.day === day && <div className="wg-seg ghost" style={{ top: preview.start * PX_PER_MIN, height: (preview.end - preview.start) * PX_PER_MIN }} />}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      {shade.length > 0 && (
        <p className="small" style={{ margin: 0, color: 'var(--warn)' }}>
          Hatched areas overlap an earlier block in the list. The earlier block airs there, so the hatched time is never used.
        </p>
      )}

      {selected ? (
        <div className="stack" style={{ gap: 8 }}>
          <div className="row wrap between">
            <h2 style={{ margin: 0 }}>{label(blocks[selected.block])} <span className="dim small">· {daysLabel(blocks[selected.block].days)}</span></h2>
            <div className="row wrap">
              {blocks[selected.block].days.length !== 1 && (
                <Btn small title={`Make ${DAY_NAMES[selected.day]} its own block, so it can differ from the other days`} onClick={() => { const r = detachDay(blocks, selected.block, selected.day); if (r.blocks !== blocks) { commit(r.blocks); setSel({ block: r.index, day: selected.day }); } }}>
                  Only {DAY_NAMES[selected.day]}
                </Btn>
              )}
              <Btn small icon="copy" onClick={() => { const r = duplicate(blocks, selected.block); commit(r.blocks); setSel({ block: r.index, day: selected.day }); }}>Duplicate</Btn>
            </div>
          </div>
          {renderEditor(blocks[selected.block], selected.block, (b) => replace(selected.block, b), () => remove(selected.block))}
        </div>
      ) : (
        <p className="small dim" style={{ margin: 0 }}>{blocks.length === 0 ? 'No blocks yet: drag on a day above to add the first one.' : 'Select a block to edit what it airs.'}</p>
      )}
    </div>
  );
}
