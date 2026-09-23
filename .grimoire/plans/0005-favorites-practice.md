# 收藏夹与收藏词练习

- **Input:** 用户本轮需求与确认（一个收藏夹、重导入单词本后收藏保留）；`.grimoire/CONTEXT.md`、`.grimoire/adr/0002-preserve-favorites-independent-of-wordbooks.md`；现有练习与导入代码
- **Date:** 2026-09-23
- **Status:** Proposed
- **Endpoint:** 实施计划；不执行实现

## Summary

允许在练习题中收藏/取消收藏当前英文与中文释义组合，在一个收藏夹中查看及移除已收藏词条，并以收藏夹为练习来源。收藏数据在本地持久保存，独立于原单词本词条的数据库行 ID，同名单词本替换后仍可查看及练习。收藏夹练习复用现有三种模式、5/10/20/50 与自定义 1–255 的数量设置、抽样、答题和结果流程；收藏不足时仅练实际词数，空收藏夹不进入练习。

讨论时已告知的可调整默认方案：相同英文与中文释义组合只收藏一次；收藏夹为空时不开始练习。未请求多个收藏夹、自动收藏错题或删除原单词本。收藏判断的大小写规则若影响用户预期，须先返回讨论，不在计划阶段新增语义。

## Design impact

### 收藏数据与查询

- **Action:** Modify
- **Location:** `src-tauri/src/wordbooks/repository.rs`；必要时将独立生命周期的收藏查询拆入相邻模块
- **Responsibility:** 在现有 SQLite 数据库初始化收藏词条表；按英文/中文组合保存副本，查询成员和列表、移除与随机抽样，不用 `entries.id` 作为收藏的唯一持久依据。
- **Relationships:** 复用 `WordbookRepository` 的连接与数据库文件；原有 `replace` 删除并重建 wordbook/entries 的事务不应触碰收藏表。
- **Rationale:** `.grimoire/adr/0002-preserve-favorites-independent-of-wordbooks.md` 已规定收藏独立于原词条；仅建外键指向原条目会在替换时丢失收藏。收藏练习返回兼容现有 `WordEntry` 的稳定数据形状。

### 前后端收藏接口

- **Action:** Modify
- **Location:** `src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/wordbooks/model.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`
- **Responsibility:** 为保存/取消、列出、检查当前词条是否收藏及抽样提供 Tauri 命令和服务方法；错误沿用现有命令错误映射，数量边界与原抽样接口一致。
- **Relationships:** React 不直接操作数据库；收藏状态由持久层返回而非浏览器内存当作真相。
- **Rationale:** 沿用已存在的 Tauri 服务边界，确保重启后及跨练习轮次状态一致。

### 练习来源与收藏入口

- **Action:** Modify / Create
- **Location:** `src/App.tsx`、`src/features/practice/PracticeSetup.tsx`、`src/features/practice/PracticeQuestion.tsx`；新增收藏夹展示组件于 `src/features/` 相邻目录
- **Responsibility:** 设置页选择当前单词本或收藏夹作为来源；练习中显示当前配对的收藏状态并允许切换；收藏夹可查看、移除词条并返回练习设置。开始和“再练一轮”均按本轮所选来源及数量重新取样。
- **Relationships:** `createQuestions` 和 reducer 沿用已有题目/计时/提示/跳过/汇总行为。收藏状态变更与答题状态分开，切题时重新对齐当前配对。
- **Rationale:** 复用现有练习引擎而不是建立另一套练习；收藏入口紧邻正在学习的单词，无需先增加单词本浏览界面。UI 实施遵循项目 Astryx 布局、组件和 token 规范，实施前先查 `bunx astryx build`、`docs layout` 与相关组件文档。

## Implementation steps

### 1. 增加独立的收藏持久化和抽样

- **Files or discovery point:** `src-tauri/src/wordbooks/repository.rs`、`model.rs` 与现有 repository 测试。
- **Change:** 幂等建表保存英文、中文配对；按组合保证单份收藏，提供新增/移除、成员检查、列表及按请求数量随机抽样。保留显示用的原始英文与中文；替换单词本不得级联删除收藏。对不存在或已移除的收藏操作给出稳定的幂等结果；以实际仓库 API 形状为准。
- **Outcome:** 关闭重开数据库、替换同名单词本后收藏仍可查询和抽样；同组合跨单词本不会重复，英文相同但释义不同仍可分别收藏。
- **Verification:** Rust repository 测试覆盖重启、替换、重复收藏、不同释义、取消收藏、少于请求数、范围与不重复抽样，以及原 wordbook 导入/抽样回归。

### 2. 暴露收藏命令和前端服务契约

- **Files or discovery point:** `src-tauri/src/wordbooks/mod.rs`、`model.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`、`src/domain/word.ts`。
- **Change:** 注册收藏写入、移除、列出/状态查询、抽样命令；令服务的返回值能用于收藏夹展示和 `createQuestions`。后端校验输入与 1–255 的抽样范围，前端保留原有错误处理风格；跨原词条 ID 变化时按已保存的配对识别收藏。
- **Outcome:** UI 可通过单一服务边界管理收藏和获取收藏练习题，不需访问源单词本记录。
- **Verification:** 命令/服务契约测试及 TypeScript 类型检查；无效参数不写入空词条或发起无效抽样。

### 3. 接入收藏夹浏览、题目收藏和练习来源

- **Files or discovery point:** `src/App.tsx`、`src/features/practice/PracticeSetup.tsx`、`src/features/practice/PracticeQuestion.tsx`、新增收藏夹展示组件及 `src/App.test.tsx`。
- **Change:** 在设置页保留单词本导入和选择，并加入收藏夹入口/练习来源；题目中提供收藏与取消操作（包括显示答案/跳过后的状态）；收藏夹展示保存的英文和中文及移除操作。启动与汇总页重练按选中来源调用对应抽样，复用已验证数量；切题、切来源、失败和请求中状态应避免旧请求覆盖当前词条或误标收藏。
- **Outcome:** 从单词本练习可添加收藏，收藏列表可查看/移除，并在收藏夹练习中使用既有练习方式；空收藏夹明确提示且不进入无题状态。
- **Verification:** 前端交互测试覆盖收藏、取消、展示、切题、跨来源、空状态、失败、重练与题数不足；检查原三种模式及结果统计不变。手动桌面验证重启及重导入后收藏仍能练习。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 同一英文/中文组合出现在不同单词本，或同英文有不同中文释义 | 同组合只收藏一次；不同释义可分别收藏 | 步骤 1、2、3 |
| 已收藏词的原单词本同名替换，原词条 ID 改变 | 收藏列表与练习仍包含保存的词和释义；新练习中相同配对可识别为已收藏 | 步骤 1、2、3 |
| 收藏为空或剩余词数小于所选题数 | 空时不开始，显示明确提示；不足时练全部可用词，进度及结果按实际题数 | 步骤 1、3 |
| 连续点击收藏/取消或切题时网络/数据库请求晚返回 | 不重复添加，不将上一题收藏状态显示在下一题；失败时反馈并恢复持久层真实状态 | 步骤 1、3 |
| 保存、移除或抽样失败 | 不误报操作成功、不进入空题练习；已有收藏及单词本数据不受损 | 步骤 1、2、3 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 收藏独立于导入词条与数据库重启 | Rust repository 集成测试 | 重导入后旧收藏保留，重开连接仍可查且可抽样 |
| 唯一配对、不同释义、数量边界与随机抽样 | Rust repository/命令测试 | 同配对一次；不同释义分别存在；无重复且最多请求数 |
| 收藏入口、浏览、来源切换与重练 | React Testing Library 服务 mock 交互测试 | 各动作调用正确命令；题目/总题数/汇总与选择一致 |
| 加载失败及快速切题 | UI 异步交互测试 | 无陈旧状态污染、不会将失败写操作显示为成功 |
| 原有行为及静态检查 | `bun run test`、`bun run check`、`cargo test --manifest-path src-tauri/Cargo.toml wordbooks` | 相关测试通过，三种练习方向及导入替换正常 |
| 桌面端体验 | 手动从两本单词本收藏、重启、替换同名本、收藏夹练习 | 列表及抽样保留，交互可达 |

## Affected files

| File | Action | Purpose |
| --- | --- | --- |
| `src-tauri/src/wordbooks/repository.rs`（或相邻收藏模块） | Modify / Create | 收藏表、查询、抽样及测试 |
| `src-tauri/src/wordbooks/model.rs`、`mod.rs`、`src-tauri/src/lib.rs` | Modify | 收藏模型、命令及注册 |
| `src/data/wordbooks.ts`、`src/domain/word.ts` | Modify as needed | 前端服务与题目词条契约 |
| `src/App.tsx`、`src/features/practice/PracticeSetup.tsx`、`PracticeQuestion.tsx` | Modify | 练习来源与收藏状态入口 |
| `src/features/` 下收藏夹展示组件 | Create | 浏览和移除收藏 |
| `src/App.test.tsx`、Rust repository 测试 | Modify | 验证跨层交互与数据保留 |

## Risks and open questions

- **风险：** 现有词条 `id` 在同名替换后变化。持久收藏及成员判断不能仅绑定该 ID；实现前检查 `createQuestions` 的 ID 使用及 React key，确保收藏抽样返回稳定可区分的题目。
- **风险：** 收藏状态异步加载与快速切题/切来源可能产生陈旧显示。通过当前配对标识和请求结果匹配或取消过期请求来验证。
- **可调整默认方案：** “相同英文和中文配对只收藏一次”与“空收藏夹不开始练习”已在 refine 讨论中告知；若用户要求按来源分别收藏或不同的空态行为，先返回澄清与知识记录再调整本计划。
