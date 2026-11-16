import { useEffect, useMemo, useState } from 'react';
import { api, errorMessage } from '../api';
import { emptyLineup, lineupSummary, nextFreeNumber, slugify, validateId } from '../lib';
import { useQuery } from '../live';
import { emptyGuide, guideProblem, withGuideDefaults } from '../guide';
import { emptyOptions, langName, withDefaults } from '../options';
import { href, navigate } from '../router';
import type { Channel, ChannelOptions, ChannelView, GuideConfig, Library, RunOut, ShowSummary, Strategy, System } from '../types';
import { Badge, Btn, Card, Chips, ConfirmBtn, Field, Segmented, Spinner, Toggle, useToast } from '../ui';
import { AdsEditor } from './AdsEditor';
import { GuideEditor } from './GuideEditor';
import { LineupEditor } from './LineupEditor';
import { PictureSound } from './PictureSound';

interface Draft {
  id: string;
  number: number;
  name: string;
  group: string;
  kind: 'programming' | 'guide' | 'test';
  strategy: Strategy;
  run_out: RunOut;
  chaos: number;
  autopilot: boolean;
  always_on: boolean;
  library_ids: number[];
  lineup: Channel['lineup'];
  guide: GuideConfig;
  options: ChannelOptions;
}

const STRATEGIES: { value: Strategy; label: string; help: string }[] = [
  { value: 'sequential', label: 'In order', help: 'Episodes in order, shows interleaved, forever.' },
  { value: 'shuffle', label: 'Shuffle', help: 'Every episode once per pass, in random order.' },
  { value: 'chaos', label: 'Chaos', help: 'Anything, at random.' },
  { value: 'block_grid', label: 'TV schedule', help: 'A real weekly lineup: time slots, seasons and events (see the Programming tab).' },
];

function soundSummary(o: ChannelOptions): string {
  const langs = o.audio.languages.length ? o.audio.languages.map(langName).join(', then ') : "the file's own default track";
  const subs = { off: 'no subtitles', always: 'subtitles always', forced: 'forced subtitles only', foreign: 'subtitles when the audio is foreign', sdh: 'subtitles for the hard of hearing' }[o.subtitles.mode];
  const n = Object.keys(o.shows).length;
  const extras = [o.audio.loudness !== 'off' ? 'volume levelling' : '', o.video.look !== 'none' ? `${o.video.look.replace('_', ' ')} look` : '', n ? `${n} show override${n === 1 ? '' : 's'}` : ''].filter(Boolean);
  return `Audio: ${langs}. ${subs[0].toUpperCase()}${subs.slice(1)}.${extras.length ? ` ${extras.join(', ')}.` : ''}`;
}

const RUN_OUT: { value: RunOut; label: string; help: string }[] = [
  { value: 'keep_going', label: 'Keep going', help: 'After the last episode, wrap to the first and carry on.' },
  { value: 'next_in_rotation', label: 'Next in rotation', help: 'Rotations hand over to the next show at each finale.' },
  { value: 'chaos_fill', label: 'Chaos fill', help: 'When a show is finished, air random content in its slot.' },
  { value: 'hybrid', label: 'Hybrid', help: 'Chaos fill, and unscheduled time airs chaos too.' },
];

const fromChannel = (c: Channel): Draft => ({
  id: c.id, number: c.number, name: c.name, group: c.group ?? '', kind: c.kind, strategy: c.strategy, run_out: c.run_out,
  chaos: c.chaos, autopilot: c.autopilot, always_on: c.always_on, library_ids: c.library_ids, lineup: { ...c.lineup, signoff: c.lineup.signoff ?? null }, guide: withGuideDefaults(c.guide), options: withDefaults(c.options),
});

export function ChannelEditor({ id, tab }: { id: string; tab: string }) {
  const isNew = id === 'new';
  const { say } = useToast();
  const channel = useQuery(() => (isNew ? Promise.resolve(null) : api.get<ChannelView>(`/channels/${id}`)), [id]);
  const all = useQuery(() => api.get<ChannelView[]>('/channels'));
  const libs = useQuery(() => api.get<Library[]>('/libraries'));
  const shows = useQuery(() => api.get<ShowSummary[]>('/shows'));
  const sys = useQuery(() => api.get<System>('/system'));

  const [draft, setDraft] = useState<Draft | null>(null);
  const [saved, setSaved] = useState('');
  const [idTouched, setIdTouched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // Fill the form once the data is in.
  useEffect(() => {
    if (draft) return;
    if (isNew && all.data && sys.data) {
      const d: Draft = {
        id: '', number: nextFreeNumber(all.data.map((c) => c.number)), name: '', group: '', kind: 'programming', strategy: 'sequential',
        run_out: 'keep_going', chaos: 0, autopilot: true, always_on: false, library_ids: [], lineup: emptyLineup(sys.data.timezone),
        guide: emptyGuide(), options: emptyOptions(),
      };
      setDraft(d);
      setSaved(JSON.stringify(d));
    } else if (!isNew && channel.data) {
      const d = fromChannel(channel.data);
      setDraft(d);
      setSaved(JSON.stringify(d));
    }
  }, [draft, isNew, all.data, sys.data, channel.data]);

  const dirty = draft !== null && JSON.stringify(draft) !== saved;
  const groups = useMemo(() => Array.from(new Set((all.data ?? []).map((c) => c.group).filter(Boolean) as string[])), [all.data]);
  const shownTab = draft?.kind === 'test' && (tab === 'programming' || tab === 'ads' || tab === 'guide') ? 'general' : draft?.kind === 'guide' && (tab === 'programming' || tab === 'ads') ? 'guide' : draft?.kind === 'programming' && tab === 'guide' ? 'general' : tab;

  if (!draft) {
    return (
      <div className="page">
        <a href={href('channels')} className="small">← Channels</a>
        {channel.error ? <div className="err-banner">{channel.error}</div> : <Spinner />}
      </div>
    );
  }

  const set = (p: Partial<Draft>) => setDraft({ ...draft, ...p });
  const shownShows = shows.data ?? [];
  const showLibs = (libs.data ?? []).filter((l) => l.kind === 'shows');
  const idError = isNew ? validateId(draft.id) : null;
  const numberTaken = (all.data ?? []).some((c) => c.number === draft.number && c.id !== draft.id);
  const problem = draft.kind === 'guide' ? guideProblem(draft.guide) : null;
  const canSave = dirty && !saving && draft.name.trim() !== '' && !idError && draft.number > 0 && !numberTaken && !problem;

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const body: Record<string, unknown> = {
        id: draft.id, number: draft.number, name: draft.name.trim(), kind: draft.kind, group: draft.group.trim() || undefined,
        autopilot: draft.autopilot, always_on: draft.always_on,
      };
      if (draft.kind === 'programming') {
        Object.assign(body, { strategy: draft.strategy, run_out: draft.run_out, chaos: draft.chaos, library_ids: draft.library_ids, lineup: draft.lineup, options: draft.options });
      } else if (draft.kind === 'test') {
        body.options = draft.options;
      } else {
        body.guide = draft.guide;
      }
      await api.post<Channel>('/channels', body);
      setSaved(JSON.stringify(draft));
      say('Saved');
      all.reload();
      if (isNew) navigate(`channels/${draft.id}`);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const tabs = draft.kind === 'guide'
    ? [['general', 'General'], ['guide', 'Guide']]
    : draft.kind === 'test'
    ? [['general', 'General'], ['picture', 'Screen']]
    : [['general', 'General'], ['programming', 'Programming'], ['picture', 'Sound & picture'], ['ads', 'Ads & bumpers']];
  const goTab = (t: string) => navigate(`channels/${id}${t === 'general' ? '' : `?tab=${t}`}`);

  return (
    <div className="page">
      <div className="page-head">
        <a href={href('channels')} className="small">← Channels</a>
        <h1>{isNew ? 'New channel' : draft.name || id}</h1>
        {!isNew && <a className="btn small" href={href(`schedule/${id}`)}>View schedule</a>}
        <span className="right row">
          {dirty && <Badge tone="warn">Unsaved</Badge>}
          <Btn kind="primary" disabled={!canSave} onClick={save}>{saving ? <Spinner /> : isNew ? 'Create channel' : 'Save'}</Btn>
        </span>
      </div>

      {error && <div className="err-banner">{error}</div>}
      {!error && problem && <div className="err-banner">{problem}</div>}

      <div className="tabs" role="tablist">
        {tabs.map(([t, label]) => (
          <button key={t} role="tab" className={shownTab === t ? 'on' : ''} onClick={() => goTab(t)}>{label}</button>
        ))}
      </div>

      {shownTab === 'general' && (
        <div className="stack gap-lg">
          <Card className="stack">
            <div className="row wrap" style={{ alignItems: 'flex-start' }}>
              <Field label="Name">
                <input className="input" style={{ width: 260 }} autoFocus={isNew} value={draft.name} placeholder="Retro TV" onChange={(e) => set({ name: e.target.value, ...(isNew && !idTouched ? { id: slugify(e.target.value) } : {}) })} />
              </Field>
              <Field label="Channel number" error={numberTaken ? 'Another channel uses this number' : null}>
                <input className="input num" type="number" min={1} value={draft.number} onChange={(e) => set({ number: Number(e.target.value) })} />
              </Field>
              <Field label="Group" hint="Kids, Movies, …">
                <input className="input" list="group-list" style={{ width: 180 }} value={draft.group} onChange={(e) => set({ group: e.target.value })} />
                <datalist id="group-list">{groups.map((g) => <option key={g} value={g} />)}</datalist>
              </Field>
              <Field label="Id" error={idError} hint={isNew ? 'Used in URLs. Cannot be changed later.' : undefined}>
                <input className="input mono" style={{ width: 180 }} disabled={!isNew} value={draft.id} onChange={(e) => { setIdTouched(true); set({ id: e.target.value }); }} />
              </Field>
            </div>
            {isNew && (
              <Field label="Kind">
                <Segmented value={draft.kind} options={[{ value: 'programming', label: 'Programming channel' }, { value: 'guide', label: 'Guide channel' }, { value: 'test', label: 'Test pattern' }]} onChange={(kind) => set(kind === 'test' ? { kind, options: { ...draft.options, off_air: { ...draft.options.off_air, screen: 'bars', tone: true } } } : { kind })} />
              </Field>
            )}
          </Card>

          {draft.kind === 'programming' && (
            <>
              <Card className="stack">
                <h2>What it airs</h2>
                <Field label="Libraries" hint={showLibs.length ? 'The shows this channel draws from.' : 'Add a TV shows library first.'}>
                  <Chips options={showLibs.map((l) => ({ value: l.id, label: `${l.path.split(/[\\/]/).filter(Boolean).pop() ?? l.path} (${l.items})` }))} value={draft.library_ids} onToggle={(lid) => set({ library_ids: draft.library_ids.includes(lid) ? draft.library_ids.filter((x) => x !== lid) : [...draft.library_ids, lid] })} />
                </Field>
                <Field label="Strategy" hint={STRATEGIES.find((s) => s.value === draft.strategy)?.help}>
                  <Segmented value={draft.strategy} options={STRATEGIES} onChange={(strategy) => set({ strategy })} />
                </Field>
                <Field label={`Chaos: ${draft.chaos}%`} hint="Swaps this share of picks for something random, so a structured channel can drift towards chaos.">
                  <input type="range" min={0} max={100} value={draft.chaos} onChange={(e) => set({ chaos: Number(e.target.value) })} />
                </Field>
                <Field label="When a show runs out" hint={RUN_OUT.find((r) => r.value === draft.run_out)?.help}>
                  <Segmented value={draft.run_out} options={RUN_OUT} onChange={(run_out) => set({ run_out })} />
                </Field>
                {draft.strategy !== 'block_grid' && draft.lineup.blocks.length > 0 && (
                  <p className="small dim">This channel has a lineup ({lineupSummary(draft.lineup)}) that applies on top of the strategy.</p>
                )}
              </Card>
            </>
          )}

          {draft.kind === 'programming' && !isNew && (
            <Card className="stack">
              <div className="row between wrap">
                <h2 style={{ margin: 0 }}>Sound &amp; subtitles</h2>
                <Btn small onClick={() => goTab('picture')}>Audio, subtitles and picture options…</Btn>
              </div>
              <p className="small dim">{soundSummary(draft.options)}</p>
            </Card>
          )}

          {draft.kind === 'test' && <p className="small dim">A test pattern channel airs only its screen: colour bars and a tone by default, or snow, black, a station card or a picture of your own. Choose it on the Screen tab.</p>}

          <Card className="stack">
            <h2>Running it</h2>
            <Toggle checked={draft.autopilot} onChange={(v) => set({ autopilot: v })} label={<span><strong>Autopilot</strong> <span className="dim">— edits re-roll the future automatically (off = manual lock until you re-roll)</span></span>} />
            <Toggle checked={draft.always_on} onChange={(v) => set({ always_on: v })} label={<span><strong>Always on</strong> <span className="dim">— keep encoding even with no viewers (otherwise it starts when someone tunes in)</span></span>} />
          </Card>

          {!isNew && (
            <Card className="row between">
              <span className="dim small">Removing a channel does not touch your files.</span>
              <ConfirmBtn kind="danger" icon="trash" label="Delete channel" confirm={`Delete “${draft.name}”? TVs tuned to it will lose the channel.`} onConfirm={async () => { try { await api.del(`/channels/${id}`); say('Channel deleted'); navigate('channels'); } catch (e) { say(errorMessage(e), true); } }} />
            </Card>
          )}
        </div>
      )}

      {shownTab === 'programming' && draft.kind === 'programming' && (
        <LineupEditor lineup={draft.lineup} shows={shownShows} onChange={(lineup) => set({ lineup, ...(draft.strategy !== 'block_grid' && lineup.blocks.length > 0 ? { strategy: 'block_grid' as Strategy } : {}) })} />
      )}

      {shownTab === 'picture' && draft.kind === 'test' && (
        <PictureSound options={draft.options} onChange={(options) => set({ options })} lineup={draft.lineup} onLineup={(lineup) => set({ lineup })} shows={[]} sys={sys.data ?? undefined} testOnly />
      )}

      {shownTab === 'picture' && draft.kind === 'programming' && (
        <PictureSound options={draft.options} onChange={(options) => set({ options })} lineup={draft.lineup} onLineup={(lineup) => set({ lineup })} shows={shownShows} sys={sys.data ?? undefined} />
      )}

      {shownTab === 'ads' && draft.kind === 'programming' && (
        <AdsEditor ads={draft.lineup.ads} libraries={libs.data ?? []} onChange={(ads) => set({ lineup: { ...draft.lineup, ads } })} />
      )}

      {shownTab === 'guide' && draft.kind === 'guide' && (
        <GuideEditor id={draft.id} name={draft.name} guide={draft.guide} onChange={(guide) => set({ guide })} channels={all.data ?? []} libs={libs.data ?? []} />
      )}
    </div>
  );
}
