import { useState } from 'react';
import { api } from '../api';
import { isLoopback, shareHosts, shareLinks, shareSchemes, type Scheme } from '../share';
import { useQuery } from '../live';
import type { AuthStatus, System } from '../types';
import { Badge, Btn, Card, CopyField, Field, Segmented, Spinner, useToast } from '../ui';

export function Settings() {
  const sys = useQuery(() => api.get<System>('/system'));
  const auth = useQuery(() => api.get<AuthStatus>('/auth'));
  const { attempt } = useToast();
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

        <h3>Plex, Jellyfin and Emby</h3>
        <p className="small dim">TVMaster imitates a network tuner (HDHomeRun). Add a tuner device with this address, then use the guide link above. These servers use plain HTTP.</p>
        {links.tuner.map((l) => <CopyField key={l.id} label={l.label} value={l.value} />)}

        <h3>Browser and phone</h3>
        <p className="small dim">On a phone, open the remote and use “Add to Home Screen” to keep it like an app.</p>
        {links.web.map((l) => <CopyField key={l.id} label={l.label} value={l.value} />)}
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
        {s && !s.auth_required && <p className="small dim">Anyone on your network can control this server. To require a token, start it with <span className="mono">tvmaster serve --token &lt;token&gt;</span> (run <span className="mono">tvmaster token</span> to make one).</p>}
        {auth.data?.required && <div><Btn onClick={() => attempt(() => api.post('/auth/logout')).then(() => location.reload())}>Sign out</Btn></div>}
      </Card>
    </div>
  );
}
