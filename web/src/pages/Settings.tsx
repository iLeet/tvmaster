import { useState } from 'react';
import { api } from '../api';
import { isLoopback, shareHosts, shareLinks, shareSchemes, type Scheme } from '../share';
import { useQuery } from '../live';
import type { AuthStatus, MonitorSettings, MonitorState, SecurityStatus, System, TimezoneState } from '../types';
import { Badge, Btn, Card, ConfirmBtn, CopyField, Field, Segmented, Spinner, Toggle, useToast } from '../ui';

export function Settings() {
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
      <div className="page-head"><h1>Settings</h1><span className="sub">Connect players, tuners and apps</span></div>

      <Card className="stack">
        <h2>Share</h2>
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
      </Card>

      <Card className="stack">
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
      </Card>

      <Card className="stack">
        <h2>Watching and alerts</h2>
        {!monitor.data ? <Spinner /> : (
          <>
            <Field label="Look for new episodes" hint="Re-reads every library's folder so files you add (or remove) show up by themselves. Only what changed is read, and a file is left alone until it has stopped being copied.">
              <select className="select" style={{ width: 220 }} value={monitor.data.settings.watch_minutes} onChange={(e) => saveMonitor({ watch_minutes: Number(e.target.value) })}>
                <option value={0}>Only when I ask</option>
                {[5, 10, 15, 30, 60, 120, 360].map((m) => <option key={m} value={m}>Every {m >= 60 ? `${m / 60} hour${m > 60 ? 's' : ''}` : `${m} minutes`}</option>)}
              </select>
            </Field>
            <div><Btn onClick={() => attempt(async () => { const r = await api.post<{ added: number; removed: number; back: number; unreachable: number }>('/monitor/rescan'); return r; }, 'Libraries re-read').then(() => monitor.reload())}>Re-read every library now</Btn></div>
            <Toggle checked={monitor.data.settings.alerts} onChange={(v) => saveMonitor({ alerts: v })} label={<span>Tell me when something goes wrong <span className="dim small">a library that goes offline, a channel that keeps failing, FFmpeg missing</span></span>} />
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
      </Card>

      <Card className="stack">
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
      </Card>

      {net?.tls.enabled && (
        <Card className="stack">
          <h2>HTTPS certificate</h2>
          <p className="small dim">{net.tls.self_signed
            ? 'This server made its own certificate. Browsers warn about it once, and the TVMaster apps ask you to trust it by this fingerprint, so compare them.'
            : 'This server uses a certificate you provided.'}</p>
          {net.tls.fingerprint && <CopyField label="SHA-256 fingerprint" value={net.tls.fingerprint} />}
        </Card>
      )}
      {net && !net.tls.enabled && (
        <Card className="stack">
          <h2>HTTPS</h2>
          <p className="small dim">HTTPS is off, so the access token crosses your network unencrypted. Turn it on with <span className="mono">tvmaster serve --tls</span> (or <span className="mono">TVMASTER_TLS=1</span>). It uses a self-signed certificate unless you give it <span className="mono">--tls-cert</span> and <span className="mono">--tls-key</span>.</p>
        </Card>
      )}

      <Card className="stack">
        <h2>Server</h2>
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
      </Card>
    </div>
  );
}
