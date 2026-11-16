import { useState } from 'react';
import { SCREENS, setShowOption, showOverride } from '../options';
import type { ChannelOptions, Corner, Lineup, ShowSummary, SignOff, System } from '../types';
import { Badge, Card, Field, Segmented, Toggle } from '../ui';
import { ASPECTS, LanguageList, LOOKS, ShowSettingsRow, SUB_MODES } from './ShowSettings';

const CORNERS: { value: Corner; label: string }[] = [
  { value: 'top_left', label: 'Top left' }, { value: 'top_right', label: 'Top right' },
  { value: 'bottom_left', label: 'Bottom left' }, { value: 'bottom_right', label: 'Bottom right' },
];

export function PictureSound({
  options, onChange, lineup, onLineup, shows, sys, testOnly = false,
}: {
  options: ChannelOptions;
  onChange: (o: ChannelOptions) => void;
  lineup: Lineup;
  onLineup: (l: Lineup) => void;
  shows: ShowSummary[];
  sys: System | undefined;
  /** A test-pattern channel only has an off-air screen to choose. */
  testOnly?: boolean;
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

  const offAir = (
    <Card className="stack">
      <div><h2>When nothing is on air</h2><p className="small dim">What fills the screen when there is nothing to show: an empty schedule, a gap between programmes, a file that cannot be played. Choose the classic test pattern, snow, black, a station card with your own words, or a picture of your own.</p></div>
      <Field label="Screen" hint={SCREENS.find((x) => x.value === o.off_air.screen)?.help}>
        <Segmented value={o.off_air.screen} options={SCREENS} onChange={(screen) => patch('off_air', { screen })} />
      </Field>
      {o.off_air.screen === 'card' && (
        <Field label="Message" hint="Blank = PLEASE STAND BY."><input className="input" style={{ maxWidth: 360 }} maxLength={60} placeholder="PLEASE STAND BY" value={o.off_air.message} onChange={(e) => patch('off_air', { message: e.target.value })} /></Field>
      )}
      {o.off_air.screen === 'image' && (
        <Field label="Picture file" hint="The full path of a png, jpg, gif or webp on the machine running TVMaster. It is fitted to the screen.">
          <input className="input mono" placeholder="/path/to/picture.png" value={o.off_air.image ?? ''} onChange={(e) => patch('off_air', { image: e.target.value || null })} />
        </Field>
      )}
      <Toggle checked={o.off_air.tone} onChange={(tone) => patch('off_air', { tone })} label={<span>Test tone <span className="dim small">{o.off_air.screen === 'static' ? 'hiss with the snow' : 'a steady 1 kHz tone, like a real test pattern'}</span></span>} />
      <p className="small dim">For a channel that is nothing but a test pattern, make a <strong>Test pattern</strong> channel (Channels, New channel).</p>
    </Card>
  );
  if (testOnly) return <div className="stack gap-lg">{offAir}</div>;

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

      {offAir}

      <Card className="stack">
        <div><h2>Sign-off</h2><p className="small dim">Like a station of old, go off the air for the night. During these hours the channel airs bars and a tone (or snow) instead of programmes, and no episodes are used up.</p></div>
        <Toggle checked={signoff !== null} onChange={(on) => setSignoff(on ? { from: '02:00', to: '05:00', style: 'bars' } : null)} label="Sign off overnight" />
        {signoff && (
          <div className="row wrap nest" style={{ alignItems: 'flex-end' }}>
            <Field label="Off the air at"><input className="input" type="time" value={signoff.from} onChange={(e) => setSignoff({ ...signoff, from: e.target.value })} /></Field>
            <Field label="Back on at"><input className="input" type="time" value={signoff.to} onChange={(e) => setSignoff({ ...signoff, to: e.target.value })} /></Field>
            <Field label="Shows"><Segmented value={signoff.style} options={SCREENS} onChange={(style) => setSignoff({ ...signoff, style })} /></Field>
          </div>
        )}
        {signoff && <p className="small dim">Times are in the channel's time zone ({lineup.timezone || 'the server\'s'}). Save the channel and the schedule updates.</p>}
      </Card>

      <Card className="stack">
        <div><h2>Per-show settings</h2><p className="small dim">Some shows want different treatment: anime in Japanese with English subtitles, a sitcom plain, a 1950s serial in black and white. A setting here applies on this channel only and beats both the channel's settings and anything set for the show on the Shows page.</p></div>
        {staleLibraries && <p className="small" style={{ color: 'var(--warn)' }}>Some files were scanned before audio and subtitle tracks were read. Re-scan the libraries (Libraries page) to see their languages.</p>}
        {shows.length > 8 && <input className="input" style={{ maxWidth: 280 }} placeholder="Filter shows…" value={showFilter} onChange={(e) => setShowFilter(e.target.value)} />}
        {shows.length === 0 && <p className="small dim">No shows in the libraries yet.</p>}
        <div className="stack">
          {shownShows.map((sh) => (
            <ShowSettingsRow
              key={sh.name}
              show={sh}
              ov={showOverride(o, sh.name)}
              inheritLabel="Default"
              inheritWhat={sh.defaults ? "the show's own settings (Shows page), then this channel's" : "this channel's settings"}
              wantedAudio={sh.defaults?.audio_languages?.length ? sh.defaults.audio_languages : o.audio.languages}
              badge="this channel"
              onChange={(p) => onChange(setShowOption(o, sh.name, p))}
            />
          ))}
        </div>
      </Card>
    </div>
  );
}
