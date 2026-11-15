import { useEffect, useMemo, useState } from 'react';
import { api } from '../api';
import { useLiveEvent, useLiveStatus, useQuery } from '../live';
import { navigate } from '../router';
import type { ChannelView, Tv } from '../types';
import { Btn, Card, Empty, Spinner, useToast } from '../ui';

const KEY = 'tvmaster.tv';

const remember = (id: string) => { try { localStorage.setItem(KEY, id); } catch { /* private mode */ } };
const recall = () => { try { return localStorage.getItem(KEY) ?? undefined; } catch { return undefined; } };

/** The phone remote: installable, works with any registered TV. */
export function Remote({ tvId }: { tvId?: string }) {
  const tvs = useQuery(() => api.get<Tv[]>('/tvs'));
  const channels = useQuery(() => api.get<ChannelView[]>('/channels'));
  const live = useLiveStatus();
  const { attempt } = useToast();
  const [busy, setBusy] = useState(false);

  useLiveEvent(['tv.state', 'tv.removed'], tvs.reload);
  useLiveEvent(['channel.now', 'schedule.changed'], channels.reload);

  const list = tvs.data ?? [];
  const id = tvId ?? (list.find((t) => t.id === recall()) ?? list[0])?.id;
  const tv = list.find((t) => t.id === id);

  useEffect(() => { if (id) remember(id); }, [id]);
  useEffect(() => { if (!tvId && id) navigate(`remote/${id}`); }, [tvId, id]);

  const send = async (path: string, body: unknown) => {
    if (!id) return;
    setBusy(true);
    await attempt(() => api.post(`/tvs/${id}/${path}`, body));
    setBusy(false);
  };

  const groups = useMemo(() => {
    const cs = channels.data ?? [];
    const names = Array.from(new Set(cs.map((c) => c.group ?? '')));
    return names.map((g) => ({ g, items: cs.filter((c) => (c.group ?? '') === g) }));
  }, [channels.data]);

  if (tvs.loading && !tvs.data) return <div className="page"><Spinner /></div>;
  if (list.length === 0) {
    return <div className="page"><Card><Empty>No TVs yet. Open <span className="mono">{location.origin}/?tv=living-room</span> on a screen first.</Empty></Card></div>;
  }

  return (
    <div className="remote stack">
      <Card className="stack">
        <div className="row">
          <span className={`dot ${tv?.online ? 'on' : ''}`} title={tv?.online ? 'Connected' : 'No player connected'} />
          <select className="select" value={id} onChange={(e) => navigate(`remote/${e.target.value}`)} aria-label="TV">
            {list.map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
          </select>
          <span className={`dot ${live ? 'on' : ''}`} title={live ? 'Live updates on' : 'Reconnecting…'} style={{ opacity: 0.6 }} />
        </div>
        {tv && !tv.online && <p className="small dim" style={{ margin: 0 }}>No player is connected for this TV. Open <span className="mono">/?tv={tv.id}</span> on the screen.</p>}
        <div className="row"><Btn className="big" disabled={busy} onClick={() => send('channel', { step: 'up' })}>Ch ▲</Btn><Btn className="big" disabled={busy} onClick={() => send('channel', { step: 'down' })}>Ch ▼</Btn></div>
        <div className="row">
          <Btn className="big" disabled={busy} onClick={() => send('volume', { level: Math.max(0, (tv?.volume ?? 50) - 5) })}>Vol −</Btn>
          <Btn className="big" disabled={busy} onClick={() => send('volume', { muted: !tv?.muted })}>{tv?.muted ? 'Unmute' : 'Mute'}</Btn>
          <Btn className="big" disabled={busy} onClick={() => send('volume', { level: Math.min(100, (tv?.volume ?? 50) + 5) })}>Vol +</Btn>
        </div>
        <div className="row"><Btn className="big" kind={tv?.power ? undefined : 'primary'} disabled={busy} onClick={() => send('power', { on: !tv?.power })}>{tv?.power ? 'Power: on' : 'Power: off'}</Btn></div>
        {tv && <div className="small dim" style={{ textAlign: 'center' }}>Volume {tv.muted ? 'muted' : tv.volume}</div>}
      </Card>

      <Card className="channels">
        {groups.map(({ g, items }) => (
          <div key={g || 'other'}>
            {groups.length > 1 && <h3 style={{ margin: '10px 4px 4px' }}>{g || 'Other'}</h3>}
            {items.map((c) => (
              <div key={c.id} className={`ch ${tv?.channel === c.number ? 'active' : ''}`} onClick={() => send('channel', { number: c.number })} role="button" tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && send('channel', { number: c.number })}>
                <span className="num-badge">{c.number}</span>
                <div className="grow">
                  <div style={{ fontWeight: 600 }}>{c.name}</div>
                  <div className="small dim ellipsis">{c.kind === 'guide' ? 'Live guide' : c.now ? `${c.now.show ?? c.now.title}` : '—'}</div>
                </div>
              </div>
            ))}
          </div>
        ))}
      </Card>
    </div>
  );
}
