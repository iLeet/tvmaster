import { api } from '../api';
import { lineupSummary } from '../lib';
import { useLiveEvent, useQuery } from '../live';
import { href } from '../router';
import type { ChannelView } from '../types';
import { Badge, Btn, Card, Empty, Spinner } from '../ui';

const SECTIONS: { id: ChannelView['kind']; title: string; about: string }[] = [
  { id: 'programming', title: 'TV channels', about: 'Shows from your libraries, on a schedule' },
  { id: 'info', title: 'Info channels', about: 'News, notices, what is on tonight, the time and pictures' },
  { id: 'guide', title: 'Guide', about: 'The scrolling listing of every channel' },
  { id: 'test', title: 'Test patterns', about: '' },
];

function summary(c: ChannelView): string {
  if (c.kind === 'guide') return `${c.guide.hours}-hour live guide`;
  if (c.kind === 'test') return `Test pattern: ${c.options?.off_air?.screen ?? 'card'}`;
  if (c.kind === 'info') {
    const n = c.info?.slides?.length ?? 0;
    return `${c.info?.about || 'Info channel'}${n ? ` · ${n} page${n === 1 ? '' : 's'}` : ''}`;
  }
  return c.strategy === 'block_grid' || c.lineup.blocks.length ? lineupSummary(c.lineup) : `${c.strategy.replace('_', ' ')}${c.chaos ? ` · chaos ${c.chaos}%` : ''}`;
}

export function Channels() {
  const q = useQuery(() => api.get<ChannelView[]>('/channels'));
  useLiveEvent(['channel.now', 'schedule.changed'], q.reload);
  const list = q.data ?? [];
  const sections = SECTIONS.map((s) => ({ ...s, items: list.filter((c) => c.kind === s.id) })).filter((s) => s.items.length > 0);

  return (
    <div className="page">
      <div className="page-head">
        <h1>Channels</h1>
        <span className="sub">Your stations, their programming and their guide</span>
        <a className="btn primary right" href={href('channels/new')}>+ New channel</a>
      </div>
      {q.error && <div className="err-banner">{q.error}</div>}
      {q.loading && !q.data ? <Spinner /> : list.length === 0 ? (
        <Card><Empty>No channels yet. Create your first one to put something on air.</Empty></Card>
      ) : (
        sections.map((s) => {
          // Within a section, channels the station has grouped (Kids, Movies) keep their headings.
          const groups = Array.from(new Set(s.items.map((c) => c.group ?? '')));
          return (
            <div key={s.id} className="stack">
              {sections.length > 1 && <h2 style={{ marginBottom: 0 }}>{s.title} <span className="small dim" style={{ fontWeight: 400 }}>{s.about}</span></h2>}
              {groups.map((g) => (
                <div key={g || 'other'} className="stack">
                  {groups.length > 1 && <h3>{g || 'Ungrouped'}</h3>}
                  <div className="grid cols-2">
                    {s.items.filter((c) => (c.group ?? '') === g).map((c) => (
                      <a key={c.id} href={href(`channels/${c.id}`)} style={{ color: 'inherit', textDecoration: 'none' }}>
                        <Card className="clickable">
                          <div className="row">
                            <span className="num-badge">{c.number}</span>
                            <div className="grow">
                              <div className="row">
                                <strong className="ellipsis">{c.name}</strong>
                                {c.kind === 'guide' && <Badge tone="accent">Guide</Badge>}
                                {c.kind === 'info' && <Badge tone="accent">Info</Badge>}
                                {c.kind === 'test' && <Badge tone="accent">Test pattern</Badge>}
                                {c.kind === 'programming' && (c.autopilot ? <Badge tone="ok">Autopilot</Badge> : <Badge tone="warn">Locked</Badge>)}
                              </div>
                              <div className="small dim ellipsis">{summary(c)}</div>
                            </div>
                            <span className={`dot ${c.on_air ? 'on' : ''}`} />
                          </div>
                        </Card>
                      </a>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          );
        })
      )}
      <div className="row"><Btn small onClick={q.reload} icon="refresh">Refresh</Btn></div>
    </div>
  );
}
