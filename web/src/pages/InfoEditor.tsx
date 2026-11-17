import { useEffect, useRef, useState } from 'react';
import { move, newSlide } from '../info';
import type { ChannelTemplates, InfoConfig, InfoSlide, Library, SlideKind } from '../types';
import { Btn, Card, Chips, Field, Toggle } from '../ui';
import { PictureField } from '../PictureField';

/** A picture of one page, redrawn a moment after each change. */
function usePagePreview(id: string, name: string, info: InfoConfig, page: number) {
  const [url, setUrl] = useState<string | null>(null);
  const [count, setCount] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const last = useRef<string | null>(null);
  const key = JSON.stringify({ id, name, info, page });
  useEffect(() => {
    let live = true;
    const timer = setTimeout(async () => {
      try {
        const res = await fetch('/api/v1/info/preview', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ id: id || undefined, name, info, page }),
          credentials: 'same-origin',
        });
        if (!res.ok) {
          const j = (await res.json().catch(() => ({}))) as { error?: string };
          throw new Error(j.error ?? `${res.status} ${res.statusText}`);
        }
        const n = Number(res.headers.get('x-page-count') ?? 1);
        const next = URL.createObjectURL(await res.blob());
        if (!live) return;
        if (last.current) URL.revokeObjectURL(last.current);
        last.current = next;
        setUrl(next);
        setCount(n || 1);
        setError(null);
      } catch (e) {
        if (live) setError(e instanceof Error ? e.message : String(e));
      }
    }, 400);
    return () => { live = false; clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  useEffect(() => () => { if (last.current) URL.revokeObjectURL(last.current); }, []);
  return { url, count, error };
}

function SlideCard({ n, total, s, slides, onChange, onMove, onRemove }: {
  n: number; total: number; s: InfoSlide; slides: ChannelTemplates['slides'];
  onChange: (s: InfoSlide) => void; onMove: (to: number) => void; onRemove: () => void;
}) {
  const set = (p: Partial<InfoSlide>) => onChange({ ...s, ...p });
  const kind = slides.find((k) => k.id === s.kind);
  return (
    <div className="card stack" style={{ borderLeft: '3px solid var(--accent)' }}>
      <div className="row wrap between">
        <strong>Page {n + 1} <span className="dim small" style={{ fontWeight: 400 }}>{kind?.name}</span></strong>
        <span className="row">
          <Btn small disabled={n === 0} onClick={() => onMove(n - 1)} aria-label="Move earlier">↑</Btn>
          <Btn small disabled={n === total - 1} onClick={() => onMove(n + 1)} aria-label="Move later">↓</Btn>
          <Btn small kind="danger" onClick={onRemove}>Remove</Btn>
        </span>
      </div>
      {s.kind === 'text' && (
        <>
          <Field label="Title"><input className="input" value={s.title} maxLength={80} placeholder="Garage sale this Saturday" onChange={(e) => set({ title: e.target.value })} /></Field>
          <Field label="Text" hint="One line per row; a blank line leaves a gap. It shrinks to fit.">
            <textarea className="input" rows={4} value={s.lines.join('\n')} onChange={(e) => set({ lines: e.target.value.split('\n') })} />
          </Field>
          <Field label="Picture" hint="Optional: shown beside the text."><PictureField value={s.picture} onChange={(picture) => set({ picture })} /></Field>
        </>
      )}
      {s.kind === 'photo' && (
        <>
          <Field label="Picture" hint="Fills the whole screen."><PictureField value={s.picture} onChange={(picture) => set({ picture })} /></Field>
          <Field label="Caption"><input className="input" value={s.title} maxLength={80} onChange={(e) => set({ title: e.target.value })} /></Field>
        </>
      )}
      {(s.kind === 'headlines' || s.kind === 'tonight') && (
        <Field label="Title" hint={s.kind === 'headlines' ? 'Five headlines to a page, as many pages as there are headlines.' : 'Five channels to a page: what is on now, and what comes next.'}>
          <input className="input" value={s.title} maxLength={80} onChange={(e) => set({ title: e.target.value })} />
        </Field>
      )}
      {s.kind === 'clock' && <p className="small dim">The time and the date, large.</p>}
      <Field label={s.seconds ? `Stays up ${s.seconds} s` : 'Stays up as long as the other pages'}>
        <span className="row">
          <input type="range" min={3} max={120} value={s.seconds ?? 12} onChange={(e) => set({ seconds: Number(e.target.value) })} />
          {s.seconds !== null && <Btn small onClick={() => set({ seconds: null })}>Use the channel's time</Btn>}
        </span>
      </Field>
    </div>
  );
}

/** An info channel: its pages, news feeds, ticker, clock and music. */
export function InfoEditor({ id, name, info, onChange, libs, templates }: {
  id: string; name: string; info: InfoConfig; onChange: (i: InfoConfig) => void; libs: Library[]; templates: ChannelTemplates | null;
}) {
  const [page, setPage] = useState(0);
  const prev = usePagePreview(id, name, info, page);
  const set = (p: Partial<InfoConfig>) => onChange({ ...info, ...p });
  const kinds = templates?.slides ?? [];
  const musicLibs = libs.filter((l) => l.kind === 'music');
  const [feed, setFeed] = useState('');
  const addFeed = (url: string) => { const u = url.trim(); if (u && !info.feeds.includes(u)) set({ feeds: [...info.feeds, u] }); setFeed(''); };
  const hasHeadlines = info.slides.some((s) => s.kind === 'headlines');
  const setSlide = (i: number, s: InfoSlide) => set({ slides: info.slides.map((x, k) => (k === i ? s : x)) });

  return (
    <div className="stack gap-lg">
      <Card className="stack">
        <div className="row between wrap">
          <h2 style={{ margin: 0 }}>Preview</h2>
          <span className="row">
            <Btn small disabled={prev.count < 2} onClick={() => setPage((page + prev.count - 1) % prev.count)}>← Page</Btn>
            <span className="small dim">{Math.min(page, prev.count - 1) + 1} of {prev.count}</span>
            <Btn small disabled={prev.count < 2} onClick={() => setPage((page + 1) % prev.count)}>Page →</Btn>
          </span>
        </div>
        {prev.error ? <div className="err-banner">{prev.error}</div> : prev.url ? <img src={prev.url} alt="One page of the channel" style={{ width: '100%', display: 'block', borderRadius: 8, aspectRatio: '16 / 9', background: '#000' }} /> : <div className="empty">Drawing…</div>}
        <p className="small dim">Drawn in the channel’s look (the station theme, or its own). Headlines shown here are stand-ins until the first fetch.</p>
      </Card>

      <Card className="stack">
        <h2>The channel</h2>
        <div className="row wrap" style={{ alignItems: 'flex-start' }}>
          <Field label="Heading" hint="Top left. Blank = the channel’s name."><input className="input" style={{ width: 260 }} value={info.title ?? ''} maxLength={60} placeholder={name} onChange={(e) => set({ title: e.target.value || null })} /></Field>
          <Field label="What the guide says it is" hint="A few words. Blank = made from the pages."><input className="input" style={{ width: 300 }} value={info.about ?? ''} maxLength={80} placeholder="News and headlines" onChange={(e) => set({ about: e.target.value || null })} /></Field>
        </div>
        <Field label={`Each page stays up ${info.seconds} s`}>
          <input type="range" min={3} max={60} value={info.seconds} onChange={(e) => set({ seconds: Number(e.target.value) })} />
        </Field>
        <div className="row wrap">
          <Toggle checked={info.clock} onChange={(clock) => set({ clock })} label="Time and date at the top right" />
          <Toggle checked={info.hour12} onChange={(hour12) => set({ hour12 })} label="12-hour clock" />
        </div>
      </Card>

      <Card className="stack">
        <div className="row between wrap">
          <h2 style={{ margin: 0 }}>Pages</h2>
          <span className="small dim">Shown in this order, round and round.</span>
        </div>
        {info.slides.length === 0 && <p className="dim">No pages yet: the channel shows a clock. Add some below.</p>}
        {info.slides.map((s, i) => (
          <div key={i} onFocus={() => setPage(i)}>
            <SlideCard n={i} total={info.slides.length} s={s} slides={kinds} onChange={(x) => setSlide(i, x)} onMove={(to) => set({ slides: move(info.slides, i, to) })} onRemove={() => set({ slides: info.slides.filter((_, k) => k !== i) })} />
          </div>
        ))}
        <div className="row wrap">
          <span className="small dim">Add a page:</span>
          {kinds.map((k) => (
            <Btn key={k.id} small title={k.about} onClick={() => set({ slides: [...info.slides, newSlide(k.id as SlideKind)] })}>+ {k.name}</Btn>
          ))}
        </div>
      </Card>

      <Card className="stack">
        <h2>Headlines</h2>
        <p className="small dim">News feeds (RSS or Atom) for the headlines pages and the ticker. They are fetched every fifteen minutes by the server.{hasHeadlines && info.feeds.length === 0 ? ' A headlines page needs at least one.' : ''}</p>
        {info.feeds.map((f) => (
          <div key={f} className="row"><span className="mono small grow ellipsis">{f}</span><Btn small onClick={() => set({ feeds: info.feeds.filter((x) => x !== f) })}>Remove</Btn></div>
        ))}
        <div className="row wrap">
          <input className="input mono" style={{ width: 360 }} placeholder="https://example.com/news.rss" value={feed} onChange={(e) => setFeed(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') addFeed(feed); }} />
          <Btn small disabled={!feed.trim()} onClick={() => addFeed(feed)}>Add feed</Btn>
        </div>
        <div className="row wrap">
          <span className="small dim">Or start with:</span>
          {(templates?.feeds ?? []).filter((f) => !info.feeds.includes(f.url)).map((f) => <Btn key={f.url} small onClick={() => addFeed(f.url)}>{f.name}</Btn>)}
        </div>
      </Card>

      <Card className="stack">
        <h2>Ticker</h2>
        <Toggle checked={info.ticker_enabled} onChange={(ticker_enabled) => set({ ticker_enabled })} label="The scrolling line along the bottom: your lines, then the headlines" />
        {info.ticker_enabled && (
          <Field label="Your lines" hint="One per row.">
            <textarea className="input" rows={3} value={info.ticker.join('\n')} onChange={(e) => set({ ticker: e.target.value.split('\n') })} />
          </Field>
        )}
      </Card>

      <Card className="stack">
        <h2>Music</h2>
        <Field label="Background music" hint={musicLibs.length ? 'Loops under the pages.' : 'Add a library of kind “Music” to play music under the pages.'}>
          <Chips options={musicLibs.map((l) => ({ value: l.id, label: `${l.path.split(/[\\/]/).filter(Boolean).pop() ?? l.path} (${l.items})` }))} value={info.music_library_ids} onToggle={(lid) => set({ music_library_ids: info.music_library_ids.includes(lid) ? info.music_library_ids.filter((x) => x !== lid) : [...info.music_library_ids, lid] })} />
        </Field>
      </Card>
    </div>
  );
}
