import { useEffect, useRef, useState } from 'react';
import { GUIDE_PRESETS, GUIDE_THEMES, PIP_PLACEMENTS, cleanColor, toggleId } from '../guide';
import { PictureField } from '../PictureField';
import type { ChannelView, GuideConfig, Library } from '../types';
import { Card, Chips, Field, Segmented, Toggle } from '../ui';

/** A picture of the guide as it would look with these settings, redrawn a moment after each change. */
function useGuidePreview(id: string, name: string, guide: GuideConfig) {
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef<string | null>(null);
  const key = JSON.stringify({ id, name, guide });
  useEffect(() => {
    let live = true;
    const timer = setTimeout(async () => {
      try {
        const res = await fetch('/api/v1/guide/preview', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ id, name, guide, t: 3 }),
          credentials: 'same-origin',
        });
        if (!res.ok) {
          const j = (await res.json().catch(() => ({}))) as { error?: string };
          throw new Error(j.error ?? `${res.status} ${res.statusText}`);
        }
        const blob = await res.blob();
        if (!live) return;
        const next = URL.createObjectURL(blob);
        if (latest.current) URL.revokeObjectURL(latest.current);
        latest.current = next;
        setUrl(next);
        setError(null);
      } catch (e) {
        if (live) setError(e instanceof Error ? e.message : String(e));
      }
    }, 350);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  useEffect(() => () => { if (latest.current) URL.revokeObjectURL(latest.current); }, []);
  return { url, error };
}

const channelLabel = (c: ChannelView) => `${c.number} ${c.name}`;

export function GuideEditor({
  id, name, guide, onChange, channels, libs,
}: {
  id: string;
  name: string;
  guide: GuideConfig;
  onChange: (g: GuideConfig) => void;
  channels: ChannelView[];
  libs: Library[];
}) {
  const preview = useGuidePreview(id, name, guide);
  const g = guide;
  const set = (p: Partial<GuideConfig>) => onChange({ ...g, ...p });
  const layout = (p: Partial<GuideConfig['layout']>) => set({ layout: { ...g.layout, ...p } });
  const header = (p: Partial<GuideConfig['header']>) => set({ header: { ...g.header, ...p } });
  const look = (p: Partial<GuideConfig['look']>) => set({ look: { ...g.look, ...p } });
  const ticker = (p: Partial<GuideConfig['ticker_options']>) => set({ ticker_options: { ...g.ticker_options, ...p } });
  const pip = (p: Partial<GuideConfig['pip']>) => set({ pip: { ...g.pip, ...p } });
  const programming = channels.filter((c) => c.kind === 'programming').sort((a, b) => a.number - b.number);
  const showable = channels.filter((c) => c.kind !== 'guide').sort((a, b) => a.number - b.number);
  const theme = GUIDE_THEMES.find((t) => t.value === g.theme);
  const musicLibs = libs.filter((l) => l.kind === 'music');

  return (
    <div className="stack gap-lg">
      <Card className="stack">
        <h2>Preview</h2>
        <div style={{ position: 'relative', maxWidth: 800 }}>
          {preview.url ? (
            <img src={preview.url} alt="What the guide will look like" style={{ width: '100%', display: 'block', borderRadius: 8, imageRendering: 'pixelated', aspectRatio: '16 / 9', background: '#000' }} />
          ) : (
            <div style={{ aspectRatio: '16 / 9', background: '#000', borderRadius: 8 }} />
          )}
        </div>
        {preview.error ? <p className="small" style={{ color: 'var(--warn)' }}>{preview.error}</p>
          : <p className="small dim">Uses your real channels and what is on them now. {g.pip.mode !== 'off' && 'The video window shows live video once the guide is on air.'}</p>}
      </Card>

      <Card className="stack">
        <h2>Start from a look</h2>
        <p className="small dim">One click sets the colours and style together; everything below still works afterwards.</p>
        <div className="row wrap" style={{ gap: 10 }}>
          {GUIDE_PRESETS.map((p) => (
            <button key={p.id} type="button" className="btn" title={p.help} onClick={() => onChange(p.apply(g))} style={{ display: 'grid', gap: 2, textAlign: 'left', padding: '8px 14px' }}>
              <strong>{p.label}</strong><span className="small dim" style={{ maxWidth: 200 }}>{p.help}</span>
            </button>
          ))}
        </div>
      </Card>

      <Card className="stack">
        <h2>Colours</h2>
        <div className="row wrap" style={{ gap: 10 }}>
          {GUIDE_THEMES.map((t) => (
            <button
              key={t.value}
              type="button"
              className="btn"
              aria-pressed={g.theme === t.value}
              title={t.help}
              onClick={() => set({ theme: t.value })}
              style={{ display: 'grid', gap: 6, justifyItems: 'center', padding: 8, minWidth: 92, outline: g.theme === t.value ? '2px solid var(--accent)' : undefined }}
            >
              <span style={{ width: 64, height: 28, borderRadius: 6, background: `linear-gradient(110deg, ${t.value === 'custom' ? (cleanColor(g.colors.primary ?? '') ?? t.colors[0]) : t.colors[0]} 0 60%, ${t.value === 'custom' ? (cleanColor(g.colors.accent ?? '') ?? t.colors[1]) : t.colors[1]} 60% 100%)`, border: '1px solid var(--edge)' }} />
              <span className="small">{t.label}</span>
            </button>
          ))}
        </div>
        {theme && <p className="small dim">{theme.help}</p>}
        {g.theme === 'custom' && (
          <div className="row wrap nest" style={{ alignItems: 'flex-end' }}>
            {(['primary', 'accent', 'background'] as const).map((k) => (
              <Field key={k} label={k === 'primary' ? 'Main colour' : k === 'accent' ? 'Highlight' : 'Background'} hint={k === 'background' ? 'Blank = a dark shade of the main colour.' : undefined}>
                <span className="row" style={{ gap: 8 }}>
                  <input type="color" aria-label={k} value={cleanColor(g.colors[k] ?? '') ?? (k === 'primary' ? '#3366ff' : k === 'accent' ? '#ffcc00' : '#101830')} onChange={(e) => set({ colors: { ...g.colors, [k]: e.target.value } })} />
                  <input className="input mono" style={{ width: 110 }} placeholder="#3366ff" value={g.colors[k] ?? ''} onChange={(e) => set({ colors: { ...g.colors, [k]: e.target.value || null } })} />
                </span>
              </Field>
            ))}
          </div>
        )}
        <Toggle checked={g.layout.scanlines} onChange={(v) => layout({ scanlines: v })} label={<span>Scanlines <span className="dim small">a faint CRT texture over everything</span></span>} />
      </Card>

      <Card className="stack">
        <h2>Style</h2>
        <Field label="Letters" hint={g.look.typeface === 'pixel' ? 'Chunky pixel letters, drawn small and scaled up: the retro look.' : 'A smooth modern typeface, drawn at full 1280x720 resolution.'}>
          <Segmented value={g.look.typeface} options={[{ value: 'pixel', label: 'Retro pixels' }, { value: 'smooth', label: 'Smooth' }]} onChange={(typeface) => set({ look: { ...g.look, typeface }, layout: typeface === 'smooth' ? { ...g.layout, scanlines: false } : g.layout })} />
        </Field>
        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          <Field label="Corners"><Segmented value={g.look.corners} options={[{ value: 'square', label: 'Square' }, { value: 'rounded', label: 'Rounded' }]} onChange={(corners) => look({ corners })} /></Field>
          <Field label="Text size" hint={g.look.typeface === 'pixel' ? 'Pixel letters only come in whole sizes, so Large doubles them.' : undefined}><Segmented value={g.look.text_size} options={[{ value: 'small', label: 'Small' }, { value: 'medium', label: 'Medium' }, { value: 'large', label: 'Large' }]} onChange={(text_size) => look({ text_size })} /></Field>
          <Field label="Heading"><Segmented value={g.look.header_style} options={[{ value: 'bar', label: 'Coloured bar' }, { value: 'minimal', label: 'Just the words' }]} onChange={(header_style) => look({ header_style })} /></Field>
        </div>
        <Field label="Background" hint={g.look.background.style === 'image' ? 'A picture fitted to the screen and darkened so the listing stays readable.' : undefined}>
          <Segmented value={g.look.background.style} options={[{ value: 'gradient', label: 'Gradient' }, { value: 'solid', label: 'Solid' }, { value: 'stripes', label: 'Stripes' }, { value: 'image', label: 'Picture' }]} onChange={(style) => look({ background: { ...g.look.background, style } })} />
        </Field>
        {g.look.background.style === 'image' && (
          <div className="stack nest">
            <PictureField value={g.look.background.image} onChange={(image) => look({ background: { ...g.look.background, image } })} />
            <Field label={`Darken the picture ${g.look.background.dim}%`}><input type="range" min={0} max={90} value={g.look.background.dim} onChange={(e) => look({ background: { ...g.look.background, dim: Number(e.target.value) } })} /></Field>
          </div>
        )}
        <div className="row wrap">
          <Toggle checked={g.look.cell_times} onChange={(v) => look({ cell_times: v })} label={<span>Times on each programme <span className="dim small">when there is room</span></span>} />
          <Toggle checked={g.look.now_marker} onChange={(v) => look({ now_marker: v })} label="Line at “now” on the time grid" />
        </div>
      </Card>

      <Card className="stack">
        <h2>Listing</h2>
        <Field label="Style" hint={g.layout.style === 'grid' ? 'Channels down the side, programmes across a time line.' : 'One big row per channel: what is on now and what is next. Easy to read from the couch.'}>
          <Segmented value={g.layout.style} options={[{ value: 'grid', label: 'Time grid' }, { value: 'now_next', label: 'Now & next' }]} onChange={(style) => layout({ style })} />
        </Field>
        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          <Field label="Channels on screen">
            <select className="select" value={g.layout.rows} onChange={(e) => layout({ rows: Number(e.target.value) })}>
              {[3, 4, 5, 6, 7, 8, 9, 10].map((n) => <option key={n} value={n}>{n}</option>)}
            </select>
          </Field>
          {g.layout.style === 'grid' && (
            <>
              <Field label="Hours shown">
                <select className="select" value={g.hours} onChange={(e) => set({ hours: Number(e.target.value) })}>
                  {[2, 3, 4, 5, 6].map((h) => <option key={h} value={h}>{h} hours</option>)}
                </select>
              </Field>
              <Field label="Time marks">
                <Segmented value={String(g.layout.time_step)} options={[{ value: '15', label: '15 min' }, { value: '30', label: '30 min' }, { value: '60', label: 'Hourly' }]} onChange={(v) => layout({ time_step: Number(v) as 15 | 30 | 60 })} />
              </Field>
            </>
          )}
        </div>
        {g.layout.style === 'now_next' && <Field label="Hours of schedule to look through"><select className="select" style={{ width: 120 }} value={g.hours} onChange={(e) => set({ hours: Number(e.target.value) })}>{[2, 3, 4, 5, 6].map((h) => <option key={h} value={h}>{h} hours</option>)}</select></Field>}
        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          <Field label="When there are more channels than fit" hint={g.layout.scroll === 'page' ? 'Hold a page, then slide to the next.' : 'Roll the list continuously, like the old cable guides.'}>
            <Segmented value={g.layout.scroll} options={[{ value: 'page', label: 'Page by page' }, { value: 'smooth', label: 'Smooth roll' }]} onChange={(scroll) => layout({ scroll })} />
          </Field>
          {g.layout.scroll === 'page' && (
            <Field label={`Each page stays ${g.layout.page_seconds} s`}>
              <input type="range" min={4} max={30} value={g.layout.page_seconds} onChange={(e) => layout({ page_seconds: Number(e.target.value) })} />
            </Field>
          )}
        </div>
        <div className="row wrap">
          <Toggle checked={g.layout.channel_numbers} onChange={(v) => layout({ channel_numbers: v })} label="Channel numbers" />
          <Toggle checked={g.layout.channel_names} onChange={(v) => layout({ channel_names: v })} label="Channel names" />
          <Toggle checked={g.layout.progress} onChange={(v) => layout({ progress: v })} label="Progress bar on what is airing" />
        </div>
        <Field label="Which channels" hint={g.channels.length ? 'Only these, in the order you picked them. Click one to remove it.' : 'Every programming channel, by number. Pick some to list only those, in your order.'}>
          <Chips
            options={programming.map((c) => ({ value: c.id, label: channelLabel(c) }))}
            value={g.channels}
            onToggle={(cid) => set({ channels: toggleId(g.channels, cid) })}
          />
        </Field>
        {g.channels.length > 0 && <div><button type="button" className="btn small" onClick={() => set({ channels: [] })}>Show every channel again</button></div>}
      </Card>

      <Card className="stack">
        <h2>Heading</h2>
        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          <Field label="Heading" hint="Blank = “<name> GUIDE”">
            <input className="input" style={{ width: 280 }} maxLength={60} value={g.title ?? ''} onChange={(e) => set({ title: e.target.value || null })} />
          </Field>
        </div>
        <div className="row wrap">
          <Toggle checked={g.header.title} onChange={(v) => header({ title: v })} label="Heading text" />
          <Toggle checked={g.header.clock} onChange={(v) => header({ clock: v })} label="Clock" />
          <Toggle checked={g.header.date} onChange={(v) => header({ date: v })} label="Date" />
        </div>
        <Field label="Times">
          <Segmented value={g.header.hour12 ? '12' : '24'} options={[{ value: '12', label: '12-hour (9:30 PM)' }, { value: '24', label: '24-hour (21:30)' }]} onChange={(v) => header({ hour12: v === '12' })} />
        </Field>
        <Field label="Logo" hint="Optional. A picture at the left of the heading; a transparent png works best.">
          <PictureField value={g.header.logo} onChange={(logo) => header({ logo })} />
        </Field>
      </Card>

      <Card className="stack">
        <h2>Ticker</h2>
        <Toggle checked={g.ticker_options.enabled} onChange={(v) => ticker({ enabled: v })} label={<span>Scrolling line along the bottom <span className="dim small">turn it off to give the listing more room</span></span>} />
        {g.ticker_options.enabled && (
          <div className="stack nest">
            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <Field label="Where"><Segmented value={g.look.ticker_position} options={[{ value: 'bottom', label: 'Bottom' }, { value: 'top', label: 'Top' }]} onChange={(ticker_position) => look({ ticker_position })} /></Field>
              <Field label="Speed"><Segmented value={g.ticker_options.speed} options={[{ value: 'slow', label: 'Slow' }, { value: 'normal', label: 'Normal' }, { value: 'fast', label: 'Fast' }]} onChange={(speed) => ticker({ speed })} /></Field>
            </div>
            <Toggle checked={g.ticker_options.now_on} onChange={(v) => ticker({ now_on: v })} label={<span>Announce what is on now <span className="dim small">“NOW ON 4: Cheers”, after your own lines</span></span>} />
            <Field label="Your lines" hint="One per line: promos, announcements.">
              <textarea className="input" value={g.ticker.join('\n')} onChange={(e) => set({ ticker: e.target.value.split('\n').filter((l) => l.trim() !== '') })} />
            </Field>
          </div>
        )}
      </Card>

      <Card className="stack">
        <h2>Video window</h2>
        <p className="small dim">A live picture-in-picture of one of your channels inside the guide, with a card saying what is on. The guide starts that channel while it is showing it.</p>
        <Field label="Show">
          <Segmented
            value={g.pip.mode}
            options={[{ value: 'off', label: 'Nothing' }, { value: 'channel', label: 'One channel' }, { value: 'rotate', label: 'Flip through channels' }]}
            onChange={(mode) => pip({ mode })}
          />
        </Field>
        {g.pip.mode === 'channel' && (
          <Field label="Channel">
            <select className="select" style={{ minWidth: 220 }} value={g.pip.channel ?? ''} onChange={(e) => pip({ channel: e.target.value || null })}>
              <option value="">Choose a channel…</option>
              {showable.map((c) => <option key={c.id} value={c.id}>{channelLabel(c)}</option>)}
            </select>
          </Field>
        )}
        {g.pip.mode === 'rotate' && (
          <>
            <Field label={`Each channel for ${g.pip.seconds} s`}>
              <input type="range" min={10} max={300} step={5} value={g.pip.seconds} onChange={(e) => pip({ seconds: Number(e.target.value) })} />
            </Field>
            <Field label="Which channels" hint={g.pip.channels.length ? 'Only these, in this order.' : 'Every channel in the guide.'}>
              <Chips options={showable.map((c) => ({ value: c.id, label: channelLabel(c) }))} value={g.pip.channels} onToggle={(cid) => pip({ channels: toggleId(g.pip.channels, cid) })} />
            </Field>
          </>
        )}
        {g.pip.mode !== 'off' && (
          <div className="stack nest">
            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <Field label="Where">
                <select className="select" value={g.pip.placement} onChange={(e) => pip({ placement: e.target.value as GuideConfig['pip']['placement'] })}>
                  {PIP_PLACEMENTS.map((p) => <option key={p.value} value={p.value}>{p.label}</option>)}
                </select>
              </Field>
              <Field label="Size"><Segmented value={g.pip.size} options={[{ value: 'small', label: 'Small' }, { value: 'medium', label: 'Medium' }, { value: 'large', label: 'Large' }]} onChange={(size) => pip({ size })} /></Field>
              <Field label="Sound" hint={g.pip.audio === 'music' ? 'The guide’s music keeps playing.' : 'You hear the channel in the window.'}>
                <Segmented value={g.pip.audio} options={[{ value: 'music', label: 'Guide music' }, { value: 'channel', label: 'The channel' }]} onChange={(audio) => pip({ audio })} />
              </Field>
            </div>
            <Toggle checked={g.pip.info} onChange={(v) => pip({ info: v })} label="A card beside the window saying what is on" />
          </div>
        )}
      </Card>

      <Card className="stack">
        <h2>Music</h2>
        <Field label="Background music" hint={musicLibs.length ? 'Loops under the guide.' : 'Add a library of kind "Music" to play music under the guide.'}>
          <Chips
            options={musicLibs.map((l) => ({ value: l.id, label: `${l.path.split(/[\\/]/).filter(Boolean).pop() ?? l.path} (${l.items})` }))}
            value={g.music_library_ids}
            onToggle={(lid) => set({ music_library_ids: g.music_library_ids.includes(lid) ? g.music_library_ids.filter((x) => x !== lid) : [...g.music_library_ids, lid] })}
          />
        </Field>
      </Card>
    </div>
  );
}
