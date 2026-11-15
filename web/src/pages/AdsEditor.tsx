import { defaultAds } from '../lib';
import type { AdConfig, Library } from '../types';
import { Card, Chips, Field, Toggle } from '../ui';

const name = (l: Library) => l.path.split(/[\\/]/).filter(Boolean).pop() ?? l.path;

export function AdsEditor({ ads, onChange, libraries }: { ads: AdConfig | null; onChange: (a: AdConfig | null) => void; libraries: Library[] }) {
  const commercial = libraries.filter((l) => l.kind === 'commercials');
  const clips = libraries.filter((l) => l.kind === 'bumpers');
  const set = (p: Partial<AdConfig>) => ads && onChange({ ...ads, ...p });
  const toggleId = (list: number[], id: number) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);

  return (
    <div className="stack gap-lg">
      <Card className="flat">
        <Toggle checked={ads !== null} onChange={(on) => onChange(on ? defaultAds() : null)} label={<strong>Commercial breaks and bumpers</strong>} />
        <p className="small dim" style={{ marginTop: 6 }}>
          Splits each show at natural breaks and fills every break to the exact second with commercials, "we'll be right back", "up next" and station IDs. Off = shows run clean.
        </p>
      </Card>

      {ads && (
        <>
          <Card className="flat stack">
            <h2>Commercials</h2>
            <Toggle checked={ads.commercials} onChange={(v) => set({ commercials: v })} label="Air commercials (off = bumpers only)" />
            <Field label="Commercial libraries" hint={commercial.length ? undefined : 'Add a library of kind "Commercials" first.'}>
              <Chips options={commercial.map((l) => ({ value: l.id, label: `${name(l)} (${l.items})` }))} value={ads.library_ids} onToggle={(id) => set({ library_ids: toggleId(ads.library_ids, id) })} />
            </Field>
            <Field label="Era" hint="Only air commercials from these years (the year comes from folder or file names).">
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
          </Card>

          <Card className="flat stack">
            <h2>Breaks</h2>
            <div className="row wrap" style={{ alignItems: 'flex-start' }}>
              <Field label="Target break length (seconds)"><input className="input num" type="number" min={20} max={900} value={ads.pod_seconds} onChange={(e) => set({ pod_seconds: Number(e.target.value) })} /></Field>
              <Field label="Most breaks per show"><input className="input num" type="number" min={0} max={8} value={ads.max_breaks} onChange={(e) => set({ max_breaks: Number(e.target.value) })} /></Field>
              <Field label="No repeat within" hint="plays"><input className="input num" type="number" min={1} max={200} value={ads.no_repeat} onChange={(e) => set({ no_repeat: Number(e.target.value) })} /></Field>
              <Field label="Ad minutes per hour" hint="Only when the channel has no time grid."><input className="input num" type="number" min={0} max={30} value={ads.minutes_per_hour} onChange={(e) => set({ minutes_per_hour: Number(e.target.value) })} /></Field>
            </div>
            <Toggle checked={ads.mid_breaks} onChange={(v) => set({ mid_breaks: v })} label="Breaks inside shows (off = all ads after the show)" />
          </Card>

          <Card className="flat stack">
            <h2>Bumpers</h2>
            <div className="row wrap">
              <Toggle checked={ads.bumpers.brb} onChange={(v) => set({ bumpers: { ...ads.bumpers, brb: v } })} label="We'll be right back" />
              <Toggle checked={ads.bumpers.back_to} onChange={(v) => set({ bumpers: { ...ads.bumpers, back_to: v } })} label="Back to the show" />
              <Toggle checked={ads.bumpers.up_next} onChange={(v) => set({ bumpers: { ...ads.bumpers, up_next: v } })} label="Up next" />
              <Toggle checked={ads.bumpers.station_id} onChange={(v) => set({ bumpers: { ...ads.bumpers, station_id: v } })} label="Station ID" />
            </div>
            <Field label='"Later today" every Nth slot' hint="0 = never. Shows the next few programmes instead of just the next one.">
              <input className="input num" type="number" min={0} max={20} value={ads.bumpers.later_today_every} onChange={(e) => set({ bumpers: { ...ads.bumpers, later_today_every: Number(e.target.value) } })} />
            </Field>
            <Field label="Your own bumper clips" hint={clips.length ? 'Used to open breaks in place of "we\'ll be right back" when they fit.' : 'Add a library of kind "Bumper clips" to use your own.'}>
              <Chips options={clips.map((l) => ({ value: l.id, label: `${name(l)} (${l.items})` }))} value={ads.bumper_library_ids} onToggle={(id) => set({ bumper_library_ids: toggleId(ads.bumper_library_ids, id) })} />
            </Field>
          </Card>
        </>
      )}
    </div>
  );
}
