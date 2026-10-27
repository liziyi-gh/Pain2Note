export interface Player {
  seat: number;
  name: string;
  stack: number;
  cards: string[];
  invested: number;
  collected: number;
  folded: boolean;
  allIn: boolean;
}
export interface HandRow {
  id: string;
  playedAt: string;
  heroCards: string[];
  position: string;
  bigBlind: number;
  net: number;
  ev: number | null;
  showdown: boolean;
  currency: string;
}
export interface Hand extends HandRow {
  tableName: string;
  smallBlind: number;
  hero: string;
  players: Player[];
  board: string[];
  evNote: string;
  rake: number;
  vpip: boolean;
  pfr: boolean;
  raw: string;
}
export interface Point {
  index: number;
  playedAt: string;
  net: number;
  ev: number | null;
  netBb: number;
  evBb: number | null;
}
export interface Overview {
  hands: number;
  net: number;
  ev: number | null;
  evKnown: number;
  evMissing: number;
  bb100: number;
  vpip: number;
  pfr: number;
  showdown: number;
  rake: number;
  currency: string;
  currencies: string[];
  stakes: number[];
  points: Point[];
}
export interface ImportReport {
  files: number;
  imported: number;
  duplicates: number;
  rejected: number;
  issues: string[];
}
export interface ImportBatch {
  at: string;
  report: ImportReport;
}
export interface Filter {
  currency?: string;
  bigBlind?: number;
  from?: string;
  to?: string;
  search?: string;
}
export const emptyOverview: Overview = {
  hands: 0,
  net: 0,
  ev: 0,
  evKnown: 0,
  evMissing: 0,
  bb100: 0,
  vpip: 0,
  pfr: 0,
  showdown: 0,
  rake: 0,
  currency: 'USD',
  currencies: [],
  stakes: [],
  points: [],
};
export const money = (n: number | null, currency = 'USD', signed = false) =>
  n == null
    ? '—'
    : new Intl.NumberFormat('en-US', {
        style: 'currency',
        currency,
        signDisplay: signed ? 'exceptZero' : 'auto',
      }).format(n / 100);
