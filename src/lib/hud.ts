export interface Frequency {
  hits: number;
  opportunities: number;
}
export interface Aggression {
  bets: number;
  raises: number;
  calls: number;
  folds: number;
}
export interface StreetStats {
  seen: number;
  cbet: Frequency;
  foldToCbet: Frequency;
  aggression: Aggression;
}
export interface HudStats {
  vpip: Frequency;
  pfr: Frequency;
  threeBet: Frequency;
  fourBet: Frequency;
  foldToThreeBet: Frequency;
  foldToFourBet: Frequency;
  wtsd: Frequency;
  wonAtShowdown: Frequency;
  wonWhenSawFlop: Frequency;
  aggression: Aggression;
  streets: StreetStats[];
}
export const emptyFrequency = (): Frequency => ({ hits: 0, opportunities: 0 });
export const emptyAggression = (): Aggression => ({
  bets: 0,
  raises: 0,
  calls: 0,
  folds: 0,
});
export const frequencyKeys = [
  'vpip',
  'pfr',
  'threeBet',
  'fourBet',
  'foldToThreeBet',
  'foldToFourBet',
  'wtsd',
  'wonAtShowdown',
  'wonWhenSawFlop',
] as const;
export function emptyHud(): HudStats {
  return {
    vpip: emptyFrequency(),
    pfr: emptyFrequency(),
    threeBet: emptyFrequency(),
    fourBet: emptyFrequency(),
    foldToThreeBet: emptyFrequency(),
    foldToFourBet: emptyFrequency(),
    wtsd: emptyFrequency(),
    wonAtShowdown: emptyFrequency(),
    wonWhenSawFlop: emptyFrequency(),
    aggression: emptyAggression(),
    streets: Array.from({ length: 3 }, () => ({
      seen: 0,
      cbet: emptyFrequency(),
      foldToCbet: emptyFrequency(),
      aggression: emptyAggression(),
    })),
  };
}
export const percent = (f: Frequency) =>
  f.opportunities ? `${((f.hits / f.opportunities) * 100).toFixed(1)}%` : '—';
export const sample = (f: Frequency) =>
  `${f.hits.toLocaleString()} / ${f.opportunities.toLocaleString()}`;
export const aggressionFactor = (a: Aggression) =>
  a.calls
    ? ((a.bets + a.raises) / a.calls).toFixed(2)
    : a.bets + a.raises
      ? '∞'
      : '—';
export const aggressionFrequency = (a: Aggression): Frequency => ({
  hits: a.bets + a.raises,
  opportunities: a.bets + a.raises + a.calls + a.folds,
});

// Exact action counts for the five opt-in synthetic sample hands, matching Rust.
export function demoHud(indices: number[]): HudStats {
  const total = emptyHud();
  for (const i of indices) {
    const h = emptyHud();
    h.vpip = h.pfr = { hits: Number(i !== 2), opportunities: 1 };
    if (i === 0 || i === 4) h.foldToThreeBet = { hits: 0, opportunities: 1 };
    if (i === 2) h.threeBet = { hits: 0, opportunities: 1 };
    if (i !== 2) {
      h.wtsd = { hits: Number(i !== 1), opportunities: 1 };
      h.wonWhenSawFlop = { hits: Number(i !== 0), opportunities: 1 };
      if (i !== 1)
        h.wonAtShowdown = { hits: Number(i !== 0), opportunities: 1 };
      h.streets[0].seen = 1;
      if (i !== 1) h.streets[1].seen = h.streets[2].seen = 1;
    }
    if (i === 1) {
      h.streets[0].cbet = { hits: 1, opportunities: 1 };
      h.streets[0].aggression.bets = h.aggression.bets = 1;
    }
    if (i === 3) h.streets[0].aggression.calls = h.aggression.calls = 1;
    for (const key of frequencyKeys) {
      total[key].hits += h[key].hits;
      total[key].opportunities += h[key].opportunities;
    }
    for (const key of ['bets', 'raises', 'calls', 'folds'] as const) {
      total.aggression[key] += h.aggression[key];
      h.streets.forEach((s, index) => {
        total.streets[index].aggression[key] += s.aggression[key];
      });
    }
    h.streets.forEach((s, index) => {
      const t = total.streets[index];
      t.seen += s.seen;
      for (const key of ['cbet', 'foldToCbet'] as const) {
        t[key].hits += s[key].hits;
        t[key].opportunities += s[key].opportunities;
      }
    });
  }
  return total;
}
