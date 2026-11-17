import { useEffect, useRef, useState } from 'react';
import { api, errorMessage } from '../api';
import { PictureField } from '../PictureField';
import { useQuery } from '../live';
import { StylePicker, ThemePicker, emptyChoice, useThemeCatalogue } from '../ThemePicker';
import type { Library, PromoDraft, PromoOptions, PromoSpot } from '../types';
import { Btn, Card, ConfirmBtn, Field, Segmented, Spinner, Toggle, useToast } from '../ui';

const blank = (): PromoDraft => ({ kind: 'sponsor', title: '', tagline: '', contact: '', slides: [''], seconds: 15, picture: null, theme: null, look: 'none', sound: 'jingle', music: null, year: null, name: '' });

/** A picture of the spot, redrawn a moment after each change. */
function usePreview(d: PromoDraft) {
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const last = useRef<string | null>(null);
  const key = JSON.stringify(d);
  useEffect(() => {
    let live = true;
    const timer = setTimeout(async () => {
      try {
        const res = await fetch('/api/v1/promos/preview', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ ...d, slides: d.slides.filter((s) => s.trim()), music: d.music, look: d.look === 'none' ? null : d.look }), credentials: 'same-origin' });
        if (!res.ok) {
          const j = (await res.json().catch(() => ({}))) as { error?: string };
          if (live) setError(j.error ?? `${res.status}`);
          return;
        }
        const next = URL.createObjectURL(await res.blob());
        if (!live) return;
        if (last.current) URL.revokeObjectURL(last.current);
        last.current = next;
        setUrl(next);
        setError(null);
      } catch { /* a convenience */ }
    }, 350);
    return () => { live = false; clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return { url, error };
}

/** Make your own commercials and promos, or bring in old ones. */
export function Promos() {
  const opts = useQuery(() => api.get<PromoOptions>('/promos/options'));
  const spots = useQuery(() => api.get<PromoSpot[]>('/promos'));
  const music = useQuery(() => api.get<{ path: string; name: string; seconds: number }[]>('/promos/music'));
  const libs = useQuery(() => api.get<Library[]>('/libraries'));
  const station = useThemeCatalogue()?.station;
  const { say } = useToast();
  const [d, setD] = useState<PromoDraft>(blank());
  const [busy, setBusy] = useState(false);
  const [own, setOwn] = useState(false);
  const [url, setUrl] = useState('');
  const [lib, setLib] = useState<number | null>(null);
  const set = (p: Partial<PromoDraft>) => setD({ ...d, ...p });
  const prev = usePreview(own ? d : { ...d, theme: null });
  const kind = opts.data?.kinds.find((k) => k.id === d.kind);
  const commercial = (libs.data ?? []).filter((l) => l.kind === 'commercials');
  const ready = d.kind === 'slides' ? d.slides.some((s) => s.trim()) : d.title.trim().length > 0;

  const make = async () => {
    setBusy(true);
    try {
      const body = { ...d, slides: d.slides.filter((s) => s.trim()), look: d.look === 'none' ? null : d.look, theme: own ? d.theme ?? station ?? emptyChoice() : null };
      await api.post('/promos', body);
      say('Made. It is in your commercials and will air in breaks.');
      spots.reload();
      libs.reload();
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };
  const fetchIt = async () => {
    const target = lib ?? commercial[0]?.id;
    if (!target) return say('Add a commercials library first (Libraries)', true);
    setBusy(true);
    try {
      await api.post(`/libraries/${target}/fetch`, { url });
      say('Downloaded and added to the commercials.');
      setUrl('');
      libs.reload();
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="page stack gap-lg">
      <div className="page-head"><h1>Promos</h1><span className="sub">Make your own commercials and promos, or use old ones</span></div>

      <Card className="stack">
        <h2>Make a spot</h2>
        <Segmented value={d.kind} options={(opts.data?.kinds ?? []).map((k) => ({ value: k.id, label: k.name }))} onChange={(k) => set({ kind: k })} />
        {kind && <p className="small dim">{kind.about}</p>}
        <div className="row wrap" style={{ alignItems: 'flex-start', gap: 20 }}>
          <div className="stack grow" style={{ minWidth: 280, flex: '1 1 320px' }}>
            {d.kind !== 'slides' ? (
              <>
                <Field label={d.kind === 'coming_up' ? 'Programme' : 'Name'}><input className="input" maxLength={80} value={d.title} onChange={(e) => set({ title: e.target.value })} placeholder={d.kind === 'coming_up' ? 'Cheers' : "Mel's Diner"} /></Field>
                <Field label={d.kind === 'coming_up' ? 'When' : 'Tagline'}><input className="input" maxLength={120} value={d.tagline} onChange={(e) => set({ tagline: e.target.value })} placeholder={d.kind === 'coming_up' ? 'Thursdays at 8' : 'Best pie in town'} /></Field>
                <Field label={d.kind === 'coming_up' ? 'Small print (optional)' : 'Contact line (optional)'}><input className="input" maxLength={100} value={d.contact} onChange={(e) => set({ contact: e.target.value })} placeholder="Call 555-0134 · open daily" /></Field>
              </>
            ) : (
              <>
                <Field label="Heading (optional)"><input className="input" maxLength={80} value={d.title} onChange={(e) => set({ title: e.target.value })} /></Field>
                <Field label="Slides" hint="One line per slide.">
                  <textarea className="input" rows={5} value={d.slides.join('\n')} onChange={(e) => set({ slides: e.target.value.split('\n').slice(0, 10) })} />
                </Field>
              </>
            )}
            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <Field label="Length"><Segmented value={String(d.seconds)} options={(opts.data?.seconds ?? [15, 30]).map((n) => ({ value: String(n), label: `${n} s` }))} onChange={(v) => set({ seconds: Number(v) })} /></Field>
              <Field label="Year" hint="Optional. Lets a channel set in one era use it."><input className="input num" type="number" min={1900} max={2100} placeholder="1987" value={d.year ?? ''} onChange={(e) => set({ year: e.target.value ? Number(e.target.value) : null })} /></Field>
            </div>
            <Field label="Picture behind the words (optional)"><PictureField value={d.picture} onChange={(picture) => set({ picture })} /></Field>
            <Field label="Make it look old" hint="Tape noise, a tube TV, old film, black and white or sepia.">
              <Segmented value={d.look} options={(opts.data?.looks ?? []).map((l) => ({ value: l.id, label: l.name }))} onChange={(look) => set({ look })} />
            </Field>
            <Field label="Sound">
              <Segmented value={d.sound} options={(opts.data?.sounds ?? []).filter((s) => s.id !== 'music' || (music.data?.length ?? 0) > 0).map((s) => ({ value: s.id, label: s.name }))} onChange={(sound) => set({ sound })} />
            </Field>
            {d.sound === 'music' && (
              <Field label="Music">
                <select className="select" value={d.music ?? ''} onChange={(e) => set({ music: e.target.value || null })}>
                  <option value="">Choose…</option>
                  {(music.data ?? []).map((m) => <option key={m.path} value={m.path}>{m.name} ({m.seconds}s)</option>)}
                </select>
              </Field>
            )}
            <Toggle checked={own} onChange={(v) => { setOwn(v); if (v && !d.theme) set({ theme: station ?? emptyChoice() }); }} label={<span>Its own look <span className="dim small">(otherwise the station's theme)</span></span>} />
            {own && d.theme && (
              <div className="stack nest">
                <ThemePicker value={d.theme} onChange={(v) => set({ theme: { ...d.theme!, ...v } })} autoName="From the title" autoAbout="Colours made from the title." />
                <StylePicker value={d.theme.style} onChange={(style) => set({ theme: { ...d.theme!, style } })} />
              </div>
            )}
          </div>
          <div className="stack" style={{ flex: '1 1 360px', minWidth: 280 }}>
            {prev.url ? <img src={prev.url} alt="What the spot looks like" style={{ width: '100%', borderRadius: 8, aspectRatio: '16 / 9', background: '#000', imageRendering: 'auto' }} /> : <div className="empty"><Spinner /></div>}
            {prev.error && <p className="small" style={{ color: 'var(--warn)' }}>{prev.error}</p>}
            <Btn kind="primary" disabled={busy || !ready} onClick={make}>{busy ? <Spinner /> : `Make the ${d.seconds}-second spot`}</Btn>
            <p className="small dim">It is saved as a video in your commercials (Station promos), so it airs in breaks like any commercial. Channels that limit ads to certain years use it only if you gave it a year.</p>
          </div>
        </div>
      </Card>

      <Card className="stack">
        <h2>Made here</h2>
        {spots.loading && !spots.data ? <Spinner /> : (spots.data ?? []).length === 0 ? <p className="small dim">Nothing yet.</p> : (
          <div className="list">
            {spots.data!.map((p) => (
              <div key={p.id} className="row">
                <span className="grow ellipsis">{p.name}</span><span className="dim small">{p.seconds} s</span>
                <ConfirmBtn small kind="danger" icon="trash" label="" aria-label="Delete spot" confirm="Delete this spot? It leaves the schedule too." onConfirm={() => api.del(`/promos/${p.id}`).then(() => spots.reload()).catch((e) => say(errorMessage(e), true))} />
              </div>
            ))}
          </div>
        )}
      </Card>

      <Card className="stack">
        <h2>Use old commercials</h2>
        <p className="small dim">
          Bring in real ones from the era: upload files from your own tapes and downloads (Libraries → your commercials library → Upload), or give the server a direct link to a video file and it
          downloads it. The Prelinger Archives and the Internet Archive have public-domain commercials and station breaks from the 1950s to the 80s (use the file's direct download link).
          To make anything look like it came off a tape, set the channel's Look to VHS or Tube TV under Sound &amp; picture. Put a year in the folder name, such as <span className="mono">Commercials/1986/</span>, to let channels pick an era.
        </p>
        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          <Field label="Direct link to a video file"><input className="input" style={{ width: 360 }} placeholder="https://…/commercial.mp4" value={url} onChange={(e) => setUrl(e.target.value)} /></Field>
          {commercial.length > 1 && (
            <Field label="Into"><select className="select" value={lib ?? commercial[0].id} onChange={(e) => setLib(Number(e.target.value))}>{commercial.map((l) => <option key={l.id} value={l.id}>{l.path.split(/[\\/]/).filter(Boolean).pop()}</option>)}</select></Field>
          )}
          <Btn disabled={busy || !/^https?:\/\//i.test(url.trim())} onClick={fetchIt}>{busy ? <Spinner /> : 'Download it'}</Btn>
        </div>
        {commercial.length === 0 && <p className="small" style={{ color: 'var(--warn)' }}>There is no commercials library yet. Add one, or create the station media folder, on the Libraries page.</p>}
      </Card>
    </div>
  );
}
