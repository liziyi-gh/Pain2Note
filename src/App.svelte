<script lang="ts">
  import { onMount } from 'svelte';
  import { focusTrap } from './lib/focusTrap';
  import { invoke, isTauri } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    LayoutDashboard,
    Layers3,
    FolderInput,
    Settings2,
    ChevronRight,
    ChevronLeft,
    ArrowUpRight,
    Database,
    Upload,
    FolderOpen,
    FileText,
    X,
    Check,
    Search,
    SlidersHorizontal,
    TrendingUp,
    Activity,
    Info,
    CheckCheck,
    Spade,
    HardDrive,
    LoaderCircle,
    RotateCcw,
    Download,
    ShieldCheck,
  } from 'lucide-svelte';
  import Cards from './components/Cards.svelte';
  import ProfitChart from './components/ProfitChart.svelte';
  import HandReplay from './components/HandReplay.svelte';
  import HudStats from './components/HudStats.svelte';
  import {
    emptyOverview,
    money,
    type Overview,
    type HandRow,
    type Hand,
    type ImportReport,
    type ImportBatch,
    type Filter,
  } from './lib/types';
  import { demoData, demoHands } from './lib/demo';

  const desktop = isTauri();
  let page = $state('overview'),
    demo = $state(false),
    ready = $state(false),
    loading = $state(false);
  let overview = $state<Overview>(emptyOverview),
    hands = $state<HandRow[]>([]),
    history = $state<ImportBatch[]>([]);
  let error = $state(''),
    dbPath = $state(''),
    hero = $state(localStorage.getItem('pain2note-hero') || 'Hero');
  let currency = $state(''),
    stake = $state(''),
    from = $state(''),
    to = $state(''),
    search = $state(''),
    offset = $state(0);
  let importOpen = $state(false),
    importing = $state(false),
    progress = $state<ImportReport | null>(null),
    report = $state<ImportReport | null>(null);
  let selected = $state<Hand | null>(null),
    detailLoading = $state(false),
    dragOver = $state(false);
  let unit = $state('cash'),
    showEv = $state(true),
    showRaw = $state(false),
    replayOpen = $state(false);
  let generation = 0,
    detailGeneration = 0;
  const nav = [
    { id: 'overview', label: '数据总览', icon: LayoutDashboard },
    { id: 'hands', label: '手牌记录', icon: Layers3 },
    { id: 'imports', label: '导入中心', icon: FolderInput },
    { id: 'settings', label: '设置与计算说明', icon: Settings2 },
  ];
  let title = $derived(
    page === 'stats'
      ? '技术统计'
      : nav.find((n) => n.id === page)?.label || '数据总览',
  );
  let filter = $derived<Filter>({
    currency: currency || undefined,
    bigBlind: stake ? Number(stake) : undefined,
    from: from || undefined,
    to: to || undefined,
    search: search || undefined,
  });

  async function refresh(current: Filter = filter, start = offset) {
    const request = ++generation;
    loading = true;
    try {
      if (current.from && current.to && current.from > current.to)
        throw new Error('开始日期不能晚于结束日期');
      if (demo) {
        const data = demoData(current);
        overview = data.overview;
        hands = data.hands.slice(start, start + 50);
      } else if (desktop) {
        const [summary, rows] = await Promise.all([
          invoke<Overview>('get_overview', { filter: current }),
          invoke<HandRow[]>('get_hands', { filter: current, offset: start }),
        ]);
        if (request !== generation) return;
        overview = summary;
        hands = rows;
      } else {
        overview = emptyOverview;
        hands = [];
      }
      if (request === generation) error = '';
    } catch (e) {
      if (request === generation) error = String(e);
    } finally {
      if (request === generation) loading = false;
    }
  }
  $effect(() => {
    const f = filter,
      start = offset,
      mode = demo;
    if (ready) {
      const timer = setTimeout(() => {
        void mode;
        void refresh(f, start);
      }, 150);
      return () => clearTimeout(timer);
    }
  });
  $effect(() => {
    localStorage.setItem('pain2note-hero', hero);
  });
  function resetFilters() {
    currency = '';
    stake = '';
    from = '';
    to = '';
    search = '';
    offset = 0;
  }
  async function loadHistory() {
    if (desktop) history = await invoke<ImportBatch[]>('get_imports');
  }
  async function importPaths(paths: string[]) {
    if (importing) return;
    if (!hero.trim()) {
      error = '请输入牌谱中的 Hero 名称';
      return;
    }
    demo = false;
    importing = true;
    report = null;
    progress = null;
    error = '';
    importOpen = true;
    try {
      report = await invoke<ImportReport>('import_hands', {
        paths,
        hero: hero.trim(),
      });
      resetFilters();
      await Promise.all([refresh({}, 0), loadHistory()]);
    } catch (e) {
      error = String(e);
    } finally {
      importing = false;
    }
  }
  async function choose(directory: boolean) {
    if (!desktop) {
      error = '文件导入需要桌面版。请在终端运行 npm run desktop。';
      return;
    }
    try {
      const paths = await open({
        directory,
        multiple: !directory,
        title: directory ? '选择 GG Poker 牌谱文件夹' : '选择牌谱或压缩包',
        filters: directory
          ? undefined
          : [
              {
                name: 'GG Poker 手牌与压缩包',
                extensions: ['txt', 'hh', 'log', 'zip', 'gz', 'tgz', 'tar'],
              },
            ],
      });
      if (paths) await importPaths(Array.isArray(paths) ? paths : [paths]);
    } catch (e) {
      error = String(e);
    }
  }
  async function inspect(id: string) {
    const request = ++detailGeneration;
    detailLoading = true;
    showRaw = false;
    replayOpen = false;
    try {
      const hand = demo
        ? demoHands.find((h) => h.id === id)!
        : await invoke<Hand>('get_hand', { id });
      if (request === detailGeneration) selected = hand;
    } catch (e) {
      error = String(e);
    } finally {
      if (request === detailGeneration) detailLoading = false;
    }
  }
  function exportCsv() {
    const lines = [
      'hand_index,played_at,net_minor_units,all_in_ev_minor_units,net_bb,all_in_ev_bb',
      ...overview.points.map((p) =>
        [p.index, p.playedAt, p.net, p.ev ?? '', p.netBb, p.evBb ?? ''].join(
          ',',
        ),
      ),
    ];
    const url = URL.createObjectURL(
      new Blob(['\ufeff' + lines.join('\n')], {
        type: 'text/csv;charset=utf-8',
      }),
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = `pain2note-curve-${overview.currency}.csv`;
    link.click();
    URL.revokeObjectURL(url);
  }
  onMount(() => {
    let disposed = false;
    const cleanups: (() => void)[] = [];
    const register = (fn: () => void) => (disposed ? fn() : cleanups.push(fn));
    if (desktop) {
      void listen<ImportReport>(
        'import-progress',
        (e) => (progress = e.payload),
      )
        .then(register)
        .catch((e) => (error = String(e)));
      void getCurrentWebview()
        .onDragDropEvent((e) => {
          if (e.payload.type === 'over' || e.payload.type === 'enter')
            dragOver = true;
          else dragOver = false;
          if (e.payload.type === 'drop') void importPaths(e.payload.paths);
        })
        .then(register)
        .catch((e) => (error = String(e)));
      void invoke<string>('database_path')
        .then((p) => (dbPath = p))
        .catch((e) => (error = String(e)));
      void loadHistory().catch((e) => (error = String(e)));
    }
    ready = true;
    return () => {
      disposed = true;
      cleanups.forEach((fn) => fn());
    };
  });
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape') {
      selected = null;
      if (!importing) importOpen = false;
    }
  }}
/>

<div class="app-shell">
  <aside class="sidebar">
    <a class="brand" href="#overview" onclick={() => (page = 'overview')}
      ><span class="brand-mark"><Spade size={22} fill="currentColor" /></span
      ><span
        >Pain<span class="brand-two">2</span>Note<small
          >YOUR POKER, IN PERSPECTIVE</small
        ></span
      ></a
    >
    <div class="workspace-label">工作空间 <span>LOCAL</span></div>
    <nav>
      {#each nav as item}<button
          class:active={page === item.id ||
            (item.id === 'overview' && page === 'stats')}
          onclick={() => (page = item.id)}
          ><item.icon size={19} /><span>{item.label}</span
          >{#if item.id === 'hands' && overview.hands}<span class="nav-count"
              >{overview.hands.toLocaleString()}</span
            >{/if}</button
        >
        {#if item.id === 'overview'}
          <div class="nav-submenu" role="group" aria-label="数据总览二级菜单">
            <button
              class:active={page === 'overview'}
              aria-current={page === 'overview' ? 'page' : undefined}
              onclick={() => (page = 'overview')}>盈利概览</button
            >
            <button
              class:active={page === 'stats'}
              aria-current={page === 'stats' ? 'page' : undefined}
              onclick={() => (page = 'stats')}
              >技术统计 <span class="nav-hud">STATS</span></button
            >
          </div>
        {/if}
      {/each}
    </nav>
    <div class="sidebar-section">
      <span>当前游戏</span>
      <div class="game-entry">
        <span class="gg-mark">GG</span>
        <div>GG Poker<small>NL Hold’em · 6-max</small></div>
        <span class="online-dot"></span>
      </div>
    </div>
    <div class="sidebar-import">
      <div class="import-symbol"><FolderInput size={22} /></div>
      <strong>让每一手牌成为经验</strong>
      <p>导入你的牌谱，发现长期表现。</p>
      <button
        onclick={() => {
          importOpen = true;
          report = null;
        }}>导入手牌 <ChevronRight size={15} /></button
      >
    </div>
    <div class="sidebar-bottom">
      <HardDrive size={17} />
      <div>本地数据，始终属于你<small>SQLite · 离线可用 · 无 HUD</small></div>
      <span class="online-dot"></span>
    </div>
    <div class="version">PAIN2NOTE <span>v0.1.0 / EARLY ACCESS</span></div>
  </aside>

  <main>
    <header class="topbar">
      <div class="breadcrumb">
        工作空间 <ChevronRight size={13} />{#if page === 'stats'}数据总览 <ChevronRight
            size={13}
          />{/if} <span>{title}</span>
      </div>
      <div class="topbar-right">
        <span class="local-badge"
          ><span class="online-dot"></span>{demo
            ? '演示工作空间'
            : desktop
              ? '本地连接正常'
              : '浏览器预览'}</span
        ><span class="top-divider"></span><span class="avatar">H</span><span
          >{hero || 'Hero'}</span
        >
      </div>
    </header>
    <div class="main-content">
      {#if demo}<div class="notice demo-notice">
          <Info size={17} /><span
            >演示模式 · 仅展示 5 手合成样例与示意 EV，不写入你的数据库。</span
          ><button
            onclick={() => {
              demo = false;
              resetFilters();
            }}>退出演示 <X size={14} /></button
          >
        </div>{/if}
      {#if error}<div class="notice error-notice" role="alert">
          <Info size={18} /><span>{error}</span><button
            aria-label="关闭提示"
            onclick={() => (error = '')}><X size={17} /></button
          >
        </div>{/if}
      <div class="page-heading">
        <div>
          <div class="eyebrow">
            {page === 'overview'
              ? 'YOUR GAME. THE BIG PICTURE.'
              : page === 'stats'
                ? 'KNOW YOUR TENDENCIES.'
                : page === 'hands'
                  ? 'EVERY HAND TELLS A STORY.'
                  : page === 'imports'
                    ? 'A CLEAN START FOR EVERY SESSION.'
                    : 'MADE FOR YOUR WORKFLOW.'}
          </div>
          <h1>{title}</h1>
          <p>
            {page === 'overview'
              ? '看清波动背后的表现，让下一次决策更有依据。'
              : page === 'stats'
                ? '从翻前到河牌，用 HUD 常见指标看清自己的打法。'
                : page === 'hands'
                  ? '回看每一次决策，从细节中找到进步空间。'
                  : page === 'imports'
                    ? '把零散的牌谱，整理成完整的扑克记录。'
                    : '本地优先、清晰透明的计算方式。'}
          </p>
        </div>
        <button
          class="primary"
          disabled={importing}
          onclick={() => {
            importOpen = true;
            report = null;
          }}
          >{#if importing}<LoaderCircle
              size={17}
              class="spin"
            />正在导入{:else}<Upload size={17} />导入手牌{/if}</button
        >
      </div>

      {#if page === 'overview' || page === 'hands' || page === 'stats'}
        <section class="filterbar" aria-label="手牌筛选">
          <div class="filter-game">
            <span class="gg-mark">GG</span> NL Hold’em
            <span class="tag">6-max</span>
          </div>
          <span class="filter-divider"></span><label
            ><span>币种</span><select
              bind:value={currency}
              onchange={() => {
                stake = '';
                offset = 0;
              }}
              ><option value="">自动 · {overview.currency}</option
              >{#each overview.currencies as c}<option value={c}>{c}</option
                >{/each}</select
            ></label
          ><label
            ><span>级别</span><select
              bind:value={stake}
              onchange={() => (offset = 0)}
              ><option value="">全部级别</option
              >{#each overview.stakes as s}<option value={String(s)}
                  >BB {money(s, overview.currency)}</option
                >{/each}</select
            ></label
          >
          <div class="date-range">
            <input
              aria-label="开始日期"
              type="date"
              bind:value={from}
              onchange={() => (offset = 0)}
            /><span>—</span><input
              aria-label="结束日期"
              type="date"
              bind:value={to}
              onchange={() => (offset = 0)}
            />
          </div>
          <button
            class="icon-button"
            title="重置筛选"
            aria-label="重置筛选"
            onclick={resetFilters}><RotateCcw size={16} /></button
          >
        </section>
        {#if page === 'stats'}
          <div class="hud-filter-search search-box">
            <Search size={16} /><input
              aria-label="搜索手牌 ID 或底牌"
              placeholder="搜索手牌 ID 或底牌…"
              bind:value={search}
              oninput={() => (offset = 0)}
            />
          </div>
          <HudStats stats={overview.hud} hands={overview.hands} {loading} />
        {:else}
          {#if page === 'overview'}
            <div class="metrics">
              <article class="metric">
                <div class="metric-label">
                  实际盈利 <TrendingUp size={17} />
                </div>
                <div
                  class="metric-value"
                  class:positive={overview.net > 0}
                  class:negative={overview.net < 0}
                >
                  {money(overview.net, overview.currency, true)}
                </div>
                <div class="metric-foot">
                  <span class="mini-dot green"></span>已扣除牌谱记录的费用
                </div>
              </article>
              <article class="metric">
                <div class="metric-label">All-in EV <Activity size={17} /></div>
                <div class="metric-value">
                  {money(overview.ev, overview.currency, true)}
                </div>
                <div class="metric-foot">
                  <span class="mini-dot gold"></span>{overview.evMissing
                    ? `${overview.evMissing} 手 EV 缺失 · 累计值不可用`
                    : '翻前精确查表 · 翻后精确枚举'}
                </div>
              </article>
              <article class="metric">
                <div class="metric-label">赢率 <ArrowUpRight size={17} /></div>
                <div class="metric-value">
                  {overview.bb100.toFixed(2)}<small>bb / 100</small>
                </div>
                <div class="metric-foot">按每手实际大盲归一化</div>
              </article>
              <article class="metric">
                <div class="metric-label">总手数 <Layers3 size={17} /></div>
                <div class="metric-value">
                  {overview.hands.toLocaleString()}<small>hands</small>
                </div>
                <div class="metric-foot">GG Poker · 6-max 现金桌</div>
              </article>
            </div>
            <section class="panel rake-panel" aria-label="抽水统计">
              <div>
                <span>总 Rake（Hero）</span>
                <strong>{money(overview.rake, overview.currency)}</strong>
                <p>
                  {overview.rakeMissing
                    ? `${overview.rakeMissing} 手无法精确归属，总计不可用；已确认 ${money(overview.knownRake, overview.currency)}`
                    : '仅计从 Hero 赢得底池中扣除的 Rake'}
                </p>
              </div>
              <div>
                <span>Hero Rake / 100 手</span>
                <strong
                  >{overview.rakeBb100?.toFixed(2) ?? '—'}<small>bb / 100</small
                  ></strong
                >
                <p>按每手大盲归一化，随当前筛选更新</p>
              </div>
              <div>
                <span>Rake 已确认手数</span>
                <strong
                  >{overview.rakeKnown.toLocaleString()}<small
                    >/ {overview.hands.toLocaleString()} 手</small
                  ></strong
                >
                <p>无法区分的分池扣费或零头分配保留缺失，不做分摊</p>
              </div>
              <div>
                <span>其他底池扣费（Hero）</span>
                <strong>{money(overview.otherFees, overview.currency)}</strong>
                <p>
                  {overview.otherFeesMissing
                    ? `${overview.otherFeesMissing} 手无法精确归属，总计不可用；已确认 ${money(overview.knownOtherFees, overview.currency)}`
                    : '仅计 Hero 赢得底池的 Jackpot / Bingo / Fortune / Tax'}
                </p>
              </div>
            </section>
            <section class="panel chart-panel">
              <div class="panel-heading">
                <div>
                  <h2>盈利曲线</h2>
                  <p>每一次波动，都是长期样本的一部分。</p>
                </div>
                <div class="chart-controls">
                  <span class="legend"
                    ><span class="mini-dot green"></span>实际盈利</span
                  ><label class="legend"
                    ><input type="checkbox" bind:checked={showEv} /><span
                      class="mini-dot gold"
                    ></span>All-in EV</label
                  >
                  <div class="segmented">
                    <button
                      class:chosen={unit === 'cash'}
                      onclick={() => (unit = 'cash')}
                      >{overview.currency}</button
                    ><button
                      class:chosen={unit === 'bb'}
                      onclick={() => (unit = 'bb')}>BB</button
                    >
                  </div>
                  <button
                    class="icon-button"
                    aria-label="导出当前曲线采样点 CSV"
                    title="导出当前曲线采样点 CSV"
                    disabled={!overview.hands}
                    onclick={exportCsv}><Download size={16} /></button
                  >
                </div>
              </div>
              {#if overview.hands}<ProfitChart
                  points={overview.points}
                  currency={overview.currency}
                  {unit}
                  {showEv}
                />{:else}<div class="chart-empty">
                  <div class="empty-chart-lines">
                    <span></span><span></span><span></span>
                  </div>
                  <div class="empty-icon"><TrendingUp size={30} /></div>
                  <h3>你的下一段曲线，从这里开始</h3>
                  <p>导入 GG Poker 牌谱，即可查看实际盈利与全下 EV。</p>
                  <button
                    class="primary"
                    onclick={() => {
                      importOpen = true;
                      report = null;
                    }}><FolderOpen size={16} />选择牌谱文件夹</button
                  ><button
                    class="text-button"
                    onclick={() => {
                      demo = true;
                      resetFilters();
                    }}>先看看演示 <ChevronRight size={13} /></button
                  >
                </div>{/if}
              <div class="chart-footer">
                <span
                  ><Info size={13} />{overview.evMissing
                    ? '累计 EV 在首个缺失点后留空，避免伪造完整曲线。'
                    : '盈利按手牌时间累计；EV 包含平局权益与记录的底池扣费。'}</span
                ><span>横轴 · 手牌数</span>
              </div>
            </section>
            <div class="stat-strip">
              <div>
                <span>VPIP</span><strong
                  >{overview.vpip.toFixed(1)}<small>%</small></strong
                >
                <p>自愿入池率</p>
              </div>
              <div>
                <span>PFR</span><strong
                  >{overview.pfr.toFixed(1)}<small>%</small></strong
                >
                <p>翻前加注率</p>
              </div>
              <div>
                <span>SHOWDOWN</span><strong
                  >{overview.showdown.toFixed(1)}<small>%</small></strong
                >
                <p>摊牌手数 / 全部手数</p>
              </div>
              <div>
                <span>RAKE</span><strong
                  >{money(overview.rake, overview.currency)}</strong
                >
                <p>
                  {overview.rakeMissing
                    ? `${overview.rakeMissing} 手无法精确归属`
                    : `Hero Rake ${overview.rakeBb100?.toFixed(2) ?? '—'} bb / 100`}
                </p>
              </div>
              <div class="stat-note">
                <ShieldCheck size={25} /><span
                  >数据可靠，从第一手开始<small
                    >自动去重 · 底池对账 · 原始牌谱保留</small
                  ></span
                >
              </div>
            </div>
          {/if}
          <section class="panel hands-panel">
            <div class="panel-heading">
              <div>
                <h2>
                  {page === 'overview' ? '最近手牌' : '手牌列表'}<span
                    class="count-badge">{overview.hands.toLocaleString()}</span
                  >
                </h2>
                <p>点击一手牌，查看底牌、结果与原始记录。</p>
              </div>
              <div class="search-box">
                <Search size={16} /><input
                  aria-label="搜索手牌 ID 或底牌"
                  placeholder="搜索手牌 ID 或底牌…"
                  bind:value={search}
                  oninput={() => (offset = 0)}
                />
              </div>
            </div>
            <div class="table-scroll">
              <table>
                <thead
                  ><tr
                    ><th>时间 / 手牌 ID</th><th>底牌</th><th>位置</th><th
                      >大盲</th
                    ><th class="right">盈利</th><th class="right">All-in EV</th
                    ><th>结果</th><th></th></tr
                  ></thead
                ><tbody
                  >{#each page === 'overview' ? hands.slice(0, 5) : hands as hand}<tr
                      ><td
                        ><button
                          class="hand-link"
                          onclick={() => inspect(hand.id)}
                          >{hand.playedAt.slice(5)}<small>#{hand.id}</small
                          ></button
                        ></td
                      ><td><Cards cards={hand.heroCards} small /></td><td
                        ><span class="position">{hand.position}</span></td
                      ><td>{money(hand.bigBlind, hand.currency)}</td><td
                        class="right numeric"
                        class:positive={hand.net > 0}
                        class:negative={hand.net < 0}
                        >{money(hand.net, hand.currency, true)}</td
                      ><td class="right numeric"
                        >{money(hand.ev, hand.currency, true)}</td
                      ><td
                        ><span class="result-tag" class:showdown={hand.showdown}
                          >{hand.showdown ? '摊牌' : '非摊牌'}</span
                        ></td
                      ><td
                        ><button
                          class="icon-button"
                          aria-label={`查看 ${hand.id}`}
                          onclick={() => inspect(hand.id)}
                          ><ChevronRight size={16} /></button
                        ></td
                      ></tr
                    >{/each}</tbody
                >
              </table>
              {#if !hands.length}<div class="table-empty">
                  <Layers3 size={25} /><span
                    >{loading
                      ? '正在读取手牌…'
                      : overview.currencies.length
                        ? '没有符合筛选条件的手牌'
                        : '还没有手牌记录，导入后会显示在这里。'}</span
                  >
                </div>{/if}
            </div>
            <div class="table-footer">
              <span
                >{loading
                  ? '正在更新…'
                  : `${overview.hands.toLocaleString()} 手符合筛选条件`}</span
              >{#if page === 'overview'}<button
                  class="text-button"
                  onclick={() => (page = 'hands')}
                  >查看全部手牌 <ChevronRight size={14} /></button
                >{:else}<div class="pagination">
                  <button
                    class="icon-button"
                    aria-label="上一页"
                    disabled={!offset}
                    onclick={() => (offset = Math.max(0, offset - 50))}
                    ><ChevronLeft size={16} /></button
                  ><span
                    >{Math.floor(offset / 50) + 1} / {Math.max(
                      1,
                      Math.ceil(overview.hands / 50),
                    )}</span
                  ><button
                    class="icon-button"
                    aria-label="下一页"
                    disabled={offset + 50 >= overview.hands}
                    onclick={() => (offset += 50)}
                    ><ChevronRight size={16} /></button
                  >
                </div>{/if}
            </div>
          </section>
        {/if}
      {:else if page === 'imports'}
        <section class="panel import-hero">
          <div class="empty-icon"><FolderInput size={32} /></div>
          <h2>一个文件夹，整个扑克旅程</h2>
          <p>递归扫描子文件夹，自动读取压缩包，重复手牌自动跳过。</p>
          <div class="button-row">
            <button
              class="primary"
              disabled={importing}
              onclick={() => choose(true)}
              ><FolderOpen size={17} />导入文件夹</button
            ><button
              class="secondary"
              disabled={importing}
              onclick={() => choose(false)}
              ><FileText size={17} />选择文件</button
            >
          </div>
          <span class="format-list"
            >TXT · ZIP · GZIP · TAR · TGZ　/　也可以将文件拖入窗口</span
          >
        </section>
        <div class="import-benefits">
          <div>
            <FolderOpen size={21} /><strong>递归读取</strong>
            <p>支持子文件夹与最多 4 层嵌套压缩包。</p>
          </div>
          <div>
            <CheckCheck size={21} /><strong>自动去重</strong>
            <p>以 GG 手牌 ID 唯一标识，重复导入无负担。</p>
          </div>
          <div>
            <Database size={21} /><strong>本地保存</strong>
            <p>牌谱、统计与权益查表都保存在本机。</p>
          </div>
        </div>
        <section class="panel">
          <div class="panel-heading">
            <div>
              <h2>导入历史</h2>
              <p>最近 30 次导入，含异常详情。</p>
            </div>
          </div>
          {#if !history.length}<div class="table-empty">
              <FolderInput size={25} /><span>还没有导入任务</span>
            </div>{/if}{#each history as batch}<details class="history-row">
              <summary
                ><span class="history-icon"><Check size={17} /></span><span
                  >{batch.at} UTC<small>{batch.report.files} 个文件</small
                  ></span
                ><span class="positive">+{batch.report.imported} 手</span><span
                  >{batch.report.duplicates} 重复</span
                ><span class:negative={batch.report.rejected > 0}
                  >{batch.report.rejected} 异常</span
                ><ChevronRight size={16} /></summary
              >
              <div class="issue-list">
                {#if !batch.report.issues.length}<p>
                    已完成，未发现异常。
                  </p>{:else}{#each batch.report.issues as issue}<p>
                      {issue}
                    </p>{/each}{/if}
              </div>
            </details>{/each}
        </section>
      {:else}
        <section class="panel settings-panel">
          <h2>导入设置</h2>
          <label class="setting-row"
            ><span
              ><strong>Hero 名称</strong><small
                >与英文牌谱中的玩家名称完全一致；GG 通常导出为 Hero。</small
              ></span
            ><input bind:value={hero} placeholder="Hero" /></label
          >
          <div class="setting-row">
            <span
              ><strong>本地数据库</strong><small
                >关闭应用后可复制该文件备份；运行中还需保留 WAL 文件。</small
              ></span
            ><code>{dbPath || '桌面版启动后显示数据库位置'}</code>
          </div>
          <div class="setting-row">
            <span
              ><strong>支持范围</strong><small
                >GG Poker 英文导出 · 6-max NLHE 现金桌 · 标准桌 / Rush &
                Cash（含现金掉落）</small
              ></span
            ><span class="tag">无 HUD</span>
          </div>
        </section>
        <section class="panel settings-panel">
          <h2>盈利与 EV 的计算方式</h2>
          <div class="explanation">
            <span>01</span>
            <div>
              <h3>实际盈利 = 收到的派彩 − 总投入</h3>
              <p>
                未被跟注的下注退回后从投入扣除。Rake 与
                Jackpot、Bingo、Fortune、Tax 分别统计，均计入底池对账。Hero Rake
                仅统计从 Hero
                赢得底池中实际扣除的金额，不按投入或派彩比例分摊；未获派彩计
                0，独赢全部底池计入该手全部 Rake。
                分池时按主池、边池和各组公共牌还原应得底池，再与派彩对账。
                无法区分 Rake
                与其他扣费、底牌未公开或零头分配不明确时保留缺失；有缺失时总
                Rake、其他底池扣费与 bb/100 分别留空，并显示已确认金额和手数。
                实际盈利已包含这些费用，不重复扣除。不包含站外返现与奖励。 Cash
                Drop 是平台注入主池的资金，计入派彩和全下
                EV，不作为玩家投入或扣费。
              </p>
            </div>
          </div>
          <div class="explanation">
            <span>02</span>
            <div>
              <h3>翻前精确查表，结果永久复用</h3>
              <p>
                先查询 SQLite 权益表。新局面由 pokers 0.10.0 Rust
                引擎精确枚举一次并存表。查表保留玩家底牌、相对花色和已知死牌；不按手牌
                ID 重算，不使用 169×169 平均胜率代替具体底牌。
              </p>
            </div>
          </div>
          <div class="explanation">
            <span>03</span>
            <div>
              <h3>翻后枚举所有可能的剩余公共牌</h3>
              <p>
                使用全下锁定时的公共牌。平局按份额计算，简单多方全下按主池与边池拆分，记录的总扣费按底池金额比例分摊。
                支持 All-in 发牌两次或三次；实际盈利合计所有派彩，EV
                使用分叉前的全下权益，不受已发结果影响。
              </p>
            </div>
          </div>
          <div class="explanation">
            <span>04</span>
            <div>
              <h3>无法可靠计算时，明确保留缺失</h3>
              <p>
                对手底牌未公开、复杂跨街边池等局面不推测权益。累计 EV
                在缺失点后留空。保险、现金退出、锦标赛和 PLO
                暂不支持；导入异常会展示具体原因。
              </p>
            </div>
          </div>
        </section>
      {/if}
      <footer class="main-footer">
        <span>PAIN2NOTE <span>让数据沉淀，让决策进步。</span></span><span
          ><span class="online-dot"></span>只在你的设备上</span
        >
      </footer>
    </div>
  </main>
</div>

{#if importOpen}
  <div class="modal-backdrop">
    <div
      class="import-modal"
      use:focusTrap
      role="dialog"
      aria-modal="true"
      aria-labelledby="import-title"
      tabindex="-1"
    >
      <button
        class="close-button"
        disabled={importing}
        aria-label="关闭导入"
        onclick={() => (importOpen = false)}><X size={20} /></button
      >
      <div class="modal-icon"><FolderInput size={26} /></div>
      <h2 id="import-title">导入 GG Poker 手牌</h2>
      <p class="modal-subtitle">选择文件夹，剩下的交给 Pain2Note。</p>
      <label class="hero-input"
        >Hero 名称<input
          bind:value={hero}
          disabled={importing}
          placeholder="Hero"
        /></label
      >
      <div class="drop-zone">
        <FolderOpen size={32} /><strong
          >{importing
            ? '正在解析、校验并写入数据库…'
            : '牌谱文件或整个文件夹'}</strong
        >
        <p>TXT、ZIP、GZIP、TAR、TGZ · 自动解压与去重</p>
        <div class="button-row">
          <button
            class="primary"
            disabled={importing || !hero.trim()}
            onclick={() => choose(true)}
            ><FolderOpen size={16} />选择文件夹</button
          ><button
            class="secondary"
            disabled={importing || !hero.trim()}
            onclick={() => choose(false)}>选择文件</button
          >
        </div>
      </div>
      {#if importing}<div class="import-progress">
          <LoaderCircle size={18} class="spin" /><span
            >{progress
              ? `${progress.workers || 0} 个解析线程 · 已读取 ${progress.files} 个文件，暂存 ${progress.imported} 手，跳过 ${progress.duplicates} 手重复`
              : '准备多线程导入…首次遇到翻前全下局面时将生成精确查表项。'}</span
          >
        </div>{/if}{#if report}<div class="report-summary" role="status">
          <CheckCheck size={21} />
          <div>
            <strong>导入完成</strong>
            <p>
              {report.imported} 手新增 · {report.duplicates} 手重复 · {report.rejected}
              项异常
            </p>
          </div>
        </div>
        {#if report.issues.length}<details class="report-issues">
            <summary>查看异常详情（最多显示 100 项）</summary>
            <div class="issue-list">
              {#each report.issues as issue}<p>{issue}</p>{/each}
            </div>
          </details>{/if}{/if}{#if error}<p class="inline-error" role="alert">
          {error}
        </p>{/if}
      <div class="modal-note">
        <ShieldCheck size={15} />原始文件不会被修改，所有数据仅保存在本地。
      </div>
    </div>
  </div>
{/if}
{#if selected}
  <div class="drawer-backdrop">
    <div
      class="hand-drawer"
      use:focusTrap
      role="dialog"
      aria-modal="true"
      aria-labelledby="detail-title"
      tabindex="-1"
    >
      <div class="drawer-heading">
        <div>
          <span class="eyebrow">HAND REVIEW</span>
          <h2 id="detail-title">#{selected.id}</h2>
        </div>
        <button
          class="icon-button"
          aria-label="关闭手牌详情"
          onclick={() => (selected = null)}><X size={21} /></button
        >
      </div>
      <div class="hand-context">
        <span>{selected.playedAt}</span><span
          >{selected.tableName} · {selected.position}</span
        >
      </div>
      <div class="review-tabs" aria-label="手牌查看方式">
        <button class:active={!replayOpen} onclick={() => (replayOpen = false)}
          >手牌详情</button
        >
        <button class:active={replayOpen} onclick={() => (replayOpen = true)}
          >手牌回放</button
        >
      </div>
      {#if replayOpen}
        <HandReplay hand={selected} />
      {:else}
        <div
          class="review-table"
          class:multiple-boards={selected.boards.length > 1}
        >
          <div class="table-brand">GG POKER <span>6-MAX · NLHE</span></div>
          <div class="review-board">
            {#if selected.boards.length > 1}
              {#each selected.boards as board, index}
                <div class="runout">
                  <span>第 {index + 1} 次</span><Cards cards={board} />
                </div>
              {/each}
            {:else if selected.board.length}<Cards
                cards={selected.board}
              />{:else}<span>未进入翻牌圈</span>{/if}
          </div>
          <div class="hero-seat">
            <span>{selected.hero} <b>{selected.position}</b></span><Cards
              cards={selected.heroCards}
            />
          </div>
        </div>
        <div class="review-results">
          <div>
            <span>实际盈利</span><strong
              class:positive={selected.net >= 0}
              class:negative={selected.net < 0}
              >{money(selected.net, selected.currency, true)}</strong
            >
          </div>
          <div>
            <span>All-in EV</span><strong
              >{money(selected.ev, selected.currency, true)}</strong
            >
          </div>
        </div>
        <p class="ev-note">
          本手全桌 Rake {money(selected.rake, selected.currency)} · 其他扣费 {money(
            selected.otherFees,
            selected.currency,
          )} · Hero Rake {money(
            selected.heroRake,
            selected.currency,
          )}{selected.heroRake == null ? '（牌谱不足以精确归属）' : ''}
          · Hero 其他扣费 {money(
            selected.heroOtherFees,
            selected.currency,
          )}{selected.heroOtherFees == null ? '（牌谱不足以精确归属）' : ''}
          {#if selected.cashDrop > 0}
            · 现金掉落 {money(selected.cashDrop, selected.currency)}{/if}
        </p>
        <p class="ev-note"><Info size={15} />{selected.evNote}</p>
        {#if selected.players.length}<h3 class="drawer-section-title">
            玩家与筹码
          </h3>
          <div class="player-list">
            {#each selected.players as player}<div>
                <span
                  ><b class="seat-number">{player.seat}</b
                  >{player.name}{#if player.name === selected.hero}<em>HERO</em
                    >{/if}</span
                ><span
                  >{#if player.cards.length}<Cards
                      cards={player.cards}
                      small
                    />{:else}<small>未公开</small>{/if}</span
                ><span
                  class="numeric"
                  class:negative={player.collected - player.invested < 0}
                  class:positive={player.collected - player.invested > 0}
                  >{money(
                    player.collected - player.invested,
                    selected.currency,
                    true,
                  )}</span
                >
              </div>{/each}
          </div>{/if}<button
          class="raw-toggle"
          onclick={() => (showRaw = !showRaw)}
          ><FileText size={17} />{showRaw
            ? '收起'
            : '查看'}原始牌谱<ChevronRight size={15} /></button
        >{#if showRaw}<pre class="raw-hand">{selected.raw}</pre>{/if}
      {/if}
    </div>
  </div>
{/if}
{#if detailLoading}<div class="loading-toast">
    <LoaderCircle size={17} class="spin" />正在读取手牌…
  </div>{/if}
{#if dragOver}<div class="drag-overlay">
    <FolderInput size={48} />
    <h2>松开以导入手牌</h2>
    <p>支持文件、文件夹与压缩包</p>
  </div>{/if}
