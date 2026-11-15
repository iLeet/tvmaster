import { api } from '../api';
import { useQuery } from '../live';
import type { AuthStatus, System } from '../types';
import { Badge, Btn, Card, CopyField, Spinner, useToast } from '../ui';

export function Settings() {
  const sys = useQuery(() => api.get<System>('/system'));
  const auth = useQuery(() => api.get<AuthStatus>('/auth'));
  const { attempt } = useToast();
  const origin = location.origin;
  const host = location.host;
  const s = sys.data;

  return (
    <div className="page">
      <div className="page-head"><h1>Settings</h1><span className="sub">Connect players, tuners and apps</span></div>

      <Card className="stack">
        <h2>Watch anywhere</h2>
        <p className="small dim">Point any IPTV app (VLC, Kodi, TiviMate…) at the playlist and guide. Every channel is one shared stream, so many TVs can watch at once.</p>
        <CopyField label="Playlist (M3U)" value={`${origin}/iptv/playlist.m3u`} />
        <CopyField label="Programme guide (XMLTV)" value={`${origin}/iptv/guide.xml`} />
        <CopyField label="Web player" value={`${origin}/`} />
        {s?.auth_required && <p className="small dim">This server uses an access token. Add <span className="mono">?token=…</span> to these addresses in apps that cannot sign in (the playlist you download already includes it).</p>}
      </Card>

      <Card className="stack">
        <h2>Plex, Jellyfin and Emby</h2>
        <p className="small dim">TVMaster imitates a network tuner (HDHomeRun). In your media server, add a tuner device and enter the address below, then use the XMLTV link above for guide data.</p>
        <CopyField label="Tuner address" value={host} />
        <p className="small dim">The tuner interface follows the HDHomeRun API those servers speak; it has not yet been tested against every version.</p>
      </Card>

      <Card className="stack">
        <h2>Phone remote</h2>
        <p className="small dim">Open the remote on your phone and use “Add to Home Screen” (or “Install app”) to keep it like an app.</p>
        <CopyField label="Remote address" value={`${origin}/admin/#/remote`} />
      </Card>

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
