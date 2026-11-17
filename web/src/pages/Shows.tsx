import { useState } from 'react';
import { api, errorMessage } from '../api';
import { useQuery } from '../live';
import type { ShowOptions, ShowSummary } from '../types';
import { Btn, Card, Empty, Field, Modal, Spinner, Toggle, useToast } from '../ui';
import { ShowDetails } from './ShowDetails';
import { ShowSettingsRow } from './ShowSettings';
import type { ShowSummary as Show } from '../types';

/** Every show, the audio and subtitle languages found in its files, and settings that follow it onto every channel. */
export function Shows() {
  const shows = useQuery(() => api.get<ShowSummary[]>('/shows'));
  const { say } = useToast();
  const [filter, setFilter] = useState('');
  const [local, setLocal] = useState<Record<string, ShowOptions>>({});
  const [detail, setDetail] = useState<string | null>(null);
  const [picked, setPicked] = useState<string[]>([]);
  const [merging, setMerging] = useState(false);
  const [mergeTitle, setMergeTitle] = useState('');
  const names = useQuery(() => api.get<{ auto_clean: boolean }>('/shows/names'));

  const save = async (name: string, settings: ShowOptions) => {
    setLocal((l) => ({ ...l, [name]: settings }));
    try {
      await api.put('/show-settings', { show: name, settings });
    } catch (e) {
      say(errorMessage(e), true);
      shows.reload();
    }
  };

  const merge = async () => {
    try {
      await api.post('/shows/merge', { shows: picked, title: mergeTitle });
      say('Put together');
      setMerging(false);
      setPicked([]);
      shows.reload();
    } catch (e) {
      say(errorMessage(e), true);
    }
  };
  const toggleAuto = async (enabled: boolean) => {
    try {
      await api.put('/shows/auto-clean', { enabled });
      names.reload();
      shows.reload();
    } catch (e) {
      say(errorMessage(e), true);
    }
  };
  const list = (shows.data ?? []).filter((s) => s.name.toLowerCase().includes(filter.toLowerCase()));
  const stale = (shows.data ?? []).some((s) => s.files_with_tracks < s.episodes);

  return (
    <div className="page">
      <div className="page-head"><h1>Shows</h1><span className="sub">Audio, subtitles and look for each show, on every channel</span></div>
      <Card className="stack">
        <p className="small dim">
          Set a show up once here and every channel that airs it follows: anime in Japanese with English subtitles, a show whose picture already has
          subtitles burned in (set Subtitles to Off), a 1950s serial in black and white. A channel can still override a show for itself in the channel's
          Picture &amp; sound tab, which beats what is set here.
        </p>
        <div className="row wrap between">
          <Toggle checked={names.data?.auto_clean ?? true} onChange={toggleAuto} label={<span>Tidy folder names <span className="dim small">"Bleach season 01" becomes the show Bleach, season 1</span></span>} />
          <Btn small disabled={picked.length < 1} onClick={() => { setMergeTitle(picked[0] ?? ''); setMerging(true); }}>{picked.length > 1 ? `Put ${picked.length} together…` : 'Rename…'}</Btn>
        </div>
        {stale && <p className="small" style={{ color: 'var(--warn)' }}>Some files were scanned before audio and subtitle tracks were read. Re-scan the libraries (Libraries page) to see their languages.</p>}
        {(shows.data?.length ?? 0) > 8 && <input className="input" style={{ maxWidth: 300 }} placeholder="Filter shows…" value={filter} onChange={(e) => setFilter(e.target.value)} />}
        {shows.loading && !shows.data && <Spinner />}
        {shows.data && shows.data.length === 0 && <Empty>No shows yet. Add a TV shows library first.</Empty>}
        <div className="stack">
          {list.map((s) => (
            <ShowSettingsRow
              key={s.name}
              show={s}
              ov={local[s.name] ?? s.defaults ?? {}}
              inheritLabel="Channel's"
              inheritWhat="whatever each channel says"
              wantedAudio={[]}
              badge="set"
              check={{ on: picked.includes(s.name), set: (v) => setPicked((p) => (v ? [...p, s.name] : p.filter((x) => x !== s.name))) }}
              actions={<Btn small onClick={() => setDetail(s.name)}>Name &amp; details</Btn>}
              onChange={(p) => save(s.name, mergeClean(local[s.name] ?? s.defaults ?? {}, p))}
            />
          ))}
        </div>
        <div><Btn small onClick={() => shows.reload()}>Refresh</Btn></div>
      </Card>
      {detail && shows.data?.find((x) => x.name === detail) && <ShowDetails show={shows.data.find((x) => x.name === detail) as Show} onClose={() => setDetail(null)} onChanged={shows.reload} />}
      {merging && (
        <Modal title={picked.length > 1 ? 'Put these together as one show' : 'Rename show'} onClose={() => setMerging(false)} footer={<><Btn onClick={() => setMerging(false)}>Cancel</Btn><Btn kind="primary" disabled={!mergeTitle.trim()} onClick={merge}>Save</Btn></>}>
          <div className="stack">
            {picked.length > 1 && <p className="small dim">{picked.join(', ')}. Their folders become one show; give each folder its season under Name &amp; details so episodes air in order.</p>}
            <Field label="Title on air"><input className="input" autoFocus value={mergeTitle} onChange={(e) => setMergeTitle(e.target.value)} maxLength={120} /></Field>
          </div>
        </Modal>
      )}
    </div>
  );
}

/** Apply a change and drop fields that were cleared, so an emptied show is removed on the server. */
function mergeClean(cur: ShowOptions, patch: ShowOptions): ShowOptions {
  const next: Record<string, unknown> = { ...cur, ...patch };
  for (const k of Object.keys(next)) {
    const v = next[k];
    if (v === null || v === undefined || (Array.isArray(v) && v.length === 0)) delete next[k];
  }
  return next as ShowOptions;
}
