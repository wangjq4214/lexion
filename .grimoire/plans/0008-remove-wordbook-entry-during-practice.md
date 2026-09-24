# 练习时从单词本删除当前词条

- **Input:** 用户需求及确认：在练习界面收藏按钮旁增加垃圾桶按钮；确认后从当前单词本永久移除词条；仅单词本来源显示；删除当前题后直接进入下一题（末题结束本轮），本题不计入答对、错误或跳过。
- **Date:** 2026-09-24
- **Status:** Completed
- **Endpoint:** 实施计划，不执行实现

## Summary

在单词本来源的练习题中，收藏按钮旁提供带可访问名称的垃圾桶按钮和删除确认框。取消确认不改变数据；确认后按当前单词本 ID 与当前词条 ID 删除 SQLite 中的对应词条，成功后才从本轮题目移除该题并推进，汇总题数和本题统计相应调整。收藏夹与错题本是独立集合（`.grimoire/adr/0002-preserve-favorites-independent-of-wordbooks.md`、`.grimoire/adr/0003-preserve-mistake-records-independent-of-wordbooks.md`），删除原词条不等于移除这些集合中的副本。复习的来源覆盖与跨来源记忆遵循 `.grimoire/adr/0004-schedule-practice-with-coverage-and-due-review.md`。

## Design impact

### 持久层删除与命令边界

- **Action:** Modify
- **Location:** `src-tauri/src/wordbooks/repository.rs`、`src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`
- **Responsibility:** 提供限定单词本及词条身份的删除操作；事务中维护该来源对应的覆盖记录，保持其他单词本、收藏、错题及跨来源记忆不受误删。
- **Relationships:** React 通过 `WordbookService` 调 Tauri 命令；不能仅按英文/中文组合删除，因为多个单词本可包含同一组合。
- **Rationale:** 沿用现有 repository/命令/服务边界；不在客户端直接变更数据库，也不复用 `removeFavorite`。

### 当前题和本轮统计

- **Action:** Modify
- **Location:** `src/App.tsx`、`src/features/practice/practiceReducer.ts`、必要时 `src/features/practice/PracticeSummary.tsx`
- **Responsibility:** 只在单词本来源发起当前题的确认与持久删除，成功后移除当前题并推进；排除该题在本轮已累计的错误、跳过计数，更新总题数和最后一题收尾。提示次数保持现有已使用次数；已有独立错题记录不因源词条删除而撤销。
- **Relationships:** 与 `pendingMistakeRef`、`completingReview` 和异步收藏状态共用当前题；按词条及本轮身份保护确认、写入与响应，避免请求晚返回时误删另一题。
- **Rationale:** reducer 持有题目及本轮统计，删除成功后集中转移状态，避免把删除当作跳过或调用 `completeReview` 记录一次并不存在的答题结果。

### 练习题操作入口

- **Action:** Modify
- **Location:** `src/features/practice/PracticeQuestion.tsx`（必要时在 `src/App.tsx` 挂载确认框）
- **Responsibility:** 在收藏图标旁呈现垃圾桶图标和确认框，仅单词本来源可见；等待写入时禁用重复操作，失败时保留当前题并提示错误。
- **Relationships:** 复用导入流程已有的 `AlertDialog` 确认交互；实施前按 `AGENTS.md` 查阅 `bunx astryx build`、`docs layout`、相关组件及 token 文档，不使用原生布局容器或硬编码样式。
- **Rationale:** 贴合既有操作位置及项目 UI 约束，而非引入另一套弹窗和样式体系。

## Implementation steps

### 1. 增加限定来源的持久删除接口

- **Files or discovery point:** `src-tauri/src/wordbooks/repository.rs`、`src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`。
- **Change:** 用 `wordbook_id` 与 `entry_id` 同时限定 `entries` 行；事务内处理该单词本对该配对的 `review_coverage`，避免清掉其他来源/同配对的记录。对不存在或不属于该单词本的 ID 返回明确结果，不把无操作误报为删除成功；验证实际行归属而不相信 UI 可见性。检查调度时创建的 pending review token 生命周期，避免把删除误记为完成；其清理方式以现有调度契约为准。
- **Outcome:** 只移除指定单词本中的指定条目；其他来源的独立记录及复习记忆不受影响。
- **Verification:** repository 测试覆盖归属不匹配、相同配对跨单词本、覆盖记录及收藏/错题保留；命令和服务契约测试覆盖成功、无操作及失败。

### 2. 让删除成为本轮独立的题目转移

- **Files or discovery point:** `src/features/practice/practiceReducer.ts`、`src/App.tsx`、相关 reducer/App 测试。
- **Change:** 新增成功删除当前题的 action：从问题列表移除该题，复位当前题输入/提示/揭晓状态并推进到下一剩余题；从本轮累计统计扣除该题已产生的错误及跳过，不增加正确数；已使用的提示次数按原记录保留。末题被删则用剩余题数结束本轮并停止计时。只有持久删除成功后 dispatch；正在记录错题或完成复习时阻止竞争操作，当前题变化/轮次变化后不应用旧响应。更新设置页缓存的单词本词条数，后续重练重新从数据源调度。
- **Outcome:** 删除不等同于跳过；进度、总题数、计时及结果与剩余题目一致。
- **Verification:** reducer 单测覆盖首/中/末题、唯一题、先输错或使用提示、已揭晓的情况；App 交互测试验证后端失败时题目和统计不变，以及重练不再抽到已删除的词条。

### 3. 添加图标入口及确认流程

- **Files or discovery point:** `src/features/practice/PracticeQuestion.tsx`、`src/App.tsx`、`src/features/wordbooks/WordbookImportFlow.tsx`、`src/App.test.tsx`。
- **Change:** 先按 Astryx 工作流发现按钮/图标/AlertDialog 用法；仅对本轮单词本来源在收藏按钮旁显示垃圾桶按钮，给图标按钮明确的中文可访问名称；确认框注明将从当前单词本删除当前词，取消不调用删除接口。提交后提供忙碌/失败反馈，确认操作绑定打开时的词条与单词本，而不是重新读取可能已切换的当前题。
- **Outcome:** 用户能明确确认或取消，且无法误把收藏夹/错题本条目当作单词本词条删除。
- **Verification:** UI 测试覆盖按钮位置/可访问名称、来源条件、取消、确认、重复点击、失败与异步切题保护；手动桌面验证确认框和图标。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 取消确认，或数据库删除失败/词条已不属于此单词本 | 不推进、不更改本轮统计；失败有可见反馈 | 步骤 1、3 |
| 当前题之前已有错误/提示，或跳过后已揭晓答案 | 删除后本题不计入本轮错误、跳过及答对；已用提示次数和独立错题记录仍按现有生命周期保留 | 步骤 2 |
| 只剩最后一题（包括单题练习） | 删除后结束本轮，汇总的总题数为剩余题数，不尝试读取不存在的下一题 | 步骤 2 |
| 相同词义配对存在于另一单词本或收藏/错题集合 | 仅删除指定单词本词条及该来源相关覆盖，其他来源仍可练 | 步骤 1 |
| 确认期间题目切换，或错题/复习写入尚未完成 | 不将旧操作施加到新题；阻止写入竞争和重复提交 | 步骤 2、3 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 数据范围、来源覆盖及独立集合 | Rust repository 测试 | 目标行被删；非目标词条、收藏/错题和跨来源记忆保留 |
| 计数、进度、末题/单题 | reducer 单测 | 已删除题对各项统计贡献为零，总题数随之变化 |
| 确认、取消、来源可见性、错误/竞争 | React Testing Library / 服务 mock | 仅确认调用删除；失败不推进；其他来源不见按钮 |
| 端到端及静态检查 | `bun run test`、`bun run check`、`cargo test --manifest-path src-tauri/Cargo.toml wordbooks`、桌面手动验证 | 原练习、收藏和错题行为无回归，删除后重练不再抽到该词 |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src-tauri/src/wordbooks/repository.rs`、`mod.rs`、`src-tauri/src/lib.rs` | Modify | 安全删除与命令注册、持久层测试 |
| `src/data/wordbooks.ts` | Modify | 类型化前端删除接口 |
| `src/features/practice/practiceReducer.ts`、`PracticeQuestion.tsx`、`src/App.tsx` | Modify | 题目移除、确认 UI 与异步协调 |
| `src/App.test.tsx`、`src/features/practice/` 下相关测试 | Modify | 跨层行为和边界验证 |

## Risks and open questions

- **风险：** 调度时即创建来源覆盖记录与 pending review token；删除不能把这次未完成的题目当作答题结果，也不能误删同配对其他来源的记忆。实施时检查调度现有清理路径并针对删除场景测试。
- **风险：** 已提交错误会写入独立错题本；“不计入错误”指本轮汇总统计，不能通过删除源词条撤销独立错题历史（见 ADR 0003）。
- **待核对：** Astryx 实际按钮图标与确认弹窗 props、App 测试服务 mock 的构造方式；按组件文档和现有测试确认，不新增产品语义。
