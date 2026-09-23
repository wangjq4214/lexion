# 单词复习：跨轮覆盖与遗忘曲线调度

- **Input:** `.grimoire/spec/0002-coverage-and-forgetting-curve-review-scheduling.md`；`.grimoire/CONTEXT.md`；`.grimoire/adr/0002-preserve-favorites-independent-of-wordbooks.md`、`0003-preserve-mistake-records-independent-of-wordbooks.md`、`0004-schedule-practice-with-coverage-and-due-review.md`
- **Date:** 2026-09-23
- **Status:** In Progress
- **Endpoint:** 自动化实现与验证已完成；桌面端手动演练待执行

## Summary

把单词本、收藏夹和错题本的独立随机抽样替换为持久的跨轮覆盖和到期复习调度。同一来源的词最终都能被抽到；到期优先但每轮给未抽到的词留名额。每词每方向用已确认的 `R(t) = exp(-t / S)` 和初始为 90% 的可调目标预测到期时点，题目完成时按本题错误次数、提示级数、是否跳过更新稳定度 `S`。保留现有错题累计规则、三种模式、题数、汇总与收藏/错题脱离原词本的生命周期。更新系数须以可测的单调性、边界与使用反馈校准，不把临时系数写成已确认的学习常数。

## Design impact

### 复习状态与选词持久层

- **Action:** Modify；必要时 Create 相邻调度模块。
- **Location:** `src-tauri/src/wordbooks/repository.rs`，相邻 Rust 模块及 `model.rs`。
- **Responsibility:** 在现有 SQLite 生命周期内维护每个词本各自的覆盖状态，以及收藏夹和错题本各自的覆盖状态；同一英文—中文组合跨来源共用按方向的稳定度/到期时间；按来源和数量无重复抽取，完成题目时持久更新。
- **Relationships:** 使用 `entries`、`favorites`、`mistakes` 当前成员；同名单词本替换重建词条 ID，收藏与错题使用独立于源词条的配对身份（ADR 0002/0003）。
- **Rationale:** 跨轮、重启和来源变动都必须可追踪；继续依赖 `ORDER BY RANDOM()` 与只存本轮汇总无法提供覆盖与到期判断。复习计算复杂时与既有导入/收藏 SQL 分开，但仍通过同一持久层事务边界。

### 前后端调度契约

- **Action:** Modify。
- **Location:** `src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`；必要时 `src-tauri/src/wordbooks/model.rs`、`src/domain/practice.ts`。
- **Responsibility:** 用来源、词本、题数和练习模式发起调度；为选出的题目保留可准确回写的身份与方向；用完成事件提交本题错误次数、提示级数及正确/跳过结果。
- **Relationships:** `App.tsx` 负责调用，Tauri/SQLite 提供持久化真相；`createQuestions` 现会再次打乱并在 mixed 模式随机选方向，接入时必须让到期判断与实际题目方向一致，不能被后续随机化打乱语义。
- **Rationale:** UI 不直接维护跨轮数据库状态；只传最终错题累计数会混淆‘同题错误次数’与‘历次累计错误次数’。

### 单题结果采集和现有错题写入

- **Action:** Modify。
- **Location:** `src/features/practice/practiceReducer.ts`、`src/App.tsx`、相关交互测试。
- **Responsibility:** 每题从错误提交、逐级提示到答对/跳过维护局部计数，并在真正完成时仅提交一次复习结果；原有每次错误提交调用 `recordMistake` 的行为继续保留。
- **Relationships:** `isExactAnswer`、第三级提示揭示、`continue-after-skip`、`PracticeSummary` 仍遵守原有流程；结果写入与开始下一轮的异步时序应保证状态可见且不重记。
- **Rationale:** 当前 reducer 只保留整轮 `errorCount` / `hintCount` 和当前 `hintLevel`，换题时不会保留上一题的局部统计；复习模型需要精确的单题反馈。

## Implementation steps

### 1. 建立可测的复习计算与持久状态

- **Files or discovery point:** `src-tauri/src/wordbooks/repository.rs`、相邻调度模块、`model.rs`；先核对 SQLite 模式与旧数据初始化。
- **Change:** 为各来源覆盖进度与相同英文—中文组合跨来源共用的方向性 `S`、上次练习时间、到期时间建幂等存储及查询；不同释义分别存储，不能以重导入会变的词条 ID 作为跨来源记忆身份。时钟可注入或在测试中可控制。定义 `S > 0` 与 `0 < r < 1`，初始 `r = 0.9`，到期间隔 `Δt = -S ln(r)`。实现以本题错误/提示数和跳过更新 `S` 的可调系数；保证无辅助成功提高稳定度，困难反馈相对更早到期，同条件下次数增加不推迟复习，未来的无辅助成功可逐渐恢复。初始稳定度及更新系数属于待校准参数，先在实现时提供可复现的测试与调整入口，不把系数声称为已证实的规律。
- **Outcome:** 数据重启后可恢复，正确、错误、提示、跳过的到期先后满足规范且不产生零值/非法时间。
- **Verification:** Rust 单元/仓库测试覆盖可控时间、曲线随时间单调、目标阈值、更新单调性、恢复、老库升级和非法参数保护。

### 2. 在同一来源实现覆盖保障与到期优先

- **Files or discovery point:** `src-tauri/src/wordbooks/repository.rs::sample`、`sample_favorites`、`sample_mistakes` 及其测试。
- **Change:** 以当前来源成员与覆盖记录形成未练、已到期、其余候选集，去重选择至多请求数；仍有未练时保留非零名额，其余尽量选已到期；集合不足则使用实际成员数。使覆盖进度与抽取形成持久一致的状态，新增词可首次覆盖、移除词不再抽取；同名替换不能因旧 ID 遗留错误候选或破坏独立收藏/错题状态。留意所选 1 题时保障覆盖与到期竞争：必须保证未练词不会被持续到期挤掉。
- **Outcome:** 三种来源连续练习可全部覆盖而不在同轮重复；已到期优先，未到期但可练时仍可按当前入口开始练习。
- **Verification:** Rust 仓库测试按多轮、持续到期、题数为 1 / 5 / 255、集合不足、空集、重启、词本替换、收藏增删和错题动态新增演练。

### 3. 接通调度请求和方向一致性

- **Files or discovery point:** `src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`、`src/domain/practice.ts::createQuestions`、`src/App.tsx::startPractice`。
- **Change:** 替换三个来源的独立随机请求；将模式纳入调度与选题方向的边界，使中译英、英译中和 mixed 模式均先确定题目方向再读取同配对跨来源共享的该方向到期状态。仍允许本轮展示顺序随机化，但不得在完成调度后改变已用于到期判断的题目方向。保留当前数量验证与空集错误提示。
- **Outcome:** 前端拿到的题目属于当前所选来源、方向与计算一致，重练继续使用当前来源和题数。
- **Verification:** Rust 命令/契约测试与 `src/domain/practice.test.ts`、`src/App.test.tsx` 覆盖三来源三模式、混合方向、题数不足及重练。

### 4. 仅在单题完成时记录复习结果

- **Files or discovery point:** `src/features/practice/practiceReducer.ts`、`src/App.tsx::submitAnswer` / `completeQuestion`，相关 reducer/App 测试。
- **Change:** 逐题累积错误提交数与使用的提示级数，答对或揭示后手动继续时发送一次完成结果；第三层提示记提示并按跳过处理，直接跳过不额外增加错误。分别处理复习结果写入与 `recordMistake` 的逐次错误写入，捕获对应题目身份/实际方向以免异步切题写错；重复提交或失败重试不重复应用同一完成事件。开始下一轮时确保已完成结果被持久调度读取，不伪报成功。
- **Outcome:** 复习状态与最终答题反馈一致，本轮汇总和错题计数不回退，快速操作/重启后也不重复计入。
- **Verification:** Reducer/App 测试覆盖多次错误后改对、两次提示后答对、第三次提示与直接跳过、末题、混合模式、写失败/重试、连续开始练习。

### 5. 跨来源回归和校准

- **Files or discovery point:** Rust wordbooks 测试、`src/App.test.tsx`、`src/domain/practice.test.ts`；桌面端人工演练。
- **Change:** 以可控时间对照初始 90% 目标检验到期；记录更新系数的配置与相对行为测试，实际使用反馈不足时保留可调性而不宣称科学准确；验证大于一轮题数的三个来源会被覆盖、跨重启/替换持续。只在需要展示新状态时遵守现有 Astryx UI 规范，不额外设计新页面。
- **Outcome:** 规范的覆盖、遗忘曲线和结果权重在持久层及用户流程中一致。
- **Verification:** `bun run test`、`bun run typecheck`、`bun run check`、`cargo test --manifest-path src-tauri/Cargo.toml wordbooks`；桌面端多轮、重启和替换词本手动验证。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 请求 1 题且同时有到期、未练词 | 不让未练长期饥饿；同轮仍只有一题 | 2 / 多轮终止性测试 |
| 来源成员多于 255 或少于所选题数 | 逐轮覆盖前者；后者按实际数量，不重复 | 2 / Rust 测试 |
| 新安装或升级旧 SQLite，无历史稳定度 | 初始化可用于首次覆盖的状态，不丢失已有词本/收藏/错题 | 1、2 / 老库升级测试 |
| 同名单词本替换导致条目 ID 变化 | 当前词本可正确覆盖；独立收藏/错题仍可练 | 1、2 / 替换测试 |
| 同词出现在多个来源、不同中文释义 | 各来源独立覆盖；同英文—中文配对且同方向共享稳定度，不同释义或方向保持独立 | 1、2、3 / 来源和身份测试 |
| 同一道题多次错误、提示后答对、第三级揭晓 | 每次错误仍累计错题；单题只产生一次包含实际次数的复习结果；揭晓计跳过 | 4 / reducer/App 测试 |
| 时钟回拨、相同时间连续练习、重复完成写入 | 稳定度/到期时间合法；同一完成事件不重复改变状态 | 1、4 / 可控时钟、幂等测试 |
| mixed 模式调度后再次随机改变方向 | 不允许到期判断与回写方向不一致 | 3 / 方向测试 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 遗忘曲线与权重 | 时钟可控 Rust 单测 | `R(0)=1`、`Δt=-S ln(0.9)`、无辅助成功后变长、提示/错误增加后不变晚、跳过提前、后续成功可恢复 |
| 三来源覆盖公平性 | Rust 仓库集成测试 | 同轮去重；有限来源在连续多轮中全部抽到；到期词占优且未练不被饿死 |
| 历史、跨来源身份与成员生命周期 | SQLite 重开/同名替换/增删测试 | 收藏和错题保留；各来源覆盖独立，跨来源同配对同方向共享 `S`，不同释义不共享；词本重建后无旧 ID 污染 |
| 模式/题数/完整答题流程 | TS 领域与 React 交互测试 | 两方向写不同状态；mixed 实际方向一致；错误与提示按题计数；汇总不改变 |
| 异步失败和重复触发 | App 模拟失败与仓库幂等测试 | 不丢失或重复写入单题结果；错题每次错误依旧累计 |
| 回归与桌面路径 | 完整测试、类型/检查、手动多轮 | `bun run test && bun run typecheck && bun run check`、`cargo test --manifest-path src-tauri/Cargo.toml wordbooks`、桌面重启/重导入成功 |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src-tauri/src/wordbooks/repository.rs`（可拆相邻调度模块） | Modify / Create | SQLite 复习状态、来源候选、抽取与完成更新 |
| `src-tauri/src/wordbooks/model.rs`、`mod.rs`、`src-tauri/src/lib.rs` | Modify | 请求、结果类型与命令注册 |
| `src/data/wordbooks.ts`、`src/domain/practice.ts` | Modify | 前端服务契约与题目实际方向 |
| `src/features/practice/practiceReducer.ts`、`src/App.tsx` | Modify | 单题结果、异步写入与开始/重练调度 |
| Rust 相关测试、`src/App.test.tsx`、`src/domain/practice.test.ts` | Modify | 覆盖、曲线、生命周期和交互回归 |

## Risks and open questions

- **校准风险：** 曲线形式和初始可调目标已确认，但 `S` 初值与更新系数尚无用户数据支持。实施时仅以可测的相对约束和可调整参数落地，采集使用反馈后再校准；若必须对外承诺绝对复习日期或新的学习语义，先返回需求澄清并更新知识，不在计划中编造常数。
- **一致性风险：** `createQuestions` 当前会独立随机指定 mixed 方向，调度与实际作答方向必须在同一请求契约上对齐；现有错题异步写入不能替代单题完成事件。
- **身份风险：** 重导入词本删除旧行、收藏/错题却继续存活；以按来源的覆盖身份和按配对+方向的共享记忆身份分开，不能用可变化的条目 ID 关联跨来源状态。
- **时序风险：** 抽到词即计覆盖，但只有完成题目才能更新复习状态；若用户中途退出，该词仍可能再次被抽到，不等于已掌握。重启和重复操作的去重策略需在持久层可验证。
