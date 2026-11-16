import { describe, expect, it } from 'vitest';
import { addLanguage, audioMatches, emptyOptions, langName, moveLanguage, removeLanguage, setShowOption, showOverride, trackNote, withDefaults } from './options';

describe('languages', () => {
  it('names the codes the server sends and shows unknown ones as written', () => {
    expect(langName('jpn')).toBe('Japanese');
    expect(langName('ENG')).toBe('English');
    expect(langName('tlh')).toBe('tlh');
    expect(langName('')).toBe('Unknown');
  });
  it('keeps an ordered list of preferences', () => {
    let l: string[] = [];
    l = addLanguage(l, 'JPN');
    l = addLanguage(l, 'eng');
    l = addLanguage(l, 'jpn');
    expect(l).toEqual(['jpn', 'eng']);
    expect(addLanguage(l, '  ')).toEqual(['jpn', 'eng']);
    expect(moveLanguage(l, 1, -1)).toEqual(['eng', 'jpn']);
    expect(moveLanguage(l, 0, -1)).toEqual(['jpn', 'eng']);
    expect(moveLanguage(l, 1, 1)).toEqual(['jpn', 'eng']);
    expect(removeLanguage(l, 'jpn')).toEqual(['eng']);
  });
});

describe('options', () => {
  it('fills holes from an older server and keeps what was set', () => {
    const o = withDefaults({ audio: { languages: ['jpn'] } } as never);
    expect(o.audio.languages).toEqual(['jpn']);
    expect(o.audio.avoid_commentary).toBe(true);
    expect(o.video.quality).toBe('720');
    expect(o.overlay.bug.opacity).toBe(60);
    expect(withDefaults(null)).toEqual(emptyOptions());
  });

  it('adds, edits and removes a per-show override, matching names without regard to case', () => {
    let o = emptyOptions();
    o = setShowOption(o, 'Cowboy Bebop', { audio_languages: ['jpn'], subtitles: 'always' });
    expect(o.shows['Cowboy Bebop']).toEqual({ audio_languages: ['jpn'], subtitles: 'always' });
    o = setShowOption(o, 'cowboy bebop', { look: 'vhs' });
    expect(Object.keys(o.shows)).toEqual(['Cowboy Bebop']);
    expect(showOverride(o, 'COWBOY BEBOP')).toEqual({ audio_languages: ['jpn'], subtitles: 'always', look: 'vhs' });
    o = setShowOption(o, 'Cowboy Bebop', { audio_languages: [], subtitles: null, look: null });
    expect(o.shows).toEqual({});
    expect(setShowOption(emptyOptions(), 'X', {}).shows).toEqual({});
  });
});

describe('what a show contains', () => {
  const base = { episodes: 10, audio_languages: ['eng', 'jpn'], subtitle_languages: ['eng'], files_with_tracks: 10 };
  it('says when tracks have not been read', () => {
    expect(trackNote({ ...base, files_with_tracks: 0 })).toContain('re-scan');
  });
  it('lists languages by name and notes a partial scan', () => {
    expect(trackNote(base)).toBe('Audio: English, Japanese. Subtitles: English.');
    expect(trackNote({ ...base, files_with_tracks: 4 })).toContain('4 of 10 files read');
    expect(trackNote({ ...base, audio_languages: [], subtitle_languages: [] })).toContain('no language tags');
  });
  it('warns only when a preference cannot be met', () => {
    expect(audioMatches(base, ['jpn'])).toBe(true);
    expect(audioMatches(base, ['deu'])).toBe(false);
    expect(audioMatches(base, [])).toBe(true);
    expect(audioMatches({ audio_languages: [] }, ['deu'])).toBe(true);
  });
});

describe('off-air screen', () => {
  it('defaults to a station card with no tone, and survives old servers that omit it', async () => {
    const { SCREENS } = await import('./options');
    expect(emptyOptions().off_air).toEqual({ screen: 'card', message: '', image: null, tone: false });
    expect(withDefaults({ audio: { languages: ['jpn'] } } as never).off_air.screen).toBe('card');
    expect(withDefaults({ off_air: { screen: 'bars', tone: true } } as never).off_air).toEqual({ screen: 'bars', message: '', image: null, tone: true });
    expect(SCREENS.map((s) => s.value)).toEqual(['card', 'bars', 'static', 'black', 'image']);
  });
});
