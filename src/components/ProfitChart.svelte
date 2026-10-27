<script lang="ts">
  import type { Point } from '../lib/types';
  import { money } from '../lib/types';
  let {
    points,
    currency,
    unit = 'cash',
    showEv = true,
  }: {
    points: Point[];
    currency: string;
    unit?: string;
    showEv?: boolean;
  } = $props();
  let hover = $state<number | null>(null);
  let data = $derived([
    { index: 0, playedAt: '', net: 0, ev: 0, netBb: 0, evBb: 0 },
    ...points,
  ]);
  const net = (p: Point) => (unit === 'bb' ? p.netBb : p.net / 100);
  const ev = (p: Point) =>
    unit === 'bb' ? p.evBb : p.ev == null ? null : p.ev / 100;
  let values = $derived(
    data.flatMap((p) => [net(p), ...(showEv && ev(p) != null ? [ev(p)!] : [])]),
  );
  let low = $derived(Math.min(0, ...values));
  let high = $derived(Math.max(0, ...values));
  let padding = $derived(
    Math.max((high - low) * 0.16, unit === 'bb' ? 1 : 0.5),
  );
  let min = $derived(low - padding),
    max = $derived(high + padding);
  const x = (p: Point) =>
    75 + (p.index / Math.max(points.at(-1)?.index ?? 1, 1)) * 850;
  const y = (v: number) => 270 - ((v - min) / (max - min)) * 240;
  const line = (which: 'net' | 'ev') => {
    let started = false;
    return data
      .map((p) => {
        const value = which === 'net' ? net(p) : ev(p);
        if (value == null) {
          started = false;
          return '';
        }
        const part = `${started ? 'L' : 'M'}${x(p)},${y(value)}`;
        started = true;
        return part;
      })
      .join(' ');
  };
  const label = (value: number) =>
    unit === 'bb'
      ? `${value.toFixed(Math.abs(value) < 10 ? 1 : 0)}`
      : money(value * 100, currency);
  let active = $derived(hover == null ? null : data[hover]);
  function move(event: PointerEvent) {
    const bounds = (event.currentTarget as SVGElement).getBoundingClientRect();
    const target = ((event.clientX - bounds.left) / bounds.width) * 960;
    let nearest = 0;
    data.forEach((p, i) => {
      if (Math.abs(x(p) - target) < Math.abs(x(data[nearest]) - target))
        nearest = i;
    });
    hover = nearest;
  }
</script>

<div class="chart-wrap">
  <svg
    viewBox="0 0 960 310"
    role="img"
    aria-label="按手牌顺序累计的实际盈利与全下 EV 曲线"
    onpointermove={move}
    onpointerleave={() => (hover = null)}
  >
    <defs
      ><linearGradient id="profit-fill" x1="0" y1="0" x2="0" y2="1"
        ><stop offset="0%" stop-color="#158f78" stop-opacity="0.12" /><stop
          offset="100%"
          stop-color="#158f78"
          stop-opacity="0"
        /></linearGradient
      ></defs
    >
    {#each [0, 1, 2, 3, 4] as i}
      {@const value = min + ((max - min) * i) / 4}
      <line
        x1="75"
        x2="925"
        y1={y(value)}
        y2={y(value)}
        stroke="#e9edf2"
        stroke-dasharray="3 5"
      />
      <text
        x="61"
        y={y(value) + 4}
        text-anchor="end"
        fill="#8c95a5"
        font-size="11">{label(value)}</text
      >
    {/each}
    <line x1="75" x2="925" y1={y(0)} y2={y(0)} stroke="#d8dde5" />
    <path
      d={`${line('net')} L925,${y(0)} L75,${y(0)} Z`}
      fill="url(#profit-fill)"
    />
    {#if showEv}<path
        d={line('ev')}
        stroke="#d2a24f"
        stroke-width="2.3"
        fill="none"
        stroke-linejoin="round"
        stroke-dasharray="6 4"
      />{/if}
    <path
      d={line('net')}
      stroke="#158f78"
      stroke-width="2.5"
      fill="none"
      stroke-linejoin="round"
    />
    {#each [0, 1, 2, 3, 4] as i}<text
        x={75 + (i * 850) / 4}
        y="298"
        text-anchor="middle"
        fill="#8c95a5"
        font-size="11"
        >{Math.round(
          ((points.at(-1)?.index ?? 0) * i) / 4,
        ).toLocaleString()}</text
      >{/each}
    {#if active}<line
        x1={x(active)}
        x2={x(active)}
        y1="25"
        y2="270"
        stroke="#b5beca"
        stroke-dasharray="3 4"
      /><circle
        cx={x(active)}
        cy={y(net(active))}
        r="4"
        fill="#158f78"
        stroke="white"
        stroke-width="2"
      />{/if}
  </svg>
  {#if active}<div
      class="chart-tooltip"
      style:left={`${Math.min(76, Math.max(8, (x(active) / 960) * 100))}%`}
    >
      <strong>第 {active.index.toLocaleString()} 手</strong><span
        >实际盈利 <b>{label(net(active))}{unit === 'bb' ? ' bb' : ''}</b></span
      ><span
        >全下 EV <b
          >{ev(active) == null ? '数据缺失' : label(ev(active)!)}{unit ===
            'bb' && ev(active) != null
            ? ' bb'
            : ''}</b
        ></span
      >
    </div>{/if}
</div>
