# 选择每轮练习的单词数量

- **Input:** 用户本轮需求及 refine 阶段已明确的可调整默认方案；`.grimoire/CONTEXT.md` 的 `active-wordbook`、`practice-round`；现有导入规范 `.grimoire/spec/0001-excel-wordbook-import.md`
- **Date:** 2026-09-23
- **Status:** Completed
- **Endpoint:** 实施计划；本次不执行实现

## Summary

在练习设置中选择本轮单词数：预设 5、10、20、50，默认 10；也可输入 1–255 的整数。开始练习及完成后的“再练一轮”都按当前选择从当前单词本随机抽取，数量不足时使用实际可用词条，进度与结果显示实际题数。保持三种练习模式及已有统计行为。现有前端固定传 10，后端将抽样限制为最多 10，必须同时调整两侧。

本需求改变此前导入功能规范中“最多十个”的**练习数量**约束；该规范和 `.grimoire/adr/0001-store-imported-wordbooks-in-sqlite.md` 的 SQLite 存储、按单词本随机抽样等既有边界仍保持。该规范的“UI 不改变每轮上限”仅是当时导入功能的范围界定，不阻止本次单独需求。

## Design impact

### 练习数量设置与启动

- **Action:** Modify
- **Location:** `src/App.tsx` 的 setup 界面、`startPractice`、summary 的“再练一轮”入口
- **Responsibility:** 保存当前预设或自定义数量，验证自定义输入，并将有效数量传给 `sampleWordbook`；启动请求期间禁止更改数量，避免请求和界面选择不一致。
- **Relationships:** 沿用 `WordbookService.sampleWordbook(wordbookId, limit)`、现有 `WordbookSummary.entryCount`、`createQuestions` 和实际题数驱动的进度/汇总。
- **Rationale:** 数量属于现有练习设置和启动流程，不引入新的服务或持久化字段；沿用结果页现有的 `startPractice(state.mode)`，让“再练一轮”使用同一选择。输入无效时停留设置页，不向后端提交非整数或越界数。保留默认 10，避免既有用户行为意外改变。

### 抽样上限

- **Action:** Modify
- **Location:** `src-tauri/src/wordbooks/repository.rs` 的 `WordbookRepository::sample`
- **Responsibility:** 移除硬编码 `limit.min(10)`，按已传入的 `u8` 请求数量从指定单词本随机抽取至多该数量的不同词条。
- **Relationships:** 现有 `src-tauri/src/wordbooks/mod.rs::sample_wordbook` 接口已接收 `u8`，前端服务接口已接收 `limit: number`；SQL 的 `ORDER BY RANDOM() LIMIT ?2` 保留。
- **Rationale:** 限制发生在持久层；只改 UI 会导致选择 20 或 50 后实际仍只得到 10。1–255 的自定义范围与当前 `u8` 边界一致，无须改变命令签名或数据库结构。直接在现有查询边界修正，不另外创建抽样算法。

## Implementation steps

### 1. 接通可选择的数量和输入验证

- **Files or discovery point:** `src/App.tsx` 的 setup、`startPractice` 和 summary 分支；需要时查阅 Astryx 的布局及控件文档。
- **Change:** 在“练习设置”中提供 5、10、20、50 预设及自定义输入入口，初始选中 10。自定义值只接受 1–255 的十进制整数；空值、非整数、0、负值或超过 255 时展示输入反馈并阻止启动。启动时把验证后的数量交给 `sampleWordbook(activeWordbookId, count)`，不再写死 10。请求中禁用数量控件；已有的“再练一轮”调用复用当前数量选择。
- **Outcome:** 选择预设或合法自定义值后可开始对应数量的一轮练习；无效自定义值不会发起抽样。
- **Verification:** `src/App.test.tsx` 中检查默认 10、各预设、自定义合法/非法值、加载时禁用及结果页重练请求参数。

### 2. 解除后端十词截断

- **Files or discovery point:** `src-tauri/src/wordbooks/repository.rs::sample` 及该文件测试；`src-tauri/src/wordbooks/mod.rs::sample_wordbook` 用于确认参数边界。
- **Change:** 把 SQL 参数中的 `limit.min(10)` 改为传入的请求数量；保留现有 wordbook 过滤、随机顺序和唯一行抽样。更新原先“传 50 仍返回 10”的测试，验证库中足够词条时能返回超过 10 的指定数量，库中不足时只返回实际数量，并且结果限定于所选单词本、无重复 ID。
- **Outcome:** 预设及自定义数量可以贯通前端服务与 SQLite 抽样，原有少量词条的行为仍成立。
- **Verification:** 运行 Rust wordbooks 测试，并检查 `u8` 边界与前端验证一致。

### 3. 回归现有练习流转

- **Files or discovery point:** `src/App.test.tsx`、必要时 `src/domain/practice.test.ts`。
- **Change:** 用服务 mock 模拟少于请求量的词库，以及超过十个词条的返回；断言进度/结果的总题数始终等于实际返回条数而不是所选数量。覆盖切换单词本后仍使用所选数量、没有词条时的现有错误提示，以及练习模式、提示、跳过、重试计时不受影响。
- **Outcome:** 可变题数不会破坏答题、结果统计和再练一轮路径。
- **Verification:** `bun run test`、`bun run typecheck`、`bun run check`；后端运行 `cargo test --manifest-path src-tauri/Cargo.toml wordbooks`，并手动使用大于 10 条的导入单词本完成一轮。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 自定义输入为空、非整数、0、负值或超过 255 | 提示有效范围，不能开始请求；已有合法预设切换后可正常开始 | 步骤 1、3 |
| 选择 20、50 或更大自定义值，而词库词条更少 | 返回所有可用词条；进度和结果按实际题数显示 | 步骤 2、3 |
| 词库为空 | 不进入无题的练习状态，维持现有错误提示 | 步骤 1、3 |
| 请求尚未返回时用户尝试更改数量或重复开始 | 不产生与所选数量不一致的第二轮请求 | 步骤 1、3 |
| 完成后点击“再练一轮” | 使用此前选择的数量重新随机抽样；本轮统计清零 | 步骤 1、3 |
| 请求超过十个词条 | 后端不再静默截成十个；不跨单词本、不重复词条 | 步骤 2 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 默认、预设、自定义和验证 | React Testing Library 的 UI 交互测试 | 服务收到选定整数；无效输入不发起请求 |
| 词库不足、空词库和结果总数 | App 交互测试 | 总题数等于服务返回的实际长度；空词库仍显示错误 |
| 后端取消固定上限、范围及不重复 | Rust repository 测试 | 请求 20/50 可超过 10；不足时返回实际条数；ID 唯一且属于目标单词本 |
| 重练、加载锁定及已有模式 | App 回归测试和手动桌面验证 | 同一选择贯穿启动和重练，请求期间不可更改；练习/汇总正常 |
| 类型、格式和完整测试 | `bun run test`、`bun run typecheck`、`bun run check`；`cargo test --manifest-path src-tauri/Cargo.toml wordbooks` | 所有相关检查通过 |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src/App.tsx` | Modify | 选择及校验数量，传给启动/重练逻辑 |
| `src/App.test.tsx` | Modify | 覆盖预设、自定义、数量不足及回归路径 |
| `src-tauri/src/wordbooks/repository.rs` | Modify | 解除十词截断，更新抽样测试 |

## Risks and open questions

- **风险：** 仅修改前端仍被后端 `limit.min(10)` 截断；前后端必须一并修改并通过跨边界测试。
- **风险：** UI 将无效文本转换为数字时可能得到 0、NaN 或小数；必须在调用 Tauri 之前验证，并确保 255 不超出当前 `u8` 命令签名。
- **可调整方案：** 预设 5/10/20/50、自定义 1–255、默认 10、词库不足则练实际数量，是 refine 阶段已告知用户的可调整默认方案；若用户提出不同范围或不足处理，再回到讨论与知识记录，不在实施时自行改变语义。
