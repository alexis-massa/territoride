/** `1 cell`, `3 cells`. */
export function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? '' : 's'}`;
}

/** The multiplier as the pages quote it: `2×`. */
export function times(multiplier: number): string {
  return `${Math.round(multiplier)}×`;
}

/** `November 2026`, from an ISO timestamp. */
export function monthYear(iso: string, locale?: string): string {
  return new Date(iso).toLocaleDateString(locale, { month: 'long', year: 'numeric' });
}

/** The rank medal colours of the leaderboard's first three places. */
export function podium(rank: number): 'top1' | 'top2' | 'top3' | '' {
  return (['top1', 'top2', 'top3'] as const)[rank - 1] ?? '';
}
