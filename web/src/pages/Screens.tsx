import { useEffect, useMemo, useRef, useState } from 'react';
import { api } from '../api';
import { useQuery } from '../live';
import { PictureField } from '../PictureField';
import {
  addLayer, COLOUR_ROLES, colourToHex, ENTERS, hasMotion, isRole, moveLayer, MOTIONS, motionSpeed, presetFor, removeLayer, sameDesign,
  setLayer, setMotionSpeed, stillDesign, textLayers, WORD_HELP,
  type Backdrop, type Design, type Layer, type LayerType, type ScreenKind, type ScreensState,
} from '../screens';
import type { ChannelView } from '../types';
import { Badge, Btn, Card, ConfirmBtn, Field, More, Segmented, Spinner, Toggle, useToast } from '../ui';

// ---------- small pieces ----------

function ColourField({ value, onChange, label }: { value: string; onChange: (c: string) => void; label?: string }) {
  const custom = !isRole(value);
  return (
    <span className="row" style={{ gap: 6 }}>
      <select className="select" aria-label={label ?? 'Colour'} value={custom ? '#custom' : value} onChange={(e) => onChange(e.target.value === '#custom' ? colourToHex(value, '#ff8800') : e.target.value)}>
        {COLOUR_ROLES.map((r) => <option key={r.value} value={r.value}>{r.label}</option>)}
        <option value="#custom">A colour of my own…</option>
      </select>
      {custom && <input type="color" aria-label="Pick a colour" value={colourToHex(value)} onChange={(e) => onChange(e.target.value)} />}
    </span>
  );
}

const num = (v: string, min: number, max: number, fallback: number) => {
  const n = Number(v);
  return Number.isFinite(n) ? Math.max(min, Math.min(max, Math.round(n))) : fallback;
};

function NumField({ label, value, onChange, min = 0, max = 2000, step = 1, hint }: { label: string; value: number; onChange: (n: number) => void; min?: number; max?: number; step?: number; hint?: string }) {
  return (
    <Field label={label} hint={hint}>
      <input className="input num" type="number" min={min} max={max} step={step} value={value} onChange={(e) => onChange(num(e.target.value, min, max, value))} />
    </Field>
  );
}

/** A picture of the design at one moment, redrawn a moment after each change. */
function Frame({ kind, design, channel, t }: { kind: string; design: Design | null; channel: string | null; t: number }) {
  const [url, setUrl] = useState<string | null>(null);
  const last = useRef<string | null>(null);
  const busy = useRef(false);
  const key = JSON.stringify({ kind, design, channel, t });
  useEffect(() => {
    let live = true;
    const timer = setTimeout(async () => {
      if (busy.current) return;
      busy.current = true;
      try {
        const res = await fetch('/api/v1/screens/preview', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ kind, design, t, channel }),
          credentials: 'same-origin',
        });
        if (!res.ok || !live) return;
        const next = URL.createObjectURL(await res.blob());
        if (last.current) URL.revokeObjectURL(last.current);
        last.current = next;
        setUrl(next);
      } catch {
        /* a convenience */
      } finally {
        busy.current = false;
      }
    }, t === 0 ? 150 : 40);
    return () => { live = false; clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return url ? <img src={url} alt="" style={{ width: '100%', display: 'block', aspectRatio: '16 / 9', background: '#000', borderRadius: 8 }} /> : <div className="empty" style={{ aspectRatio: '16 / 9' }}><Spinner /></div>;
}

/** The big preview with play and a scrubber. */
function Player({ kind, design, channel, seconds }: { kind: string; design: Design | null; channel: string | null; seconds: number }) {
  const length = seconds || 10;
  const [t, setT] = useState(2.5);
  const [playing, setPlaying] = useState(false);
  useEffect(() => {
    if (!playing) return;
    const id = setInterval(() => setT((x) => (x + 0.25 > length ? 0 : x + 0.25)), 250);
    return () => clearInterval(id);
  }, [playing, length]);
  return (
    <div className="stack" style={{ gap: 8 }}>
      <Frame kind={kind} design={design} channel={channel} t={t} />
      <div className="row" style={{ gap: 10 }}>
        <Btn small onClick={() => { if (!playing && t >= length - 0.3) setT(0); setPlaying(!playing); }}>{playing ? 'Pause' : 'Play'}</Btn>
        <input type="range" className="grow" min={0} max={length} step={0.25} value={t} aria-label="Moment in the animation" onChange={(e) => { setPlaying(false); setT(Number(e.target.value)); }} />
        <span className="small dim mono" style={{ minWidth: 54 }}>{t.toFixed(1)}s</span>
      </div>
      {!seconds && <p className="small dim">This screen stays up for as long as it is needed, so its entrance plays once and any motion carries on.</p>}
    </div>
  );
}

// ---------- the background ----------

const BACKS: { value: Backdrop['type']; label: string }[] = [
  { value: 'theme', label: 'Station theme' },
  { value: 'aurora', label: 'Soft colour' },
  { value: 'gradient', label: 'Gradient' },
  { value: 'solid', label: 'One colour' },
  { value: 'pattern', label: 'Pattern' },
  { value: 'bars', label: 'Colour bars' },
  { value: 'picture', label: 'My picture' },
];

function BackdropEditor({ value, onChange }: { value: Backdrop; onChange: (b: Backdrop) => void }) {
  const set = (type: Backdrop['type']) => {
    if (type === value.type) return;
    switch (type) {
      case 'theme': return onChange({ type });
      case 'bars': return onChange({ type });
      case 'solid': return onChange({ type, color: 'dark' });
      case 'gradient': return onChange({ type, top: 'primary', bottom: 'dark' });
      case 'aurora': return onChange({ type, colors: [] });
      case 'pattern': return onChange({ type, kind: 'dots', color: 'primary', bg: 'dark', size: 48 });
      case 'picture': return onChange({ type, path: '' });
    }
  };
  return (
    <div className="stack" style={{ gap: 8 }}>
      <Segmented value={value.type} options={BACKS} onChange={set} />
      {value.type === 'solid' && <Field label="Colour"><ColourField value={value.color} onChange={(color) => onChange({ ...value, color })} /></Field>}
      {value.type === 'gradient' && (
        <div className="row wrap">
          <Field label="Top"><ColourField value={value.top} onChange={(top) => onChange({ ...value, top })} /></Field>
          <Field label="Bottom"><ColourField value={value.bottom} onChange={(bottom) => onChange({ ...value, bottom })} /></Field>
        </div>
      )}
      {value.type === 'aurora' && (
        <Field label="Colours" hint="Blank = the station theme's colours. Up to five.">
          <div className="row wrap">
            {value.colors.map((c, i) => (
              <span key={i} className="row" style={{ gap: 4 }}>
                <ColourField value={c} onChange={(nc) => onChange({ ...value, colors: value.colors.map((x, j) => (j === i ? nc : x)) })} />
                <Btn small kind="ghost" icon="x" aria-label="Remove colour" onClick={() => onChange({ ...value, colors: value.colors.filter((_, j) => j !== i) })} />
              </span>
            ))}
            {value.colors.length < 5 && <Btn small icon="plus" onClick={() => onChange({ ...value, colors: [...value.colors, 'accent'] })}>Add a colour</Btn>}
          </div>
        </Field>
      )}
      {value.type === 'pattern' && (
        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          <Field label="Pattern"><Segmented value={value.kind} options={[{ value: 'dots', label: 'Dots' }, { value: 'grid', label: 'Grid' }, { value: 'stripes', label: 'Stripes' }]} onChange={(kind) => onChange({ ...value, kind })} /></Field>
          <Field label="Pattern colour"><ColourField value={value.color} onChange={(color) => onChange({ ...value, color })} /></Field>
          <Field label="Behind it"><ColourField value={value.bg} onChange={(bg) => onChange({ ...value, bg })} /></Field>
          <NumField label="Size" value={value.size} min={8} max={200} onChange={(size) => onChange({ ...value, size })} />
        </div>
      )}
      {value.type === 'picture' && (
        <Field label="Picture" hint="Fills the screen, cropped to fit. It is uploaded to the server.">
          <PictureField value={value.path || null} onChange={(path) => onChange({ ...value, path: path ?? '' })} />
        </Field>
      )}
    </div>
  );
}

// ---------- layers ----------

const LAYER_NAMES: Record<LayerType, string> = { text: 'Words', clock: 'Clock', shape: 'Shape', list: 'Programme list', image: 'Picture' };

function layerTitle(l: Layer): string {
  if (l.type === 'text') return `Words: ${l.text.slice(0, 28) || '(empty)'}`;
  if (l.type === 'shape') return l.glass ? 'Shape: frosted glass' : 'Shape';
  return LAYER_NAMES[l.type];
}

function LayerEditor({ layer, onChange, onRemove, onUp, onDown }: { layer: Layer; onChange: (p: Partial<Layer>) => void; onRemove: () => void; onUp: () => void; onDown: () => void }) {
  const a = layer.anim;
  const setAnim = (p: Partial<Layer['anim']>) => onChange({ anim: { ...a, ...p } } as Partial<Layer>);
  const c = (p: Record<string, unknown>) => onChange(p as Partial<Layer>);
  return (
    <details className="block-card">
      <summary className="row between" style={{ cursor: 'pointer' }}>
        <strong>{layerTitle(layer)}</strong>
        <span className="row" onClick={(e) => e.preventDefault()}>
          <Btn small kind="ghost" aria-label="Move up (behind)" onClick={onUp}>▲</Btn>
          <Btn small kind="ghost" aria-label="Move down (in front)" onClick={onDown}>▼</Btn>
          <Btn small kind="ghost" icon="trash" aria-label="Remove this layer" onClick={onRemove} />
        </span>
      </summary>
      <div className="stack" style={{ marginTop: 10 }}>
        {layer.type === 'text' && (
          <>
            <Field label="Words" hint="You can use {channel} {number} {show} {title} {time} {message} {clock} {date}."><input className="input" value={layer.text} onChange={(e) => c({ text: e.target.value })} /></Field>
            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <NumField label="Size" value={layer.size} min={8} max={400} onChange={(size) => c({ size })} />
              <Field label="Colour"><ColourField value={layer.color} onChange={(color) => c({ color })} /></Field>
              <NumField label="Widest (‰ of the screen)" value={layer.width} min={50} max={1000} step={10} onChange={(width) => c({ width })} />
              <NumField label="Lines" value={layer.lines} min={1} max={6} onChange={(lines) => c({ lines })} />
            </div>
            <div className="row wrap">
              <Toggle checked={layer.bold} onChange={(bold) => c({ bold })} label="Bold" />
              <Toggle checked={layer.pixel} onChange={(pixel) => c({ pixel })} label="Chunky pixel letters" />
              <Toggle checked={layer.shadow} onChange={(shadow) => c({ shadow })} label="Shadow" />
            </div>
          </>
        )}
        {layer.type === 'clock' && (
          <div className="row wrap" style={{ alignItems: 'flex-end' }}>
            <NumField label="Size" value={layer.size} min={8} max={400} onChange={(size) => c({ size })} />
            <Field label="Colour"><ColourField value={layer.color} onChange={(color) => c({ color })} /></Field>
            <Toggle checked={layer.bold} onChange={(bold) => c({ bold })} label="Bold" />
            <Toggle checked={layer.hour12} onChange={(hour12) => c({ hour12 })} label="12-hour" />
            <Toggle checked={layer.seconds} onChange={(seconds) => c({ seconds })} label="Seconds" />
            <Toggle checked={layer.date} onChange={(date) => c({ date })} label="Date underneath" />
          </div>
        )}
        {layer.type === 'shape' && (
          <>
            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <NumField label="Width (‰ of the screen)" value={layer.w} min={1} max={2000} onChange={(w) => c({ w })} />
              <NumField label="Height (‰ of the screen)" value={layer.h} min={1} max={2000} onChange={(h) => c({ h })} />
              <NumField label="Rounded corners (px)" value={layer.radius} min={0} max={400} onChange={(radius) => c({ radius })} />
            </div>
            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <Toggle checked={layer.glass} onChange={(glass) => c({ glass })} label={<span>Frosted glass <span className="dim small">blurs what is behind it</span></span>} />
              <Toggle checked={layer.circle} onChange={(circle) => c({ circle })} label="Oval / circle" />
            </div>
            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <Field label={layer.glass ? 'Tint' : 'Colour'}><ColourField value={layer.color} onChange={(color) => c({ color })} /></Field>
              {!layer.glass && (
                <Field label="Fades to" hint="Top to bottom.">
                  <span className="row" style={{ gap: 6 }}>
                    <Toggle checked={layer.color2 !== null} onChange={(on) => c({ color2: on ? 'dark' : null })} label="" />
                    {layer.color2 !== null && <ColourField value={layer.color2} onChange={(color2) => c({ color2 })} />}
                  </span>
                </Field>
              )}
            </div>
          </>
        )}
        {layer.type === 'list' && (
          <div className="row wrap" style={{ alignItems: 'flex-end' }}>
            <NumField label="Size" value={layer.size} min={8} max={400} onChange={(size) => c({ size })} />
            <NumField label="Lines" value={layer.rows} min={1} max={8} onChange={(rows) => c({ rows })} />
            <NumField label="Widest (‰ of the screen)" value={layer.width} min={100} max={1000} step={10} onChange={(width) => c({ width })} />
            <Field label="Titles"><ColourField value={layer.color} onChange={(color) => c({ color })} /></Field>
            <Field label="Times"><ColourField value={layer.time_color} onChange={(time_color) => c({ time_color })} /></Field>
          </div>
        )}
        {layer.type === 'image' && (
          <>
            <Field label="Picture" hint="A logo or photo. See-through parts of a png stay see-through."><PictureField value={layer.path || null} onChange={(path) => c({ path: path ?? '' })} /></Field>
            <div className="row wrap">
              <NumField label="Width (‰ of the screen)" value={layer.w} min={1} max={2000} onChange={(w) => c({ w })} />
              <NumField label="Height (‰ of the screen)" value={layer.h} min={1} max={2000} onChange={(h) => c({ h })} />
            </div>
          </>
        )}

        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          <NumField label="Across (‰)" value={layer.x} min={-500} max={1500} onChange={(x) => c({ x })} hint="0 = left edge, 1000 = right." />
          <NumField label="Down (‰)" value={layer.y} min={-500} max={1500} onChange={(y) => c({ y })} hint="0 = top, 1000 = bottom." />
          <Field label="Anchored at"><Segmented value={layer.align} options={[{ value: 'left', label: 'Its left' }, { value: 'center', label: 'Middle' }, { value: 'right', label: 'Its right' }]} onChange={(align) => c({ align })} /></Field>
          <NumField label="Opacity %" value={layer.opacity} min={0} max={100} onChange={(opacity) => c({ opacity })} />
        </div>
        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          <Field label="Arrives"><select className="select" value={a.enter} onChange={(e) => setAnim({ enter: e.target.value as typeof a.enter })}>{ENTERS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}</select></Field>
          <NumField label="Takes (ms)" value={a.enter_ms} min={0} max={20000} step={50} onChange={(enter_ms) => setAnim({ enter_ms })} />
          <NumField label="After (ms)" value={a.delay_ms} min={0} max={60000} step={50} onChange={(delay_ms) => setAnim({ delay_ms })} />
          <Field label="Then"><select className="select" value={a.motion} onChange={(e) => setAnim({ motion: e.target.value as typeof a.motion })}>{MOTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}</select></Field>
          <NumField label="Speed %" value={a.speed} min={10} max={500} step={10} onChange={(speed) => setAnim({ speed })} />
        </div>
      </div>
    </details>
  );
}

// ---------- one screen ----------

function ScreenEditor({ kind, channel, onSaved }: { kind: ScreenKind; channel: string | null; onSaved: () => void }) {
  const { attempt } = useToast();
  const initial = kind.current?.design ?? null;
  const [design, setDesign] = useState<Design | null>(initial);
  const [preset, setPreset] = useState(presetFor(kind));
  const [json, setJson] = useState<string | null>(null);
  const [jsonErr, setJsonErr] = useState<string | null>(null);
  const q = channel ? `?channel=${encodeURIComponent(channel)}` : '';
  // Another screen, or the same one reloaded after saving: start from what is saved.
  useEffect(() => {
    setDesign(kind.current?.design ?? null);
    setPreset(presetFor(kind));
    setJson(null);
  }, [kind.id, kind.current ? JSON.stringify(kind.current) : '', channel]); // eslint-disable-line react-hooks/exhaustive-deps
  const saved = kind.current?.design ?? null;
  const dirty = (design === null) !== (saved === null) || (design !== null && saved !== null && !sameDesign(design, saved));
  const pick = (p: { id: string; design: Design }) => { setDesign(p.design); setPreset(p.id); };
  const edit = (d: Design) => { setDesign(d); setPreset('custom'); };
  const save = () => attempt(async () => {
    if (design === null) await api.del(`/screens/${kind.id}${q}`);
    else await api.put(`/screens/${kind.id}${q}`, { preset, design });
    onSaved();
  }, 'Saved. Anything on air picks it up now');
  const words = design ? textLayers(design) : [];
  const where = channel ? 'this channel' : 'the whole station';

  return (
    <div className="stack gap-lg">
      <Card className="stack">
        <div className="row between wrap">
          <div>
            <h2 style={{ margin: 0 }}>{kind.name}</h2>
            <p className="small dim" style={{ margin: '4px 0 0' }}>{kind.about}</p>
          </div>
          <span>
            {kind.source === 'channel' && <Badge tone="accent">This channel's own</Badge>}
            {kind.source === 'station' && channel && <Badge>Follows the station</Badge>}
            {kind.source === 'none' && <Badge>Classic</Badge>}
          </span>
        </div>
        <Player kind={kind.id} design={design} channel={channel} seconds={kind.seconds} />
        {design === null && <p className="small dim">This is the classic screen, as it airs now. It follows the station’s style (chunky retro or smooth modern) and can’t be edited: choose a design below to try one, then change it.</p>}
        <div className="row wrap">
          <Btn kind="primary" disabled={!dirty} onClick={save}>Use this on {where}</Btn>
          {dirty && <Btn onClick={() => { setDesign(saved); setPreset(presetFor(kind)); }}>Undo changes</Btn>}
          {channel && kind.source === 'channel' && <ConfirmBtn label="Follow the station instead" confirm="This channel goes back to the station's design for this screen." onConfirm={() => attempt(async () => { await api.del(`/screens/${kind.id}${q}`); onSaved(); }, 'Following the station')} />}
          {!channel && kind.source === 'station' && <ConfirmBtn label="Back to the classic screen" confirm="The station goes back to its classic screen for this (channels with their own design keep it)." onConfirm={() => attempt(async () => { await api.del(`/screens/${kind.id}`); onSaved(); }, 'Back to the classic screen')} />}
        </div>
      </Card>

      <Card className="stack">
        <h2>Choose a design</h2>
        <div className="gallery">
          <button type="button" className={`tile ${design === null ? 'on' : ''}`} onClick={() => { setDesign(null); setPreset('classic'); }}>
            <Frame kind={kind.id} design={null} channel={channel} t={3.2} />
            <strong>Classic</strong>
            <span className="small dim">The built-in screen, as it airs now.</span>
          </button>
          {kind.presets.map((p) => (
            <button type="button" key={p.id} className={`tile ${design && preset === p.id ? 'on' : ''}`} onClick={() => pick(p)}>
              <Frame kind={kind.id} design={p.design} channel={channel} t={3.2} />
              <strong>{p.name}</strong>
              <span className="small dim">{p.about}</span>
            </button>
          ))}
        </div>
      </Card>

      {design && (
        <Card className="stack">
          <h2>Make it yours</h2>
          {words.length > 0 && (
            <div className="stack" style={{ gap: 8 }}>
              <strong>Words</strong>
              {words.map(({ index, layer }) => (
                <div key={index} className="row wrap" style={{ alignItems: 'center' }}>
                  <input className="input grow" style={{ minWidth: 220 }} aria-label="Words" value={layer.text} onChange={(e) => edit(setLayer(design, index, { text: e.target.value } as Partial<Layer>))} />
                  <ColourField value={layer.color} label="Colour of these words" onChange={(color) => edit(setLayer(design, index, { color } as Partial<Layer>))} />
                  <input type="range" min={16} max={220} value={layer.size} aria-label="Size" title="Size" onChange={(e) => edit(setLayer(design, index, { size: Number(e.target.value) } as Partial<Layer>))} />
                </div>
              ))}
              <p className="small dim">Fill-ins: {kind.words.map((w) => `{${w}} (${WORD_HELP[w] ?? w})`).join(', ')}.</p>
            </div>
          )}
          <div className="stack" style={{ gap: 6 }}>
            <strong>Background</strong>
            <BackdropEditor value={design.backdrop} onChange={(backdrop) => edit({ ...design, backdrop })} />
          </div>
          <div className="stack" style={{ gap: 6 }}>
            <strong>Motion</strong>
            <Field label={`Speed ${motionSpeed(design)}%`} hint="How fast the movement goes.">
              <input type="range" min={20} max={300} step={10} value={motionSpeed(design)} onChange={(e) => edit(setMotionSpeed(design, Number(e.target.value)))} />
            </Field>
            {hasMotion(design) && <div><Btn small onClick={() => edit(stillDesign(design))}>Keep it still after it arrives</Btn></div>}
          </div>

          <More summary="every layer, fading out, the design as text">
            <div className="stack">
              <strong>Layers</strong>
              <p className="small dim">The bottom of the list is in front. Open a layer to change where it sits, how it arrives and what it keeps doing.</p>
              {design.layers.map((l, i) => (
                <LayerEditor key={i} layer={l} onChange={(p) => edit(setLayer(design, i, p))} onRemove={() => edit(removeLayer(design, i))} onUp={() => edit(moveLayer(design, i, -1))} onDown={() => edit(moveLayer(design, i, 1))} />
              ))}
              <div className="row wrap">
                {(['text', 'shape', 'clock', 'list', 'image'] as LayerType[]).map((t) => <Btn key={t} small icon="plus" onClick={() => edit(addLayer(design, t))}>{LAYER_NAMES[t]}</Btn>)}
              </div>
            </div>
            {kind.seconds > 0 && (
              <NumField label="Fade everything out over (ms)" value={design.fade_out_ms} min={0} max={5000} step={50} hint="At the end of the screen. 0 = it just cuts." onChange={(fade_out_ms) => edit({ ...design, fade_out_ms })} />
            )}
            <div className="stack">
              <div className="row between">
                <strong>The design as text (JSON)</strong>
                <Btn small onClick={() => { setJson(json === null ? JSON.stringify(design, null, 2) : null); setJsonErr(null); }}>{json === null ? 'Show' : 'Hide'}</Btn>
              </div>
              {json !== null && (
                <>
                  <textarea className="input mono" style={{ minHeight: 240 }} value={json} onChange={(e) => setJson(e.target.value)} spellCheck={false} />
                  {jsonErr && <div className="err-banner">{jsonErr}</div>}
                  <div><Btn onClick={() => { try { edit(JSON.parse(json) as Design); setJsonErr(null); } catch (e) { setJsonErr(`Not valid JSON: ${(e as Error).message}`); } }}>Apply</Btn></div>
                </>
              )}
            </div>
          </More>
        </Card>
      )}
    </div>
  );
}

// ---------- the page ----------

export function Screens({ kindId }: { kindId?: string }) {
  const [channel, setChannel] = useState<string | null>(null);
  const channels = useQuery(() => api.get<ChannelView[]>('/channels'));
  const state = useQuery(() => api.get<ScreensState>(`/screens${channel ? `?channel=${encodeURIComponent(channel)}` : ''}`), [channel]);
  const [sel, setSel] = useState(kindId ?? 'standby');
  const kinds = state.data?.kinds ?? [];
  const kind = kinds.find((k) => k.id === sel) ?? kinds[0];
  const own = useMemo(() => (channels.data ?? []).filter((c) => c.kind === 'programming' || c.kind === 'test'), [channels.data]);

  return (
    <div className="page">
      <div className="page-head">
        <h1>Screens</h1>
        <span className="sub">Stand-by, sign-off and the screens around breaks</span>
      </div>
      <Card className="row wrap" style={{ alignItems: 'center' }}>
        <strong>Applies to</strong>
        <Segmented value={channel === null ? 'station' : 'channel'} options={[{ value: 'station', label: 'The whole station' }, { value: 'channel', label: 'One channel' }]} onChange={(v) => setChannel(v === 'station' ? null : own[0]?.id ?? null)} />
        {channel !== null && (
          <select className="select" value={channel} onChange={(e) => setChannel(e.target.value)} aria-label="Channel">
            {own.map((c) => <option key={c.id} value={c.id}>{c.number} {c.name}</option>)}
          </select>
        )}
        <span className="small dim">{channel === null ? 'Every channel follows these unless it has its own.' : 'Choose a design for just this channel.'}</span>
      </Card>
      {!state.data ? <Spinner /> : (
        <div className="screens-layout">
          <nav className="screens-list" aria-label="Screens">
            {kinds.map((k) => (
              <button type="button" key={k.id} className={`screens-item ${kind?.id === k.id ? 'on' : ''}`} onClick={() => setSel(k.id)}>
                <strong>{k.name}</strong>
                <span className="small dim">{k.source === 'none' ? 'Classic' : k.current?.design.name ?? 'Designed'}{channel && k.source === 'channel' ? ' · its own' : ''}</span>
              </button>
            ))}
          </nav>
          {kind && <ScreenEditor key={`${kind.id}-${channel ?? ''}`} kind={kind} channel={channel} onSaved={state.reload} />}
        </div>
      )}
    </div>
  );
}
