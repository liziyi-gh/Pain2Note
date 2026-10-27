import { test, expect } from '@playwright/test';

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
