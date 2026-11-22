import { describe, expect, it } from 'vitest';
import { addLayer, colourToHex, hasMotion, moveLayer, motionSpeed, newLayer, removeLayer, sameDesign, setLayer, setMotionSpeed, stillDesign, textLayers, type Design, type Layer } from './screens';

const design = (): Design => ({
  name: 'x',
  backdrop: { type: 'theme' },
  layers: [newLayer('shape'), newLayer('text'), ({ ...newLayer('text'), text: 'Second' } as Layer)],
  fade_out_ms: 0,
});

describe('screen designs', () => {
  it('finds the words to edit and changes one layer only', () => {
    const d = design();
    expect(textLayers(d).map((t) => t.index)).toEqual([1, 2]);
    const e = setLayer(d, 2, { text: 'Changed' } as never);
    expect((e.layers[2] as { text: string }).text).toBe('Changed');
    expect(e.layers[1]).toBe(d.layers[1]);
  });

  it('adds, moves and removes layers', () => {
    let d = addLayer(design(), 'clock');
    expect(d.layers.at(-1)?.type).toBe('clock');
    d = moveLayer(d, 0, 1);
    expect(d.layers[1].type).toBe('shape');
    expect(moveLayer(d, 0, -1)).toBe(d);
    d = removeLayer(d, 1);
    expect(d.layers).toHaveLength(3);
  });

  it('sets motion speed everywhere, within limits, and can stop the motion', () => {
    const d = setMotionSpeed(design(), 9999);
    expect(motionSpeed(d)).toBe(500);
    expect(d.layers.every((l) => l.anim.speed === 500)).toBe(true);
    const moving = setLayer(d, 1, { anim: { ...d.layers[1].anim, motion: 'pulse' } });
    expect(hasMotion(moving)).toBe(true);
    expect(hasMotion(stillDesign(moving))).toBe(false);
  });

  it('shows colours for the picker and compares designs by what they draw', () => {
    expect(colourToHex('#abc')).toBe('#aabbcc');
    expect(colourToHex('accent', '#123456')).toBe('#123456');
    expect(sameDesign(design(), { ...design(), name: 'other' })).toBe(true);
    expect(sameDesign(design(), { ...design(), fade_out_ms: 300 })).toBe(false);
  });
});

import { lookFile, type Look } from './pages/Looks';

describe('looks as files', () => {
  it('keeps only what can be brought back', () => {
    const l = { id: 'my-x', name: 'X', about: 'a', builtin: false, theme: { theme: 'glass', style: 'modern', gentle: false }, guide: {}, preview_guide: { big: true }, screens: {} } as Look;
    const f = JSON.parse(lookFile(l));
    expect(Object.keys(f).sort()).toEqual(['about', 'guide', 'name', 'screens', 'theme']);
    expect(f.theme.theme).toBe('glass');
  });
});
