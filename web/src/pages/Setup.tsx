import { useState } from 'react';
import { api, errorMessage } from '../api';
import { useQuery } from '../live';
import { navigate } from '../router';
import { StylePicker, ThemePicker, ThemePreview } from '../ThemePicker';
import type { Library, SetupState, ThemeChoice } from '../types';
import { Badge, Btn, Card, Spinner, useToast } from '../ui';
import { AddLibrary, UploadFiles, kindLabel } from './Libraries';

/** From nothing to a channel on air: add media, choose a look, make the first channels. */
export function Setup({ onDone }: { onDone: () => void }) {
  const setup = useQuery(() => api.get<SetupState>('/setup'));
  const libs = useQuery(() => api.get<Library[]>('/libraries'));
  const { say } = useToast();
  const [step, setStep] = useState(1);
  const [adding, setAdding] = useState(false);
  const [uploading, setUploading] = useState<Library | null>(null);
  const [theme, setTheme] = useState<ThemeChoice | null>(null);
  const [busy, setBusy] = useState(false);
  const st = setup.data;
  const look = theme ?? st?.theme ?? null;
  const shows = (libs.data ?? []).filter((l) => l.kind === 'shows');

  const run = async (what: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await what();
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };
  const folder = () => run(async () => {
    await api.post('/media/station-folder');
    const l = await api.get<Library[]>('/libraries');
    libs.reload();
    setup.reload();
    const target = l.find((x) => x.kind === 'shows' && x.path.replace(/\\/g, '/').includes('/media/'));
    if (target) setUploading(target);
  });
  const finish = () => run(async () => {
    if (theme) await api.put('/theme', theme);
    const r = await api.post<{ channels: { name: string }[] }>('/setup/starter', {});
    say(r.channels.length ? `Made ${r.channels.map((c) => c.name).join(' and ')}` : 'Your channels are already there');
    onDone();
    navigate('dashboard');
  });
  const skip = () => run(async () => { await api.post('/setup/skip'); onDone(); });

  if (!st) return <div className="empty"><Spinner /></div>;
  return (
    <div className="page stack gap-lg">
      <div className="page-head">
        <h1>Set up your station</h1>
        <span className="sub">Three steps from nothing to a channel on air</span>
        <Btn className="right" kind="ghost" onClick={skip}>Not now</Btn>
      </div>
      <div className="row wrap" style={{ gap: 8 }}>
        {['Your media', 'The look', 'Channels'].map((t, i) => (
          <button key={t} className={`btn ${step === i + 1 ? 'primary' : ''}`} onClick={() => setStep(i + 1)}>{i + 1}. {t}</button>
        ))}
      </div>

      {step === 1 && (
        <Card className="stack">
          <h2>Where are your shows?</h2>
          <p className="dim">Use files already on the server, upload them from this device, or both. You can add commercials, bumper clips and music later on the Libraries page.</p>
          <div className="row wrap">
            <Btn icon="folder" onClick={() => setAdding(true)}>Use a folder on the server…</Btn>
            {st.can_upload && <Btn icon="plus" disabled={busy} onClick={folder}>Upload files from here…</Btn>}
          </div>
          {shows.length > 0 && (
            <div className="list">
              {shows.map((l) => <div key={l.id} className="row"><span className="grow mono ellipsis">{l.path}</span><Badge tone="accent">{kindLabel(l.kind)}</Badge><span className="dim small">{l.items} files</span></div>)}
            </div>
          )}
          <p className="small dim">{st.items > 0 ? `${st.shows} show${st.shows === 1 ? '' : 's'}, ${st.items} files found so far.` : 'Nothing found yet.'}</p>
          <div className="row" style={{ justifyContent: 'flex-end' }}><Btn kind="primary" disabled={st.items === 0} onClick={() => setStep(2)}>Next</Btn></div>
        </Card>
      )}

      {step === 2 && look && (
        <Card className="stack">
          <h2>How should it look?</h2>
          <p className="dim">One look for the guide, the cards between shows, alerts and the on-screen graphics. You can change it any time under Appearance.</p>
          <ThemePicker value={look} onChange={(v) => setTheme({ ...look, ...v })} autoName="Per channel" autoAbout="Colours made from each channel." />
          <StylePicker value={look.style} onChange={(style) => setTheme({ ...look, style })} />
          <ThemePreview choice={look} what="card" />
          <div className="row between"><Btn onClick={() => setStep(1)}>Back</Btn><Btn kind="primary" onClick={() => setStep(3)}>Next</Btn></div>
        </Card>
      )}

      {step === 3 && (
        <Card className="stack">
          <h2>Make your first channels</h2>
          <p className="dim">A <strong>Guide</strong> channel that lists what is on, and a <strong>Mix</strong> channel that shuffles all your shows. Add more channels, weekly lineups, breaks and alerts afterwards.</p>
          <div className="row between"><Btn onClick={() => setStep(2)}>Back</Btn><Btn kind="primary" disabled={busy || st.items === 0} onClick={finish}>{busy ? <Spinner /> : 'Make my channels'}</Btn></div>
        </Card>
      )}

      {adding && <AddLibrary onClose={() => setAdding(false)} onAdded={() => { libs.reload(); setup.reload(); }} />}
      {uploading && <UploadFiles library={uploading} onClose={() => setUploading(null)} onDone={() => { libs.reload(); setup.reload(); }} />}
    </div>
  );
}
