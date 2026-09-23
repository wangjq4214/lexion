# 错题本：浏览、累计错误次数与专项练习

- **Input:** 用户需求及确认（每次错误提交计数、跳过不计错、替换单词本后保留）；`.grimoire/CONTEXT.md`、`.grimoire/adr/0003-preserve-mistake-records-independent-of-wordbooks.md`；现有练习、收藏及导入代码
- **Date:** 2026-09-23
- **Status:** Proposed
- **Endpoint:** 实施计划；不执行实现

## Summary

为答错的英文词与中文释义组合累计错误次数，在错题本中浏览词条及对应次数，并将错题本作为练习来源。一次错误提交加一次（同一道题反复提交错误亦然）；单纯跳过及第三层提示揭示答案不额外计错。错题记录持久保存，不因源单词本同名替换而消失；从错题本练习沿用已有模式、数量选择、抽样、提示、跳过、计时和汇总。讨论中已告知的可调整默认规则：同一英文和中文释义组合视为同一错题；不同释义为不同条目。当前没有提出手动移除、重置次数或自动清除答对词，不把这些功能加入本计划。

## Design impact

### 独立的错题记录

- **Action:** Modify / Create
- **Location:** `src-tauri/src/wordbooks/repository.rs`（或相邻持久化模块）、`src-tauri/src/wordbooks/model.rs`
- **Responsibility:** 在现有本地 SQLite 中按英文与中文释义组合保存独立于 `entries.id` 的累计错误次数；提供按组合递增、列表与抽样。
- **Relationships:** 复用 `WordbookRepository` 的连接与已有收藏的独立生命周期模式；同名单词本替换的事务不删除错题记录。
- **Rationale:** 原词条 ID 在重导入时变化（见 ADR 0003）；只保存在 React 内存或仅引用原词条 ID 都不能保证记录留存。递增用数据库原子操作，避免并发读改写丢失次数；计数的数据形状与供 `createQuestions` 使用的词条形状可分别表达。

### 错误事件与持久化接口

- **Action:** Modify
- **Location:** `src/features/practice/practiceReducer.ts`、`src/App.tsx`、`src/data/wordbooks.ts`、`src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`
- **Responsibility:** 只在现有 `isExactAnswer` 判定为错误的提交路径发出与当时题目配对一致的增量记录；后端校验输入、返回可供列表展示的记录。跳过和揭示路径不额外触发写入。
- **Relationships:** 沿用当前 reducer 的本轮 `errorCount` 行为和 Tauri 服务边界，不在普通渲染或切题时计数。写操作失败的反馈及异步切题竞态需与现有答题进度协调，避免把前一题写到后一题。
- **Rationale:** 计数发生于已存在的错误判定边界，不在列表浏览或抽样时推断历史错误；持久层维护累计真相。

### 错题本入口及练习来源

- **Action:** Modify / Create
- **Location:** `src/App.tsx`、`src/features/practice/PracticeSetup.tsx`、`src/features/` 下错题列表组件
- **Responsibility:** 提供错题列表（英文、中文、累计错误次数）和练习来源选项；开始练习及汇总页再练按选中的来源抽样；空列表展示空态、不进入无题练习。
- **Relationships:** 复用 `createQuestions`、现有练习模式和数量校验；参照收藏夹浏览与抽样调用的服务模式。UI 实施前依照项目规范先查 `bunx astryx build`、`docs layout`、相关 template/component 与 tokens，不新增手写布局习惯。
- **Rationale:** 一个新的练习来源即可满足专项练习，无需复制练习引擎或另造答题流程。

## Implementation steps

### 1. 持久化累计计数及取样

- **Files or discovery point:** `src-tauri/src/wordbooks/repository.rs`、`model.rs`；现有收藏表、抽样与仓库测试。
- **Change:** 幂等初始化错题存储，按现有词条配对规范匹配并保存可展示的原词和释义；原子递增累计次数并返回结果；提供含次数的列表及最多请求数量的不重复随机抽样。抽样返回可用于 `WordEntry`/`createQuestions` 的稳定词条形状；保留原单词本替换流程，不关联会级联删除的源词条 ID。
- **Outcome:** 同一配对的每次错误加一，不同释义各自计数；应用重启、同名单词本替换后仍可浏览和练习。
- **Verification:** Rust 仓库测试覆盖重复递增、不同释义、重新打开数据库、同名替换、随机抽样无重复且不超过请求数、空集和原有收藏/单词本回归。

### 2. 暴露错误记录命令与前端服务契约

- **Files or discovery point:** `src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`、`src/domain/word.ts`。
- **Change:** 注册记录一次错误、获取带计数列表、抽取错题练习词条的命令；沿用现有结构化错误处理，后端验证配对与 1–255 的抽样数量；前端类型区分带次数的记录与普通练习词条。
- **Outcome:** React 仅通过服务 API 更新、浏览及抽样错题；原练习词条接口仍可用于创建题目。
- **Verification:** Rust 命令/仓库测试覆盖输入校验及计数调用；TypeScript 类型检查与服务 mock 验证契约。

### 3. 在错误提交路径记录当前题目

- **Files or discovery point:** `src/features/practice/practiceReducer.ts`、`src/App.tsx`、`src/domain/practice.ts`、相关 reducer/App 测试。
- **Change:** 在提交时使用与现有 reducer 相同的正确性判定，捕获该次提交对应的题目英文/中文配对，仅错误时调用增量记录接口；保留当前轮次的错误次数、重试、切题与汇总逻辑。处理失败或未完成写入的竞态时不误报持久化成功，也不把旧题错误归到新题；在错题本练习中再次提交错误仍累计。
- **Outcome:** 重复错误提交逐次累计；正确、单纯跳过及提示揭示不新增错误记录；现有本轮统计不回退。
- **Verification:** reducer/App 交互测试覆盖同一题多次错误、纠正后提交、跳过及第三层提示、混合模式双方向、快速切题、错题来源再犯错及写失败。

### 4. 提供浏览与专项练习入口

- **Files or discovery point:** `src/App.tsx`、`src/features/practice/PracticeSetup.tsx`、新增错题列表组件、`src/App.test.tsx`。
- **Change:** 在设置页加入错题本浏览入口与来源选择；列表展示英文、中文及累计次数，并支持加载/重试/返回。练习启动与再练从选定来源请求最多所选题数的词条，空集提示不进入练习；显示进度与汇总使用实际抽样数。前端列表重新打开时获取持久层最新次数。
- **Outcome:** 能找到错题及次数、从错题本练习，原单词本和收藏夹练习不受影响。
- **Verification:** UI 测试覆盖空列表、加载失败、正确展示次数、选择来源、题数不足、空练习和汇总再练；桌面手动验证重启及重导入后列表与练习保持可用。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 同一道题多次答错后答对，或答错后跳过 | 每次错误提交各计一次；答对或跳过不再额外计数 | 步骤 1、3 |
| 第三层提示揭示答案但未提交错误答案 | 不新增错题或次数；现有跳过数照常记录 | 步骤 3 |
| 同英文在两个单词本出现且中文释义相同／不同 | 相同组合合并次数；不同释义分别记录 | 步骤 1、2 |
| 重启或替换源单词本 | 错题与累计次数仍可查询及抽样 | 步骤 1、4 |
| 错题本为空或不足所选题数 | 空时显示提示并不启动练习；不足时按实际数练习与汇总 | 步骤 1、4 |
| 错误提交后迅速切题或持久化失败 | 写入对应提交时的题目；失败不显示虚假的持久成功状态 | 步骤 2、3 |
| 错题练习中再次答错 | 该配对累计次数继续增加，本轮错误数也照常增加 | 步骤 1、3、4 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 独立持久化与原子累计 | Rust repository 测试 | 多次递增/重启/替换后的次数和可练习词条一致 |
| 输入、抽样及前后端契约 | Rust 测试、服务 mock、类型检查 | 无效输入不写入，抽样不重复且边界正确 |
| 错误触发条件与练习行为 | Reducer/App 交互测试 | 每次错误提交一次，跳过与正确提交不写，原有统计不变 |
| 列表及来源切换 | App 交互测试 | 次数展示、空态、失败重试、错题来源练习及再练正确 |
| 静态与原功能回归 | `bun run test`、`bun run check`、`cargo test --manifest-path src-tauri/Cargo.toml wordbooks` | 测试通过；导入、收藏和三种练习方向保持有效 |
| 桌面持久化 | 手动启动、答错、重启及替换同名本 | 错题列表次数保持、可重新抽取练习 |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src-tauri/src/wordbooks/repository.rs`（或相邻模块）、`model.rs` | Modify / Create | 错题表、计数、查询、抽样、测试 |
| `src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs` | Modify | 错题命令及注册 |
| `src/data/wordbooks.ts`、`src/domain/word.ts` | Modify as needed | 服务契约和次数展示类型 |
| `src/features/practice/practiceReducer.ts`、`src/App.tsx` | Modify | 错误提交关联词条、浏览与练习来源 |
| `src/features/practice/PracticeSetup.tsx`、`src/features/` 下错题列表 | Modify / Create | 来源选择与列表 |
| `src/App.test.tsx`、Rust repository 测试 | Modify | 跨层行为与持久化回归 |

## Risks and open questions

- **风险：** 当前 reducer 同步更新本轮 `errorCount`，SQLite 写入异步完成；实现时须避免调用两次或写入错误词条，重点验证失败/重试边界。若需要改变用户可见的计数语义，应先返回澄清，不在实施中擅自更改。
- **可调整默认规则：** 已在讨论中告知按英文/中文释义组合合并错题；若需区分单词本或练习方向，先澄清并更新领域记录与本计划。
- **范围边界：** 未确认手动删除、次数清零及答对后自动移除；不在本轮实现。
