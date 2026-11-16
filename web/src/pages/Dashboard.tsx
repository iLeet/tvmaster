import { useState } from 'react';
import { api } from '../api';
import { formatClock, formatDuration } from '../lib';
import { useLiveEvent, useLiveStatus, useQuery } from '../live';
import { href } from '../router';
import type { ChannelView, Library, LiveState, MonitorState, System, Tv, TimezoneState } from '../types';
import { Badge, Card, Empty, Icon, Spinner } from '../ui';

export function Dashboard() {
  const channels = useQuery(() => api.get<ChannelView[]>('/channels'));
  const tvs = useQuery(() => api.get<Tv[]>('/tvs'));
  const libs = useQuery(() => api.get<Library[]>('/libraries'));
  const sys = useQuery(() => api.get<System>('/system'));
  const monitor = useQuery(() => api.get<MonitorState>('/monitor'));
  const tz = useQuery(() => api.get<TimezoneState>('/timezone'));
  const takeover = useQuery(() => api.get<LiveState>('/live'));
  const device = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const [tzDismissed, setTzDismissed] = useState(() => { try { return localStorage.getItem('tvmaster.tzDismissed') === device; } catch { return false; } });
  const live = useLiveStatus();

  useLiveEvent(['channel.now', 'schedule.changed'], () => channels.reload());
  useLiveEvent(['tv.state', 'tv.removed'], () => tvs.reload());
  useLiveEvent(['alert.raised', 'alert.cleared'], () => monitor.reload());
  useLiveEvent(['live.changed'], () => takeover.reload());
  useLiveEvent(['library.changed'], () => libs.reload());

  const list = channels.data ?? [];
  const items = (libs.data ?? []).reduce((n, l) => n + l.items, 0);

  return (
    <div className="page">
      <div className="page-head">
        <h1>Home</h1>
        <span className="sub">What is on across the house right now</span>
        <span className="right row small dim">
          <span className={`dot ${live ? 'on' : ''}`} /> {live ? 'Live' : 'Connecting…'}
        </span>
      </div>

      {tz.data && tz.data.source !== 'setting' && tz.data.zone !== device && !tzDismissed && (
        <div className="err-banner" style={{ borderColor: 'var(--accent)', color: 'inherit' }}>
          <strong>Is this your time zone?</strong> The station is on <span className="mono">{tz.data.zone}</span>, but this device is in <span className="mono">{device}</span>. Schedules and the guide follow the station's zone, so programmes could air at the wrong hour.{' '}
          <button className="btn small primary" onClick={() => api.put('/timezone', { zone: device, apply_to_channels: true }).then(() => { tz.reload(); channels.reload(); })}>Use {device} for everything</button>{' '}
          <button className="btn small" onClick={() => { try { localStorage.setItem('tvmaster.tzDismissed', device); } catch { /* ignore */ } setTzDismissed(true); }}>It is right as it is</button>
        </div>
      )}
      {(takeover.data?.overrides.length ?? 0) > 0 && (
        <div className="err-banner">
          <strong>{takeover.data!.overrides.length} channel{takeover.data!.overrides.length === 1 ? ' is' : 's are'} showing a test screen or alert instead of programming.</strong>{' '}
          <a className="btn small primary" href={href('live')}>Open live control</a>
        </div>
      )}
      {(monitor.data?.alerts ?? []).map((a) => (
        <div key={a.id} className="err-banner" style={a.severity === 'warning' ? { borderColor: 'var(--warn)', color: 'var(--warn)' } : undefined}>
          <strong>{a.title}.</strong> {a.detail}
        </div>
      ))}
      {sys.data && !sys.data.ffmpeg && !(monitor.data?.alerts ?? []).some((a) => a.id === 'system:ffmpeg') && (
        <div className="err-banner">
          FFmpeg was not found, so channels cannot stream. Install FFmpeg (it needs <span className="mono">ffmpeg</span> and <span className="mono">ffprobe</span> on the PATH) and restart the server.
        </div>
      )}
      {libs.data && libs.data.length === 0 && (
        <Card>
          <h2>Welcome</h2>
          <p className="dim">Start by adding a folder of shows, then make a channel.</p>
          <a className="btn primary" href={href('libraries')}>Add your first library</a>
        </Card>
      )}

      <div className="grid cols-3">
        <Card>
          <h3>Channels</h3>
          <div style={{ fontSize: 30, fontWeight: 800 }}>{list.length}</div>
          <div className="dim small">{sys.data ? `${sys.data.on_air.length} on air now` : ''}</div>
        </Card>
        <Card>
          <h3>Library</h3>
          <div style={{ fontSize: 30, fontWeight: 800 }}>{items.toLocaleString()}</div>
          <div className="dim small">files in {libs.data?.length ?? 0} libraries</div>
        </Card>
        <Card>
          <h3>Server</h3>
          <div style={{ fontWeight: 700 }}>{sys.data ? `TVMaster ${sys.data.version}` : <Spinner />}</div>
          <div className="dim small">
            {sys.data?.ffmpeg ? `FFmpeg ${sys.data.ffmpeg.version} · ${sys.data.ffmpeg.encoder ?? 'software'}` : 'FFmpeg missing'}
          </div>
        </Card>
      </div>

      <div>
        <h3>On air</h3>
        {channels.loading && !channels.data ? <Spinner /> : list.length === 0 ? (
          <Card><Empty>No channels yet. <a href={href('channels/new')}>Create one</a>.</Empty></Card>
        ) : (
          <div className="grid cols-2">
            {list.map((c) => (
              <a key={c.id} href={href(`schedule/${c.id}`)} style={{ color: 'inherit', textDecoration: 'none' }}>
                <Card className="clickable">
                  <div className="row">
                    <span className="num-badge">{c.number}</span>
                    <div className="grow">
                      <div className="row"><strong className="ellipsis">{c.name}</strong>{c.group && <Badge>{c.group}</Badge>}{c.kind === 'guide' && <Badge tone="accent">Guide</Badge>}</div>
                      <div className="small dim ellipsis">
                        {c.kind === 'guide' ? 'Live listing of every channel' : c.now ? `${c.now.show ?? c.now.title}${c.now.show ? ` — ${c.now.title}` : ''}` : 'Nothing scheduled'}
                      </div>
                    </div>
                    <span className={`dot ${c.on_air ? 'on' : ''}`} title={c.on_air ? 'Streaming now' : 'Idle'} />
                  </div>
                  {c.next && (
                    <div className="small dim" style={{ marginTop: 8 }}>
                      Next {formatClock(c.next.start_ms, c.lineup.timezone || undefined)}: {c.next.show ?? c.next.title} · {formatDuration(c.next.end_ms - c.next.start_ms)}
                    </div>
                  )}
                </Card>
              </a>
            ))}
          </div>
        )}
      </div>

      <div>
        <div className="row between"><h3>TVs</h3><a className="small" href={href('tvs')}>Manage</a></div>
        <Card>
          {(tvs.data ?? []).length === 0 ? (
            <Empty>No TVs registered. Open <span className="mono">/?tv=living-room</span> on a screen, or <a href={href('tvs')}>add one</a>.</Empty>
          ) : (
            <div className="list">
              {(tvs.data ?? []).map((t) => {
                const ch = list.find((c) => c.number === t.channel);
                return (
                  <div key={t.id} className="row">
                    <span className={`dot ${t.online ? 'on' : ''}`} title={t.online ? 'Connected' : 'Not connected'} />
                    <strong>{t.name}</strong>
                    <span className="dim small grow">{!t.power ? 'Off' : ch ? `${ch.number} · ${ch.name}` : t.channel ? `Channel ${t.channel}` : 'No channel'}</span>
                    <a className="btn small" href={href(`remote/${t.id}`)}><Icon name="remote" /> Remote</a>
                  </div>
                );
              })}
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}
