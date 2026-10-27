# 翻前权益表与 Rust 实现核验

调研日期：2026-10-26。目标是具体亮牌后的 all-in equity，不是起手牌对随机范围的平均胜率，也不是 GTO 开局表。

## 公开表候选

### 169 类起手牌表：不适用于具体亮牌直接结算

- [oscar6echo/Poker2](https://github.com/oscar6echo/Poker2)：有穷举程序和 CSV；仓库首页未见明确开源许可证。
- [py-poker-equity](https://github.com/mitchtabian/py-poker-equity)：说明翻前通过 169 类手牌查询来自 Poker2 的结果。
- [GeorgeLiPoker 数据说明](https://georgelipoker.com/data)：说明其 hand-vs-hand 数据按 169 类组合平均。

由此判断：这些类别平均表不能替代实际 `As Ah vs Kd Kc` 与 `As Ah vs Ks Kh` 的不同权益。需要保留双方的相对花色阻断关系，多人或已知死牌还需要额外状态。未将这些表复制到项目。

### pkcore 的具体双人表：完整性通过，花色一致性失败

[pkcore](https://github.com/ImperialBower/pkcore) 声明 MIT / Apache-2.0 双许可，并提供预生成的双人翻前表。

本次下载核验的是 revision `6d1bfac9c6077c54aee2a8a94a0f51e44728f525` 的 [generated/hups.db](https://github.com/ImperialBower/pkcore/blob/6d1bfac9c6077c54aee2a8a94a0f51e44728f525/generated/hups.db)。

- SHA-256：`40ed65cedb8deafda7058f412a4b3c95b6596871f312de62e65fb3d3307cbd41`
- 812,175 条不同双人具体底牌记录；数量与合法无序对局数相符。
- 所有记录的 wins + losses + ties 都是 `C(48,5) = 1,712,304`。
- 对 24 种全局花色置换和玩家交换进行规范化时，出现了不同记录映射到相同局面却互换胜负计数的冲突。

首个可复现反例（字段为 higher bitmask, lower bitmask, higher wins, lower wins, ties）：

```text
(211106232532992, 8592031744, 122559, 37216, 1552529)
(70385924046848, 384, 122559, 37216, 1552529)

规范化 key 1524786（9c Tc vs 9d Th）：
(122559, 37216, 1552529) != (37216, 122559, 1552529)
```

这说明这一具体版本不能未经修复就作为可信权益表使用；不代表该项目所有模块都不可靠。下载文件只用于临时核验，未随应用分发。

可复现审计脚本：[convert_preflop_table.py](../scripts/convert_preflop_table.py)。脚本只有在全部覆盖与对称性断言通过时才写出转换后的表；上述版本会在断言处停止，**不会产生可用表**。

## 选择：pokers 0.10.0 + 本地精确表

- [维护者仓库](https://github.com/EddieMataEwy/pokers)
- [固定版本文档](https://docs.rs/pokers/0.10.0/pokers/)
- [exact_equity API](https://docs.rs/pokers/0.10.0/pokers/fn.exact_equity.html)
- [原始 rust_poker](https://github.com/kmurf1999/rust_poker)

这是 OMPEval → rust_poker 路线的 Rust 实现，MIT 许可。维护者说明修正过原库 `exact_equity` 的问题。它支持具体底牌、多人、公共牌和 dead cards 的精确枚举。项目锁定版本 `=0.10.0` 和 Cargo.lock，不使用随机模拟接口。

核验不是只看 README：已阅读枚举入口、牌面编码和结果结构，执行精确已知对局核对、枚举总数检查、跨数据库重开查表检查、花色等价检查、相对花色差异检查、重复牌拒绝检查。翻后自有评估器另外与库的评估器做 5,000 组固定输入随机牌型比较。

本次实际生成的精确权益（含平局份额）：

| Hero | 对手 | Hero equity |
| --- | --- | ---: |
| As Ah | Kd Kc | 81.25548968% |
| As Ah | Ks Kh | 82.63661126% |
| As Ks | Qd Qc | 46.21445725% |
| As Kd | Qh Qc | 42.83515077% |
| 9c Tc | 9d Th | 52.49205165% |
| 6s 6h | 5d 5c | 80.67153963% |
| As Ah | Ad Ac | 50.00000000% |

## 查表契约

`equity_cache` 存储 key / equity / method / boards / created_at，和手牌数据库一起持久化。

- key 带算法及版本前缀 `pokers-0.10.0-exact-v2`，不复用 v1 的缓存结果。
- Hero 保持第一个，手内牌序、对手顺序不影响 key。
- 全局枚举 24 种花色重命名后取最小序列；绝不逐位玩家独立改花色。
- known dead cards 是 key 的一部分。
- 没有手牌 ID、时间、币种、筹码量，权益与金额分离。
- 先查表，缺项精确生成一次并写表；换手牌或重启后都不重算已有局面。
- 这个版本按需增长，附带可恢复的离线生成工具。未生成或声称附带完整所有多人翻前表。

所有 equity 是底池份额而非单独胜率，平局按获胜者人数分摊。预期金额在每手的主池/边池基础上计算，不能把“整手 EV 金额”当成跨手牌复用的查表值。

## 引擎计数适配

0.10.0 源码的 `SimulationResults.eval_count` 是经过等价发牌压缩后的评估次数。`wins_by_mask` 保留了组合权重和赢家集合，应使用它的总和核对公共牌组合数。具体手牌的默认权重是 100，调用前将各手权重设为 1，使总和直接对应物理发牌组合数。

库在汇总多人平局的 `ties` 时使用整数除法。应用直接按每个赢家集合的组合数除以集合大小，再除以全部组合数，保留例如三人平局的 1/3 权益。写入缓存前必须满足总数等于 `C(52 - 2 × 玩家数 - 已知死牌数, 5)`；不完整结果不会存表。回归测试包含只有一张合法牌面的三人公共皇家同花顺，以及 AA 对 KK 的 1,712,304 张合法牌面、花色置换和数据库重开。
