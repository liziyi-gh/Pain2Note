<script lang="ts">
  import { Info, Activity } from 'lucide-svelte';
  import {
    percent,
    sample,
    aggressionFactor,
    aggressionFrequency,
    type HudStats,
    type Frequency,
  } from '../lib/hud';

  let {
    stats,
    hands,
    loading,
  }: { stats: HudStats; hands: number; loading: boolean } = $props();
  const preflop = [
    {
      key: 'vpip',
      label: 'VPIP',
      name: '自愿入池率',
      note: '翻前自愿跟注或加注的手数 / 全部手数，盲注与 ante 不计入。',
    },
    {
      key: 'pfr',
      label: 'PFR',
      name: '翻前加注率',
      note: '翻前至少加注一次的手数 / 全部手数。',
    },
    {
      key: 'threeBet',
      label: '3B',
      name: '翻前再加注',
      note: '面对一次加注且可以再加注时，做出 3-bet 的手数 / 机会数；包含 squeeze。',
    },
    {
      key: 'fourBet',
      label: '4B',
      name: '翻前四次下注',
      note: '面对 3-bet 且可以再加注时，做出 4-bet 的手数 / 机会数；包含冷 4-bet。',
    },
    {
      key: 'foldToThreeBet',
      label: 'Fold to 3B',
      name: '面对 3-bet 弃牌',
      note: '自己 open-raise 后面对 3-bet 时，弃牌手数 / 机会数。',
    },
    {
      key: 'foldToFourBet',
      label: 'Fold to 4B',
      name: '面对 4-bet 弃牌',
      note: '自己 3-bet 后面对 4-bet 时，弃牌手数 / 机会数。',
    },
  ] as const;
  const showdown = [
    {
      key: 'wtsd',
      label: 'WTSD',
      name: '进入摊牌率',
      note: '看到翻牌后进入摊牌的手数 / 看到翻牌的手数，包含全下发牌。',
    },
    {
      key: 'wonAtShowdown',
      label: 'W$SD',
      name: '摊牌胜率',
      note: '摊牌后获派彩的手数 / 摊牌手数；分池获派彩也计入，不要求净盈利。',
    },
    {
      key: 'wonWhenSawFlop',
      label: 'WWSF',
      name: '见翻牌后赢池率',
      note: '看到翻牌后获派彩的手数 / 看到翻牌的手数；包含非摊牌赢池。',
    },
  ] as const;
  const streets = ['Flop · 翻牌', 'Turn · 转牌', 'River · 河牌'];
</script>

{#snippet rate(f: Frequency)}
  <strong class="hud-rate">{percent(f)}</strong>
  <small class="hud-sample">{sample(f)} 次</small>
{/snippet}

<div class="hud-summary" aria-live="polite">
  <div><Activity size={18} /><span>Hero 技术统计</span></div>
  <span
    >{loading ? '正在更新…' : `${hands.toLocaleString()} 手符合当前筛选`}</span
  >
</div>
{#if !hands && !loading}
  <div class="notice">
    <Info size={17} /><span
      >当前没有符合条件的手牌。导入牌谱或调整筛选后，统计会自动更新。</span
    >
  </div>
{/if}

<section class="panel hud-panel" aria-label="翻前统计">
  <div class="panel-heading">
    <div>
      <h2>翻前 <span class="hud-section-label">PREFLOP</span></h2>
      <p>按可行动的机会计算，每手每项最多记录一次。</p>
    </div>
  </div>
  <div class="hud-grid">
    {#each preflop as item}
      <article class="hud-card" aria-label={item.label}>
        <div class="hud-card-heading">
          <span>{item.label}</span><span>{item.name}</span>
        </div>
        {@render rate(stats[item.key])}
        <p>{item.note}</p>
      </article>
    {/each}
  </div>
</section>

<section class="panel hud-panel" aria-label="摊牌与进攻度">
  <div class="panel-heading">
    <div>
      <h2>
        摊牌与进攻度 <span class="hud-section-label">SHOWDOWN & AGGRESSION</span
        >
      </h2>
      <p>区分赢池能力、摊牌倾向与翻后主动性。</p>
    </div>
  </div>
  <div class="hud-grid">
    {#each showdown as item}
      <article class="hud-card" aria-label={item.label}>
        <div class="hud-card-heading">
          <span>{item.label}</span><span>{item.name}</span>
        </div>
        {@render rate(stats[item.key])}
        <p>{item.note}</p>
      </article>
    {/each}
    <article class="hud-card" aria-label="AF">
      <div class="hud-card-heading">
        <span>AF</span><span>翻后进攻系数</span>
      </div>
      <strong class="hud-rate">{aggressionFactor(stats.aggression)}</strong>
      <small class="hud-sample"
        >{stats.aggression.bets + stats.aggression.raises} 次进攻 / {stats
          .aggression.calls} 次跟注</small
      >
      <p>（Bet + Raise）/ Call，不计 check 与 fold；有进攻但无跟注时显示 ∞。</p>
    </article>
    <article class="hud-card" aria-label="AFq">
      <div class="hud-card-heading">
        <span>AFq</span><span>翻后进攻频率</span>
      </div>
      {@render rate(aggressionFrequency(stats.aggression))}
      <p>（Bet + Raise）/（Bet + Raise + Call + Fold），不计 check。</p>
    </article>
    <article class="hud-card" aria-label="Saw Flop">
      <div class="hud-card-heading">
        <span>Saw Flop</span><span>见翻牌率</span>
      </div>
      {@render rate({ hits: stats.streets[0].seen, opportunities: hands })}
      <p>看到翻牌的手数 / 全部手数；包含全下后的发牌。</p>
    </article>
  </div>
</section>

<section class="panel hud-panel" aria-label="分街统计">
  <div class="panel-heading">
    <div>
      <h2>分街统计 <span class="hud-section-label">STREET BY STREET</span></h2>
      <p>持续下注、面对持续下注的弃牌率，以及每条街的进攻度。</p>
    </div>
  </div>
  <div class="table-scroll">
    <table class="hud-streets">
      <thead
        ><tr
          ><th>街道 / 到达手数</th><th>C-bet <small>持续下注率</small></th><th
            >Fold to C-bet <small>面对持续下注弃牌</small></th
          ><th>AF <small>进攻系数</small></th><th
            >AFq <small>进攻频率</small></th
          ></tr
        ></thead
      >
      <tbody
        >{#each stats.streets as street, i}
          <tr aria-label={streets[i]}>
            <th scope="row"
              >{streets[i]}<small>{street.seen.toLocaleString()} 手</small></th
            >
            <td>{@render rate(street.cbet)}</td>
            <td>{@render rate(street.foldToCbet)}</td>
            <td
              ><strong class="hud-rate"
                >{aggressionFactor(street.aggression)}</strong
              ><small class="hud-sample"
                >{street.aggression.bets + street.aggression.raises} / {street
                  .aggression.calls} 次</small
              ></td
            >
            <td>{@render rate(aggressionFrequency(street.aggression))}</td>
          </tr>
        {/each}</tbody
      >
    </table>
  </div>
</section>

<section class="hud-method" aria-label="统计口径">
  <Info size={17} />
  <div>
    <h3>如何阅读这些数据</h3>
    <p>
      百分比下方为「命中次数 / 机会数」。没有机会时显示 —，有机会但未命中时显示
      0.0%。小样本只适合查看动作分布。
    </p>
    <p>
      Flop
      C-bet：翻前最后加注者，在无人领先下注且仍有可下注对手时的首次行动。Turn /
      River C-bet：上一街持续下注且未被加注，并在本街继续下注。Donk bet 不算
      C-bet；面对 C-bet 之前已有其他玩家加注时，不计 Fold to C-bet 机会。
    </p>
    <p>
      全下后自动发牌不产生下注机会，多次发牌每手只计一次。3B / 4B
      排除无法重新加注、对手全下后只能跟注的情况。参考常见 HUD
      指标，各平台的具体统计口径可能不同。
    </p>
  </div>
</section>
