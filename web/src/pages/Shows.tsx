import { useState } from 'react';
import { api, errorMessage } from '../api';
import { useQuery } from '../live';
import type { ShowOptions, ShowSummary } from '../types';
import { Btn, Card, Empty, Spinner, useToast } from '../ui';
import { ShowSettingsRow } from './ShowSettings';

/** Every show, the audio and subtitle languages found in its files, and settings that follow it onto every channel. */
export function Shows() {
  const shows = useQuery(() => api.get<ShowSummary[]>('/shows'));
  const { say } = useToast();
  const [filter, setFilter] = useState('');
  const [local, setLocal] = useState<Record<string, ShowOptions>>({});

  const save = async (name: string, settings: ShowOptions) => {
    setLocal((l) => ({ ...l, [name]: settings }));
    try {
      await api.put('/show-settings', { show: name, settings });
    } catch (e) {
      say(errorMessage(e), true);
      shows.reload();
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
              onChange={(p) => save(s.name, mergeClean(local[s.name] ?? s.defaults ?? {}, p))}
            />
          ))}
        </div>
        <div><Btn small onClick={() => shows.reload()}>Refresh</Btn></div>
      </Card>
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
