# 单词本管理页

- **Input:** 本轮对话的需求与确认；`.grimoire/CONTEXT.md`、`.grimoire/adr/0008-keep-practice-entry-deletion-alongside-central-management.md`；沿用 `.grimoire/spec/0001-excel-wordbook-import.md` 中适用的导入规则及 `.grimoire/plans/0012-delete-wordbook-with-confirmation.md` 的整本删除规则（其中关于旧页面位置的描述由本轮决定取代）。
- **Date:** 2026-09-25
- **Status:** Completed

## Summary

新增单词本管理页面：首页单词本入口指向该页，在此查看所有单词本、浏览每本全部英文/中文词条、导入和删除单词本，以及从浏览列表逐词删除。移走练习设置中的整本删除与独立导入入口；练习题中“从单词本删除当前单词”保持原状，练习/考试中的来源和单词本选择不变。收藏夹、错题本继续独立。这里只制定实施计划，不实施代码。

## Design impact

### 完整词条读取边界

- **Action:** Modify
- **Location:** `src-tauri/src/wordbooks/repository.rs`、`src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`
- **Responsibility:** 为指定单词本读取所有实际词条（含 ID、英文、中文），供浏览及按 ID 删除使用。
- **Relationships:** 复用 `WordEntry`、`WordbookService`、现有 Tauri 命令错误映射；`sampleWordbook` 仍专用于随机、限量的练习抽样。
- **Rationale:** 浏览不能复用随机采样，否则会漏词；沿用现有仓储/命令/服务链，无需改变表结构或练习调度。

### 管理页面与现有操作

- **Action:** Create / Modify
- **Location:** 新建 `src/routes/wordbooks.tsx`（或符合当前文件路由命名的等价路径），复用 `src/features/wordbooks/WordbookImportFlow.tsx`、`src/features/wordbooks/WordbookProvider.tsx`；修改 `src/routes/index.tsx`、`src/routes/import.tsx` 的旧入口。
- **Responsibility:** 集中提供单词本摘要、按本浏览、导入、整本删除和逐词删除；首页进入管理而非单独导入页；零本时依然能进入管理并导入。
- **Relationships:** `wordbooksAtom` / `activeWordbookAtom` 由现有刷新边界维护；删除使用现有 `deleteWordbook`、`deleteWordbookEntry` 服务，整本删除沿用既有确认与失败恢复语义。
- **Rationale:** 把一般管理操作放到一个页面，复用成熟的导入、删除命令而不是复制逻辑；仍用单独的学习来源选择器来启动练习和考试。

### 练习设置与答题

- **Action:** Modify / Retain
- **Location:** `src/features/practice/PracticeSetup.tsx`、`src/routes/practice-setup.tsx`；保留 `src/features/practice/PracticeQuestion.tsx` 与 `src/routes/practice.tsx` 的练习删词行为。
- **Responsibility:** 设置页移除整本删除按钮及对应状态/回调；空状态文案指向首页管理入口；答题时的当前词删除与确认流程保持可用。
- **Relationships:** 练习/考试依旧从有效的单词本列表选择来源，管理页变更后刷新摘要和选中项。
- **Rationale:** 用户确认的特例是答题过程快速删当前词，而非在设置页管理整本。

## Implementation steps

### 1. 增加指定单词本的完整词条读取

- **Files:** `src-tauri/src/wordbooks/repository.rs`、`src-tauri/src/wordbooks/mod.rs`、`src-tauri/src/lib.rs`、`src/data/wordbooks.ts`、`src/data/wordbooks.test.ts`。
- **Change:** 按单词本 ID 查询所属词条，使用稳定结果顺序供浏览；注册读取命令并扩展 `WordbookService` 及调用适配器。检查非法 ID、不存在的单词本及空词条本的现有错误/空集合惯例，保持命令、客户端和测试一致。不改动 `sampleWordbook`。
- **Outcome:** 浏览可获得本中全部词条及各自 ID，别本词条不混入，也不依赖随机练习样本。
- **Verification:** Rust 仓储测试覆盖多本、空本、全部词条、顺序及非法输入；TypeScript 服务测试覆盖命令参数和错误映射。

### 2. 建立管理页面并接入导入和浏览

- **Files:** `src/routes/wordbooks.tsx`（新）、`src/features/wordbooks/WordbookImportFlow.tsx`（仅在需要时调整）、`src/routes/index.tsx`、相关路由生成文件（按现有生成机制）。
- **Change:** 页面展示所有单词本及数量，可选择某本并显示其全部词条与英文/中文释义；复用导入流程、同名替换确认及 `useRefreshWordbooks`。首页“导入单词本”入口改为“单词本管理”并指向该页；停用独立 `/import` 管理页面入口，处理旧路由时遵循项目路由生成惯例，不让其成为第二个管理界面。按所选本切换、导入/替换后重新取词条，不展示过期异步请求结果。
- **Outcome:** 无论零本或多本，均可从首页进入同一管理页完成导入与逐本浏览。
- **Verification:** 页面测试覆盖首页导航、空状态导入、切换多本、同名替换后刷新与慢请求返回时的列表一致性。

### 3. 在管理页接入整本和逐词删除

- **Files:** `src/routes/wordbooks.tsx`，必要时拆分就近的管理 UI 组件；现有 `WordbookService` 删除方法保持复用。
- **Change:** 对选定本复用现有删除前确认和按 ID 删除；逐词操作按当前本 ID 与词条 ID 调用 `deleteWordbookEntry`。删除成功后刷新摘要、当前词条及有效选中项；失败或目标已不存在时显示可恢复的错误并重新核对列表，不错误地移除其他本/词条。并发操作须绑定点击时目标，避免切换本后误删。
- **Outcome:** 删除只影响目标本或目标词条，零词条本仍可浏览，删掉最后一本后仍能导入。
- **Verification:** UI 测试覆盖取消/确认整本删除、删选中本及最后一本、逐词删除、重复提交、失效目标、服务失败；Rust 已有按本/词条 ID 删除及收藏/错题独立性测试回归。

### 4. 移走分散入口并保护练习特例

- **Files:** `src/features/practice/PracticeSetup.tsx`、`src/routes/practice-setup.tsx`、`src/routes/import.tsx`（按既有路由机制整合或移除）、`src/App.test.tsx` 及邻近测试。
- **Change:** 清除设置页整本删除状态、按钮和回调；更新空状态中让用户返回首页导入的指引为进入管理页；检查其他页面不再显示独立单词本管理入口。**不得删除**答题页当前单词的删除控件/处理器，也不得移走练习及考试来源选择。
- **Outcome:** 日常单词本管理集中在管理页；练习中仍可直接删当前词，且收藏、错题和考试正常。
- **Verification:** 导航/设置页测试不再出现整本删除及独立导入按钮；答题页原有删词、来源选择、错题/收藏独立性回归测试仍通过。

### 5. 执行回归

- **Files:** 上述测试及现有测试套件。
- **Change:** 执行 `bun run test`、`bun run check`、`cargo test --manifest-path src-tauri/Cargo.toml`；在桌面环境检查 Excel 文件选择、同名替换、多本浏览/删除、练习直接删词和重新启动后的持久化。
- **Outcome:** 新浏览读取与原导入/删除/学习流程同时可用。
- **Verification:** 自动检查通过；桌面交互逐项观察，若环境不可运行，明确标注未验证项。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 零本、空本或删除最后一本 | 管理页仍有导入入口；空本显示无词条；学习来源按原规则可用或禁用 | 步骤 1–3，页面测试 |
| 多本有相同英文或不同中文释义 | 读取和删除只作用于指定本的指定词条 ID，其他本不变 | 步骤 1、3，仓储/UI 测试 |
| 浏览中切换单词本时旧读取才完成 | 旧响应不得覆盖当前本词条 | 步骤 2，异步 UI 测试 |
| 同名替换或整本删除后持有旧词条 ID | 刷新摘要与词条；失效操作不得删新本或别本词条 | 步骤 2–3，UI/仓储测试 |
| 删除请求失败或重复提交 | 不乐观宣称成功；阻止重复操作并可恢复 | 步骤 3，UI 测试 |
| 收藏/错题保存了被删除词对 | 独立集合不随单词本或其词条删除 | 步骤 3–5，仓储回归 |

## Risks and verification

- **旧要求与新入口冲突：** `.grimoire/spec/0001-excel-wordbook-import.md` 要求首页仅导入或顶部导入，这是旧范围；本轮确认的管理入口取代位置约束，但导入格式、替换确认和持久化规则仍沿用。
- **大单词本浏览：** 完整读取与渲染可能昂贵；优先遵循“可浏览每个单词本中的单词”的合同，实施时测量大本响应/渲染，若需要引入分页等新体验决策，返回澄清而非在计划中擅自改变覆盖语义。
- **共享状态与过期请求：** 导入/删除改变本 ID 与数量；使用现有刷新策略，并验证快速切换或并发写入不会把过期内容呈现为当前本。
- **UI 规范：** 写页面前遵循 `AGENTS.md` 的 Astryx build、layout、tokens、模板及组件查询工作流，不猜组件 API，不使用原生布局容器或硬编码样式。
