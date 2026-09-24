# 答错后展示答案差异并继续练习

- **Input:** 用户确认的对话需求；`.grimoire/CONTEXT.md` 的 practice-round、mistake-entry、review-outcome；ADR 0003、0004。既有 `.grimoire/spec/0002-coverage-and-forgetting-curve-review-scheduling.md` 描述了旧的同题反复答错场景，本计划以本次确认的新行为为准，不把旧场景继续作为验收条件。
- **Date:** 2026-09-24
- **Status:** Completed

## Summary

提交错误答案后立即展示用户原答案与正确答案的字符级差异，英文拼写和中文释义均适用；无需改对，在查看对照后手动进入下一题或结果页。本题结果为“答错完成”，本轮错误数和持久错题次数各增加一次，不增加答对或跳过数。保持既有提示、主动跳过、计时、删除及复习记录的边界，尤其不能把已揭示的答错题误当作跳过。此为一个可原子交付的练习交互变更，无需新增独立 spec 或拆票。

## Design impact

### 练习状态与完成动作

- **Action:** Modify
- **Location:** `src/features/practice/practiceReducer.ts`
- **Responsibility:** 区分答错揭晓与主动跳过/第三级提示揭晓；答错仅计一次错误并保留提交时的输入供对照，揭晓后拒绝重复提交、输入及提示；继续时不计答对或跳过，切题时清理揭晓状态。删除当前答错题时回滚本轮该题错误，但不回滚独立持久错题历史；仅跳过揭晓才回滚跳过数。
- **Relationships:** `App.tsx` 的错题写入、复习完成及删除路径；`PracticeQuestion.tsx` 的展示。
- **Rationale:** 当前 `isAnswerRevealed` 只表示跳过，且删除路径按这个布尔值扣减跳过数；在同一状态机中明确揭晓原因，避免新增平行状态来源。保留已有题目级错误/提示统计以兼容复习契约。

### 错误提交与持久化协调

- **Action:** Modify
- **Location:** `src/App.tsx`
- **Responsibility:** 错误提交时按提交瞬间的题目、原输入和唯一提交 ID 锁定错误结果并立即展示对照；沿用 `recordMistakeOnce` 幂等写入与失败重试。错题写入未确认时不得继续/切题或重复产生新提交；写入完成后，手动继续以 `errorCount: 1`、`skipped: false` 完成一次复习结果。复习写失败仍留在对照页重试，避免误推进或重复计数。
- **Relationships:** reducer、`wordbookService.recordMistakeOnce`、`completeReview` 及现有 busy/error 提示。
- **Rationale:** 既有服务的 `skipped` 布尔值与本题 `errorCount` 已可区分答错和跳过，无需仅为 UI 结果新增后端枚举或变更数据库；显示答案不应等待异步写入，但继续动作仍遵守现有持久化安全约束。

### 字符差异与展示

- **Action:** Create / Modify
- **Location:** 建议在 `src/domain/` 增加纯差异计算函数及测试；修改 `src/features/practice/PracticeQuestion.tsx`
- **Responsibility:** 对提交时保留的原答案和 `getExpectedAnswer` 产生有序字符差异，对两侧分别标示缺失/多余/替换对应的不同片段；不可仅按相同索引比较，插入或删除字符不应将后续相同后缀全部标错。展示文字标签（“你的答案”“正确答案”）及可感知的差异说明，不仅依靠颜色；空答案也可对照。跳过仍显示原有英文及中文答案，不伪造用户作答。
- **Relationships:** `src/domain/practice.ts` 的判题与期望答案；Astryx 的 Text/Stack 等组件和 token。
- **Rationale:** 纯函数使字符级对齐可单测，视图只负责语义渲染；算法具体实现可用最长公共子序列或等效的插删对齐，选择是可替换的实现细节，不更改既有正确性判定。写 UI 前按 `bunx astryx build`、`docs layout`、`template` 与所用 `component` 文档检查组件用法；不手写裸布局或硬编码样式。

## Implementation steps

### 1. 明确 reducer 的答错揭晓状态

- **Files or discovery point:** `src/features/practice/practiceReducer.ts`、`practiceReducer.test.ts`
- **Change:** 将揭晓原因与是否揭晓对应起来；错误提交一次后保存原答案、错误数加一并进入答错揭晓状态；跳过和第三级提示进入跳过揭晓状态。继续动作按原因结算，删除动作仅在跳过时回滚跳过计数、在答错时回滚该题本轮错误。新题重置原因和答案；旧的反复错误后改对测试改为答错后不能再次提交、也不能改成答对。
- **Outcome:** 首次错误结束作答但不自动切题，末题也先留在对照页。
- **Verification:** reducer 测试覆盖两题、单题末题、揭晓后重复动作、提示、删除及三项计数。

### 2. 对齐提交时的答案差异

- **Files or discovery point:** `src/domain/practice.ts` 或新纯函数文件及对应测试
- **Change:** 在不改变 `isExactAnswer` 的前提下实现字符序列对齐：对用户实际输入和正确答案逐字符形成一致/删除/插入片段，再按原顺序分别生成可渲染片段；保留原输入供展示，不将判题用的 trim/英文大小写折叠误当成原输入。相同后缀对齐、完全不同、空串、中英文 Unicode 字符及混合大小写/空白与其他错误共存的输入均进入测试。
- **Outcome:** 输入和标准答案均能准确标出差异，字符插删不会污染后续一致片段。
- **Verification:** 纯函数表驱动测试；由实际 `isExactAnswer` 判错的输入触发展示。

### 3. 协调持久化与手动继续

- **Files or discovery point:** `src/App.tsx`、`src/App.test.tsx`
- **Change:** 错误提交时立即触发答错揭晓，并按提交时题目创建一次幂等错题写入；原有失败/超时重试复用同一 ID，不允许再次提交叠加计数。已揭晓答错题的继续操作走复习完成路径，传入本题错误数和提示数、`skipped: false`；跳过继续仍传 `skipped: true`。保持复习完成成功后才真正推进题目/汇总，失败可在对照页重试。检查删除与待写入限制，避免过期异步响应推进其他题。
- **Outcome:** 错误答案立刻可见对照；用户确认后进入下一题，写入失败仅阻止不安全的推进而不隐藏对照或误报已保存。
- **Verification:** App 交互测试覆盖错误计数、错题列表、复习参数、写入待定/失败/超时重试、复习完成失败与再次继续、快速双击及末题。

### 4. 呈现差异并复核回归

- **Files or discovery point:** `src/features/practice/PracticeQuestion.tsx`、`src/App.test.tsx`
- **Change:** 答错揭晓时呈现带文字标签和差异语义的双答案对照、已有下一题/查看结果按钮；跳过揭晓沿用原答案展示。检查按钮忙碌/禁用、读屏可理解的状态消息，不使用颜色作为唯一错误信息。调整依赖重试作答的旧测试，保留直接跳过、三级提示、正确提交和删除的断言。
- **Outcome:** 两种答题方向均能看到明确对照，答错不再卡在“请检查后重试”。
- **Verification:** React 交互测试与 `bun run test`、`bun run check`；手动检查中英短词与长释义的可读性。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 错误答案为空、只有空白，或错误词同时含大小写/边界空白差异 | 保留原输入、展示正确答案和可理解的差异；不改变既有 trim 与英文不区分大小写的判题规则 | 步骤 2、4 |
| 词中插入/漏掉字符，或中文多字释义 | 对齐共同后缀，仅标出真正差异 | 步骤 2 |
| 同一题 Enter/点击连发，或揭晓后再次触发提交 | 本轮与错题只加一次，不能答错后改成答对 | 步骤 1、3 |
| 错题写入失败/响应丢失，用户点击下一题 | 继续受阻并可用同一提交 ID 重试；对照仍可见；不误记到下一题 | 步骤 3 |
| 复习写入失败或请求超时 | 不推进、不重复计错，保留原结果可重试相同 reviewId | 步骤 3 |
| 答错揭晓后删除单词本当前题 | 本轮错误与该题数回滚，不扣跳过数；已保存的独立错题记录保留 | 步骤 1、3 |
| 最后一题答错或三级提示揭晓 | 都须手动查看结果；前者错误一次、不算跳过，后者算跳过、不计错 | 步骤 1、4 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 差异算法和原输入 | 领域单测 | 插入、删除、替换、共同后缀、中英文及空白均有准确片段 |
| 答错/跳过/答对及删除统计 | reducer 单测 | 错误一次、答对/跳过零；删除只回滚对应本轮计数 |
| 持久化与调度 | App 集成测试 | 一次错题增量；答错 review `errorCount: 1, skipped: false`；跳过 `skipped: true`；失败重试幂等 |
| 展示和交互 | React 测试、手动检查 | 原/正确答案均可读、差异不只靠颜色、两方向可手动继续，末题可看结果 |
| 既有流程 | `bun run test`、`bun run check` | 正确提交、提示、跳过、计时与删除回归通过 |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src/features/practice/practiceReducer.ts` / `.test.ts` | Modify | 结果状态、计数、继续与删除 |
| `src/App.tsx` / `.test.tsx` | Modify | 即时揭晓、幂等写入、复习完成和交互回归 |
| `src/features/practice/PracticeQuestion.tsx` | Modify | 错误答案对照与操作 |
| `src/domain/practice.ts` 或新差异模块及其测试 | Modify / Create | 纯字符差异算法 |

## Risks and open questions

- **Risk:** 既有复习 spec 和旧测试假设同题可反复错误后改对；按本轮已确认行为更新测试，旧 spec 仅作历史背景，不能要求新流程重现已废止的重试交互。若需要正式维护旧 spec，由后续单独授权处理。
- **Risk:** 当前 `isAnswerRevealed` 同时控制显示、提交以及删除时扣除跳过次数；不能仅把答错设置为 true 而不改删除和完成路径。
- **Risk:** 差异算法对于很长文本的计算成本与换行显示；用现实的释义长度测试，必要时选空间受控的实现，但不改变字符级对照的语义。
