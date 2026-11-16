import { describe, expect, it } from 'vitest';
import { GUIDE_PRESETS, GUIDE_THEMES, cleanColor, emptyGuide, guideProblem, toggleId, withGuideDefaults } from './guide';

describe('guide settings', () => {
  it('start with the look guides have always had', () => {
    const g = emptyGuide();
    expect(g.theme).toBe('auto');
    expect(g.layout.rows).toBe(6);
    expect(g.pip.mode).toBe('off');
    expect(g.ticker_options.enabled && g.ticker_options.now_on).toBe(true);
  });

  it('fill in what an older server left out, keeping what it sent', () => {
    const g = withGuideDefaults({ hours: 4, title: 'Retro', ticker: ['hi'], music_library_ids: [2] });
    expect(g.hours).toBe(4);
    expect(g.title).toBe('Retro');
    expect(g.layout.rows).toBe(6);
    expect(g.pip.placement).toBe('side_right');
    const partial = withGuideDefaults({ layout: { rows: 8 } as never, pip: { mode: 'rotate' } as never });
    expect(partial.layout.rows).toBe(8);
    expect(partial.layout.scroll).toBe('page');
    expect(partial.pip.mode).toBe('rotate');
    expect(partial.pip.seconds).toBe(30);
    expect(withGuideDefaults(undefined)).toEqual(emptyGuide());
  });

  it('keeps channel lists in the order they were picked', () => {
    expect(toggleId([], 'b')).toEqual(['b']);
    expect(toggleId(['b'], 'a')).toEqual(['b', 'a']);
    expect(toggleId(['b', 'a'], 'b')).toEqual(['a']);
  });

  it('accepts only #rrggbb colours', () => {
    expect(cleanColor(' #3366FF ')).toBe('#3366ff');
    expect(cleanColor('3366ff')).toBeNull();
    expect(cleanColor('#36f')).toBeNull();
  });

  it('says what is missing before saving', () => {
    const g = emptyGuide();
    expect(guideProblem(g)).toBeNull();
    g.pip.mode = 'channel';
    expect(guideProblem(g)).toMatch(/channel/);
    g.pip.channel = 'retro';
    expect(guideProblem(g)).toBeNull();
    g.theme = 'custom';
    g.colors.primary = 'blue';
    expect(guideProblem(g)).toMatch(/Colours/);
  });
});

describe('guide looks', () => {
  it('start as the original retro look', () => {
    const g = emptyGuide();
    expect(g.look.typeface).toBe('pixel');
    expect(g.look.corners).toBe('square');
    expect(g.look.background.style).toBe('gradient');
    expect(g.look.now_marker).toBe(true);
  });

  it('fill in look options an older server left out, keeping what it sent', () => {
    const g = withGuideDefaults({ look: { typeface: 'smooth', background: { style: 'image', image: '/x.png' } } as never });
    expect(g.look.typeface).toBe('smooth');
    expect(g.look.corners).toBe('square');
    expect(g.look.background).toEqual({ style: 'image', image: '/x.png', dim: 45 });
  });

  it('presets change the look and leave what the guide lists alone', () => {
    const g = { ...emptyGuide(), title: 'Mine', ticker: ['hi'], channels: ['a'] };
    for (const p of GUIDE_PRESETS) {
      const out = p.apply(g);
      expect(out.title).toBe('Mine');
      expect(out.channels).toEqual(['a']);
      expect(out.ticker).toEqual(['hi']);
    }
    const modern = GUIDE_PRESETS.find((p) => p.id === 'modern-dark')!.apply(g);
    expect(modern.look.typeface).toBe('smooth');
    expect(modern.layout.scanlines).toBe(false);
    expect(GUIDE_PRESETS.find((p) => p.id === 'retro')!.apply(modern).look.typeface).toBe('pixel');
  });

  it('every theme has a name, help and two swatch colours', () => {
    for (const t of GUIDE_THEMES) {
      expect(t.label && t.help).toBeTruthy();
      expect(t.colors.every((c) => /^#[0-9a-f]{6}$/.test(c))).toBe(true);
    }
    expect(new Set(GUIDE_THEMES.map((t) => t.value)).size).toBe(GUIDE_THEMES.length);
  });

  it('wants a picture before a picture background can be saved', () => {
    const g = emptyGuide();
    g.look.background.style = 'image';
    expect(guideProblem(g)).toMatch(/picture/);
    g.look.background.image = '/data/assets/x.png';
    expect(guideProblem(g)).toBeNull();
  });
});
