# BanSolver 翻前范围数据

这两份文件是从 [BanSolver 翻前范围表](https://bansolver.com/tools/preflop) 获取的原始 JSON，保存时没有转换或修改。获取日期：2026-10-27。

| 范围 | 原始地址 | 字节数 | SHA-256 |
| --- | --- | ---: | --- |
| GG Zoom 2BB | [gg-zoom-2bb.json?v=2](https://bansolver.com/preflop-tree/gg-zoom-2bb.json?v=2) | 4,807,178 | `3271cb6746f789481024f2875f5faaa641d9c617ee06e0c9626c7019951304e1` |
| GG Zoom 2.5BB | [gg-zoom-2p5bb.json?v=1](https://bansolver.com/preflop-tree/gg-zoom-2p5bb.json?v=1) | 4,825,408 | `9542f042696fd9f53df4ebe26b76764a42a6525482e8a82db40b07a20fce6737` |

每份数据的 `meta` 记录了来源、100 BB 筹码、盲注和 169 种起手牌顺序；`nodes` 按动作路径保存决策树。两份文件各包含 9,204 个节点，其中 4,161 个节点包含策略。策略字段采用网站的紧凑编码，并非可直接读取的百分比；牌型顺序见 `meta.order`。来源标注 EV 相对于弃牌（弃牌为 0），EV 单位尚未校准。

## 可查询格式

`bansolver-ranges.sqlite3` 同时收录两套表，SQLite `user_version` 为 1。原 JSON 保留作为来源快照，可用 `python3 scripts/normalize_bansolver_ranges.py` 重新生成数据库；脚本先校验来源 SHA-256，再转换并检查数据库完整性。

- `datasets`：表 ID、原始文件哈希及完整 `metadata_json`。
- `hands`：每套表的 169 个牌型及原始顺序。
- `nodes`：`path` 是以 `/` 连接的动作路径，空字符串为根节点；`previous_actor` 是来源的 `p`，根节点为 -1；`terminal_x` 保留来源的 `x`，缺省为 NULL。没有策略数据的终止节点也会保留。
- `actions`：每个决策节点的动作名称、行动者和按 `hands.hand_index` 排列的频率数组。`frequency_35ths_json` 中整数除以 35 得到频率；这是来源数据的精度，不是精确到 1% 的原始 solver 值。
- `nodes.ev_twentieths_json`、`fold_ev_twentieths`：来源 EV 解码后的整数，除以 20 得到网站解码值。网站显示的相对弃牌 EV 为 `(ev_twentieths - fold_ev_twentieths) / 20`；EV 单位未校准。
- `hand_actions`：展开后的查询视图，每行是一条节点、动作和牌型的频率。这里的 EV 是该牌型在节点的 EV，并非各动作单独的 EV。

位置编号沿用网站：3 = EP，2 = MP，1 = CO，0 = BTN，9 = SB，8 = BB。动作名称与路径保持原样，`Raise 7 bb` 表示加注后的总额，不是新增投入。

例如查询 2BB 根节点的 AKs：

```sql
SELECT action, frequency,
       (ev_twentieths - fold_ev_twentieths) / 20.0 AS relative_ev
FROM hand_actions
WHERE dataset_id = 'gg-zoom-2bb' AND path = '' AND hand = 'AKs'
ORDER BY action_index;
```

## 后续对接 solver

这些动作频率是**给定到达该节点时**的条件策略。计算某位玩家到达翻牌的范围时，需要沿实际翻前路径，将该玩家每次所选动作对应的牌型频率相乘；单个节点的动作频率不能直接当作完整到达范围。牌谱动作大小、位置、人数、有效筹码和范围表不匹配时，应提示无法精确匹配，不能悄悄套用其他分支。两份表对应 6-max、100 BB，网站描述的抽水假设为 5% 上限 3 BB，超过 30 BB 再抽 1.5 BB；使用时应核对实际牌局条件。

可以按精确路径导出某个位置的范围文本，例如 CO 加注、BB 跟注到翻牌：

```sh
python3 scripts/export_bansolver_range.py gg-zoom-2bb CO 'Fold/Fold/Raise 2 bb/Fold/Fold/Call'
python3 scripts/export_bansolver_range.py gg-zoom-2bb BB 'Fold/Fold/Raise 2 bb/Fold/Fold/Call'
```

导出从每种牌型权重 1 开始，仅乘以该玩家在路径上的各次动作频率；权重输出时四舍五入至 6 位小数，不归一化。缺失路径或该玩家已经弃牌会报错。脚本不解析牌谱、不判断剩余人数或牌桌是否符合来源假设，也不处理公共牌阻断；调用方须先核对这些条件。该文本可供 solver 导入范围，不会自动启动求解。

[Desktop Postflop 启动接口](https://github.com/liziyi-gh/desktop-postflop/blob/a2ee98fc51fcc8d5dbeb1e7c0fb457722e2b42b9/docs/launch-api.md) 接收 `--import <request.json>`，支持 OOP/IP 范围、公共牌、底池、有效筹码和下注树。当前 v1 只导入街道起始局面，求解仍须在 solver 中手动启动；没有街内节点定位或自动求解接口。转牌、河牌的范围需要考虑此前翻后动作，不能直接复用翻前到达范围。这里尚未把数据库接入回放或启动 solver。

数据及其来源的使用权利和后续更新应以 BanSolver 的规定为准。
