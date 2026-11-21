import { SCREENS } from '../options';
import { PictureField } from '../PictureField';
import type { ChannelOptions, Lineup, SignOff } from '../types';
import { Card, Field, Segmented, Toggle } from '../ui';

/** The one place the off-air screen is chosen: unscheduled time, a finished show, sign-off, an unplayable file. */
export function OffAirEditor({ options, onChange, intro }: { options: ChannelOptions; onChange: (o: ChannelOptions) => void; intro?: string }) {
  const o = options;
  const patch = (v: Partial<ChannelOptions['off_air']>) => onChange({ ...o, off_air: { ...o.off_air, ...v } });
  return (
    <Card className="stack">
      <div>
        <h2>The off-air screen</h2>
        <p className="small dim">{intro ?? 'What the channel shows whenever there is nothing to play: time nothing is scheduled (if you chose that), a finished show, sign-off hours, a file that cannot be played. The classic test pattern, snow, black, a station card with your own words, or a picture of your own.'}</p>
      </div>
      <Field label="Screen" hint={SCREENS.find((x) => x.value === o.off_air.screen)?.help}>
        <Segmented value={o.off_air.screen} options={SCREENS} onChange={(screen) => patch({ screen, ...(screen === 'bars' ? { tone: true } : {}) })} />
      </Field>
      {o.off_air.screen === 'card' && (
        <Field label="Message" hint="Blank = PLEASE STAND BY.">
          <input className="input" style={{ maxWidth: 360 }} maxLength={60} placeholder="PLEASE STAND BY" value={o.off_air.message} onChange={(e) => patch({ message: e.target.value })} />
        </Field>
      )}
      {o.off_air.screen === 'image' && (
        <Field label="Picture" hint="A png, jpg, gif or webp, fitted to the screen. It is uploaded to the server, so it keeps working if you move files around.">
          <PictureField value={o.off_air.image} onChange={(image) => patch({ image })} />
        </Field>
      )}
      <Toggle checked={o.off_air.tone} onChange={(tone) => patch({ tone })} label={<span>Test tone <span className="dim small">{o.off_air.screen === 'static' ? 'hiss with the snow' : 'a steady 1 kHz tone, like a real test pattern'}</span></span>} />
    </Card>
  );
}

/** Go off the air overnight. */
export function SignOffEditor({ lineup, onLineup }: { lineup: Lineup; onLineup: (l: Lineup) => void }) {
  const signoff: SignOff | null = lineup.signoff;
  const set = (s: SignOff | null) => onLineup({ ...lineup, signoff: s });
  return (
    <div className="stack">
      <div>
        <strong>Sign off overnight</strong>
        <p className="small dim">Like a station of old: during these hours the channel airs bars and a tone (or snow) instead of programmes, and no episodes are used up.</p>
      </div>
      <Toggle checked={signoff !== null} onChange={(on) => set(on ? { from: '02:00', to: '05:00', style: 'bars' } : null)} label="Go off the air overnight" />
      {signoff && (
        <div className="row wrap nest" style={{ alignItems: 'flex-end' }}>
          <Field label="Off the air at"><input className="input" type="time" value={signoff.from} onChange={(e) => set({ ...signoff, from: e.target.value })} /></Field>
          <Field label="Back on at"><input className="input" type="time" value={signoff.to} onChange={(e) => set({ ...signoff, to: e.target.value })} /></Field>
          <Field label="Shows"><Segmented value={signoff.style} options={SCREENS} onChange={(style) => set({ ...signoff, style })} /></Field>
        </div>
      )}
      {signoff && <p className="small dim">Times are in the channel's time zone ({lineup.timezone || "the server's"}).</p>}
    </div>
  );
}
