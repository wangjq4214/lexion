# 提前退出练习并结算

- **Input:** 用户对话确认的退出结算规则；`.grimoire/CONTEXT.md` 的 `practice-round`、`review-outcome`；`.grimoire/adr/0004-schedule-practice-with-coverage-and-due-review.md`
- **Date:** 2026-09-24
- **Status:** Completed
- **Endpoint:** 实施计划，不执行实现

## Summary

练习中提供“退出并结算”，直接进入现有结果页。仅已完成题进入题数和统计；未提交的当前题及其已用提示、剩余题均不计入成绩或复习记录。若当前题已答错或跳过且答案正在展示，先保存本题复习结果，成功后结算；写入失败则留在练习页重试。结果页展示实际完成题数和截至退出的用时。不增设规格、票据或持久化接口。

## Design impact

### 练习状态与汇总

- **Action:** Modify
- **Location:** `src/features/practice/practiceReducer.ts`
- **Responsibility:** 增加提前结算的状态转移；区分正在作答的未完成题与已揭晓、待保存的完成题。汇总的 `totalCount` 为实际完成题数，未完成题的提示数不进入汇总；已揭晓题在其复习结果保存成功后计入。
- **Relationships:** `App.tsx` 管理复习写入和用时，`PracticeSummary.tsx` 消费汇总状态。
- **Rationale:** 计数和阶段切换仍由现有 reducer 统一维护，不在页面另存一套汇总数据；保留常规完成和删除题目的现有路径。

### 退出与持久化协调

- **Action:** Modify
- **Location:** `src/App.tsx`
- **Responsibility:** 退出时取得截至退出的用时；未揭晓题不调用 `completeReview`；已揭晓题先按当前题的错误数、提示数和跳过原因调用既有 `completeReview`，写入成功才触发结算。沿用现有待保存错题、正在完成复习与正在删除时的操作锁和错误提示。
- **Relationships:** `wordbookService.completeReview`、`pendingMistakeRef`、`completingReview`、`deletingRef` 和 reducer；不改变后端复习调度契约。
- **Rationale:** 复用 `completeQuestion` 的同题写入和失败重试边界，避免退出时绕过持久化、重复记账或在后台写入中途丢失题目身份。

### 练习页及结果页

- **Action:** Modify
- **Location:** `src/features/practice/PracticeQuestion.tsx`、必要时 `src/features/practice/PracticeSummary.tsx`
- **Responsibility:** 在作答与答案揭晓两种状态均提供清晰的退出结算入口，并在保存中禁用重复操作；结果页呈现实际完成题数，避免把提前退出显示为全部题目已做完。
- **Relationships:** 通过组件回调交给 `App.tsx`；使用现有 Astryx 布局和按钮组件。实施 UI 前按 `AGENTS.md` 执行 `bunx astryx build`、`docs layout`、模板和所用组件的查询。
- **Rationale:** 用户可在练习任一状态退出，复用已有结果视图而不建平行页面；展示文案反映实际进度。

## Implementation steps

### 1. 集中提前结算的计数规则

- **Files or discovery point:** `src/features/practice/practiceReducer.ts`、`src/features/practice/practiceReducer.test.ts`
- **Change:** 添加带用时的提前结算动作：当前题未揭晓时，完成题数为 `questionIndex`，汇总提示数扣除 `questionHintCount`；当前题已揭晓时，仅在上层确认其复习写入成功后结算，完成题数为 `questionIndex + 1`，保留本题错误/跳过/提示。其余未做题不计入。维持现有常规完成和删除动作的结果。
- **Outcome:** 首题直接退出可得到零题结果；完成若干题后退出只统计已完成题。
- **Verification:** reducer 测试覆盖零完成、先答对再退出、未完成题使用提示、已揭晓的答错/跳过及常规末题完成。

### 2. 串联退出动作与现有写入保护

- **Files or discovery point:** `src/App.tsx` 中 `completeQuestion`、`submitAnswer`、计时和 pending 写入保护；`src/App.test.tsx`
- **Change:** 添加退出回调：若当前题未揭晓，直接以退出时的时间结算而不写复习；若已揭晓，则沿用当前题 `reviewId`、`questionErrorCount`、`questionHintCount` 和揭晓原因写复习，只有成功后结算。错题保存未确认、复习写入或删除正在进行时不能退出；失败仍留当前练习题，保留错误提示供重试。避免与现有继续按钮并发触发两次完成。
- **Outcome:** 结算不会为未完成题创建复习结果，也不会丢掉已完成但仍在展示答案的题目。
- **Verification:** App 交互测试检查写入参数/次数、零写入退出、失败后重试、错题保存等待以及写入/删除竞争。

### 3. 提供操作入口和准确结果文案

- **Files or discovery point:** `src/features/practice/PracticeQuestion.tsx`、`src/features/practice/PracticeSummary.tsx`、`src/App.tsx`、`src/App.test.tsx`
- **Change:** 在题目作答和揭晓界面显示“退出并结算”，绑定退出回调并遵循忙碌/禁用状态；结果页明确显示已完成题数和用时，正常做完全部题的结果保持原有统计含义。遵循 Astryx 组件查询与 token 约束，不另写裸布局。
- **Outcome:** 用户可在任何可操作的练习题上提前结算并识别实际成绩。
- **Verification:** 测试按钮在两种状态可见且状态受控、结果为 0 题或部分题时文案不误导；手动检查练习与结果页。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 第一题尚未提交，包括已输入答案或使用一、二级提示 | 直接结算 0 题，错误/正确/跳过/提示均为 0；不调用 `completeReview` | 步骤 1、2 |
| 已完成若干题，当前题尚未提交 | 只保留先前完成题的统计；当前题提示和剩余题不计入 | 步骤 1、2 |
| 当前题已答错、主动跳过或第三级提示揭晓 | 先保存本题结果，再把本题计入汇总；答错不误算作跳过 | 步骤 1、2 |
| 错题保存、复习写入或删除正在进行 | 退出不可与写入竞争；失败时留在练习页，可重试而不重复结算 | 步骤 2、3 |
| 揭晓题复习保存失败或超时 | 不进入结果页、不丢失揭晓状态；按现有失败提示重试 | 步骤 2 |
| 单题练习或已在末题揭晓时退出 | 总题数按实际完成题数显示，不读取不存在的后续题 | 步骤 1、3 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 提前结算计数与正常完成互不干扰 | reducer 单测 | `totalCount`、四项统计及用时正确；原末题逻辑通过 |
| 未完成题不留复习记录，已揭晓题先保存 | React Testing Library + mock `WordbookService` | `completeReview` 调用参数/次数；写入完成前不出现结果 |
| 异步错题/复习失败、重复触发及重试 | App 集成测试 | 未结算时题目仍在；重试后只结算一次；未绕过待保存错题 |
| 可用入口与结果呈现 | 组件交互测试和手动检查 | 作答/揭晓均可退出；部分题/零题结果及实际用时可读 |
| 项目回归 | `bun run test`、`bun run check`、`bun run build` | 现有练习和静态检查通过 |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src/features/practice/practiceReducer.ts`、`practiceReducer.test.ts` | Modify | 结算动作及计数测试 |
| `src/App.tsx`、`src/App.test.tsx` | Modify | 写入、计时和退出操作协调及集成测试 |
| `src/features/practice/PracticeQuestion.tsx` | Modify | 退出入口 |
| `src/features/practice/PracticeSummary.tsx` | Modify if needed | 结果文案明确实际完成题数 |

## Risks and open questions

- **Risk:** 当前错题记录与复习结果在两个异步步骤写入；退出不能绕过 `pendingMistakeRef` 或重复提交 `completeReview`。沿用既有锁和重试路径，并覆盖请求失败/超时。
- **Risk:** 已揭晓题在 reducer 中已有本轮计数，但复习记录尚未落库；只有复习写入成功后才能结算，以免本轮结果和持久复习进度不一致。
