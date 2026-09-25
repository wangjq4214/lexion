# 单词考试实现计划

- **Input:** [考试规格 0003](../spec/0003-vocabulary-exam.md)；[项目上下文](../CONTEXT.md)；[ADR 0006](../adr/0006-keep-exam-scoring-separate-from-practice-review.md)
- **Date:** 2026-09-24
- **Status:** Proposed

## Summary

新增独立于逐题练习的考试流程：选定一个词本或收藏/错题集合，分别设置两方向题数，从来源抽出合计数量的不同词条；答完后一次核对各题、显示百分制成绩，并将错误或空白题逐题且仅一次写入错题本。不调用练习调度或复习完成接口。此计划只安排实施，不修改产品要求。

## Design impact

### 来源与取题契约

- **Action:** Modify
- **Location:** `src-tauri/src/wordbooks/repository.rs`、`src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`。
- **Responsibility:** 接收所选来源/词本及两方向题数，验证可用数量，在一次来源读取中抽取不重复配对并返回固定方向的题；数量不足明确报错。收藏和错题无需原词本，且取题不写 `review_coverage`、`review_memory` 或 `pending_reviews`。
- **Relationships:** 复用 SQLite 三类来源及 Tauri→TypeScript 服务边界，不复用会产生练习复习令牌/覆盖写入的 `schedulePractice`。现有 `sample*` 使用 `u8` 限制并在不足时返回短列表，不能直接承诺考试所需的“要么足量、要么提示”契约；实现时核对所需数值范围及原子取样校验。
- **Rationale:** 在持有数据的边界验证真实可用量，比只用界面预估值更能应对集合变化；不改动既有练习调度语义。

### 考试作答和核对

- **Action:** Create / Modify
- **Location:** 新建 `src/domain/exam.ts`、考试设置与作答界面（建议 `src/features/exam/`、`src/routes/exam.tsx`），修改 `src/routes/index.tsx` 入口；按路由数据存续需求在 `src/state/appState.ts` 添加独立考试状态。
- **Responsibility:** 分别输入两方向题量、管理考试题与用户答案，在最终核对前不暴露正确答案或正误；核对时重用 `src/domain/practice.ts` 的 `isExactAnswer`、`getExpectedAnswer`，以正确数/总题数×100 得出百分制分数，并显示每题结果。至少一题才可开始；某方向可以为零。
- **Relationships:** 与现有单词本列表、集合可用性及来源选择协作；不把考试塞进 `practiceReducer` 的提交、提示、跳过或复习状态机。实施 UI 前按项目要求运行 `bunx astryx build`、查阅布局/令牌与使用组件的 CLI 文档，以组件布局实现。
- **Rationale:** 考试的最终统一判分与练习的即时结算有不同生命周期；共享答案规则而不共享练习完成副作用。

### 错题写入

- **Action:** Modify / reuse
- **Location:** 考试核对流程、`src/data/wordbooks.ts` 的 `recordMistakeOnce`；必要时仅补充对应测试。
- **Responsibility:** 为每道核对为错的题保留稳定且相互不同的提交标识，写入错题本一次；阻止重复点击和重复查看造成重复写入；失败后能重试而不重计已成功的题。
- **Relationships:** 复用 `repository.rs` 中 `mistake_submissions` 的事务防重与配对独立生命周期；不得调用 `completeReview`。检查既有防重标识的保留窗口与考试页面存续是否足以支持重试，不引入与用户无关的成绩持久化要求。
- **Rationale:** 直接逐题调用非防重的 `recordMistake` 会在网络超时/重复核对时重复累计；沿用现有可靠的防重入口更小。

## Implementation steps

### 1. 建立考试取题读契约

- **Files or discovery point:** `src-tauri/src/wordbooks/repository.rs`、`mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`；核对 `entries`、`favorites`、`mistakes` 当前配对唯一性和 `sample*` 限制。
- **Change:** 增加不触及复习表的考试专用读取路径；校验来源、词本、题数及来源实时可用量。在同一快照抽出恰好两方向之和的不同配对，然后确定各方向题数；不足时返回可呈现的验证错误，不返回缩水的考试。
- **Outcome:** 三类来源（含无词本时的收藏/错题）可以独立出题；超量请求明确失败，练习调度状态不变。
- **Verification:** Rust 仓储测试覆盖空来源、恰好足量、少于总数、词本范围、同场去重及取题前后复习表不变。

### 2. 实现纯考试判分和状态

- **Files or discovery point:** `src/domain/exam.ts`、`src/domain/exam.test.ts`；必要时 `src/state/appState.ts`。
- **Change:** 用考试专用状态保存题目、方向、输入值及一次最终核对的结果；复用既有方向提示/答案与严格判定函数，空白算错，分数保持数学百分比（展示格式按界面需求处理，不另定新评分规则）。一方向为零而总数大于零仍可运行；总数为零拒绝开始。
- **Outcome:** 核对前没有题目级正误；核对后结果与分数一致且重复查看稳定。
- **Verification:** 单元测试覆盖两方向比例、大小写/空白、中文精确判定、全对/全错/部分正确、未作答、单一方向和重复核对。

### 3. 接入设置、答题和结果界面

- **Files or discovery point:** `src/routes/index.tsx`、`src/features/practice/PracticeSetup.tsx` 的现有入口模式、建议新建 `src/routes/exam.tsx` 与 `src/features/exam/`；`src/App.test.tsx`。
- **Change:** 提供考试入口和来源/词本选择及两个题数字段；开始前验证题数、开始时处理来源变化导致的不足；作答阶段录入各题答案且不显示对错，最终核对后呈现答案、逐题正误和成绩。避免与练习路由状态互相覆盖；在考试需要访问来源而无词本的场景沿用现有集合入口行为。
- **Outcome:** 用户可从三类来源完成独立的双向考试，统一核对才看到结果。
- **Verification:** 交互测试覆盖设置→作答→核对，空集合、不足、纯单方向、无词本但有收藏/错题、切换来源及重复点击。

### 4. 接通错题写入并确保核对幂等

- **Files or discovery point:** `src/routes/exam.tsx` 或 `src/features/exam/` 的核对协调逻辑，既有 `WordbookService.recordMistakeOnce` 和 `src-tauri/src/wordbooks/repository.rs` 测试。
- **Change:** 最终核对冻结考试答案，逐道为错误/空白题生成稳定提交标识，使用防重写入；禁用并发重复提交，对部分成功/失败保留可重试状态，不在重新查看成绩时再次提交。错误反馈可见，确认成功的题不可再增计。
- **Outcome:** 每道错题仅累计一次；写失败不会伪装成成功，也不修改练习复习状态。
- **Verification:** 模拟写入超时、失败后重试及重复触发；仓储测试验证同一标识同配对幂等、不同标识各增一次，并核对复习表不变。

### 5. 回归与桌面验收

- **Files or discovery point:** `src/App.test.tsx`、对应领域与 Rust 测试、`package.json`。
- **Change:** 回归原有练习入口、练习错题计数、收藏/错题脱离词本后的入口；覆盖规格中的双向考试、超量、统一核对及复习隔离。
- **Outcome:** 考试新增行为成立，练习与来源生命周期不退化。
- **Verification:** 运行 `bun run test`、`bun run check`、`bun run build`、`cargo test --manifest-path src-tauri/Cargo.toml`；实际桌面端手动测试三类来源。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 题数都为零、非有效整数，或合计超过当前集合大小 | 禁止无题考试；超过集合大小提示减少题数，不重复或静默缩量 | 步骤 1–3 |
| 两方向共享同一小型来源，或开始前集合内容变化 | 仍仅使用不同配对；开始时重新核对实际可用量 | 步骤 1、3 |
| 无任何词本但收藏/错题非空 | 该集合可出题，词本选项不可用 | 步骤 1、3 |
| 中文释义有多个近义表达、英文大小写/首尾空白 | 严格沿用现有判定规则，不新增语义匹配 | 步骤 2 |
| 用户留空一题后核对，或连续点击核对/查看结果 | 空白算错；每题写一次，结果与分数不变 | 步骤 2、4 |
| 部分错题写入成功后遇到数据库失败/超时 | 告知错误并安全重试，已成功项不重复累计 | 步骤 4 |
| 考试来源词本随后被替换或删除 | 已写入错题按独立配对继续存在 | 步骤 4、5 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 来源及题数与不重复、无复习副作用 | Rust 仓储集成测试 | 返回题数精确、配对唯一、超量验证失败、复习表无新增/更改 |
| 统一核对及百分制成绩 | 纯领域单测与界面测试 | 核对前不揭示结果；每题等分、空白为错、正确数/总数×100 |
| 错题幂等与失败恢复 | Rust 幂等测试 + 前端服务失败模拟 | 同题只累计一次，部分成功后重试不重复，复习反馈未被调用 |
| 三类来源/无词本与练习回归 | App 交互测试及桌面演练 | 各入口可用，原练习与收藏/错题生命周期正常 |
| 静态质量 | `bun run check`、`bun run build`、`cargo test --manifest-path src-tauri/Cargo.toml` | 类型、格式、构建及后端测试通过 |

## Risks and open questions

- **Risk:** 现有 `sample*` 只返回最多请求数量，不保证足额；必须在数据边界作实时校验，不能仅依赖旧练习的“抽到多少练多少”。
- **Risk:** 现有 `recordMistakeOnce` 的提交标识有过期清理；让同一次考试的失败重试沿用稳定标识且成功核对后不再触发写入，避免跨页面重复提交。
- **Risk:** UI 数据与 SQLite 来源可能在抽题前发生变化；实际取题结果而非界面预估决定能否开考。
- **Discovery point:** 考试状态在页面导航中的存续方式与现有 TanStack Router/Jotai 路由模式适配；实施时选用最小不丢作答状态的方案，不预设成绩历史或跨重启续考。
