import { useEffect, useState } from 'react';
import { api } from '../api';
import { formatClock, formatDay, formatDuration } from '../lib';
import { useLiveEvent, useQuery } from '../live';
import { href, navigate } from '../router';
import type { ChannelView, Cursor, Program, ScheduleItem } from '../types';
import { Badge, Btn, Card, Empty, Field, Progress, Segmented, Spinner, useToast } from '../ui';

const KIND_CLASS: Record<string, string> = { show: 'tl-show', commercial: 'tl-commercial', bumper: 'tl-bumper', filler: 'tl-filler' };

function Timeline({ items, from, to, tz }: { items: ScheduleItem[]; from: number; to: number; tz?: string }) {
  const span = to - from;
  const now = Date.now();
  return (
    <div className="stack" style={{ gap: 6 }}>
      <div className="timeline" role="img" aria-label="Schedule timeline">
        {items.map((it, i) => {
          const s = Math.max(it.start_ms, from);
          const e = Math.min(it.end_ms, to);
          if (e <= s) return null;
          const w = ((e - s) / span) * 100;
          return (
            <div key={i} className={`tl-seg ${KIND_CLASS[it.kind] ?? ''}`} style={{ left: `${((s - from) / span) * 100}%`, width: `${w}%` }} title={`${it.title ?? it.kind} · ${formatClock(it.start_ms, tz)}–${formatClock(it.end_ms, tz)}`}>
              {w > 6 && <span className="ellipsis">{it.kind === 'bumper' ? it.bumper?.replace('_', ' ') ?? 'bumper' : it.title ?? it.kind}</span>}
            </div>
          );
        })}
        {now >= from && now <= to && <div className="tl-now" style={{ left: `${((now - from) / span) * 100}%` }} />}
      </div>
      <div className="row between small dim"><span>{formatClock(from, tz)}</span><span>{formatClock(from + span / 2, tz)}</span><span>{formatClock(to, tz)}</span></div>
    </div>
  );
}

export function Schedule({ channelId }: { channelId?: string }) {
  const channels = useQuery(() => api.get<ChannelView[]>('/channels'));
  const programmable = (channels.data ?? []).filter((c) => c.kind === 'programming');
  const id = channelId ?? programmable[0]?.id;
  const [hours, setHours] = useState('6');
  const [mode, setMode] = useState<'guide' | 'items'>('guide');
  const { say, attempt } = useToast();

  useEffect(() => {
    if (!channelId && programmable[0]) navigate(`schedule/${programmable[0].id}`);
  }, [channelId, programmable]);

  const channel = programmable.find((c) => c.id === id);
  const tz = channel?.lineup.timezone || undefined;
  const guide = useQuery(() => (id ? api.get<Program[]>(`/channels/${id}/schedule?hours=${hours}`) : Promise.resolve([] as Program[])), [id, hours]);
  const items = useQuery(() => (id && mode === 'items' ? api.get<ScheduleItem[]>(`/channels/${id}/schedule?hours=${Math.min(Number(hours), 3)}&detail=items`) : Promise.resolve([] as ScheduleItem[])), [id, hours, mode]);
  const cursors = useQuery(() => (id ? api.get<Record<string, Cursor>>(`/channels/${id}/cursors`) : Promise.resolve({} as Record<string, Cursor>)), [id]);

  useLiveEvent(['schedule.changed'], (m) => {
    if (m.channel === id) { guide.reload(); items.reload(); cursors.reload(); }
  });

  const reroll = async () => {
    const r = await attempt(() => api.post<{ items: number }>(`/channels/${id}/reroll`));
    if (r) say(`Re-rolled: ${r.items} new items`);
  };

  const now = Date.now();
  const winFrom = Math.floor(now / 600_000) * 600_000 - 10 * 60_000;
  const winTo = winFrom + Math.min(Number(hours), 3) * 3_600_000;

  return (
    <div className="page">
      <div className="page-head">
        <h1>Schedule</h1>
        <select className="select" style={{ width: 220 }} value={id ?? ''} onChange={(e) => navigate(`schedule/${e.target.value}`)}>
          {programmable.map((c) => <option key={c.id} value={c.id}>{c.number} · {c.name}</option>)}
        </select>
        <span className="right row">
          <Segmented value={mode} options={[{ value: 'guide', label: 'Guide' }, { value: 'items', label: 'Breaks & bumpers' }]} onChange={setMode} />
          <select className="select" style={{ width: 110 }} value={hours} onChange={(e) => setHours(e.target.value)}>
            {['3', '6', '12', '24', '48'].map((h) => <option key={h} value={h}>{h} hours</option>)}
          </select>
        </span>
      </div>

      {channels.loading && !channels.data && <Spinner />}
      {!id && channels.data && <Card><Empty>No programming channels yet. <a href={href('channels/new')}>Create one</a>.</Empty></Card>}

      {channel && (
        <>
          <Card className="row between wrap">
            <div>
              <div className="row"><span className="num-badge">{channel.number}</span><strong>{channel.name}</strong>{channel.autopilot ? <Badge tone="ok">Autopilot</Badge> : <Badge tone="warn">Locked</Badge>}</div>
              <div className="small dim" style={{ marginTop: 4 }}>{channel.now ? `Now: ${channel.now.show ?? channel.now.title}${channel.next ? ` · Next: ${channel.next.show ?? channel.next.title} at ${formatClock(channel.next.start_ms, tz)}` : ''}` : 'Nothing on right now'}</div>
            </div>
            <div className="row">
              <a className="btn" href={href(`channels/${channel.id}`)}>Edit channel</a>
              <Btn icon="refresh" onClick={reroll} title="Regenerate everything that has not aired yet">Re-roll</Btn>
            </div>
          </Card>

          {mode === 'items' ? (
            <Card className="stack">
              <div className="legend"><span><i className="tl-show" />Show</span><span><i className="tl-commercial" />Commercial</span><span><i className="tl-bumper" />Bumper</span><span><i className="tl-filler" />Filler</span></div>
              {items.loading && !items.data ? <Spinner /> : <Timeline items={items.data ?? []} from={winFrom} to={winTo} tz={tz} />}
              <table className="table">
                <thead><tr><th>Time</th><th>What</th><th>Length</th></tr></thead>
                <tbody>
                  {(items.data ?? []).slice(0, 300).map((it, i) => (
                    <tr key={i}>
                      <td className="mono">{formatClock(it.start_ms, tz)}</td>
                      <td><Badge tone={it.kind === 'show' ? 'accent' : it.kind === 'commercial' ? 'warn' : ''}>{it.kind === 'bumper' ? it.bumper?.replace('_', ' ') ?? 'clip' : it.kind}</Badge> {it.title ?? ''}{it.kind === 'show' && it.in_ms > 0 ? <span className="dim small"> · act from {formatDuration(it.in_ms)}</span> : null}</td>
                      <td className="dim">{formatDuration(it.duration_ms)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {(items.data ?? []).length === 0 && !items.loading && <Empty>Nothing scheduled in this window.</Empty>}
            </Card>
          ) : (
            <Card>
              {guide.loading && !guide.data ? <Spinner /> : (guide.data ?? []).length === 0 ? <Empty>Nothing scheduled. Add shows to the channel's libraries.</Empty> : (
                <table className="table">
                  <thead><tr><th>Day</th><th>Time</th><th>Programme</th><th>Block</th></tr></thead>
                  <tbody>
                    {(guide.data ?? []).map((p, i) => (
                      <tr key={i}>
                        <td className="dim">{formatDay(p.start_ms, tz)}</td>
                        <td className="mono">{formatClock(p.start_ms, tz)}–{formatClock(p.end_ms, tz)}</td>
                        <td><strong>{p.show ?? p.title}</strong>{p.show && <span className="dim"> — {p.title}</span>}{p.season != null && p.episode != null && <span className="dim small"> · S{String(p.season).padStart(2, '0')}E{String(p.episode).padStart(2, '0')}</span>}</td>
                        <td>{p.label && <Badge>{p.label}</Badge>}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </Card>
          )}

          <Card className="stack">
            <h2>Where each show is up to</h2>
            <p className="small dim">“Aired” episodes have started; “scheduled” ones are already on the schedule. A re-roll never skips or repeats aired episodes.</p>
            {Object.keys(cursors.data ?? {}).length === 0 ? <span className="dim small">No progress recorded yet.</span> : (
              <div className="stack" style={{ gap: 10 }}>
                {Object.entries(cursors.data ?? {}).sort(([a], [b]) => a.localeCompare(b)).map(([k, c]) => (
                  <Field key={k} label={k.replace(/^show:/, '').replace(/^pool:/, 'pool ').replace(/^event:/, 'event ')}>
                    <Progress value={c.committed} max={Math.max(c.reserved, 1)} />
                    <span className="small dim">{c.committed.toLocaleString()} aired · {c.reserved.toLocaleString()} scheduled</span>
                  </Field>
                ))}
              </div>
            )}
          </Card>
        </>
      )}
    </div>
  );
}
