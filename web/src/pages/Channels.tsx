import { api } from '../api';
import { lineupSummary } from '../lib';
import { useLiveEvent, useQuery } from '../live';
import { href } from '../router';
import type { ChannelView } from '../types';
import { Badge, Btn, Card, Empty, Spinner } from '../ui';

export function Channels() {
  const q = useQuery(() => api.get<ChannelView[]>('/channels'));
  useLiveEvent(['channel.now', 'schedule.changed'], q.reload);
  const list = q.data ?? [];
  // Group headings, like the player and the remote.
  const groups = Array.from(new Set(list.map((c) => c.group ?? '')));

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
        groups.map((g) => (
          <div key={g || 'other'}>
            {groups.length > 1 && <h3>{g || 'Ungrouped'}</h3>}
            <div className="grid cols-2">
              {list.filter((c) => (c.group ?? '') === g).map((c) => (
                <a key={c.id} href={href(`channels/${c.id}`)} style={{ color: 'inherit', textDecoration: 'none' }}>
                  <Card className="clickable">
                    <div className="row">
                      <span className="num-badge">{c.number}</span>
                      <div className="grow">
                        <div className="row">
                          <strong className="ellipsis">{c.name}</strong>
                          {c.kind === 'guide' && <Badge tone="accent">Guide</Badge>}
                          {c.kind === 'test' && <Badge tone="accent">Test pattern</Badge>}
                          {c.autopilot ? <Badge tone="ok">Autopilot</Badge> : <Badge tone="warn">Locked</Badge>}
                        </div>
                        <div className="small dim ellipsis">
                          {c.kind === 'guide' ? `${c.guide.hours}-hour live guide` : c.kind === 'test' ? `Test pattern: ${c.options?.off_air?.screen ?? 'card'}` : c.strategy === 'block_grid' || c.lineup.blocks.length ? lineupSummary(c.lineup) : `${c.strategy.replace('_', ' ')}${c.chaos ? ` · chaos ${c.chaos}%` : ''}`}
                        </div>
                      </div>
                      <span className={`dot ${c.on_air ? 'on' : ''}`} />
                    </div>
                  </Card>
                </a>
              ))}
            </div>
          </div>
        ))
      )}
      <div className="row"><Btn small onClick={q.reload} icon="refresh">Refresh</Btn></div>
    </div>
  );
}
