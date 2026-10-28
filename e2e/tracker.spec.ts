import { test, expect } from '@playwright/test';

test('chart hover follows rendered SVG points across aspect ratios and filters', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: '先看看演示' }).click();
  for (const width of [1280, 1920]) {
    await page.setViewportSize({ width, height: 1040 });
    const chart = page.locator('.chart-wrap > svg');
    for (const index of [0, 1, 3, 5]) {
      const target = await chart.evaluate((svg: SVGSVGElement, index) => {
        const point = new DOMPoint(75 + (index / 5) * 850, 150).matrixTransform(
          svg.getScreenCTM()!,
        );
        return { x: point.x, y: point.y };
      }, index);
      await page.mouse.move(target.x, target.y);
      await expect(page.locator('.chart-tooltip strong')).toHaveText(
        `第 ${index} 手`,
      );
      const actual = await chart
        .locator('circle')
        .evaluate((circle: SVGCircleElement) => {
          return new DOMPoint(
            circle.cx.baseVal.value,
            circle.cy.baseVal.value,
          ).matrixTransform(circle.getScreenCTM()!).x;
        });
      expect(Math.abs(actual - target.x)).toBeLessThan(1);
    }
  }
  await page
    .getByRole('textbox', { name: '搜索手牌 ID 或底牌' })
    .fill('RCDEMO002');
  await expect(page.locator('tbody tr')).toHaveCount(1);
  await expect(page.locator('.chart-tooltip')).toHaveCount(0);
});

test('total rake contains only exact Hero deductions and follows filters', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: '先看看演示' }).click();
  const panel = page.getByRole('region', { name: '抽水统计' });
  await expect(panel.locator('strong').first()).toHaveText('$5.30');
  await expect(panel).toContainText('106.00');
  await expect(panel).not.toContainText('估算');
  await expect(panel).not.toContainText('全桌总 Rake');
  await page
    .getByRole('textbox', { name: '搜索手牌 ID 或底牌' })
    .fill('RCDEMO002');
  await expect(page.locator('tbody tr')).toHaveCount(1);
  await expect(panel).toContainText('$0.30');
  await expect(panel).toContainText('30.00');
  await expect(page.locator('.metric').first()).toContainText('+$3.20');
  await page.getByLabel('开始日期').fill('2026-10-21');
  await expect(panel.locator('strong').first()).toHaveText('$0.00');
  await expect(panel).toContainText('0.00');
});

test('ambiguous Hero rake never appears as an exact total or zero', async ({
  page,
}) => {
  await page.goto('/');
  await page.evaluate(async () => {
    const fixture = '/src/lib/demo.ts';
    const { demoHands } = await import(/* @vite-ignore */ fixture);
    demoHands[0].heroRake = null;
  });
  await page.getByRole('button', { name: '先看看演示' }).click();
  const panel = page.getByRole('region', { name: '抽水统计' });
  await expect(panel.locator('strong').first()).toHaveText('—');
  await expect(panel.locator('strong').nth(1)).toContainText('—');
  await expect(panel).toContainText(
    '1 手无法精确归属，总计不可用；已确认 $5.30',
  );
  await page
    .getByRole('textbox', { name: '搜索手牌 ID 或底牌' })
    .fill('RCDEMO002');
  await expect(panel.locator('strong').first()).toHaveText('$0.30');
  await expect(panel).not.toContainText('总计不可用');
});

test('other pot fees show only Hero deductions and disclose missing attribution', async ({
  page,
}) => {
  await page.goto('/');
  await page.evaluate(async () => {
    const fixture = '/src/lib/demo.ts';
    const { demoHands } = await import(/* @vite-ignore */ fixture);
    demoHands[0].otherFees = 500;
    demoHands[0].heroOtherFees = 0;
    demoHands[1].otherFees = 30;
    demoHands[1].heroOtherFees = 30;
    demoHands[2].otherFees = 20;
    demoHands[2].heroOtherFees = null;
  });
  await page.getByRole('button', { name: '先看看演示' }).click();
  const fees = page
    .getByRole('region', { name: '抽水统计' })
    .locator('div')
    .filter({ hasText: '其他底池扣费（Hero）' });
  await expect(fees.locator('strong')).toHaveText('—');
  await expect(fees).toContainText(
    '1 手无法精确归属，总计不可用；已确认 $0.30',
  );
  await page
    .getByRole('textbox', { name: '搜索手牌 ID 或底牌' })
    .fill('RCDEMO001');
  await expect(fees.locator('strong')).toHaveText('$0.00');
  await page
    .getByRole('textbox', { name: '搜索手牌 ID 或底牌' })
    .fill('RCDEMO002');
  await expect(fees.locator('strong')).toHaveText('$0.30');
});

test('hand review displays every runout and the fee breakdown', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: '先看看演示' }).click();
  await page
    .getByRole('button', { name: '查看 RCDEMO001', exact: true })
    .click();
  const dialog = page.getByRole('dialog', { name: '#RCDEMO001' });
  await expect(dialog.locator('.runout')).toHaveCount(2);
  await expect(dialog.locator('.runout').first()).toContainText('第 1 次');
  await expect(dialog.locator('.runout').nth(1)).toContainText('第 2 次');
  await expect(dialog).toContainText('本手全桌 Rake $3.00');
  await expect(dialog).toContainText('Hero Rake $0.00');
});

test('empty state, opt-in demo, chart controls, filters and hand review', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await expect(page.getByText('你的下一段曲线，从这里开始')).toBeVisible();
  await expect(page.locator('tbody tr')).toHaveCount(0);
  await page.getByRole('button', { name: '先看看演示' }).click();
  await expect(page.locator('tbody tr')).toHaveCount(5);
  await expect(
    page.getByText(
      '演示模式 · 仅展示 5 手合成样例与示意 EV，不写入你的数据库。',
    ),
  ).toBeVisible();
  await page.getByRole('button', { name: 'BB', exact: true }).click();
  await page.getByRole('checkbox', { name: 'All-in EV' }).uncheck();
  await page.getByRole('checkbox', { name: 'All-in EV' }).check();
  await page.getByRole('button', { name: 'USD', exact: true }).click();
  await page.locator('.chart-wrap svg').hover({ position: { x: 500, y: 100 } });
  await expect(page.locator('.chart-tooltip')).toBeVisible();
  await page.mouse.move(100, 100);
  await page.screenshot({ path: 'artifacts/ui-overview.png', fullPage: true });
  await page
    .getByRole('button', { name: '查看 RCDEMO005', exact: true })
    .click();
  await expect(page.getByRole('dialog', { name: '#RCDEMO005' })).toBeVisible();
  await page.getByRole('button', { name: '查看原始牌谱' }).click();
  await expect(page.locator('.raw-hand')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.getByRole('button', { name: '手牌记录' }).click();
  await page
    .getByRole('textbox', { name: '搜索手牌 ID 或底牌' })
    .fill('RCDEMO002');
  await expect(page.locator('tbody tr')).toHaveCount(1);
  await expect(page.locator('tbody')).toContainText('$3.20');
  await page.getByRole('button', { name: '重置筛选' }).click();
  await expect(page.locator('tbody tr')).toHaveCount(5);
  await page.getByLabel('开始日期').fill('2026-10-21');
  await expect(page.locator('tbody tr')).toHaveCount(0);
  await page.getByRole('button', { name: '重置筛选' }).click();
  await page.getByRole('button', { name: '退出演示' }).click();
  await expect(page.locator('tbody tr')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('import flow explains desktop requirement and keyboard focus stays in dialog', async ({
  page,
}) => {
  await page.goto('/');
  await page
    .locator('.page-heading')
    .getByRole('button', { name: '导入手牌' })
    .click();
  const dialog = page.getByRole('dialog', { name: '导入 GG Poker 手牌' });
  await expect(dialog).toBeVisible();
  await page.getByRole('button', { name: '选择文件夹', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('文件导入需要桌面版');
  await page.getByRole('button', { name: '选择文件', exact: true }).focus();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: '关闭导入' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await page.getByRole('button', { name: '设置与计算说明' }).click();
  await expect(page.getByText('翻前精确查表，结果永久复用')).toBeVisible();
});
