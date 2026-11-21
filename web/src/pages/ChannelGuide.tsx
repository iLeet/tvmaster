import { useEffect, useMemo, useState } from 'react';
import { api, errorMessage } from '../api';
import { PLAY_STYLES, adsFor, buildChannelBody, stepProblem, summarize, type GuideAnswers, type GuideWorld, type PlayStyle } from '../channelguide';
import { nextFreeNumber, slugify, validateId } from '../lib';
import { useQuery } from '../live';
import { href, navigate } from '../router';
import { ThemeChoiceEditor, emptyChoice, kidsChoice, useThemeCatalogue } from '../ThemePicker';
import type { AdOptions, ChannelView, Library, ShowSummary, System } from '../types';
import { Btn, Card, Field, More, Segmented, Spinner, Toggle, useToast } from '../ui';

const STEPS = ['Name', 'What it plays', 'How it plays', 'Commercials', 'Look', 'Ready'];

/** A new TV channel, one plain question at a time. Everything it sets can be changed later in the full editor. */
export function ChannelGuide() {
  const { say } = useToast();
  const channels = useQuery(() => api.get<ChannelView[]>('/channels'));
  const libs = useQuery(() => api.get<Library[]>('/libraries'));
  const shows = useQuery(() => api.get<ShowSummary[]>('/shows'));
  const sys = useQuery(() => api.get<System>('/system'));
  const ad = useQuery(() => api.get<AdOptions>('/ads/options'));
  const station = useThemeCatalogue()?.station;
  const [step, setStep] = useState(0);
  const [a, setA] = useState<GuideAnswers | null>(null);
  const [idTouched, setIdTouched] = useState(false);
  const [search, setSearch] = useState('');
  const [lookChoice, setLookChoice] = useState<'station' | 'kids' | 'own'>('station');
  const [words, setWords] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ready = channels.data && libs.data && shows.data && sys.data && ad.data;
  useEffect(() => {
    if (a || !channels.data) return;
    const n = nextFreeNumber(channels.data.map((c) => c.number), 1);
    setA({ name: '', id: '', number: n, group: '', shows: [], style: 'shuffle', breaks: 'clean', look: null, alwaysOn: false, chaos: 0 });
  }, [a, channels.data]);

  const world: GuideWorld | null = useMemo(() => {
    if (!ready) return null;
    const commercial = libs.data!.filter((l) => l.kind === 'commercials').map((l) => l.id);
    return { timezone: sys.data!.timezone, showLibraryIds: libs.data!.filter((l) => l.kind === 'shows').map((l) => l.id), commercialLibraryIds: commercial, presets: ad.data!.presets };
  }, [ready, libs.data, sys.data, ad.data]);

  // What the chosen breaks mean for a half-hour slot, in words (worked out by the server).
  const adsKey = a && world ? JSON.stringify(adsFor(a.breaks, world)) : '';
  useEffect(() => {
    if (!a || !world || step !== 3) return;
    const ads = adsFor(a.breaks, world);
    if (!ads) { setWords(''); return; }
    let live = true;
    api.post<{ text: string }>('/ads/explain', { ads, slot_minutes: 30, show_minutes: 22 }).then((r) => live && setWords(r.text)).catch(() => undefined);
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [adsKey, step]);

  if (channels.error || libs.error) return <div className="page"><div className="err-banner">{channels.error ?? libs.error}</div></div>;
  if (!a || !world) return <div className="page"><Spinner /></div>;

  const set = (p: Partial<GuideAnswers>) => setA({ ...a, ...p });
  const allShows = shows.data ?? [];
  const taken = (channels.data ?? []).map((c) => c.number);
  const idError = validateId(a.id) ?? ((channels.data ?? []).some((c) => c.id === a.id) ? 'That id is already used' : null);
  const problem = stepProblem(step, a, taken, idError);
  const noLibrary = world.showLibraryIds.length === 0 || allShows.length === 0;
  const visible = allShows.filter((s) => s.name.toLowerCase().includes(search.toLowerCase()));
  const noCommercials = world.commercialLibraryIds.length === 0;

  const create = async (then: 'channel' | 'schedule') => {
    setSaving(true);
    setError(null);
    try {
      await api.post('/channels', buildChannelBody(a, world));
      say(`Channel ${a.number} is ready`);
      navigate(then === 'schedule' || a.style === 'weekly' ? `schedule/${a.id}` : `channels/${a.id}`);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <a href={href('channels/new')} className="small">← Kinds of channel</a>
        <h1>New TV channel</h1>
        <a className="btn small right" href={href('channels/new?template=tv&editor=1')}>Use the full editor instead</a>
      </div>
      <div className="row wrap" role="list" aria-label="Steps">
        {STEPS.map((s, i) => (
          <span key={s} role="listitem" className={`badge ${i === step ? 'accent' : ''}`} style={{ opacity: i <= step ? 1 : 0.55 }}>{i + 1}. {s}</span>
        ))}
      </div>
      {error && <div className="err-banner">{error}</div>}

      {step === 0 && (
        <Card className="stack">
          <h2>What is it called?</h2>
          <div className="row wrap" style={{ alignItems: 'flex-start' }}>
            <Field label="Name"><input className="input" style={{ width: 280 }} autoFocus value={a.name} placeholder="Saturday Cartoons" onChange={(e) => set({ name: e.target.value, ...(!idTouched ? { id: slugify(e.target.value) } : {}) })} /></Field>
            <Field label="Channel number" hint="The next free one is filled in."><input className="input num" type="number" min={1} value={a.number} onChange={(e) => set({ number: Number(e.target.value) })} /></Field>
          </div>
          <More summary={`id ${a.id || '…'}${a.group ? ` · group ${a.group}` : ''}`}>
            <Field label="Group" hint="Kids, Movies, …"><input className="input" style={{ width: 200 }} value={a.group} onChange={(e) => set({ group: e.target.value })} /></Field>
            <Field label="Id" error={a.id ? idError : null} hint="Used in addresses. Made from the name."><input className="input mono" style={{ width: 220 }} value={a.id} onChange={(e) => { setIdTouched(true); set({ id: e.target.value }); }} /></Field>
          </More>
        </Card>
      )}

      {step === 1 && (
        <Card className="stack">
          <h2>What should it play?</h2>
          {noLibrary ? (
            <div className="callout stack" style={{ gap: 8 }}>
              <strong>There are no shows to play yet.</strong>
              <span className="small">Add a folder of TV shows first (or upload some), then come back.</span>
              <span className="row"><a className="btn small" href={href('libraries')}>Add shows</a></span>
            </div>
          ) : (
            <>
              <Segmented value={a.shows.length === 0 ? 'all' : 'some'} options={[{ value: 'all', label: `Everything (${allShows.length} shows)` }, { value: 'some', label: 'Only some shows' }]} onChange={(v) => set({ shows: v === 'all' ? [] : allShows.slice(0, 1).map((s) => s.name) })} />
              {a.shows.length > 0 && (
                <>
                  <div className="row wrap">
                    <input className="input" style={{ width: 240 }} placeholder="Find a show" value={search} onChange={(e) => setSearch(e.target.value)} />
                    <Btn small onClick={() => set({ shows: allShows.map((s) => s.name) })}>All</Btn>
                    <Btn small onClick={() => set({ shows: [] })}>Back to everything</Btn>
                    <span className="small dim">{a.shows.length} chosen</span>
                  </div>
                  <div className="grid cols-3">
                    {visible.map((s) => (
                      <Toggle key={s.name} checked={a.shows.includes(s.name)} onChange={(on) => set({ shows: on ? [...a.shows, s.name] : a.shows.filter((x) => x !== s.name) })} label={<span>{s.name} <span className="small dim">{s.episodes} episodes</span></span>} />
                    ))}
                  </div>
                </>
              )}
            </>
          )}
        </Card>
      )}

      {step === 2 && (
        <Card className="stack">
          <h2>How should it play?</h2>
          <div className="grid cols-3">
            {PLAY_STYLES.map((s) => (
              <button key={s.value} type="button" className="card clickable" aria-pressed={a.style === s.value} style={{ textAlign: 'left', outline: a.style === s.value ? '2px solid var(--accent)' : undefined }} onClick={() => set({ style: s.value as PlayStyle })}>
                <strong>{s.title}</strong>
                <div className="small dim">{s.about}</div>
              </button>
            ))}
          </div>
          {a.style !== 'weekly' && (
            <More summary={a.chaos ? `${a.chaos}% surprises` : 'no surprises'}>
              <Field label={`Surprises: ${a.chaos}%`} hint="Swaps this share of picks for something random, so an orderly channel now and then airs something unexpected. 0 = never.">
                <input type="range" min={0} max={100} value={a.chaos} onChange={(e) => set({ chaos: Number(e.target.value) })} />
              </Field>
            </More>
          )}
        </Card>
      )}

      {step === 3 && (
        <Card className="stack">
          <h2>Commercial breaks?</h2>
          <Segmented value={a.breaks} options={[{ value: 'clean', label: 'No breaks' }, ...world.presets.map((p) => ({ value: p.id, label: p.id === 'none' ? 'Station cards only' : p.name }))]} onChange={(breaks) => set({ breaks })} />
          <p className="small dim">{a.breaks === 'clean' ? 'Shows run clean, with only the padding to the next time slot.' : world.presets.find((p) => p.id === a.breaks)?.about}</p>
          {words && <p className="callout">{words}</p>}
          {a.breaks !== 'clean' && world.presets.find((p) => p.id === a.breaks)?.commercials && noCommercials && (
            <div className="callout stack" style={{ gap: 8 }}>
              <strong>There are no commercials yet.</strong>
              <span className="small">Breaks will use station cards until you add some: a folder of old commercials, or ones you make.</span>
              <span className="row"><a className="btn small" href={href('libraries')}>Add a commercials folder</a><a className="btn small" href={href('promos')}>Make or download commercials</a></span>
            </div>
          )}
          <p className="small dim">Everything about breaks (timing, years, station cards) is under the channel’s Breaks tab afterwards.</p>
        </Card>
      )}

      {step === 4 && (
        <Card className="stack">
          <h2>How should it look?</h2>
          <Segmented value={lookChoice} options={[{ value: 'station', label: 'Same as the station' }, { value: 'kids', label: 'The kids look' }, { value: 'own', label: 'Pick a theme' }]} onChange={(v) => { setLookChoice(v); set({ look: v === 'station' ? null : v === 'kids' ? kidsChoice() : station ? { ...station } : emptyChoice() }); }} />
          <p className="small dim">{lookChoice === 'station' ? 'Cards, bumpers, alerts and the guide follow the look chosen under Appearance.' : lookChoice === 'kids' ? 'Bright colours, smooth type and gentle alerts, just for this channel.' : 'Choose colours and style for this channel.'}</p>
          {lookChoice === 'own' && a.look && <ThemeChoiceEditor value={a.look} onChange={(look) => set({ look })} autoName="Per channel" autoAbout="Colours made from this channel." />}
        </Card>
      )}

      {step === 5 && (
        <Card className="stack">
          <h2>Ready</h2>
          <ul className="stack" style={{ gap: 4, margin: 0, paddingLeft: 18 }}>
            {summarize(a, world, allShows.length).map((l) => <li key={l}>{l}</li>)}
          </ul>
          <More summary={a.alwaysOn ? 'always on' : 'starts when someone tunes in'}>
            <Toggle checked={a.alwaysOn} onChange={(alwaysOn) => set({ alwaysOn })} label={<span><strong>Always on</strong> <span className="dim small">keep encoding even with no viewers. It uses processor time all day; leave it off unless the channel must be instant.</span></span>} />
          </More>
          <div className="row wrap">
            <Btn kind="primary" disabled={saving} onClick={() => create('channel')}>{saving ? <Spinner /> : 'Create the channel'}</Btn>
            <Btn disabled={saving} onClick={() => create('schedule')}>Create it and open the schedule</Btn>
          </div>
        </Card>
      )}

      <div className="row between">
        <Btn disabled={step === 0} onClick={() => setStep(step - 1)}>← Back</Btn>
        <span className="row">
          {problem && <span className="small dim">{problem}</span>}
          {step < STEPS.length - 1 && <Btn kind="primary" disabled={!!problem || (step === 1 && noLibrary)} onClick={() => setStep(step + 1)}>Next →</Btn>}
        </span>
      </div>
    </div>
  );
}
