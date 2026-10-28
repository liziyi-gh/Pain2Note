(async () => {
  const nativeInvoke = window.__TAURI_INTERNALS__.invoke;
  const waitFor = async (check, label) => {
    const deadline = Date.now() + 30000;
    while (Date.now() < deadline) {
      const result = check();
      if (result) return result;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`Timed out: ${label}`);
  };
  const button = (label) =>
    [...document.querySelectorAll('button')].find(
      (element) => element.textContent.trim() === label,
    );
  try {
    await waitFor(
      () => document.querySelector('.chart-empty'),
      'empty application',
    );
    document.querySelector('.page-heading button.primary').click();
    await waitFor(() => button('选择文件'), 'import dialog');
    (
      await waitFor(
        () => !button('选择文件')?.disabled && button('选择文件'),
        'ready file picker',
      )
    ).click();
    await waitFor(
      () =>
        document
          .querySelector('.report-summary')
          ?.textContent.includes('5 手新增'),
      'import 5 hands',
    );
    await waitFor(
      () => document.querySelectorAll('tbody tr').length === 5,
      'load native rows',
    );
    const overview = await nativeInvoke('get_overview', { filter: {} });
    if (
      overview.net !== 3870 ||
      overview.hands !== 5 ||
      overview.evMissing !== 0 ||
      overview.hud.wtsd.hits !== 3 ||
      overview.hud.wtsd.opportunities !== 4 ||
      overview.hud.streets[0].cbet.hits !== 1 ||
      overview.hud.streets[0].cbet.opportunities !== 1
    )
      throw new Error(`Incorrect overview: ${JSON.stringify(overview)}`);
    (
      await waitFor(
        () => !button('选择文件')?.disabled && button('选择文件'),
        'ready to reimport',
      )
    ).click();
    await waitFor(
      () =>
        document
          .querySelector('.report-summary')
          ?.textContent.includes('0 手新增') &&
        document
          .querySelector('.report-summary')
          ?.textContent.includes('5 手重复'),
      'deduplicate reimport',
    );
    (
      await waitFor(() => {
        const close = document.querySelector('[aria-label="关闭导入"]');
        return close && !close.disabled && close;
      }, 'import finished')
    ).click();
    document.querySelector('.nav-submenu button:last-child').click();
    await waitFor(
      () =>
        document.querySelector('[aria-label="WTSD"] .hud-rate')?.textContent ===
        '75.0%',
      'native HUD showdown rate',
    );
    await waitFor(
      () =>
        document.querySelector('.hud-streets tbody tr:first-child td .hud-rate')
          ?.textContent === '100.0%',
      'native HUD flop continuation bet',
    );
    button('盈利概览').click();
    await waitFor(
      () => document.querySelectorAll('tbody tr').length === 5,
      'restore hand rows',
    );
    document.querySelector('[aria-label="查看 RCDEMO005"]').click();
    await waitFor(
      () => document.querySelector('.hand-drawer'),
      'native hand detail',
    );
    button('查看原始牌谱').click();
    await waitFor(
      () =>
        document
          .querySelector('.raw-hand')
          ?.textContent.includes('Poker Hand #RCDEMO005'),
      'original history rendered',
    );
    await nativeInvoke('plugin:event|emit', {
      event: 'desktop-smoke-result',
      payload: {
        ok: true,
        hands: overview.hands,
        net: overview.net,
        ev: overview.ev,
        checks: [
          'real IPC',
          'SQLite import',
          'EV',
          'duplicate import',
          'overview',
          'HUD opportunity counts',
          'HUD submenu',
          'hand detail',
        ],
      },
    });
  } catch (error) {
    await nativeInvoke('plugin:event|emit', {
      event: 'desktop-smoke-result',
      payload: { ok: false, error: String(error) },
    });
  }
})();
