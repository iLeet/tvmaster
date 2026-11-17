import { describe, expect, it } from 'vitest';
import { emptyInfo, infoProblem, move, newSlide, withInfoDefaults } from './info';

describe('info channel settings', () => {
  it('fills in what an older server did not send', () => {
    const i = withInfoDefaults({ seconds: 20 });
    expect(i.seconds).toBe(20);
    expect(i.ticker_enabled).toBe(true);
    expect(i.slides).toEqual([]);
    expect(withInfoDefaults(undefined)).toEqual(emptyInfo());
  });

  it('starts each kind of page with a sensible title', () => {
    expect(newSlide('headlines').title).toBe('Headlines');
    expect(newSlide('tonight').title).toBe('On tonight');
    expect(newSlide('clock').title).toBe('');
    expect(newSlide('photo').picture).toBeNull();
  });

  it('says what the server would refuse', () => {
    const i = emptyInfo();
    expect(infoProblem(i)).toBeNull();
    expect(infoProblem({ ...i, slides: [newSlide('headlines')] })).toMatch(/feed/);
    expect(infoProblem({ ...i, slides: [newSlide('headlines')], feeds: ['https://x.org/rss'] })).toBeNull();
    expect(infoProblem({ ...i, slides: [newSlide('photo')] })).toMatch(/Page 1.*picture/);
    expect(infoProblem({ ...i, slides: [{ ...newSlide('text'), title: '' }] })).toMatch(/nothing in it/);
    expect(infoProblem({ ...i, feeds: ['not a url'] })).toMatch(/not a web address/);
  });

  it('moves a page without changing the original', () => {
    const l = ['a', 'b', 'c'];
    expect(move(l, 0, 2)).toEqual(['b', 'c', 'a']);
    expect(move(l, 2, 0)).toEqual(['c', 'a', 'b']);
    expect(move(l, 1, 9)).toBe(l);
    expect(l).toEqual(['a', 'b', 'c']);
  });
});
