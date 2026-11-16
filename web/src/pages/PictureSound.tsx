import { useState } from 'react';
import { addLanguage, audioMatches, langName, LANGUAGES, moveLanguage, removeLanguage, setShowOption, showOverride, trackNote } from '../options';
import type { Aspect, ChannelOptions, Corner, Lineup, Look, ShowSummary, SignOff, SignOffStyle, SubMode, System } from '../types';
import { Badge, Btn, Card, Field, Segmented, Toggle } from '../ui';

const SUB_MODES: { value: SubMode; label: string; help: string }[] = [
  { value: 'off', label: 'Off', help: 'Never burn subtitles in.' },
  { value: 'foreign', label: 'When foreign', help: 'Subtitles only when the audio is not in one of your languages; otherwise just the forced ones (signs, foreign phrases).' },
  { value: 'always', label: 'Always', help: 'Always show subtitles in your language.' },
  { value: 'forced', label: 'Forced only', help: 'Only the forced subtitles inside the programme.' },
  { value: 'sdh', label: 'For the hard of hearing', help: 'Captions with sound cues and speaker names when the file has them.' },
];

const LOOKS: { value: Look; label: string; help: string }[] = [
  { value: 'none', label: 'None', help: 'The picture as it is.' },
  { value: 'crt', label: 'CRT', help: 'Scanlines and a soft vignette, like a tube TV.' },
  { value: 'vhs', label: 'VHS', help: 'Noise, softness and washed colour, like a worn tape.' },
  { value: 'film', label: 'Film', help: 'Grain and warmth.' },
  { value: 'black_white', label: 'Black & white', help: 'Colour removed.' },
  { value: 'sepia', label: 'Sepia', help: 'An old-photograph tint.' },
];

const ASPECTS: { value: Aspect; label: string; help: string }[] = [
  { value: 'fit', label: 'Fit', help: 'The whole picture, with bars where the shapes differ.' },
  { value: 'fill', label: 'Fill', help: 'Fill the screen, cropping the edges.' },
  { value: 'stretch', label: 'Stretch', help: 'Fill the screen by stretching (distorts).' },
];

const CORNERS: { value: Corner; label: string }[] = [
  { value: 'top_left', label: 'Top left' }, { value: 'top_right', label: 'Top right' },
  { value: 'bottom_left', label: 'Bottom left' }, { value: 'bottom_right', label: 'Bottom right' },
];

/** An ordered list of languages with add, remove and reorder. */
function LanguageList({ value, onChange, placeholder }: { value: string[]; onChange: (v: string[]) => void; placeholder: string }) {
  const [pick, setPick] = useState('');
  return (
    <div className="stack" style={{ gap: 8 }}>
      {value.length > 0 && (
        <div className="stack" style={{ gap: 4 }}>
          {value.map((l, i) => (
            <div key={l} className="row small">
              <span className="num-badge" style={{ minWidth: 26, height: 26 }}>{i + 1}</span>
              <span className="grow">{langName(l)} <span className="dim mono">{l}</span></span>
              <Btn small kind="ghost" icon="up" aria-label={`Move ${langName(l)} up`} disabled={i === 0} onClick={() => onChange(moveLanguage(value, i, -1))} />
              <Btn small kind="ghost" icon="down" aria-label={`Move ${langName(l)} down`} disabled={i === value.length - 1} onClick={() => onChange(moveLanguage(value, i, 1))} />
              <Btn small kind="ghost" icon="x" aria-label={`Remove ${langName(l)}`} onClick={() => onChange(removeLanguage(value, l))} />
            </div>
          ))}
        </div>
      )}
      <div className="row">
        <select className="select" style={{ minWidth: 200 }} value={pick} onChange={(e) => { if (e.target.value) { onChange(addLanguage(value, e.target.value)); } setPick(''); }} aria-label={placeholder}>
          <option value="">{value.length ? 'Add another language…' : placeholder}</option>
          {LANGUAGES.filter(([c]) => !value.includes(c)).map(([c, n]) => <option key={c} value={c}>{n}</option>)}
        </select>
      </div>
    </div>
  );
}

export function PictureSound({
  options, onChange, lineup, onLineup, shows, sys,
}: {
  options: ChannelOptions;
  onChange: (o: ChannelOptions) => void;
  lineup: Lineup;
  onLineup: (l: Lineup) => void;
  shows: ShowSummary[];
  sys: System | undefined;
}) {
  const o = options;
  const patch = <K extends keyof ChannelOptions>(k: K, v: Partial<ChannelOptions[K]>) => onChange({ ...o, [k]: { ...(o[k] as object), ...(v as object) } } as ChannelOptions);
  const ff = sys?.ffmpeg;
  const noLibass = ff && !ff.has_subtitles;
  const noDrawtext = ff && !ff.has_drawtext;
  const signoff: SignOff | null = lineup.signoff;
  const setSignoff = (s: SignOff | null) => onLineup({ ...lineup, signoff: s });
  const [showFilter, setShowFilter] = useState('');
  const shownShows = shows.filter((s) => s.name.toLowerCase().includes(showFilter.toLowerCase()));
  const staleLibraries = shows.some((s) => s.files_with_tracks < s.episodes);

  return (
    <div className="stack gap-lg">
      <Card className="stack">
        <div><h2>Audio</h2><p className="small dim">Many files carry several audio tracks (original and dubbed languages, commentary). The channel picks one for every programme.</p></div>
        <Field label="Preferred languages" hint="In order. The first one a file has is used; if none match, the file's own default track plays. Commentary and audio-description tracks are skipped unless they are all there is.">
          <LanguageList value={o.audio.languages} onChange={(languages) => patch('audio', { languages })} placeholder="Any (the file's default)" />
        </Field>
        <div className="row wrap">
          <Toggle checked={o.audio.avoid_commentary} onChange={(v) => patch('audio', { avoid_commentary: v })} label="Skip commentary tracks" />
          <Toggle checked={o.audio.avoid_description} onChange={(v) => patch('audio', { avoid_description: v })} label="Skip audio description" />
        </div>
        <Field label="Volume levelling" hint={{ off: 'Leave levels as they are.', light: 'Gentle automatic levelling: cheap on the CPU.', broadcast: 'EBU R128 broadcast loudness: shows and commercials come out equally loud.' }[o.audio.loudness]}>
          <Segmented value={o.audio.loudness} options={[{ value: 'off', label: 'Off' }, { value: 'light', label: 'Light' }, { value: 'broadcast', label: 'Broadcast' }]} onChange={(loudness) => patch('audio', { loudness })} />
        </Field>
        <div className="row wrap">
          <Toggle checked={o.audio.night_mode} onChange={(v) => patch('audio', { night_mode: v })} label={<span>Night mode <span className="dim small">quieter explosions, clearer speech</span></span>} />
          <Toggle checked={o.audio.dialogue_boost} onChange={(v) => patch('audio', { dialogue_boost: v })} label={<span>Dialogue boost <span className="dim small">surround sources only</span></span>} />
          <Toggle checked={o.audio.mono} onChange={(v) => patch('audio', { mono: v })} label={<span>Mono <span className="dim small">one speaker, like an old set</span></span>} />
        </div>
      </Card>

      <Card className="stack">
        <div><h2>Subtitles</h2><p className="small dim">Subtitles are burned into the picture, so every TV, phone and IPTV app shows them. They come from the video file or from a subtitle file beside it (<span className="mono">Show S01E01.en.srt</span>).</p></div>
        {noLibass && (
          <div className="err-banner">This server's FFmpeg cannot draw text subtitles (it was built without libass). Subtitles that are pictures (many Blu-ray and DVD rips) still work. Install a full FFmpeg build for text ones; <span className="mono">tvmaster doctor</span> shows what yours has.</div>
        )}
        <Field label="When to show them" hint={SUB_MODES.find((m) => m.value === o.subtitles.mode)?.help}>
          <Segmented value={o.subtitles.mode} options={SUB_MODES} onChange={(mode) => patch('subtitles', { mode })} />
        </Field>
        {o.subtitles.mode !== 'off' && (
          <>
            <Field label="Subtitle languages" hint="In order. Blank = the same as your audio languages.">
              <LanguageList value={o.subtitles.languages} onChange={(languages) => patch('subtitles', { languages })} placeholder="Same as audio" />
            </Field>
            <div className="row wrap">
              <Field label="Size"><Segmented value={o.subtitles.size} options={[{ value: 'small', label: 'Small' }, { value: 'medium', label: 'Medium' }, { value: 'large', label: 'Large' }]} onChange={(size) => patch('subtitles', { size })} /></Field>
              <Field label="Style"><Segmented value={o.subtitles.style} options={[{ value: 'outline', label: 'Outline' }, { value: 'box', label: 'Caption box' }]} onChange={(style) => patch('subtitles', { style })} /></Field>
            </div>
          </>
        )}
      </Card>

      <Card className="stack">
        <div><h2>Picture</h2><p className="small dim">How everything on this channel is shaped and styled before it is streamed.</p></div>
        <div className="row wrap" style={{ alignItems: 'flex-start' }}>
          <Field label="Quality" hint="Lower sizes suit old TVs and slow networks.">
            <Segmented value={o.video.quality} options={[{ value: '480', label: '480p' }, { value: '720', label: '720p' }, { value: '1080', label: '1080p' }]} onChange={(quality) => patch('video', { quality })} />
          </Field>
          <Field label="Frame rate" hint="Blank = 30.">
            <input className="input num" type="number" min={15} max={60} placeholder="30" value={o.video.fps ?? ''} onChange={(e) => patch('video', { fps: e.target.value ? Number(e.target.value) : null })} />
          </Field>
          <Field label="Bitrate (kbit/s)" hint="Blank = the quality's default.">
            <input className="input num" type="number" min={300} max={40000} step={100} placeholder="auto" value={o.video.kbps ?? ''} onChange={(e) => patch('video', { kbps: e.target.value ? Number(e.target.value) : null })} />
          </Field>
        </div>
        <Field label="Screen shape" hint={ASPECTS.find((a) => a.value === o.video.aspect)?.help}>
          <Segmented value={o.video.aspect} options={ASPECTS} onChange={(aspect) => patch('video', { aspect })} />
        </Field>
        <Field label="Deinterlacing" hint="Old broadcast video is interlaced and shows combing on moving edges.">
          <Segmented value={o.video.deinterlace} options={[{ value: 'auto', label: 'When the file says so' }, { value: 'always', label: 'Always' }, { value: 'off', label: 'Never' }]} onChange={(deinterlace) => patch('video', { deinterlace })} />
        </Field>
        <Field label="Look" hint={LOOKS.find((l) => l.value === o.video.look)?.help}>
          <Segmented value={o.video.look} options={LOOKS} onChange={(look) => patch('video', { look })} />
        </Field>
      </Card>

      <Card className="stack">
        <div><h2>On-screen graphics</h2><p className="small dim">The little touches that make it feel like a station.</p></div>
        <Toggle checked={o.overlay.bug.enabled} onChange={(v) => patch('overlay', { bug: { ...o.overlay.bug, enabled: v } })} label={<span><strong>Channel logo</strong> <span className="dim">in a corner of the picture</span></span>} />
        {o.overlay.bug.enabled && (
          <div className="row wrap nest" style={{ alignItems: 'flex-end' }}>
            <Field label="Text" hint="Blank = number and name."><input className="input" style={{ width: 200 }} maxLength={24} placeholder="4 Retro TV" value={o.overlay.bug.text} onChange={(e) => patch('overlay', { bug: { ...o.overlay.bug, text: e.target.value } })} /></Field>
            <Field label="Corner"><select className="select" value={o.overlay.bug.position} onChange={(e) => patch('overlay', { bug: { ...o.overlay.bug, position: e.target.value as Corner } })}>{CORNERS.map((c) => <option key={c.value} value={c.value}>{c.label}</option>)}</select></Field>
            <Field label={`Opacity ${o.overlay.bug.opacity}%`}><input type="range" min={10} max={100} value={o.overlay.bug.opacity} onChange={(e) => patch('overlay', { bug: { ...o.overlay.bug, opacity: Number(e.target.value) } })} /></Field>
          </div>
        )}
        <Toggle checked={o.overlay.show_title.enabled} onChange={(v) => patch('overlay', { show_title: { ...o.overlay.show_title, enabled: v } })} label={<span><strong>Now-playing card</strong> <span className="dim">the programme's name when it starts</span></span>} />
        {o.overlay.show_title.enabled && (
          <div className="nest"><Field label={`Stays up for ${o.overlay.show_title.seconds} seconds`}><input type="range" min={3} max={30} value={o.overlay.show_title.seconds} onChange={(e) => patch('overlay', { show_title: { ...o.overlay.show_title, seconds: Number(e.target.value) } })} /></Field></div>
        )}
        <Toggle checked={o.overlay.clock.enabled} onChange={(v) => patch('overlay', { clock: { ...o.overlay.clock, enabled: v } })} label={<span><strong>Clock</strong> <span className="dim">the time, drawn on the picture</span></span>} />
        {o.overlay.clock.enabled && (
          <div className="row wrap nest" style={{ alignItems: 'flex-end' }}>
            <Field label="Corner"><select className="select" value={o.overlay.clock.position} onChange={(e) => patch('overlay', { clock: { ...o.overlay.clock, position: e.target.value as Corner } })}>{CORNERS.map((c) => <option key={c.value} value={c.value}>{c.label}</option>)}</select></Field>
            <Toggle checked={o.overlay.clock.hour12} onChange={(v) => patch('overlay', { clock: { ...o.overlay.clock, hour12: v } })} label="12-hour" />
            {noDrawtext && <Badge tone="warn">needs FFmpeg with drawtext (this one has none, so no clock appears)</Badge>}
          </div>
        )}
      </Card>

      <Card className="stack">
        <div><h2>Sign-off</h2><p className="small dim">Like a station of old, go off the air for the night. During these hours the channel airs bars and a tone (or snow) instead of programmes, and no episodes are used up.</p></div>
        <Toggle checked={signoff !== null} onChange={(on) => setSignoff(on ? { from: '02:00', to: '05:00', style: 'bars' } : null)} label="Sign off overnight" />
        {signoff && (
          <div className="row wrap nest" style={{ alignItems: 'flex-end' }}>
            <Field label="Off the air at"><input className="input" type="time" value={signoff.from} onChange={(e) => setSignoff({ ...signoff, from: e.target.value })} /></Field>
            <Field label="Back on at"><input className="input" type="time" value={signoff.to} onChange={(e) => setSignoff({ ...signoff, to: e.target.value })} /></Field>
            <Field label="Shows"><Segmented value={signoff.style} options={[{ value: 'bars', label: 'Colour bars' }, { value: 'static', label: 'Snow' }, { value: 'card', label: 'Station card' }] as { value: SignOffStyle; label: string }[]} onChange={(style) => setSignoff({ ...signoff, style })} /></Field>
          </div>
        )}
        {signoff && <p className="small dim">Times are in the channel's time zone ({lineup.timezone || 'the server\'s'}). Save the channel and the schedule updates.</p>}
      </Card>

      <Card className="stack">
        <div><h2>Per-show settings</h2><p className="small dim">Some shows want different treatment: anime in Japanese with English subtitles, a sitcom plain, a 1950s serial in black and white. A setting here replaces the channel's for that show only.</p></div>
        {staleLibraries && <p className="small" style={{ color: 'var(--warn)' }}>Some files were scanned before audio and subtitle tracks were read. Re-scan the libraries (Libraries page) to see their languages.</p>}
        {shows.length > 8 && <input className="input" style={{ maxWidth: 280 }} placeholder="Filter shows…" value={showFilter} onChange={(e) => setShowFilter(e.target.value)} />}
        {shows.length === 0 && <p className="small dim">No shows in the libraries yet.</p>}
        <div className="stack">
          {shownShows.map((s) => {
            const ov = showOverride(o, s.name);
            const customised = Object.keys(ov).length > 0;
            return (
              <ShowRow key={s.name} show={s} ov={ov} channelAudio={o.audio.languages} customised={customised} onChange={(p) => onChange(setShowOption(o, s.name, p))} />
            );
          })}
        </div>
      </Card>
    </div>
  );
}

function ShowRow({ show, ov, channelAudio, customised, onChange }: { show: ShowSummary; ov: import('../types').ShowOptions; channelAudio: string[]; customised: boolean; onChange: (p: import('../types').ShowOptions) => void }) {
  const [open, setOpen] = useState(customised);
  const wanted = ov.audio_languages?.length ? ov.audio_languages : channelAudio;
  return (
    <div className="block-card stack" style={{ gap: 8 }}>
      <div className="row wrap between">
        <div>
          <strong>{show.name || '(no name)'}</strong> {customised && <Badge tone="accent">custom</Badge>}
          <div className="small dim">{trackNote(show)}</div>
          {!audioMatches(show, wanted) && <div className="small" style={{ color: 'var(--warn)' }}>None of its audio is in {wanted.map(langName).join(' or ')}; the file's default will play.</div>}
        </div>
        <Btn small onClick={() => setOpen(!open)}>{open ? 'Done' : customised ? 'Edit' : 'Customise'}</Btn>
      </div>
      {open && (
        <div className="stack nest" style={{ gap: 10 }}>
          <Field label="Audio languages" hint="Blank = the channel's.">
            <LanguageList value={ov.audio_languages ?? []} onChange={(audio_languages) => onChange({ audio_languages })} placeholder="Use the channel's" />
          </Field>
          <Field label="Subtitles">
            <Segmented value={(ov.subtitles ?? 'inherit') as SubMode | 'inherit'} options={[{ value: 'inherit', label: "Channel's" }, ...SUB_MODES.map((m) => ({ value: m.value, label: m.label }))]} onChange={(v) => onChange({ subtitles: v === 'inherit' ? null : (v as SubMode) })} />
          </Field>
          {ov.subtitles && ov.subtitles !== 'off' && (
            <Field label="Subtitle languages" hint="Blank = the channel's.">
              <LanguageList value={ov.subtitle_languages ?? []} onChange={(subtitle_languages) => onChange({ subtitle_languages })} placeholder="Use the channel's" />
            </Field>
          )}
          <Field label="Look">
            <Segmented value={(ov.look ?? 'inherit') as Look | 'inherit'} options={[{ value: 'inherit', label: "Channel's" }, ...LOOKS.map((l) => ({ value: l.value, label: l.label }))]} onChange={(v) => onChange({ look: v === 'inherit' ? null : (v as Look) })} />
          </Field>
          <Field label="Screen shape">
            <Segmented value={(ov.aspect ?? 'inherit') as Aspect | 'inherit'} options={[{ value: 'inherit', label: "Channel's" }, ...ASPECTS.map((a) => ({ value: a.value, label: a.label }))]} onChange={(v) => onChange({ aspect: v === 'inherit' ? null : (v as Aspect) })} />
          </Field>
        </div>
      )}
    </div>
  );
}
