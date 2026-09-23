# 跳过练习词并统计次数

- **Input:** 用户对话；`.grimoire/CONTEXT.md` 的 `practice-round` 定义
- **Date:** 2026-09-23
- **Status:** Proposed
- **Endpoint:** 实施计划；不在本次 refine 中执行实现

## Summary

练习中允许跳过当前词条，立即显示完整英文单词及中文释义，待学习者看过后点击进入下一题；跳过不算答对。练习结束时仅展示跳过次数，不列跳过的词条；已有错误、提示和用时统计继续保留。显示后手动继续是讨论中明确告知的可调整交互假设，避免答案一闪而过。

## Design impact

### 练习状态与流转

- **Action:** Modify
- **Location:** `src/App.tsx` 的 `PracticeState`、`SummaryState`、`AppAction` 和 `appReducer`
- **Responsibility:** 区分作答中与跳过后答案展示状态；跳过计数一次且不增加答对数；答对或跳过均完成当前题，最后一题也能结束本轮。
- **Relationships:** 复用现有 `Question.entry` 的 `english`、`chinese`，保留 `now`/`roundStartedAt` 计时边界。
- **Rationale:** 当前题目的生命周期和汇总计数已集中在 reducer；无需另建服务、数据库字段或修改题目生成逻辑。将跳过后的可见阶段作为状态处理，避免重复触发跳过或在未看见答案时自动前进。

### 练习与结果界面

- **Action:** Modify
- **Location:** `src/App.tsx` 的 practice/summary 渲染分支
- **Responsibility:** 练习题中提供跳过操作，揭示英文及中文并提供继续入口；揭示状态下不再把输入提交当作作答；总结只增加跳过次数。
- **Relationships:** 沿用现有 Astryx 布局、文本和按钮组件以及进度条；不新增独立的词条清单。
- **Rationale:** 单屏小功能直接放在现有交互位置，维持三个练习模式一致。

## Implementation steps

### 1. 完成跳过与结束状态流转

- **Files or discovery point:** `src/App.tsx`，`appReducer`。
- **Change:** 为每轮加入跳过计数和当前题答案已揭示的状态。跳过当前未揭示题目时计数一次并展示答案；继续时重置当前题的答案、错误、提示及揭示状态，进入下一题。普通正确作答与跳过共用一致的题目完成/末题汇总边界；正确数按实际正确作答数，不再固定等于题目总数。末题跳过后仍先展示答案，再由继续动作生成汇总。用时在真正结束本轮时截取，并在揭示期间继续累计。
- **Outcome:** 一题可正确完成或跳过；混合完成与全跳过都能结束，计数一致。
- **Verification:** reducer 相关行为通过 `src/App.test.tsx` 的交互断言覆盖。

### 2. 呈现答案、继续操作和统计

- **Files or discovery point:** `src/App.tsx` 的练习与汇总分支。
- **Change:** 在未揭示状态提供“跳过”按钮；揭示时显示当前词条英文及中文，提供“下一题”或末题的完成操作；隐藏或禁用会再次提交当前题的输入/提示操作。总结增加“跳过次数”，保留“答对题数 / 总题数”、错误次数、提示次数和总用时，不列词条。
- **Outcome:** 在看中文、看英文、混合三种模式下均能看到完整词条并主动继续。
- **Verification:** UI 测试检查答案可见、继续后题目变化、总结仅展示次数而非跳过词列表。

### 3. 补齐回归验证

- **Files or discovery point:** `src/App.test.tsx`。
- **Change:** 覆盖单题直接跳过、先答错/用提示再跳过、两题中一题答对一题跳过、末题跳过、全部跳过、揭示后重复操作不可重复计数，以及揭示期间用时和重新开始计数归零。复跑原有正确作答路径。
- **Outcome:** 新行为与原有答题、计时行为均受测试保护。
- **Verification:** `bun run test`、`bun run typecheck`；若影响代码风格再运行 `bun run check`。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 仅一题且直接跳过 | 展示该题完整词条，继续后总结答对 0/1、跳过 1 | 步骤 1、3 |
| 跳过前已输入错误答案或已使用提示 | 已产生的错误/提示次数保留，跳过只增加一次 | 步骤 1、3 |
| 已揭示答案时重复点击跳过或提交 | 不再改变本题计数或绕过答案展示 | 步骤 1、2、3 |
| 最后一题揭示答案期间 | 计时继续；进入总结时锁定最终用时 | 步骤 1、3 |
| 开始下一轮 | 跳过、答对及其他本轮计数按新一轮重置 | 步骤 1、3 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 跳过显示完整释义并手动继续 | React Testing Library 交互测试，覆盖三个模式 | 揭示英文与中文后仍停留当前题，继续后切换题目 |
| 答对数和跳过数准确且互不重叠 | 单题及混合结果 UI 测试 | 总结正确数为实际答对数，跳过次数正确，不显示跳过词列表 |
| 计时边界及提示/错误统计未回退 | 受控 `now` 的计时测试 + 现有回归测试 | 跳过揭示期间计时累计，结束时冻结，下一轮归零 |
| 类型和构建契约 | `bun run test`、`bun run typecheck` | 命令通过 |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src/App.tsx` | Modify | 跳过流转、揭示交互及汇总计数 |
| `src/App.test.tsx` | Modify | 行为及回归验证 |

## Risks and open questions

- **风险：** 末题跳过若在揭示时立即汇总，会看不到答案；将揭示和继续拆为两个状态转换并测试。
- **风险：** 当前提交路径把正确数固定为总题数；修改时需保持总题数不变、只更新实际正确数。
- **可调整交互假设：** 答案揭示后通过手动继续进入下一题；若产品需要自动延时跳转，须先重新确认交互和计时边界。
