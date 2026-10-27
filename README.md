# Pain2Note

本地优先的 Poker Tracker。Svelte 5 + Tauri 2 + Rust + SQLite，借鉴 Hand2Note 的数据工作区，不包含 HUD。

## 运行

本机已安装 Node.js 24.22.1 / npm 11.19.1、Rust/Cargo 1.99.1、rustfmt、Clippy；macOS 已有 Xcode Command Line Tools。SQLite 通过 `rusqlite/bundled` 编译进应用，无需单独安装数据库服务。完整 Xcode 不是本项目 macOS 桌面构建的必要条件。

```sh
npm install
npm run desktop
```

如果终端尚未加载 rustup：先运行 `source "$HOME/.cargo/env"`。Node.js 官方校验包在当前用户的 `.local/share/pain2note`，命令链接在 `.local/bin`。

```sh
npm run check           # Svelte / TypeScript
npm run test:rust       # Rust 与导入集成测试
npm run lint:rust       # Clippy，warnings as errors
npm exec playwright -- install chromium
npm run test:ui         # 真实浏览器交互测试（预览模式）
npm run test:desktop    # macOS 真实 WebView + Tauri IPC + 临时 SQLite 冒烟测试
npm run tauri -- build --bundles app
```

macOS 构建产物：[Pain2Note.app](src-tauri/target/release/bundle/macos/Pain2Note.app)。这是本地构建，未进行 Apple 开发者签名与公证。`npm run dev` 仅启动浏览器界面预览；实际文件导入使用桌面版的 Rust 后端。

## 第一版范围

- GG Poker / PokerCraft **英文导出**，6-max NLHE 现金桌；允许六人桌内只有 2–6 人。支持普通桌、Rush & Cash，含 `Cash Drop to Pot : total $…` 现金掉落。
- 文件选择、文件夹递归扫描、窗口拖入；TXT / HH / LOG、ZIP、GZIP、TAR、TGZ，支持嵌套压缩包。
- UTF-8（可含 BOM）、UTF-16 LE/BE（需要 BOM）。解压内容在内存中处理，不修改原始文件、不写出压缩包路径。
- GG 手牌 ID 唯一约束去重，重新导入、换文件名和换压缩包都不会重复统计。重复检查先于解析和 EV 计算。
- 原始牌谱、玩家、投入、退回、派彩、公共牌、VPIP、PFR、位置、抽水及其他底池扣费入库；必须通过底池对账。
- 支持 All-in 发牌两次 / 三次，含翻前、翻牌或转牌全下后的分叉；保留各组公共牌并合计全部派彩，EV 按分叉前的全下权益计算，不按发牌次数倍增。
- 总 Rake 仅统计从 Hero 赢得的底池中实际扣除的抽水，以整数分保存，不按投入或派彩比例分摊。未获派彩计 0，独赢全部底池计入该手全部 Rake；分池按主池、边池和各组公共牌还原应得底池，再与净派彩对账。GG 只记录底池总 Rake 与其他扣费；无法唯一确定 Hero 份额时（例如 Jackpot 与 Rake 无法分别归属、底牌缺失或零头归属不明）保留缺失，不估算、不当作 0。有缺失时总 Rake 和 bb/100 留空，并展示已确认金额与手数。bb/100 为 `100 × Σ(每手 Hero Rake / 该手 BB) / 筛选手数`，空样本为 0，随筛选更新。旧数据库自动重新计算，保留原有盈利、EV 和多组公共牌。Jackpot / Bingo / Fortune / Tax 单列为全桌其他扣费，不计入 Rake。实际盈利已扣费，不再重复扣除。
- 现金掉落单独保留，按 `玩家投入 + Cash Drop = 派彩 + Rake + 其他扣费` 对账；实际盈利仍为 Hero 派彩减投入，不额外加一次掉落金额。全下 EV 将平台资金计入主池，扣费按包含掉落的底池金额比例分摊；支持短筹码边池和多次发牌。
- 实际盈利、All-in EV、bb/100、手牌列表、手牌详情、日期/币种/级别/ID/具体底牌筛选。
- 曲线可切换货币 / BB、开关 EV、悬停查看。CSV 导出的是**当前曲线采样点**，大样本约 1,600 点，并非完整手牌导出。
- 导入进度、逐项错误报告、最近 30 批导入历史；后台任务使用持久子进程解析和计算 EV，按可用 CPU 数启动，最多 8 个进程。单个大文件也会逐手分发，进度显示实际进程数。主进程统一写入 SQLite 并提交事务；worker 共享临时权益缓存，继承已有缓存，新权益随整批事务保存。子进程或数据库写入失败时整批回滚并清理进程；损坏牌谱仍逐手报告，其他有效手牌可保存。

默认 Hero 名称为 `Hero`，可在导入弹窗或设置中修改。日期保持原始牌谱时间，不擅自推断时区。不同币种分别查看，不进行隐式汇率换算。

[五手合成样例牌谱](fixtures/gg-six-max.txt) 可用于试导入：第一次新增 5 手，重复导入新增 0 手、跳过 5 手，总实际盈利为 USD 38.70。**这是测试样例，不是真实战绩。** 界面中的“演示模式”是独立的内存预览，EV 为示意值，完全不写入数据库；导入样例文件后才使用真实 Rust 计算结果。

## 翻前 EV：查表优先

已调研现成表和 Rust 实现，选择 `pokers = 0.10.0` 的精确枚举引擎，详见[来源核验与选择依据](docs/equity-sources.md)。没有继续使用逐手蒙特卡洛估计。

1. 按底牌、相对花色、对手集合和已知死牌规范化查表键，保留 Hero 身份。
2. 查询 SQLite `equity_cache`；命中直接复用。
3. 新局面精确枚举全部合法五张公共牌，含平局权益，将结果和引擎版本写入表。
4. 再次遇到该局面或重启应用后继续查表，手牌 ID、筹码金额和文件名不影响权益复用。

目前是**按需增长的精确权益表**，不是随程序附带完整翻前对局库。可以提前离线生成常用对局，或完整双人无死牌对局；支持中断后重新执行，已有表项会跳过计算：

```sh
# 把数据库路径替换为“设置”页面显示的实际路径。建议先退出桌面应用。
cargo run --release --manifest-path src-tauri/Cargo.toml --example precompute_equity -- /path/to/tracker.sqlite3

# --all：所有具体双人对局，花色等价去重。可能需要较长时间。
cargo run --release --manifest-path src-tauri/Cargo.toml --example precompute_equity -- /path/to/tracker.sqlite3 --all
```

多人全下和带已知死牌的局面按需补表。对每个 Hero 有资格争夺的主池/边池单独计算权益，其余已知底牌作为死牌；总扣费按各底池金额比例分摊。这一扣费分摊约定会影响与其他 Tracker 的 EV 对比。

翻后在所有后续下注已锁定的街道精确枚举；未参与全下的手牌 EV 等于实际盈利。未知底牌、跨街复杂边池等无法可靠处理的 EV 为 `NULL`，累计 EV 从首个缺失点后留空，不用真实盈利伪装未知 EV。

权益缓存使用 `pokers-0.10.0-exact-v2` 版本键。针对库的计数接口，具体底牌使用单位权重，从赢家集合计算平局份额，并核对代表的公共牌总数；不把压缩后的评估次数当成公共牌组合数。旧版缓存不会被新算法复用。

## 数据与边界

macOS 数据库默认位于 `~/Library/Application Support/com.pain2note.tracker/tracker.sqlite3`；其他平台通过 Tauri 应用数据目录定位。运行中采用 WAL，备份时先关闭应用或同时保留相关 WAL 文件。所有数据留在本机，运行时不请求在线胜率服务。

单个展开文件上限 64 MiB，每批读取/展开预算 512 MiB（嵌套层会累计），最多 10,000 个处理条目、4 层压缩嵌套；超出时报告错误，已成功解析的内容仍按该批事务提交。符号链接不跟随。加密包、RAR、7z 不支持。

暂不支持锦标赛、PLO、9-max、保险、All-in Cashout、复合补盲/死盲格式。未识别的 Cash Drop 格式、重复掉落记录及发底牌后才出现的掉落记录会被拒绝并显示原因。未包含站外返现、站外奖金、存取款、实时自动监控文件夹和逐动作动画回放。

自动化解析测试使用合成 GG 格式牌谱，也已用本地导出的 Cash Drop 牌谱核对格式与底池对账。支持对手只亮一张牌的记录，仍要求 Hero 有两张底牌；全下对手底牌不完整时保留 EV 缺失。不同版本/地区出现的新格式会进入错误报告。

桌面冒烟测试使用生产界面与生产命令，只替换系统文件选择器的返回路径，数据库位于独立临时目录。它自动验证导入 5 手、盈利 USD 38.70、无缺失 EV、再次导入去重以及原始牌谱详情，不读取或写入正式数据库。需要可用的 macOS 图形会话；超时会以非零状态退出。

## 代码结构

- [界面](src/App.svelte) / [曲线](src/components/ProfitChart.svelte)
- [GG 解析器](src-tauri/src/parser.rs)
- [导入与安全解压](src-tauri/src/importer.rs)
- [导入子进程与权益缓存](src-tauri/src/import_worker.rs)
- [SQLite 查询与汇总](src-tauri/src/db.rs)
- [翻前权益表](src-tauri/src/preflop.rs) / [牌型与底池 EV](src-tauri/src/equity.rs)
- [离线制表工具](src-tauri/examples/precompute_equity.rs)
- [集成测试](src-tauri/tests/tracker.rs) / [浏览器测试](e2e/tracker.spec.ts)
