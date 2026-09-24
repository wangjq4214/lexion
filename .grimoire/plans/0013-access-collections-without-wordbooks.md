# 无单词本时仍可使用收藏夹与错题本

- **Input:** 本轮对话确认的目标：没有单词本时，只要收藏夹或错题本有词，仍可导入、浏览独立集合并从有词的集合练习；若两者也都为空，则保持仅导入界面。导入前单词本来源不可用。
- **Date:** 2026-09-24
- **Status:** Completed

## Summary

当单词本列表为空时，按收藏夹与错题本是否有词区分界面：两者都确认无词时保留现有仅导入界面；任一集合有词时保留导入、浏览入口与练习设置，只限制需要有效单词本 ID 的来源。不得把查询尚未完成或失败误判为“两个集合都为空”。集合数据本就独立保存，本计划不修改其持久化或删除逻辑，也不执行实现。

依据：`.grimoire/CONTEXT.md` 中的 `favorites-collection`、`mistake-collection`、`practice-source`；`.grimoire/adr/0002-preserve-favorites-independent-of-wordbooks.md` 和 `0003-preserve-mistake-records-independent-of-wordbooks.md`。旧 `.grimoire/spec/0001-excel-wordbook-import.md` 第 7 项及正在实施的 `.grimoire/plans/0012-delete-wordbook-with-confirmation.md` 所述“无单词本时仅显示导入”，在两个独立集合也为空时仍适用；本次用户确认的后续要求只改变独立集合非空时的空本状态。其他导入及删除语义不变。实施 0012 时须结合这一条件理解其旧预期。

## Design impact

### 首页设置与来源显示

- **Action:** Modify
- **Location:** `src/routes/index.tsx`、`src/features/practice/PracticeSetup.tsx`
- **Responsibility:** 没有单词本时，确认收藏夹和错题本均空才仅显示导入；任一非空则显示导入、浏览入口和练习设置，单词本来源仍不可练习。
- **Relationships:** 继续使用 `wordbooksAtom`、`activeWordbookAtom`、`practiceSourceAtom`、现有 `WordbookImportFlow` 和 `PracticeSetup`；只在存在有效单词本时呈现其选择及删除操作。避免对空数组渲染可选择的无效单词本。
- **Rationale:** 问题发生在首页条件渲染，不是集合数据遗失；复用现有设置页和路由入口比新建一个并行空状态页面更少重复。

### 独立集合有无的判定

- **Action:** Modify
- **Location:** `src/routes/index.tsx`；必要时联动 `src/routes/favorites.tsx` 的收藏删除返回流程
- **Responsibility:** 无单词本时通过现有 `WordbookService.listFavorites()` 与 `listMistakes()` 判断集合内容；查询中的状态不能当作空集合，返回首页或收藏删除后要重新判断。
- **Relationships:** 复用现有服务接口及列表页的返回导航；练习来源列表不作为集合是否有词的证据。
- **Rationale:** 仅检查单词本数组或旧的收藏状态无法判定两集合是否都空；复用读接口避免新建持久化表或改动删除命令。

### 练习启动的来源校验

- **Action:** Modify
- **Location:** `src/features/practice/usePracticeStart.ts`
- **Responsibility:** 仅在来源是 `wordbook` 时要求有效的 `activeWordbookId`；收藏/错题来源可在无单词本时使用 `schedulePractice({ source, wordbookId: null, ... })`；空集合沿用已有可读错误，不进入空练习轮。
- **Relationships:** 保持 `WordbookService.schedulePractice` 的现有来源与 nullable ID 协议；导入并刷新后，单词本来源重新可用。
- **Rationale:** 当前入口之外还有 `activeWordbookId === null` 的启动门槛；只放开页面不足以兑现独立来源练习。

### 回归测试

- **Action:** Modify
- **Location:** `src/App.test.tsx`；必要时同目录的练习启动测试
- **Responsibility:** 保留“首次真正空状态仅显示导入”的旧断言；改写“删除最后一本即仅导入”的旧断言，分别验证独立集合非空/均空，并覆盖无单词本时的独立来源练习。
- **Relationships:** 复用现有 `createService` mock 的收藏、错题和 `schedulePractice` 接口；保留有单词本时选择、删除、导入的既有回归断言。
- **Rationale:** 旧测试把单词本数量直接等同于“完全空”；需要保留完全空的预期，并补上独立集合非空的情况。

## Implementation steps

### 1. 区分完全空状态与有独立集合的状态

- **Files or discovery point:** `src/routes/index.tsx`、`src/features/practice/PracticeSetup.tsx`。
- **Change:** 已加载的单词本列表为空时，读取收藏夹和错题本。两者明确为空才显示仅导入入口；任一非空则显示导入、两个浏览入口与练习设置。查询未完成或失败时不冒充“确认无词”，沿用可理解的加载/错误与重试交互；返回首页、收藏删除最后一条后重新判定。单词本来源无有效 ID 时不可启动，不展示可操作的空单词本选择或删除入口。
- **Outcome:** 完全空数据时保持强制导入；删除最后一本但仍有收藏或错题时入口和可用来源保留；删尽收藏后返回首页可恢复仅导入。
- **Verification:** UI 测试分别检查初始全空、删末本但有集合、删末本且两集合空、最后一条收藏删除后返回、查询中/失败及非末本删除。

### 2. 解除独立来源的错误启动门槛

- **Files or discovery point:** `src/features/practice/usePracticeStart.ts`。
- **Change:** 按来源校验是否需要单词本 ID；仅单词本来源缺失 ID 时阻止启动，收藏/错题来源传 `null` 给调度。若来源集合没有词，保留既有空集合错误处理。
- **Outcome:** 无单词本而收藏或错题有词时可正常生成题目；空集合不启动，单词本来源不会带空 ID 发起调度。
- **Verification:** 使用服务 mock 分别验证两种来源的 `schedulePractice` 参数和进入练习页面；检查无词时提示与无单词本时单词本来源不调用服务。

### 3. 更新回归测试并核查联动

- **Files or discovery point:** `src/App.test.tsx`、相关测试。
- **Change:** 保留最初全空时仅导入的旧断言；为最后一本删除前后仍有收藏、仍有错题、两者均空三种情况增加覆盖。验证从收藏/错题页返回时重新判定、导入新本后单词本来源重新可用。运行 `bun run test`、`bun run check`；桌面手动验证删除最后一本、浏览/练习、删除最后一个收藏及重新导入。
- **Outcome:** 新条件空状态和独立数据在导航与启动层面有可执行证据；不需改变 SQLite 模式或既有删除命令。
- **Verification:** 前端测试与静态检查通过；手动检查重启后无单词本但仍有收藏/错题的场景。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 从未导入过单词本，收藏/错题均为空 | 仅显示导入入口，不展示练习设置及两个浏览入口 | 步骤 1、3 |
| 删除最后一本时收藏或错题至少一个有词 | 导入、收藏/错题浏览入口及练习设置仍可见；有词的来源可练习 | 步骤 1–3 |
| 删除最后一本时两个集合均为空 | 保持仅导入状态，不出现不可用练习入口 | 步骤 1、3 |
| 删除最后一本时已选择单词本来源 | 不发送 `wordbookId: null` 的单词本调度；有词的独立来源可选择，均空则仅导入 | 步骤 1–2 |
| 收藏夹移除最后一条，且没有单词本与错题 | 返回首页后恢复仅导入状态，不依赖过期的非空判定 | 步骤 1、3 |
| 删除后重新导入一本 | 新单词本可以选择并练习；收藏/错题入口保持可用 | 步骤 1–3 |
| 单词本或独立集合查询尚未完成或失败 | 不把未知误判为两集合空；展示已有加载/重试体验或相应的错误恢复入口 | 步骤 1、3 |

## Verification strategy

| Behavior or risk | Method | Evidence |
| --- | --- | --- |
| 三种空本情形得到正确界面 | `src/App.test.tsx` 组件/路由测试 | 双集合空时仅导入；仅收藏非空或仅错题非空时有浏览入口和练习设置 |
| 数据变化后判定不陈旧 | `src/App.test.tsx` 路由测试 | 删除末本、收藏末条与返回首页后分别更新；加载/错误不显示错误空状态 |
| 无本仍能练习有词的收藏/错题 | `src/App.test.tsx` 服务 mock 集成测试 | 各来源的 `schedulePractice` 收到 `wordbookId: null`；有词进入练习 |
| 缺失单词本 ID 不误启动单词本练习 | 组件/路由测试 | 选中单词本来源时不能启动，且 `schedulePractice` 未被调用 |
| 既有导入/删除及非空设置不退化 | 现有 App 测试、`bun run test`、`bun run check` | 导入后选择可用；剩余一本时删除后改选正常；命令通过 |
| 数据确实独立而非临时界面状态 | 已有 Rust 删除仓储测试与桌面重启手动验证 | 收藏与错题依然持久且可打开；无需改变持久化层 |

## Risks and open questions

- **旧合同衔接：** 旧导入规格与 0012 删除计划的“仅导入”仍适用于单词本、收藏、错题均空；独立集合有词时以本轮确认的条件为准。实施时更新 0012 对删末本的无条件断言，保留全空情况下的旧测试。
- **双重门槛：** 页面空状态、独立集合内容判定及练习启动空 ID 检查都需处理；只改其中一个无法满足目标。
- **异步竞态：** 从有词页面删除最后一条收藏并返回、删除末本触发刷新时，避免旧查询覆盖新状态；未知/失败不是零条。
- **工作区已有未提交改动：** 删除功能实现和 0012 计划尚未提交；执行时在当前工作树上做增量修改，勿覆盖这些改动。
