import { emptyOverview, type Filter, type Hand, type Overview } from './types';

// Explicitly opt-in UI preview. Never sent to the native database.
export const demoHands: Hand[] = [
  {
    id: 'RCDEMO001',
    playedAt: '2026-10-20 20:00:00',
    heroCards: ['As', 'Ah'],
    position: 'UTG',
    net: -10000,
    ev: 6047.9,
    board: ['2c', '7d', '9h', 'Ks', '3c'],
    showdown: true,
    rake: 300,
  },
  {
    id: 'RCDEMO002',
    playedAt: '2026-10-20 20:01:00',
    heroCards: ['Kh', 'Qh'],
    position: 'BTN',
    net: 320,
    ev: 320,
    board: ['Ks', '8d', '2h'],
    showdown: false,
    rake: 30,
  },
  {
    id: 'HDDEMO003',
    playedAt: '2026-10-20 20:02:00',
    heroCards: ['7c', '2d'],
    position: 'SB',
    net: -50,
    ev: -50,
    board: [],
    showdown: false,
    rake: 0,
  },
  {
    id: 'RCDEMO004',
    playedAt: '2026-10-20 20:03:00',
    heroCards: ['Ac', 'Kc'],
    position: 'UTG',
    net: 3850,
    ev: -1300,
    board: ['Qc', 'Jc', '2h', 'Tc', '5s'],
    showdown: true,
    rake: 200,
  },
  {
    id: 'RCDEMO005',
    playedAt: '2026-10-20 20:04:00',
    heroCards: ['Ac', 'Ad'],
    position: 'UTG',
    net: 9750,
    ev: 6047.9,
    board: ['2h', '7s', '9d', '3h', '5s'],
    showdown: true,
    rake: 300,
  },
].map((h, i) => ({
  ...h,
  boards: i === 0 ? [h.board, ['4c', '5d', '6h', '8s', 'Kh']] : [h.board],
  otherFees: 0,
  cashDrop: 0,
  heroRake: [0, 30, 0, 200, 300][i],
  heroOtherFees: 0,
  currency: 'USD',
  bigBlind: 100,
  smallBlind: 50,
  hero: 'Hero',
  tableName: 'RushAndCashSample',
  players: [],
  vpip: i !== 2,
  pfr: i !== 2,
  evNote: '界面演示值；真实 EV 由 Rust 引擎和 SQLite 权益表计算。',
  raw: '演示预览。实际导入后可在这里查看完整 GG Poker 原始牌谱。',
}));

export function demoData(filter: Filter): {
  overview: Overview;
  hands: Hand[];
} {
  const hands = demoHands.filter(
    (h) =>
      (!filter.currency || h.currency === filter.currency) &&
      (!filter.bigBlind || h.bigBlind === filter.bigBlind) &&
      (!filter.from || h.playedAt.slice(0, 10) >= filter.from) &&
      (!filter.to || h.playedAt.slice(0, 10) <= filter.to) &&
      (!filter.search ||
        `${h.id} ${h.heroCards.join(' ')}`
          .toLowerCase()
          .includes(filter.search.toLowerCase())),
  );
  let net = 0,
    ev = 0;
  const points = hands.map((h, i) => {
    net += h.net;
    ev += h.ev ?? 0;
    return {
      index: i + 1,
      playedAt: h.playedAt,
      net,
      ev,
      netBb: net / 100,
      evBb: ev / 100,
    };
  });
  const overview: Overview = {
    ...emptyOverview,
    hands: hands.length,
    net,
    ev,
    evKnown: hands.length,
    bb100: hands.length ? net / hands.length : 0,
    vpip: hands.length
      ? (hands.filter((h) => h.vpip).length / hands.length) * 100
      : 0,
    pfr: hands.length
      ? (hands.filter((h) => h.pfr).length / hands.length) * 100
      : 0,
    showdown: hands.length
      ? (hands.filter((h) => h.showdown).length / hands.length) * 100
      : 0,
    currencies: ['USD'],
    stakes: [100],
    points,
    rake: hands.every((h) => h.heroRake != null)
      ? hands.reduce((sum, h) => sum + h.heroRake!, 0)
      : null,
    otherFees: hands.every((h) => h.heroOtherFees != null)
      ? hands.reduce((sum, h) => sum + h.heroOtherFees!, 0)
      : null,
    otherFeesKnown: hands.filter((h) => h.heroOtherFees != null).length,
    otherFeesMissing: hands.filter((h) => h.heroOtherFees == null).length,
    knownOtherFees: hands.reduce((sum, h) => sum + (h.heroOtherFees ?? 0), 0),
    rakeKnown: hands.filter((h) => h.heroRake != null).length,
    rakeMissing: hands.filter((h) => h.heroRake == null).length,
    knownRake: hands.reduce((sum, h) => sum + (h.heroRake ?? 0), 0),
    rakeBb100: hands.some((h) => h.heroRake == null)
      ? null
      : hands.length
        ? (hands.reduce((sum, h) => sum + h.heroRake! / h.bigBlind, 0) * 100) /
          hands.length
        : 0,
  };
  return { overview, hands: [...hands].reverse() };
}
