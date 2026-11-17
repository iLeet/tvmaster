import { useEffect, useState } from 'react';
import { api, unauthorized } from './api';
import { useQuery } from './live';
import { href, useRoute } from './router';
import type { AuthStatus } from './types';
import { Btn, Card, Field, Icon, Spinner, ToastProvider, useToast } from './ui';
import { Appearance } from './pages/Appearance';
import { Channels } from './pages/Channels';
import { ChannelEditor } from './pages/ChannelEditor';
import { Dashboard } from './pages/Dashboard';
import { LiveControl } from './pages/LiveControl';
import { Libraries } from './pages/Libraries';
import { Remote } from './pages/Remote';
import { Shows } from './pages/Shows';
import { Schedule } from './pages/Schedule';
import { Settings } from './pages/Settings';
import { Tvs } from './pages/Tvs';

const NAV: { path: string; label: string; icon: string }[] = [
  { path: 'dashboard', label: 'Home', icon: 'home' },
  { path: 'live', label: 'Live control', icon: 'live' },
  { path: 'channels', label: 'Channels', icon: 'channels' },
  { path: 'schedule', label: 'Schedule', icon: 'calendar' },
  { path: 'libraries', label: 'Libraries', icon: 'folder' },
  { path: 'shows', label: 'Shows', icon: 'tv' },
  { path: 'tvs', label: 'TVs', icon: 'tv' },
  { path: 'remote', label: 'Remote', icon: 'remote' },
  { path: 'appearance', label: 'Appearance', icon: 'palette' },
  { path: 'settings', label: 'Settings', icon: 'settings' },
];

function Login({ onDone }: { onDone: () => void }) {
  const [token, setToken] = useState('');
  const [busy, setBusy] = useState(false);
  const { say } = useToast();
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await api.post('/auth/login', { token: token.trim() });
      onDone();
    } catch {
      say('That token is not right', true);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card className="login">
      <form className="stack" onSubmit={submit}>
        <div className="brand" style={{ padding: 0 }}>
          <span className="logo">TV</span> TVMaster
        </div>
        <p className="dim">This server needs an access token. It was set with <span className="mono">--token</span> when the server started.</p>
        <Field label="Access token">
          <input className="input mono" type="password" autoFocus value={token} onChange={(e) => setToken(e.target.value)} autoComplete="current-password" />
        </Field>
        <Btn kind="primary" type="submit" disabled={busy || token.trim().length < 8}>
          {busy ? <Spinner /> : 'Sign in'}
        </Btn>
      </form>
    </Card>
  );
}

function Shell() {
  const route = useRoute();
  const section = route.parts[0] ?? 'dashboard';
  const standalone = matchMedia('(display-mode: standalone)').matches;
  const bare = section === 'remote' && standalone; // the installed phone remote fills the screen

  let page;
  switch (section) {
    case 'channels':
      page = route.parts[1] ? <ChannelEditor key={route.parts[1]} id={route.parts[1]} tab={route.query.get('tab') ?? 'general'} /> : <Channels />;
      break;
    case 'schedule':
      page = <Schedule channelId={route.parts[1]} />;
      break;
    case 'live':
      page = <LiveControl />;
      break;
    case 'libraries':
      page = <Libraries />;
      break;
    case 'shows':
      page = <Shows />;
      break;
    case 'tvs':
      page = <Tvs />;
      break;
    case 'remote':
      page = <Remote tvId={route.parts[1]} />;
      break;
    case 'appearance':
      page = <Appearance />;
      break;
    case 'settings':
      page = <Settings />;
      break;
    default:
      page = <Dashboard />;
  }

  if (bare) return <div className="main">{page}</div>;
  return (
    <div className="app">
      <nav className="side" aria-label="Main">
        <div className="brand">
          <span className="logo">TV</span> TVMaster
        </div>
        {NAV.map((n) => (
          <a key={n.path} href={href(n.path)} className={`nav ${section === n.path ? 'active' : ''}`}>
            <Icon name={n.icon} />
            {n.label}
          </a>
        ))}
        <div className="foot small dim">
          <a href="/" target="_blank" rel="noreferrer">Open web player</a>
        </div>
      </nav>
      <main className="main">{page}</main>
    </div>
  );
}

export default function App() {
  const auth = useQuery(() => api.get<AuthStatus>('/auth'), []);
  const [locked, setLocked] = useState(false);
  useEffect(() => {
    const on = () => setLocked(true);
    unauthorized.addEventListener('unauthorized', on);
    return () => unauthorized.removeEventListener('unauthorized', on);
  }, []);

  let body;
  if (auth.loading && !auth.data) {
    body = <div className="empty"><Spinner /></div>;
  } else if (auth.error && !auth.data) {
    body = (
      <Card className="login">
        <div className="stack">
          <h2>Cannot reach the server</h2>
          <p className="dim">{auth.error}</p>
          <Btn onClick={auth.reload}>Try again</Btn>
        </div>
      </Card>
    );
  } else if (auth.data?.required && (!auth.data.authenticated || locked)) {
    body = <Login onDone={() => { setLocked(false); auth.reload(); }} />;
  } else {
    body = <Shell />;
  }
  return <ToastProvider>{body}</ToastProvider>;
}
