# 删除单词本前确认

- **Input:** 对话需求（添加单词本删除能力，删除时弹出提示）；refine 阶段将“提示”明确为删除前确认。
- **Date:** 2026-09-24
- **Status:** In Progress

## Summary

在练习设置页为当前选中的整个单词本提供删除入口。先展示包含单词本名称及删除影响的确认弹窗；取消不修改数据，确认后删除该单词本及其词条，并刷新列表和有效选中项。删除最后一本后恢复现有仅显示导入入口的空状态。独立的收藏夹与错题本不随单词本删除。此计划只规划实施，不执行代码修改。

依据：`.grimoire/CONTEXT.md` 的 wordbook、active-wordbook、favorites-collection、mistake-collection；`.grimoire/adr/0001-store-imported-wordbooks-in-sqlite.md`、`0002-preserve-favorites-independent-of-wordbooks.md`、`0003-preserve-mistake-records-independent-of-wordbooks.md`；`.grimoire/spec/0001-excel-wordbook-import.md` 第 7 项的空状态。该旧规格将单词本独立删除列为当时范围外；本次对话新增的删除目标覆盖该范围外声明，不修改其他旧要求。

## Design impact

### SQLite 单词本存储

- **Action:** Modify
- **Location:** `src-tauri/src/wordbooks/repository.rs`，`WordbookRepository`
- **Responsibility:** 按 ID 删除指定单词本与关联词条；在同一事务中清理该单词本的 `review_coverage` 行。
- **Relationships:** 现有 `entries.wordbook_id` 外键级联；独立的 `favorites`、`mistakes` 和按词对共享的 `review_memory` 不随单词本删除；沿用 `replace` 的覆盖记录清理方式。
- **Rationale:** 将数据完整性保持在现有仓储边界，不另设删除服务；避免按名称删除误伤同名重导后的新记录。

### 命令与前端服务

- **Action:** Modify
- **Location:** `src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`
- **Responsibility:** 暴露按 wordbook ID 删除的 Tauri 命令与类型化 `WordbookService` 方法；沿用 `CommandError` 到 `WordbookError` 的现有映射。
- **Relationships:** 仓储删除返回是否实际删除，前端据此避免将不存在的 ID 当作成功删除。
- **Rationale:** 复用现有 list/import/delete-entry 命令链，不混淆单词本删除和词条删除。

### 练习设置及选择状态

- **Action:** Modify
- **Location:** `src/features/practice/PracticeSetup.tsx`、`src/routes/index.tsx`，复用 `src/features/wordbooks/WordbookProvider.tsx` 的 `useRefreshWordbooks`
- **Responsibility:** 在当前单词本选中处提供删除动作及 `AlertDialog`；只在用户确认后请求删除；完成后刷新列表，并由现有刷新逻辑选择有效单词本或清空选择。
- **Relationships:** `activeWordbookAtom`、`wordbooksAtom` 和现有首页空状态；参考 `WordbookImportFlow.tsx` 的确认交互以及 `routes/practice.tsx` 的删除弹窗。
- **Rationale:** 删除入口与所选对象相邻；沿用页面已有状态所有权，避免新增全局删除状态或手写弹窗。删除过程中禁用重复提交，失败时保留列表并显示可读错误。

## Implementation steps

### 1. 完成持久化删除边界

- **Files:** `src-tauri/src/wordbooks/repository.rs`。
- **Change:** 新增按正整数 ID 删除方法；在事务中清理 `review_coverage` 中该来源/ID 的记录，再删除 `wordbooks` 行，让外键级联删除其 `entries`。不存在的 ID 返回未删除；不删除独立的收藏、错题以及按词对共享的复习记忆。
- **Outcome:** 指定单词本及其覆盖记录消失，其他单词本和独立集合不变。
- **Verification:** Rust 仓储测试覆盖目标/非目标、重复或无效 ID、收藏/错题保留、覆盖记录清理及数据库重开后的持久性。

### 2. 接通 Tauri 命令与前端服务

- **Files:** `src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`。
- **Change:** 注册 `delete_wordbook` 命令，参数 `wordbook_id`，返回删除结果；为 `WordbookService` 和 Tauri 适配器新增相同语义方法。检查现有服务 mock，并补齐新方法。
- **Outcome:** UI 能通过稳定的服务接口删除特定 ID，数据库错误沿现有错误通道返回。
- **Verification:** `src/data/wordbooks.test.ts` 的调用/错误映射检查；Rust 测试验证命令接线或仓储结果。

### 3. 在设置页加入确认与状态刷新

- **Files:** `src/features/practice/PracticeSetup.tsx`、`src/routes/index.tsx`；必要时调整 `src/features/wordbooks/WordbookProvider.tsx`。
- **Change:** 以当前选中 ID 和列表摘要确定目标，在选中处放删除按钮；点击打开 `AlertDialog` 并显示目标名称及会删除其词条的说明。取消关闭弹窗且不调用服务；确认时禁用重复操作，成功后调用现有 `refreshWordbooks()`，使非末本删除选中有效单词本、末本删除进入导入空状态。失败时保留可重试交互并展示错误；勿在请求失败时乐观移除列表。
- **Outcome:** 仅确认才发生删除，界面选中状态与持久化列表一致。
- **Verification:** `src/App.test.tsx` 或就近 `PracticeSetup` 测试覆盖取消、确认、删除选中本后改选剩余本、最后一本、失败、重复点击；核对 mock 接口。

### 4. 回归检查

- **Files:** 上述文件和对应测试。
- **Change:** 执行 `bun run test`、`bun run check`、`cargo test --manifest-path src-tauri/Cargo.toml`；在桌面环境手动检查弹窗、删除后刷新及重启后的结果。
- **Outcome:** 导入、选择、练习、收藏/错题及复习调度未因新删除入口退化。
- **Verification:** 命令通过；手动观察删除前取消、确认后重启、删除最后一本三个流程。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 取消或关闭确认框 | 不调用删除命令，数据与选择保持原样 | 步骤 3，UI 测试 |
| 删除当前唯一一本 | 列表为空、选中项为空、显示现有导入空状态 | 步骤 3，UI 测试 |
| 删除当前选中本而仍有其他本 | 不保留悬空选中 ID；刷新逻辑选择现存单词本 | 步骤 3，UI 测试 |
| 确认时目标 ID 已不存在或请求失败 | 不将未删除当作成功；保持可恢复状态并提示错误 | 步骤 1–3，仓储/UI 测试 |
| 连续确认或删除过程中另起操作 | 避免重复提交和目标漂移，不误删另一本 | 步骤 3，UI 测试 |
| 收藏/错题中仍有被删本的词对 | 独立集合仍可浏览和练习；其他来源的共享记忆不被清除 | 步骤 1、4，仓储回归 |

## Risks and verification

- **数据误删：** 删除必须基于确认时显示的 ID，而非名称或刷新后当前位置；用两个单词本及重导入场景验证。
- **孤立覆盖记录：** `review_coverage` 不受词条外键级联保护；事务内显式清理目标来源/ID，并验证其他来源不变。
- **复习记忆作用域：** `review_memory` 与词对及方向相关，可能还被收藏/错题或其他本使用；不能按被删本词条清空。
- **旧规格边界：** `.grimoire/spec/0001-excel-wordbook-import.md` 的“删除范围外”反映旧版交付范围；实现以本次确认的新增需求为准，不借此扩大到批量删除或词条管理。
