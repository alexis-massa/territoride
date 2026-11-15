import { describe, expect, it } from 'vitest';

import { monthYear, plural, podium, times } from './format';

describe('format', () => {
  it('pluralises', () => {
    expect(plural(1, 'cell')).toBe('1 cell');
    expect(plural(0, 'POI')).toBe('0 POIs');
    expect(plural(12, 'cell')).toBe('12 cells');
  });

  it('quotes multipliers as whole numbers', () => {
    expect(times(2)).toBe('2×');
    expect(times(5.0)).toBe('5×');
  });

  it('names the month and year', () => {
    expect(monthYear('2026-11-15T12:00:00Z', 'en-GB')).toBe('November 2026');
  });

  it('colours only the podium', () => {
    expect([1, 2, 3, 4].map(podium)).toEqual(['top1', 'top2', 'top3', '']);
  });
});
