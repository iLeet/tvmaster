import { useEffect, useState } from 'react';
import { api } from '../api';
import { defaultAds } from '../lib';
import { ThemePreview, emptyChoice, useThemeCatalogue } from '../ThemePicker';
import type { AdConfig, AdOptions, AdPreset, Library, ThemeChoice } from '../types';
import { href } from '../router';
import { Card, Chips, Field, More, Segmented, Toggle } from '../ui';

const name = (l: Library) => l.path.split(/[\\/]/).filter(Boolean).pop() ?? l.path;

/** What these settings do to a typical slot, in words (worked out by the server). */
function useExplain(ads: AdConfig | null, slot: number, show: number) {
  const [text, setText] = useState('');
  const key = JSON.stringify({ ads, slot, show });
  useEffect(() => {
    if (!ads) return;
    let live = true;
    const t = setTimeout(() => {
      api.post<{ text: string }>('/ads/explain', { ads, slot_minutes: slot, show_minutes: show }).then((r) => live && setText(r.text)).catch(() => undefined);
    }, 200);
    return () => { live = false; clearTimeout(t); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return text;
}

export function AdsEditor({ ads, onChange, libraries, theme }: { ads: AdConfig | null; onChange: (a: AdConfig | null) => void; libraries: Library[]; theme?: ThemeChoice | null }) {
  const commercial = libraries.filter((l) => l.kind === 'commercials');
  const clips = libraries.filter((l) => l.kind === 'bumpers');
  const set = (p: Partial<AdConfig>) => ads && onChange({ ...ads, ...p });
  const toggleId = (list: number[], id: number) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);
  const [opts, setOpts] = useState<AdOptions | null>(null);
  const [slot, setSlot] = useState(30);
  const station = useThemeCatalogue()?.station;
  const look = theme ?? station ?? emptyChoice();
  useEffect(() => { api.get<AdOptions>('/ads/options').then(setOpts).catch(() => undefined); }, []);
  const show = Math.max(1, Math.round(slot * 0.73));
  const words = useExplain(ads, slot, show);

  const presets = opts?.presets ?? [];
  const current = !ads ? 'custom' : !ads.commercials ? 'none' : presets.find((p) => p.commercials && p.pod_seconds === ads.pod_seconds && p.max_breaks === ads.max_breaks && p.minutes_per_hour === ads.minutes_per_hour && p.mid_breaks === ads.mid_breaks)?.id ?? 'custom';
  const cardOn = (id: string) => (ads ? { brb: ads.bumpers.brb, back_to: ads.bumpers.back_to, up_next: ads.bumpers.up_next, station_id: ads.bumpers.station_id, later_today: ads.bumpers.later_today_every > 0 }[id] : false);
  const setCard = (id: string, on: boolean) => {
    if (!ads) return;
    if (id === 'later_today') set({ bumpers: { ...ads.bumpers, later_today_every: on ? 3 : 0 } });
    else set({ bumpers: { ...ads.bumpers, [id]: on } });
  };

  const allCommercial = commercial.map((l) => l.id);
  // Commercials come from every commercials library until you choose some.
  useEffect(() => {
    if (ads && ads.commercials && ads.library_ids.length === 0 && allCommercial.length > 0) onChange({ ...ads, library_ids: allCommercial });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ads?.commercials, ads?.library_ids.length, allCommercial.length]);
  const choice = ads === null ? 'clean' : current;
  const choose = (id: string) => {
    if (id === 'clean') { onChange(null); return; }
    const p = presets.find((x) => x.id === id);
    if (!p) return;
    const base = ads ?? { ...defaultAds(), library_ids: allCommercial };
    onChange({ ...base, commercials: p.commercials, pod_seconds: p.pod_seconds, max_breaks: p.max_breaks, minutes_per_hour: p.minutes_per_hour, mid_breaks: p.mid_breaks, library_ids: base.library_ids.length ? base.library_ids : allCommercial });
  };
  const label = (p: AdPreset) => (p.id === 'none' ? 'Station cards only' : p.name);
  const cardsOn = (opts?.cards ?? []).filter((c) => cardOn(c.id)).length;

  return (
    <div className="stack gap-lg">
      <Card className="flat stack">
        <h2>How much advertising?</h2>
        <Segmented value={choice} options={[{ value: 'clean', label: 'No breaks' }, ...presets.map((p) => ({ value: p.id, label: label(p) })), ...(choice === 'custom' ? [{ value: 'custom', label: 'Custom' }] : [])]} onChange={choose} />
        {!ads && <p className="small dim">Shows run clean: no commercials and no station cards, only the padding to the next time slot.</p>}
        {ads && <p className="small dim">{presets.find((p) => p.id === current)?.about ?? 'Your own numbers (under More options).'}</p>}
        {ads && words && <p className="callout">{words}</p>}
        {ads && ads.commercials && (
          commercial.length === 0 ? (
            <div className="callout stack" style={{ gap: 8 }}>
              <strong>There are no commercials yet.</strong>
              <span className="small">Breaks need some. Add a folder of them, or make your own.</span>
              <span className="row"><a className="btn small" href={href('libraries')}>Add a commercials folder</a><a className="btn small" href={href('promos')}>Make or download commercials</a></span>
            </div>
          ) : (
            <Field label="Commercials come from" hint={commercial.length > 1 ? 'Every commercials library unless you pick some.' : undefined}>
              <Chips options={commercial.map((l) => ({ value: l.id, label: `${name(l)} (${l.items})` }))} value={ads.library_ids} onToggle={(id) => set({ library_ids: toggleId(ads.library_ids, id) })} />
            </Field>
          )
        )}
      </Card>

      {ads && (
        <Card className="flat stack">
          <More summary={`${ads.pod_seconds}-second breaks · ${cardsOn} station card${cardsOn === 1 ? '' : 's'}${ads.years ? ` · ${ads.years[0]}-${ads.years[1]}` : ''}`}>
            <h3 style={{ margin: 0 }}>Break timing</h3>
            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <Field label="Try it on a slot of"><Segmented value={String(slot)} options={[{ value: '30', label: '30 min' }, { value: '60', label: '60 min' }, { value: '120', label: '2 hours' }]} onChange={(v) => setSlot(Number(v))} /></Field>
            </div>
            <div className="row wrap" style={{ alignItems: 'flex-start' }}>
              <Field label="About how long is each break (seconds)" hint="The time left in a slot is split into breaks about this long."><input className="input num" type="number" min={20} max={900} value={ads.pod_seconds} onChange={(e) => set({ pod_seconds: Number(e.target.value) })} /></Field>
              <Field label="At most this many breaks inside one show" hint="The break after the show is extra."><input className="input num" type="number" min={0} max={8} value={ads.max_breaks} onChange={(e) => set({ max_breaks: Number(e.target.value) })} /></Field>
              <Field label="Don't repeat a commercial for" hint="this many commercials"><input className="input num" type="number" min={1} max={200} value={ads.no_repeat} onChange={(e) => set({ no_repeat: Number(e.target.value) })} /></Field>
              <Field label="Minutes of ads per hour" hint="Only for a channel with no weekly lineup (no time slots); with a lineup the ads fill whatever is left of each slot."><input className="input num" type="number" min={0} max={30} value={ads.minutes_per_hour} onChange={(e) => set({ minutes_per_hour: Number(e.target.value) })} /></Field>
            </div>
            <Toggle checked={ads.mid_breaks} onChange={(v) => set({ mid_breaks: v })} label="Breaks inside shows (off = all the ads after the show)" />

            <h3 style={{ margin: '8px 0 0' }}>Commercials</h3>
            <Toggle checked={ads.commercials} onChange={(v) => set({ commercials: v })} label="Air commercials (off = only station cards fill the breaks)" />
            <Field label="Only from these years" hint="The year comes from folder or file names. Handy for a channel set in 1987.">
              <div className="row">
                <Toggle checked={ads.years !== null} onChange={(on) => set({ years: on ? [1980, 1989] : null })} />
                {ads.years && (
                  <>
                    <input className="input num" type="number" value={ads.years[0]} onChange={(e) => set({ years: [Number(e.target.value), ads.years![1]] })} />
                    <span className="dim">to</span>
                    <input className="input num" type="number" value={ads.years[1]} onChange={(e) => set({ years: [ads.years![0], Number(e.target.value)] })} />
                  </>
                )}
              </div>
            </Field>

            <h3 style={{ margin: '8px 0 0' }}>Station cards</h3>
            <p className="small dim">Short graphics in the station's look (change it under Appearance) that tell viewers what is happening. Turn off the ones you don't want.</p>
            <div className="grid cols-3">
              {(opts?.cards ?? []).map((c) => (
                <div key={c.id} className="stack" style={{ gap: 6 }}>
                  <ThemePreview choice={look} what="bumper" scene={c.id} id="preview" />
                  <Toggle checked={!!cardOn(c.id)} onChange={(v) => setCard(c.id, v)} label={<span><strong>{c.name}</strong><br /><span className="small dim">{c.about}</span></span>} />
                </div>
              ))}
            </div>
            {ads.bumpers.later_today_every > 0 && (
              <Field label="“Later today” replaces “Up next” every" hint="programmes. 1 = always.">
                <input className="input num" type="number" min={1} max={20} value={ads.bumpers.later_today_every} onChange={(e) => set({ bumpers: { ...ads.bumpers, later_today_every: Math.max(1, Number(e.target.value)) } })} />
              </Field>
            )}
            <Field label="Your own station clips" hint={clips.length ? 'Short videos of your own (idents, "we\'ll be right back" animations). One is used to open a break, in place of the generated card, when it fits.' : 'Add a library of kind “Bumper clips” (or upload some on the Libraries page) to use your own videos.'}>
              <Chips options={clips.map((l) => ({ value: l.id, label: `${name(l)} (${l.items})` }))} value={ads.bumper_library_ids} onToggle={(id) => set({ bumper_library_ids: toggleId(ads.bumper_library_ids, id) })} />
            </Field>
          </More>
        </Card>
      )}
    </div>
  );
}
