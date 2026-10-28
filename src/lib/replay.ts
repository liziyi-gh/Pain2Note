import type { Hand } from './types';

export interface ReplayPlayer {
  seat: number;
  name: string;
  stack: number;
  bet: number;
  invested: number;
  collected: number;
  folded: boolean;
  allIn: boolean;
  cards: string[];
}
export interface ReplayStep {
  street: string;
  description: string;
  boards: string[][];
  pot: number;
  players: ReplayPlayer[];
}

const amount = (value: string) => {
  const match = value.match(/[$€£¥]?[\d,]+(?:\.\d+)?/);
  return match ? Math.round(Number(match[0].replace(/[$€£¥,]/g, '')) * 100) : 0;
};
const cards = (line: string) =>
  [...line.matchAll(/\[([^\]]+)\]/g)].flatMap((m) => m[1].split(/\s+/));

/** Reconstruct snapshots from the original history, including old imported hands. */
export function replayHand(hand: Hand): ReplayStep[] {
  const lines = hand.raw.split(/\r?\n/).map((line) => line.trim());
  const seats = lines.flatMap((line) => {
    const match = line.match(/^Seat (\d): (.+) \(([^ ]+)(?: in chips)?\)$/);
    return match
      ? [{ seat: Number(match[1]), name: match[2], stack: amount(match[3]) }]
      : [];
  });
  if (!seats.length) return [];
  const players: ReplayPlayer[] = seats.map((p) => ({
    ...p,
    bet: 0,
    invested: 0,
    collected: 0,
    folded: false,
    allIn: false,
    cards: p.name === hand.hero ? [...hand.heroCards] : [],
  }));
  let street = '翻前',
    description = '发牌前',
    boards: string[][] = [[]],
    pot = 0;
  const steps: ReplayStep[] = [];
  const save = () =>
    steps.push({
      street,
      description,
      pot,
      boards: boards.map((board) => [...board]),
      players: players.map((p) => ({ ...p, cards: [...p.cards] })),
    });
  save();
  let summary = false;
  for (const line of lines) {
    if (line.startsWith('*** SUMMARY')) {
      summary = true;
      street = '结算';
      continue;
    }
    if (summary) {
      const player = players.find((p) =>
        line.startsWith(`Seat ${p.seat}: ${p.name} `),
      );
      if (!player) continue;
      const exposed = / (?:showed|mucked) \[/.test(line) ? cards(line) : [];
      const wins = [
        ...line.matchAll(/(?:won|collected) \(([$€£¥]?[\d,.]+)\)/g),
      ];
      const won =
        player.collected === 0
          ? wins.reduce((sum, m) => sum + amount(m[1]), 0)
          : 0;
      if (exposed.length || won) {
        if (exposed.length) player.cards = exposed;
        if (won) {
          player.collected += won;
          player.stack += won;
        }
        description = line;
        save();
      }
      continue;
    }
    const drop = line.match(/^Cash Drop to Pot\s*:\s*total\s+(.+)$/i);
    if (drop) {
      pot += amount(drop[1]);
      description = line;
      save();
      continue;
    }
    const board = line.match(
      /^\*\*\* (?:(FIRST|SECOND|THIRD) )?(FLOP|TURN|RIVER) \*\*\*/,
    );
    if (board) {
      const run = board[1] === 'SECOND' ? 1 : board[1] === 'THIRD' ? 2 : 0;
      boards[run] = cards(line);
      street = `${board[1] ? `第 ${run + 1} 次 · ` : ''}${{ FLOP: '翻牌', TURN: '转牌', RIVER: '河牌' }[board[2]]}`;
      players.forEach((p) => {
        p.bet = 0;
      });
      description = street;
      save();
      continue;
    }
    if (/^\*\*\* (?:(?:FIRST|SECOND|THIRD) )?SHOWDOWN/.test(line)) {
      street = '摊牌';
      description = line;
      save();
      continue;
    }
    const refund = line.match(/^Uncalled bet \(([^)]+)\) returned to (.+)$/);
    if (refund) {
      const player = players.find((p) => p.name === refund[2]);
      if (player) {
        const n = amount(refund[1]);
        player.stack += n;
        player.bet -= n;
        player.invested -= n;
        pot -= n;
        description = line;
        save();
      }
      continue;
    }
    const collect = line.match(
      /^(.+) collected ([$€£¥]?[\d,.]+) from (?:main |side )?pot/,
    );
    if (collect) {
      const player = players.find((p) => p.name === collect[1]);
      if (player) {
        const n = amount(collect[2]);
        player.stack += n;
        player.collected += n;
        street = '结算';
        description = line;
        save();
      }
      continue;
    }
    const player = players.find((p) => line.startsWith(`${p.name}: `));
    if (!player) continue;
    const action = line.slice(player.name.length + 2);
    if (action.startsWith('folds')) player.folded = true;
    else if (action.startsWith('shows [')) player.cards = cards(action);
    else if (/^(checks|mucks|doesn't show)/.test(action)) {
      /* no chips change */
    } else if (/^(raises|calls|bets|posts) /.test(action)) {
      const n = action.startsWith('raises ')
        ? amount(action.split(' to ')[1] || '') - player.bet
        : amount(action);
      player.stack -= n;
      player.invested += n;
      pot += n;
      if (!action.includes('ante')) player.bet += n;
      if (action.includes('all-in') || player.stack === 0) player.allIn = true;
    } else continue;
    description = line;
    save();
  }
  return steps;
}
