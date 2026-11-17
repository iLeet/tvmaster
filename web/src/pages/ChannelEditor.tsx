import { useEffect, useMemo, useState } from 'react';
import { api, errorMessage } from '../api';
import { emptyLineup, lineupSummary, nextFreeNumber, slugify, validateId } from '../lib';
import { useQuery } from '../live';
import { emptyGuide, guideProblem, withGuideDefaults } from '../guide';
import { SCREENS, emptyOptions, langName, withDefaults } from '../options';
import { href, navigate } from '../router';
import type { Channel, ChannelOptions, ChannelTemplates, ChannelView, GuideConfig, InfoConfig, Library, RunOut, ShowSummary, Strategy, System } from '../types';
import { Badge, Btn, Card, Chips, ConfirmBtn, Field, Segmented, Spinner, Toggle, useToast } from '../ui';
import { AdsEditor } from './AdsEditor';
import { GuideEditor } from './GuideEditor';
import { InfoEditor } from './InfoEditor';
import { infoProblem, withInfoDefaults } from '../info';
import { LineupEditor } from './LineupEditor';
import { PictureSound } from './PictureSound';
import { ThemeChoiceEditor, ThemePreview, emptyChoice, useThemeCatalogue } from '../ThemePicker';

interface Draft {
  id: string;
  number: number;
  name: string;
  group: string;
  kind: 'programming' | 'guide' | 'test' | 'info';
  strategy: Strategy;
  run_out: RunOut;
  chaos: number;
  autopilot: boolean;
  always_on: boolean;
  library_ids: number[];
  lineup: Channel['lineup'];
  guide: GuideConfig;
  info: InfoConfig;
  options: ChannelOptions;
}

const STRATEGIES: { value: Strategy; label: string; help: string }[] = [
  { value: 'sequential', label: 'In order', help: 'Episodes in order, shows interleaved, forever.' },
  { value: 'shuffle', label: 'Shuffle', help: 'Every episode once per pass, in random order.' },
  { value: 'chaos', label: 'Chaos', help: 'Anything, at random.' },
  { value: 'block_grid', label: 'Weekly lineup', help: 'A real TV schedule: time slots by day and time, seasons and events (set them on the Programming tab).' },
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
  { value: 'off_air', label: 'Test screen', help: 'Each episode airs once. When a show has run out its slot shows the channel’s off-air screen (colour bars and a tone, if you chose that under “When nothing is scheduled”).' },
  { value: 'hybrid', label: 'Hybrid', help: 'Chaos fill, and unscheduled time airs chaos too.' },
];

const fromChannel = (c: Channel): Draft => ({
  id: c.id, number: c.number, name: c.name, group: c.group ?? '', kind: c.kind, strategy: c.strategy, run_out: c.run_out,
  chaos: c.chaos, autopilot: c.autopilot, always_on: c.always_on, library_ids: c.library_ids, lineup: { ...c.lineup, signoff: c.lineup.signoff ?? null }, guide: withGuideDefaults(c.guide), info: withInfoDefaults(c.info), options: withDefaults(c.options),
});

export function ChannelEditor({ id, tab, template }: { id: string; tab: string; template?: string | null }) {
  const isNew = id === 'new';
  const { say, attempt } = useToast();
  const station = useThemeCatalogue()?.station;
  const channel = useQuery(() => (isNew ? Promise.resolve(null) : api.get<ChannelView>(`/channels/${id}`)), [id]);
  const all = useQuery(() => api.get<ChannelView[]>('/channels'));
  const libs = useQuery(() => api.get<Library[]>('/libraries'));
  const shows = useQuery(() => api.get<ShowSummary[]>('/shows'));
  const sys = useQuery(() => api.get<System>('/system'));
  const templates = useQuery(() => api.get<ChannelTemplates>('/channel-templates'));
  const tpl = isNew ? templates.data?.templates.find((t) => t.id === (template ?? 'tv')) ?? null : null;

  const [draft, setDraft] = useState<Draft | null>(null);
  const [saved, setSaved] = useState('');
  const [idTouched, setIdTouched] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // Fill the form once the data is in.
  useEffect(() => {
    if (draft) return;
    if (isNew && all.data && sys.data && templates.data) {
      // A new channel starts from the kind chosen on the first screen: its name, the next free number and the settings that make it work.
      const t = templates.data.templates.find((x) => x.id === (template ?? 'tv')) ?? templates.data.templates[0];
      const b = t.body as Record<string, unknown>;
      const d: Draft = {
        id: t.suggested.id, number: t.suggested.number || nextFreeNumber(all.data.map((c) => c.number)), name: t.suggested.name, group: '', kind: t.kind, strategy: 'sequential',
        run_out: 'keep_going', chaos: 0, autopilot: true, always_on: false, library_ids: [], lineup: emptyLineup(sys.data.timezone),
        guide: emptyGuide(), info: withInfoDefaults(b.info as Partial<InfoConfig> | undefined), options: withDefaults(b.options as Partial<ChannelOptions> | undefined) ?? emptyOptions(),
      };
      setDraft(d);
      setSaved(JSON.stringify(d));
      setIdTouched(true);
    } else if (!isNew && channel.data) {
      const d = fromChannel(channel.data);
      setDraft(d);
      setSaved(JSON.stringify(d));
    }
  }, [draft, isNew, all.data, sys.data, channel.data, templates.data, template]);

  const dirty = draft !== null && JSON.stringify(draft) !== saved;
  const groups = useMemo(() => Array.from(new Set((all.data ?? []).map((c) => c.group).filter(Boolean) as string[])), [all.data]);
  const shownTab = draft?.kind === 'info' ? (tab === 'info' ? 'info' : 'general') : draft?.kind === 'test' && (tab === 'programming' || tab === 'ads' || tab === 'guide' || tab === 'info') ? 'general' : draft?.kind === 'guide' && (tab === 'programming' || tab === 'ads' || tab === 'info') ? 'guide' : draft?.kind === 'programming' && (tab === 'guide' || tab === 'info') ? 'general' : tab;

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
  const problem = draft.kind === 'guide' ? guideProblem(draft.guide) : draft.kind === 'info' ? infoProblem(draft.info) : null;
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
      } else if (draft.kind === 'info') {
        body.info = draft.info;
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
    : draft.kind === 'info'
    ? [['general', 'General'], ['info', 'Pages']]
    : draft.kind === 'test'
    ? [['general', 'General'], ['picture', 'Screen']]
    : [['general', 'General'], ['programming', 'Programming'], ['picture', 'Sound & picture'], ['ads', 'Breaks']];
  const goTab = (t: string) => navigate(isNew ? `channels/new?template=${template ?? 'tv'}${t === 'general' ? '' : `&tab=${t}`}` : `channels/${id}${t === 'general' ? '' : `?tab=${t}`}`);

  return (
    <div className="page">
      <div className="page-head">
        <a href={href('channels')} className="small">← Channels</a>
        <h1>{isNew ? `New ${tpl?.name.toLowerCase() ?? 'channel'}` : draft.name || id}</h1>
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
            {isNew && tpl && (
              <div className="stack" style={{ gap: 6 }}>
                <div className="row wrap"><Badge tone="accent">{tpl.name}</Badge><span className="small dim">{tpl.about}</span><a className="small" href={href('channels/new')}>Choose a different kind</a></div>
                {tpl.needs.length > 0 && (
                  <p className="small dim">
                    Before it is worth airing: {tpl.needs.map((n) => ({ libraries: 'pick the libraries it draws from below', feeds: 'add a news feed on the Pages tab', pages: 'add your pages on the Pages tab', music: 'choose some music on the Pages tab' } as Record<string, string>)[n] ?? n).join('; ')}.
                  </p>
                )}
                {draft.kind === 'guide' && <p className="small dim">The guide lists every TV and info channel by number, with what is on now and next. Choose only some on the Guide tab.</p>}
              </div>
            )}
          </Card>

          {!isNew && draft.kind === 'programming' && (
            <Card className="stack">
              <h2>Controls</h2>
              <p className="small dim">Act on this channel right now. None of these change your settings.</p>
              <div className="row wrap">
                <Btn onClick={() => attempt(() => api.post(`/channels/${draft.id}/stream`, { action: 'restart' }), 'Stream restarted').then(() => all.reload())}>Restart the stream</Btn>
                {all.data?.find((c) => c.id === draft.id)?.on_air
                  ? <Btn onClick={() => attempt(() => api.post(`/channels/${draft.id}/stream`, { action: 'stop' }), 'Taken off air').then(() => all.reload())}>Take off air</Btn>
                  : <Btn onClick={() => attempt(() => api.post(`/channels/${draft.id}/stream`, { action: 'start' }), 'On air').then(() => all.reload())}>Put on air</Btn>}
                <Btn onClick={() => attempt(() => api.post(`/channels/${draft.id}/refresh`), 'On the current plan now')}>Refresh now</Btn>
                <Btn onClick={() => attempt(() => api.post(`/channels/${draft.id}/reset`, { what: 'schedule' }), 'Schedule re-planned from now')}>Re-plan the schedule</Btn>
                <ConfirmBtn kind="danger" label="Start every show from episode 1" confirm="Every show on this channel starts again at its first episode, and the schedule is re-planned from now." onConfirm={() => attempt(() => api.post(`/channels/${draft.id}/reset`, { what: 'progress' }), 'Progress reset')} />
              </div>
            </Card>
          )}

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
                {draft.strategy !== 'chaos' && (
                  <Field label={`Surprises: ${draft.chaos}%`} hint="Swaps this share of picks for something random, so an orderly channel now and then airs something unexpected. 0 = never.">
                    <input type="range" min={0} max={100} value={draft.chaos} onChange={(e) => set({ chaos: Number(e.target.value) })} />
                  </Field>
                )}
                <Field label="When a show runs out of episodes" hint={RUN_OUT.find((r) => r.value === draft.run_out)?.help}>
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

          {draft.kind === 'info' && <p className="small dim">An info channel has nothing to schedule: it draws its pages itself, round and round. Add and arrange them on the Pages tab. It appears on the guide, with what it is.</p>}
          {draft.kind === 'test' && <p className="small dim">A test pattern channel airs only its screen: colour bars and a tone by default, or snow, black, a station card or a picture of your own. Choose it on the Screen tab.</p>}

          <Card className="stack">
            <h2>Look</h2>
            <p className="small dim">Cards, break screens, the stand-by screen, alerts and on-screen graphics{draft.kind === 'guide' ? ', and the guide' : ''}. Follows the station theme unless you give this channel its own.</p>
            <Toggle checked={!draft.options.theme} onChange={(on) => set({ options: { ...draft.options, theme: on ? null : { ...emptyChoice(), ...(station ?? {}) } } })} label={<span><strong>Follow the station theme</strong> <span className="dim small">{station ? `${station.theme === 'auto' ? 'per channel' : station.theme}, ${station.style}` : ''}</span></span>} />
            {draft.options.theme && (
              <div className="stack nest">
                <ThemeChoiceEditor value={draft.options.theme} onChange={(theme) => set({ options: { ...draft.options, theme } })} autoName="Per channel" autoAbout="Colours made from this channel." />
                <ThemePreview choice={draft.options.theme} what="card" id={draft.id} name={draft.name} number={draft.number} />
              </div>
            )}
          </Card>
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
        <>
          <LineupEditor lineup={draft.lineup} shows={shownShows} onChange={(lineup) => set({ lineup, ...(draft.strategy !== 'block_grid' && lineup.blocks.length > 0 ? { strategy: 'block_grid' as Strategy } : {}) })} />
          <Card className="stack">
            <h2>When nothing is scheduled</h2>
            <p className="small dim">What the channel shows in the gaps between your time slots, when a show has run out, or when its files are not available (a drive that is unplugged).</p>
            <Field label="Screen" hint={SCREENS.find((x) => x.value === draft.options.off_air.screen)?.help}>
              <Segmented value={draft.options.off_air.screen} options={SCREENS} onChange={(screen) => set({ options: { ...draft.options, off_air: { ...draft.options.off_air, screen, tone: screen === 'bars' ? true : draft.options.off_air.tone } } })} />
            </Field>
            <Toggle checked={draft.options.off_air.tone} onChange={(tone) => set({ options: { ...draft.options, off_air: { ...draft.options.off_air, tone } } })} label={<span>Test tone <span className="dim small">a steady 1 kHz tone, like a real test pattern</span></span>} />
            <p className="small dim">The station card message, your own picture and the rest are on the Sound & picture tab.</p>
          </Card>
        </>
      )}

      {shownTab === 'picture' && draft.kind === 'test' && (
        <PictureSound options={draft.options} onChange={(options) => set({ options })} lineup={draft.lineup} onLineup={(lineup) => set({ lineup })} shows={[]} sys={sys.data ?? undefined} testOnly />
      )}

      {shownTab === 'picture' && draft.kind === 'programming' && (
        <PictureSound options={draft.options} onChange={(options) => set({ options })} lineup={draft.lineup} onLineup={(lineup) => set({ lineup })} shows={shownShows} sys={sys.data ?? undefined} />
      )}

      {shownTab === 'ads' && draft.kind === 'programming' && (
        <AdsEditor ads={draft.lineup.ads} theme={draft.options.theme} libraries={libs.data ?? []} onChange={(ads) => set({ lineup: { ...draft.lineup, ads } })} />
      )}

      {shownTab === 'info' && draft.kind === 'info' && (
        <InfoEditor id={isNew ? '' : draft.id} name={draft.name} info={draft.info} onChange={(info) => set({ info })} libs={libs.data ?? []} templates={templates.data ?? null} />
      )}

      {shownTab === 'guide' && draft.kind === 'guide' && (
        <GuideEditor id={draft.id} name={draft.name} guide={draft.guide} onChange={(guide) => set({ guide })} channels={all.data ?? []} libs={libs.data ?? []} />
      )}
    </div>
  );
}
