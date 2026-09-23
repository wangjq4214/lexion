# 三级拼写提示

- **Input:** 用户对话；`.grimoire/CONTEXT.md` 的 `practice-round` 定义；现有 `src/App.tsx` 与 `src/domain/practice.ts`
- **Date:** 2026-09-23
- **Status:** Proposed
- **Endpoint:** 实施计划；本次 refine 不执行实现

## Summary

将看中文拼英文（含混合模式中的该方向）的单次提示改为每题依次可用的三级提示：约 1/3 字母、约 2/3 字母、完整单词。第二级保留第一级已展示的字母；每次升级计一次提示。第三级按跳过处理：完整展示英文与中文，计入跳过而非答对，学习者看过后手动继续。看英文拼中文方向保持现有不提供拼写提示的行为。

## Design impact

### 提示生成

- **Action:** Modify
- **Location:** `src/domain/practice.ts` 的 `createEnglishHint` 或其替代函数
- **Responsibility:** 为同一单词生成两级递增、位置稳定的掩码，保留标点、空格等非字母；完整答案交给现有揭示流程。
- **Relationships:** 使用当前 `RandomSource` 注入，供 `src/App.tsx` 的当前题状态使用。
- **Rationale:** 提示的字符选择放在既有可单测的领域逻辑中；不让两次独立随机生成造成已揭示字母重新隐藏。

### 练习状态与界面

- **Action:** Modify
- **Location:** `src/App.tsx` 的 `PracticeState`、`AppAction`、`appReducer` 与练习渲染
- **Responsibility:** 跟踪当前题提示级别和稳定的提示内容；前两级继续允许提交答案，第三级复用答案揭示及继续流程，并保持提示、跳过、答对计数互不混淆。
- **Relationships:** 复用现有 `skip-question` / `continue-after-skip` 的状态转移与计时；每题切换或新一轮开始时重置提示级别。
- **Rationale:** 题目生命周期及计数已有统一 reducer，不额外引入独立状态容器或持久化层。提示按钮按下一级状态更新，而不是首次显示后禁用。

## Implementation steps

### 1. 生成同题递进提示

- **Files or discovery point:** `src/domain/practice.ts`、`src/domain/practice.test.ts`。
- **Change:** 调整现有提示生成接口或增加可一次生成两级提示的函数：按英语字母位置选择约三分之一、三分之二可见位置，第二级是第一级的超集；未显示字母仍用下划线占位。对短词保持前两级为非完整答案，在字母数不足以形成两个不同的非完整级别时允许可见数量相同。第三级不由掩码生成，直接走完整答案揭示。
- **Outcome:** 连续两级不会重新隐藏已显示的字母，也不会在第三级前揭示完整单词。
- **Verification:** 单测覆盖位置单调递增、短词、标点与注入随机源。

### 2. 扩展题目状态流转及操作入口

- **Files or discovery point:** `src/App.tsx` 中 `appReducer`、练习状态和“显示提示”按钮。
- **Change:** 第一次和第二次操作分别展示对应级别，每次提示计数加一；第三次提示计数加一，同时转入现有跳过揭示状态、跳过计数加一、正确数不变。揭示状态仍显示完整英文及中文并等待手动继续。直接跳过不额外增加提示次数；从前两级直接跳过保留此前提示次数。进入下一题时清理提示级别和掩码，其他方向不增加提示入口。
- **Outcome:** 当前题最多消费三级提示，第三级和直接跳过共享后续交互及计时边界。
- **Verification:** `src/App.test.tsx` 中点击各级、提交答案、直接跳过及末题揭示的交互断言。

### 3. 覆盖统计和回归

- **Files or discovery point:** `src/App.test.tsx`、`src/domain/practice.test.ts`。
- **Change:** 测试前两级后答对、三级后查看结果、前两级后直接跳过、换题及新一轮重置、混合模式方向和揭示期用时；保留原有答题、跳过与汇总测试并按新的按钮生命周期更新断言。
- **Outcome:** 汇总中的提示次数是实际使用级数，跳过次数包含第三级提示导致的跳过，且同一题不会重复计数。
- **Verification:** `bun run test`、`bun run typecheck`、`bun run check`。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 单字母或极短英文词 | 前两级不提前给出完整答案；级别可以不同而展示内容相同 | 步骤 1、3 |
| 含标点、空格的词条 | 非字母保持原位，仅英文字符参与可见比例和掩码 | 步骤 1、3 |
| 已用一或两级提示后直接跳过 | 保留已有提示次数，跳过只计一次；不额外计第三级 | 步骤 2、3 |
| 第三级提示发生在最后一题 | 先展示答案；手动查看结果时结算用时、跳过与提示次数 | 步骤 2、3 |
| 混合模式切换到中文作答题、进入下一轮 | 不沿用上一题提示，不在中文作答题显示拼写提示 | 步骤 2、3 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 字母比例与逐级保留 | 领域单元测试 | 前两级约 1/3 与 2/3，已显示位置不倒退，第三次才完整揭示 |
| 第三级等同跳过并正确统计 | React Testing Library 交互测试 | 英文和中文可见、不能提交答案、手动继续后汇总正确数不增加、提示数加三、跳过数加一 |
| 现有行为不退化 | 原有测试及静态检查 | 正确作答、独立跳过、用时及中文作答模式保持通过；`bun run test && bun run typecheck && bun run check` |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src/domain/practice.ts` | Modify | 可复用的递进提示生成 |
| `src/domain/practice.test.ts` | Modify | 字母掩码与边界测试 |
| `src/App.tsx` | Modify | 逐级提示、第三次跳过和界面交互 |
| `src/App.test.tsx` | Modify | 流程、统计及回归测试 |

## Risks and open questions

- **风险：** 短词不能严格同时满足“1/3、2/3、前两级不同且不完整”；按“约”处理，允许内容相同但级数与计数照常递进，确保第三次才完整展示。
- **风险：** 直接以每次点击重新随机生成掩码会使已显示字母消失；应固定同题字符位置并测试包含关系。
- **风险：** 第三级若仅显示完整英文提示而未进入既有跳过状态，可能仍允许提交并被算作答对；应复用揭示、继续和结算路径。
