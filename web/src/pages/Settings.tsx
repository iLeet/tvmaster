import { useEffect, useState } from 'react';
import { api } from '../api';
import { href } from '../router';
import { isLoopback, shareHosts, shareLinks, shareSchemes, type Scheme } from '../share';
import { useQuery } from '../live';
import type { AuthStatus, MonitorSettings, MonitorState, SecurityStatus, System, TimezoneState } from '../types';
import { Badge, Btn, Card, ConfirmBtn, CopyField, Field, Segmented, Spinner, Toggle, useToast } from '../ui';
import { Backups } from './Backups';
import { bytes, busyWord, capacitySentence, uptime, type SysStats } from '../sysstats';

const TABS: { id: string; label: string }[] = [
  { id: 'station', label: 'Station' },
  { id: 'devices', label: 'Connect devices' },
  { id: 'alerts', label: 'Alerts' },
  { id: 'backups', label: 'Backups' },
  { id: 'server', label: 'Server' },
];

function Meter({ label, pct, text, tone }: { label: string; pct: number; text: string; tone?: 'ok' | 'warn' | 'bad' }) {
  const t = tone ?? (pct < 60 ? 'ok' : pct < 85 ? 'warn' : 'bad');
  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className="row between"><span>{label}</span><span className="dim small">{text}</span></div>
      <div style={{ height: 10, borderRadius: 5, background: 'var(--edge-soft)', overflow: 'hidden' }}>
        <div style={{ width: `${Math.max(1, Math.min(100, pct))}%`, height: '100%', background: t === 'ok' ? 'var(--ok, #3fb27f)' : t === 'warn' ? 'var(--warn, #e0a030)' : 'var(--bad, #e05555)', transition: 'width .6s' }} />
      </div>
    </div>
  );
}

/** The machine this server runs on and how hard it is working, refreshed every few seconds. */
function Machine() {
  const [st, setSt] = useState<SysStats | null>(null);
  useEffect(() => {
    let live = true;
    const load = () => api.get<SysStats>('/system/stats').then((d) => live && setSt(d)).catch(() => undefined);
    load();
    const id = setInterval(load, 4000);
    return () => { live = false; clearInterval(id); };
  }, []);
  if (!st) return <Card className="stack"><h2>This machine</h2><Spinner /></Card>;
  const h = st.host;
  const busy = busyWord(st.cpu.usage_pct, st.cpu.load[0], h.threads);
  const memPct = st.memory.total ? (st.memory.used / st.memory.total) * 100 : 0;
  return (
    <>
      <Card className="stack">
        <div className="row between wrap">
          <h2 style={{ margin: 0 }}>How it is doing</h2>
          <Badge tone={busy.tone === 'ok' ? 'ok' : 'warn'}>{busy.word}</Badge>
        </div>
        <Meter label="Processor" pct={st.cpu.usage_pct} text={`${st.cpu.usage_pct.toFixed(0)}% of ${h.threads} threads`} />
        <Meter label="Memory" pct={memPct} text={`${bytes(st.memory.used)} of ${bytes(st.memory.total)}${st.memory.limited_by_container ? ' (this container’s limit)' : ''}`} />
        <div className="row wrap small dim">
          <span>Load average {st.cpu.load.map((x) => x.toFixed(2)).join(' · ')} <span title="the last 1, 5 and 15 minutes; about the number of threads is full">(1, 5, 15 min)</span></span>
          {st.memory.swap_total > 0 && <span>Swap {bytes(st.memory.swap_used)} of {bytes(st.memory.swap_total)}</span>}
        </div>
        <p>{capacitySentence(st.capacity, st.encoder)}</p>
        {st.cpu.per_core.length > 1 && (
          <details>
            <summary className="small dim" style={{ cursor: 'pointer' }}>Each processor thread</summary>
            <div className="stack" style={{ gap: 6, marginTop: 8 }}>
              {st.cpu.per_core.map((p, i) => <Meter key={i} label={`Thread ${i + 1}`} pct={p} text={`${p.toFixed(0)}%`} />)}
            </div>
          </details>
        )}
      </Card>

      <Card className="stack">
        <h2>What TVMaster is using</h2>
        <table className="table"><tbody>
          <tr><td className="dim">The server itself</td><td>{st.tvmaster ? `${st.tvmaster.cpu_pct.toFixed(0)}% of a thread · ${bytes(st.tvmaster.memory)} · up ${uptime(st.tvmaster.uptime_s)}` : '—'}</td></tr>
          <tr><td className="dim">Encoders (FFmpeg)</td><td>{st.ffmpeg.processes} running · {st.ffmpeg.cpu_cores.toFixed(2)} cores · {bytes(st.ffmpeg.memory)}</td></tr>
          <tr><td className="dim">Encoding with</td><td>{st.encoder ?? 'not available'}{st.encoder === 'software' ? ' (the processor does it all)' : ''}</td></tr>
          <tr><td className="dim">On air now</td><td>{st.on_air.length ? st.on_air.join(', ') : 'nothing: channels start when someone tunes in'}</td></tr>
        </tbody></table>
        {st.ffmpeg.top.length > 0 && <p className="small dim">Busiest encoders: {st.ffmpeg.top.map((f) => `${f.cpu_pct.toFixed(0)}% / ${bytes(f.memory)}`).join(' · ')}</p>}
      </Card>

      <Card className="stack">
        <h2>The machine</h2>
        <table className="table"><tbody>
          <tr><td className="dim">Name</td><td>{h.name ?? '—'}{h.container && <> <Badge>running in a container</Badge></>}</td></tr>
          <tr><td className="dim">System</td><td>{[h.os, h.os_version].filter(Boolean).join(' ')} · {h.arch}{h.kernel ? ` · kernel ${h.kernel}` : ''}</td></tr>
          <tr><td className="dim">Processor</td><td>{h.cpu_model ?? '—'} · {h.cores ? `${h.cores} cores, ` : ''}{h.threads} threads</td></tr>
          <tr><td className="dim">Memory</td><td>{bytes(st.memory.total)} · {bytes(st.memory.available)} free</td></tr>
          <tr><td className="dim">Up for</td><td>{uptime(h.uptime_s)}</td></tr>
          <tr><td className="dim">TVMaster</td><td>{st.version}</td></tr>
        </tbody></table>
      </Card>

      <Card className="stack">
        <h2>Disks</h2>
        {st.disks.length === 0 ? <p className="small dim">No disk information.</p> : st.disks.map((d) => {
          const used = d.total - d.free;
          return <Meter key={d.mount} label={`${d.mount}${d.name ? ` (${d.name})` : ''}`} pct={d.total ? (used / d.total) * 100 : 0} text={`${bytes(d.free)} free of ${bytes(d.total)} · ${d.used_for.join(', ')}`} />;
        })}
      </Card>
    </>
  );
}

export function Settings({ tab: wanted }: { tab?: string }) {
  const tab = TABS.some((t) => t.id === wanted) ? (wanted as string) : 'station';
  const sys = useQuery(() => api.get<System>('/system'));
  const auth = useQuery(() => api.get<AuthStatus>('/auth'));
  const sec = useQuery(() => api.get<SecurityStatus>('/security'));
  const monitor = useQuery(() => api.get<MonitorState>('/monitor'));
  const tz = useQuery(() => api.get<TimezoneState>('/timezone'));
  const deviceZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const [zoneText, setZoneText] = useState<string | null>(null);
  const [applyAll, setApplyAll] = useState(true);
  const setZone = (zone: string | null) =>
    attempt(async () => {
      await api.put<TimezoneState>('/timezone', { zone, apply_to_channels: applyAll });
      setZoneText(null);
      tz.reload();
      sys.reload();
    }, 'Time zone saved');
  const { attempt } = useToast();
  const [custom, setCustom] = useState('');
  const setSecurity = (body: { enabled: boolean; token?: string; rotate?: boolean }) =>
    attempt(async () => {
      await api.put<SecurityStatus>('/security', body);
      setCustom('');
      sec.reload();
      sys.reload();
      auth.reload();
    });
  const [hook, setHook] = useState<string | null>(null);
  const saveMonitor = (patch: Partial<MonitorSettings>) =>
    attempt(async () => {
      if (!monitor.data) return;
      await api.put<MonitorState>('/monitor', { ...monitor.data.settings, ...patch });
      monitor.reload();
    });
  const s = sys.data;
  const net = s?.network;
  const pageScheme: Scheme = location.protocol === 'https:' ? 'https' : 'http';
  const hosts = shareHosts(net, location.hostname);
  const schemes = shareSchemes(net, pageScheme);
  const [host, setHost] = useState<string | null>(null);
  const [scheme, setScheme] = useState<Scheme | null>(null);
  const [token, setToken] = useState('');
  const chosen = {
    host: host && hosts.includes(host) ? host : hosts[0] ?? location.hostname,
    scheme: scheme && schemes.includes(scheme) ? scheme : schemes.includes(pageScheme) ? pageScheme : schemes[0],
    token,
  };
  const links = shareLinks(net, chosen, location.port);
  const loopbackOnly = isLoopback(location.hostname) && !!net;

  return (
    <div className="page">
      <div className="page-head"><h1>Settings</h1></div>
      <div className="tabs" role="tablist">
        {TABS.map((t) => <a key={t.id} role="tab" className={tab === t.id ? 'on' : ''} href={href(t.id === 'station' ? 'settings' : `settings/${t.id}`)}>{t.label}</a>)}
      </div>

      {tab === 'devices' && <Card className="stack">
        <h2>Connect devices</h2>
        <p className="small dim">Addresses to give other apps, TVs and phones. Pick how they will reach this server.</p>
        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          {hosts.length > 1 && (
            <Field label="Address">
              <select className="select" style={{ minWidth: 220 }} value={chosen.host} onChange={(e) => setHost(e.target.value)}>
                {hosts.map((h) => <option key={h} value={h}>{h}</option>)}
              </select>
            </Field>
          )}
          {schemes.length > 1 && (
            <Field label="Connection">
              <Segmented value={chosen.scheme} options={schemes.map((x) => ({ value: x, label: x.toUpperCase() }))} onChange={setScheme} />
            </Field>
          )}
          {s?.auth_required && (
            <Field label="Access token" hint="Paste it to add it to the links below. It is not stored.">
              <input className="input mono" type="password" autoComplete="off" style={{ minWidth: 260 }} placeholder="Optional" value={token} onChange={(e) => setToken(e.target.value)} />
            </Field>
          )}
        </div>
        {loopbackOnly && <p className="small" style={{ color: 'var(--warn)' }}>You opened this page as {location.hostname}, so other devices need one of the addresses above, and the server must be listening on your network (not just on this machine).</p>}
        {token.trim() && <p className="small dim">Links with the token in them can control your server. Share them like a password.</p>}

        <h3>IPTV apps</h3>
        <p className="small dim">VLC, Kodi, TiviMate and most others take the playlist and the guide. Every channel is one shared stream, so many TVs can watch at once.</p>
        {links.iptv.map((l) => <CopyField key={l.id} label={l.label} value={l.value} />)}
        {chosen.scheme === 'https' && net?.tls.self_signed && <p className="small dim">IPTV apps usually refuse a self-signed certificate. If one will not connect over HTTPS, use the HTTP addresses.</p>}

        <h3>Smart TV</h3>
        <p className="small dim">Open this in your TV's own web browser (Samsung Internet, the LG browser, Silk on Fire TV, Chrome on Android TV). It fills the screen and follows the remote: <strong>CH ▲▼</strong> or <strong>▲▼</strong> flip channels, <strong>number keys</strong> jump to a channel, <strong>OK</strong> opens the channel list, <strong>Info</strong> says what is on, <strong>◀</strong> goes back to the last channel. For a nicer TV experience, an IPTV app with the playlist below works too.</p>
        {links.smarttv.map((l) => <CopyField key={l.id} label={l.label} value={l.value} />)}

        <h3>Plex, Jellyfin and Emby</h3>
        <p className="small dim">TVMaster imitates a network tuner (HDHomeRun). Add a tuner device with this address, then use the guide link above. These servers use plain HTTP.</p>
        {links.tuner.map((l) => <CopyField key={l.id} label={l.label} value={l.value} />)}

        <h3>Browser and phone</h3>
        <p className="small dim">On a phone, open the remote and use “Add to Home Screen” to keep it like an app.</p>
        {links.web.map((l) => <CopyField key={l.id} label={l.label} value={l.value} />)}
      </Card>}

      {tab === 'station' && <Card className="stack">
        <h2>Time zone</h2>
        <p className="small dim">Every schedule, the guide and the clock on screen use this. It starts as the server's own zone; if that is not where you are (a server in a container is often on UTC), set it here.</p>
        {!tz.data ? <Spinner /> : (
          <>
            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <Field label="Station time zone" hint={tz.data.source === 'setting' ? 'Chosen by you.' : tz.data.source === 'environment' ? 'From the server’s TZ setting.' : 'Automatic: the server machine’s own zone.'}>
                <input className="input mono" list="settings-zone-list" style={{ width: 240 }} value={zoneText ?? tz.data.zone} onChange={(e) => setZoneText(e.target.value)} />
                <datalist id="settings-zone-list">{(Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf?.('timeZone')?.map((z) => <option key={z} value={z} />)}</datalist>
              </Field>
              <Btn kind="primary" disabled={zoneText === null || zoneText === tz.data.zone} onClick={() => setZone((zoneText ?? '').trim() || null)}>Save</Btn>
              {deviceZone !== tz.data.zone && <Btn onClick={() => setZone(deviceZone)}>Use this device’s: {deviceZone}</Btn>}
              {tz.data.source === 'setting' && <Btn onClick={() => setZone(null)}>Back to automatic</Btn>}
            </div>
            <Toggle checked={applyAll} onChange={setApplyAll} label={<span>Move every channel's schedule into this zone <span className="dim small">time slots keep their clock times (3:30 PM stays 3:30 PM) and schedules are re-planned</span></span>} />
            {tz.data.channels_elsewhere.length > 0 && <p className="small" style={{ color: 'var(--warn)' }}>{tz.data.channels_elsewhere.map((c) => `${c.name} (${c.zone})`).join(', ')} {tz.data.channels_elsewhere.length === 1 ? 'is' : 'are'} in another zone.</p>}
          </>
        )}
      </Card>}

      {tab === 'station' && <Card className="stack">
        <h2>New episodes</h2>
        {!monitor.data ? <Spinner /> : (
          <>
            <Field label="Look for new episodes" hint="Re-reads every library's folder so files you add (or remove) show up by themselves. Only what changed is read, and a file is left alone until it has stopped being copied.">
              <select className="select" style={{ width: 220 }} value={monitor.data.settings.watch_minutes} onChange={(e) => saveMonitor({ watch_minutes: Number(e.target.value) })}>
                <option value={0}>Only when I ask</option>
                {[5, 10, 15, 30, 60, 120, 360].map((m) => <option key={m} value={m}>Every {m >= 60 ? `${m / 60} hour${m > 60 ? 's' : ''}` : `${m} minutes`}</option>)}
              </select>
            </Field>
            <div><Btn onClick={() => attempt(async () => { const r = await api.post<{ added: number; removed: number; back: number; unreachable: number }>('/monitor/rescan'); return r; }, 'Libraries re-read').then(() => monitor.reload())}>Re-read every library now</Btn></div>
          </>
        )}
      </Card>}

      {tab === 'alerts' && <Card className="stack">
        <h2>Alerts</h2>
        <p className="small dim">Told about problems before the family is: a drive that goes offline, a channel that keeps failing, FFmpeg missing.</p>
        {!monitor.data ? <Spinner /> : (
          <>
            <Toggle checked={monitor.data.settings.alerts} onChange={(v) => saveMonitor({ alerts: v })} label="Tell me when something goes wrong" />
            {monitor.data.settings.alerts && (
              <div className="stack nest">
                <Field label="Also send alerts to a web address" hint="ntfy, Slack, Discord, Home Assistant and similar take a POST. Leave blank to see alerts only here and in the app.">
                  <span className="row">
                    <input className="input mono" style={{ minWidth: 320 }} placeholder="https://ntfy.sh/my-tv-station" value={hook ?? monitor.data.settings.webhook ?? ''} onChange={(e) => setHook(e.target.value)} />
                    <Btn disabled={hook === null || hook === (monitor.data.settings.webhook ?? '')} onClick={() => saveMonitor({ webhook: (hook ?? '').trim() || null }).then(() => setHook(null))}>Save</Btn>
                    <Btn disabled={!monitor.data.settings.webhook} onClick={() => attempt(() => api.post('/monitor/test'), 'Test message sent')}>Send a test</Btn>
                  </span>
                </Field>
                {!monitor.data.curl && <p className="small" style={{ color: 'var(--warn)' }}>The server needs <span className="mono">curl</span> to send these. It is not installed here.</p>}
                <Toggle checked={monitor.data.settings.webhook_text} onChange={(v) => saveMonitor({ webhook_text: v })} label={<span>Send plain text <span className="dim small">for ntfy; leave off for JSON</span></span>} />
                <Toggle checked={monitor.data.settings.notify_recovery} onChange={(v) => saveMonitor({ notify_recovery: v })} label="Say when it is fine again" />
              </div>
            )}
            {monitor.data.alerts.length === 0 ? <p className="small dim">Nothing is wrong right now.</p> : monitor.data.alerts.map((a) => <p key={a.id} className="small" style={{ color: 'var(--warn)' }}><strong>{a.title}.</strong> {a.detail}</p>)}
          </>
        )}
      </Card>}

      {tab === 'server' && <Machine />}

      {tab === 'server' && <Card className="stack">
        <h2>Access token</h2>
        <p className="small dim">Optional. With a token on, every app, TV and browser must present it; with it off, anyone on your network can use and change this server.</p>
        {!sec.data ? <Spinner /> : sec.data.locked ? (
          <>
            <p className="small">The token was set with <span className="mono">--token</span> when the server started, so it is managed there. Restart without it to manage it here.</p>
            <Toggle
              checked={!!sec.data.open_viewing}
              onChange={(v) => attempt(async () => { await api.put('/security', { open_viewing: v }); sec.reload(); }, v ? 'Anyone can watch without the token' : 'Watching needs the token again')}
              label={<span>Let anyone tune in without the token <span className="dim small">changing anything still needs it</span></span>}
            />
          </>
        ) : (
          <>
            <Toggle
              checked={sec.data.enabled}
              onChange={(v) => setSecurity({ enabled: v })}
              label={sec.data.enabled ? 'Token required' : 'No token (open to your network)'}
            />
            {sec.data.enabled && sec.data.token && (
              <>
                <CopyField label="Current token" value={sec.data.token} />
                <div className="row wrap" style={{ alignItems: 'flex-end' }}>
                  <ConfirmBtn
                    label="Make a new token"
                    confirm="Everything using the old token (IPTV apps, other browsers, the TVMaster apps) will be signed out until it gets the new one."
                    onConfirm={() => setSecurity({ enabled: true, rotate: true })}
                  />
                  <Field label="Or choose your own" hint="8-128 letters, digits, - _ . ~">
                    <input className="input mono" autoComplete="off" style={{ minWidth: 240 }} value={custom} onChange={(e) => setCustom(e.target.value)} />
                  </Field>
                  <Btn disabled={!custom.trim()} onClick={() => setSecurity({ enabled: true, token: custom.trim() })}>Use this token</Btn>
                </div>
              </>
            )}
            <Toggle
              checked={!!sec.data.open_viewing}
              onChange={(v) => attempt(async () => { await api.put('/security', { open_viewing: v }); sec.reload(); }, v ? 'Anyone can watch without the token' : 'Watching needs the token again')}
              label={<span>Let anyone tune in without the token <span className="dim small">streams, the playlist and the web player stay open; changing anything still needs the token</span></span>}
            />
            {sec.data.enabled && net && !net.tls.enabled && <p className="small" style={{ color: 'var(--warn)' }}>HTTPS is off, so the token crosses your network unencrypted.</p>}
          </>
        )}
      </Card>}

      {tab === 'server' && net?.tls.enabled && (
        <Card className="stack">
          <h2>HTTPS certificate</h2>
          <p className="small dim">{net.tls.self_signed
            ? 'This server made its own certificate. Browsers warn about it once, and the TVMaster apps ask you to trust it by this fingerprint, so compare them.'
            : 'This server uses a certificate you provided.'}</p>
          {net.tls.fingerprint && <CopyField label="SHA-256 fingerprint" value={net.tls.fingerprint} />}
        </Card>
      )}
      {tab === 'server' && net && !net.tls.enabled && (
        <Card className="stack">
          <h2>HTTPS</h2>
          <p className="small dim">HTTPS is off, so the access token crosses your network unencrypted. Turn it on with <span className="mono">tvmaster serve --tls</span> (or <span className="mono">TVMASTER_TLS=1</span>). It uses a self-signed certificate unless you give it <span className="mono">--tls-cert</span> and <span className="mono">--tls-key</span>.</p>
        </Card>
      )}

      {tab === 'backups' && <Backups />}

      {tab === 'server' && <Card className="stack">
        <h2>This server</h2>
        {!s ? <Spinner /> : (
          <table className="table">
            <tbody>
              <tr><td className="dim">Version</td><td>{s.version}</td></tr>
              <tr><td className="dim">System</td><td>{s.os} · {s.arch}</td></tr>
              <tr><td className="dim">Time zone</td><td>{s.timezone}</td></tr>
              <tr><td className="dim">FFmpeg</td><td>{s.ffmpeg ? <>{s.ffmpeg.version} <Badge tone="ok">{s.ffmpeg.encoder ?? 'software'}</Badge> {s.ffmpeg.has_drawtext ? '' : <span className="dim small">(no drawtext; bumpers use TVMaster's own renderer)</span>}</> : <Badge tone="warn">not found</Badge>}</td></tr>
              <tr><td className="dim">On air now</td><td>{s.on_air.length ? s.on_air.join(', ') : 'none'}</td></tr>
              <tr><td className="dim">Connection</td><td>{net?.tls.enabled ? <Badge tone="ok">HTTPS available</Badge> : <Badge tone="warn">HTTP only</Badge>}</td></tr>
              <tr><td className="dim">Access</td><td>{s.auth_required ? <Badge tone="ok">token required</Badge> : <Badge tone="warn">open to your network</Badge>}</td></tr>
            </tbody>
          </table>
        )}
        <div className="row wrap">
          {auth.data?.required && <Btn onClick={() => attempt(() => api.post('/auth/logout')).then(() => location.reload())}>Sign out</Btn>}
          <ConfirmBtn label="Restart the server" confirm="Restart the server now? Every channel stops for about ten seconds. It comes back by itself when it runs in Docker, as a service, or as the Linux or Windows package; started by hand it just stops." onConfirm={() => attempt(() => api.post('/server/restart'), 'Restarting… the page reconnects in a moment')} />
        </div>
      </Card>}
    </div>
  );
}
