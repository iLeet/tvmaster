import { describe, expect, it } from 'vitest';
import { cleanColor, emptyGuide, guideProblem, toggleId, withGuideDefaults } from './guide';

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
