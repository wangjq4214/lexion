# 考试与练习入口选择实施计划

- **Input:** 用户请求（2026-09-25）：“添加一个路由界面，选择去考试还是练习，具体的模式设置可以跳转之后再显示”；补充：“导入单词本的路由也应该在这里，两个大的卡片是练习和考试，下面3个不太显眼的按钮是错题、收藏、导入单词本”；[项目上下文](../CONTEXT.md)、[路由 ADR 0005](../adr/0005-use-file-based-tanstack-router-and-jotai-for-frontend-navigation-and-state.md)、[考试计划 0014](./0014-vocabulary-exam.md)
- **Date:** 2026-09-25
- **Status:** Completed

## Summary

首页用两张突出的练习/考试卡片选择主要去向，下方放错题本、收藏夹、导入单词本三个次要按钮；五个入口均导航至各自路由。考试与练习的具体设置在跳转后显示；导入使用独立路由并复用既有导入流程。后续用户调整：练习设置只保留来源、模式等参数，不显示收藏/错题浏览或导入按钮；原导入按钮的位置改为“返回首页”，无词本时也从首页导入。入口及练习设置路径是与现有文件路由配合的实施假设；不改变取题、评分、复习或词本/集合的数据规则。实施期间保护现有未提交的考试相关改动。

## Design impact

### 入口路由与练习设置路由

- **Action:** Modify / Create
- **Location:** `src/routes/index.tsx`、新建练习设置文件路由（建议 `src/routes/practice-setup.tsx`）；`src/routeTree.gen.ts` 由路由工具生成。
- **Responsibility:** 首页只呈现两张主要卡片（练习、考试）及下面三个视觉上不抢眼的按钮（错题本、收藏夹、导入单词本），不提前展示模式、来源或题数；将原 `/` 的 `PracticeSetup`、无词本时的集合检查、启动练习等逻辑迁入练习设置路由。练习设置不提供收藏/错题浏览或导入按钮；由页面顶部“返回首页”去往首页导入入口。
- **Relationships:** 主卡片进入练习设置和已有 `/exam`；次要入口进入已有 `/mistakes`、`/favorites` 和新增导入路由。复用 `usePracticeStart`、Jotai 的词本/来源/题数/轮次状态；练习开始仍进入已有 `/practice` 作答路由。
- **Rationale:** 已有 `/practice` 是进行中的练习；独立设置路径避免与 `RouteFrame` 的轮次守卫冲突。首页只负责选择去向，不增加业务状态。

### 导入单词本路由

- **Action:** Create / Reuse
- **Location:** 新建导入路由（建议 `src/routes/import.tsx`），复用 `src/features/wordbooks/WordbookImportFlow.tsx`、`src/features/wordbooks/WordbookProvider.tsx`。
- **Responsibility:** 首页的导入按钮先导航至导入路由，再由现有文件选择、命名、冲突替换确认流程执行导入；导入后刷新可选词本，并提供清晰的返回入口。无词本时练习设置提示从首页导入，而不显示导入按钮。
- **Relationships:** 不复制文件导入逻辑，也不改动现有词本持久化规则；导入成功后的去向以返回入口/进入练习设置为界面导航实现选择，不改变导入结果。
- **Rationale:** 当前 `WordbookImportFlow` 只是打开文件选择器和对话框的按钮；用轻量路由承载它，才满足“导入单词本的路由也在首页”的导航语义。

### 返回入口与路由守卫

- **Action:** Modify
- **Location:** `src/routes/exam.tsx`、`src/routes/summary.tsx`、`src/routes/favorites.tsx`、`src/routes/mistakes.tsx`、`src/App.tsx` 中的 `RouteFrame`，以及新建导入路由；视实际组件结构调整返回按钮文字。
- **Responsibility:** 考试设置、导入、词库浏览可返回入口选择；练习设置可返回入口；练习结算后需要继续设置时保留可达的练习设置入口。进行中练习的离开限制及考试未核对/错题写入期间的阻挡保持原样。
- **Relationships:** 遵循 ADR 0005 的文件路由与守卫，保持 Jotai 轮次状态转换；检查直接访问或刷新 `/practice`、`/summary` 时应落到仍存在的设置界面，而不是空的作答页面。
- **Rationale:** 原来 `/` 兼任练习设置、集合入口和导入入口；迁出后只改首页而不处理回退会把用户带到错误页面。

## Implementation steps

### 1. 分离入口与设置

- **Files or discovery point:** `src/routes/index.tsx`、新练习设置路由、`src/features/practice/PracticeSetup.tsx`、`src/routeTree.gen.ts`。
- **Change:** 将既有练习设置逻辑搬到独立路由；首页用两张大的 Astryx 卡片表现“练习”和“考试”，在下方用三个视觉优先级较低的按钮导航至错题本、收藏夹和导入单词本。首页不展示任何设置项。实施 UI 前按项目规范运行 `bunx astryx build`，阅读 `bunx astryx docs layout`、`bunx astryx docs tokens`、所用组件的 `component` 说明；确认卡片的点击方式与可访问名称。更新生成路由树。
- **Outcome:** 首屏能清楚选择五个去向；具体练习/考试模式设置只在各自路由呈现，空词本时入口仍可用。
- **Verification:** 路由交互测试验证两张卡片和三个按钮分别导航至正确页面、键盘可操作，首页没有设置项。

### 2. 接入独立导入路由

- **Files or discovery point:** `src/routes/import.tsx`、`src/features/wordbooks/WordbookImportFlow.tsx`、`src/features/wordbooks/WordbookProvider.tsx`。
- **Change:** 在独立路由承载已有导入按钮及对话框，导入成功后刷新词本；提供返回入口，取消文件选择或导入失败时仍停留在可重试的页面。后续调整：练习设置不再显示导入按钮，仅保留回首页的路径。
- **Outcome:** 首页“导入单词本”先进入明确的导入页面，现有文件/命名/替换操作完整复用。
- **Verification:** 测试首页→导入页、取消、成功刷新、同名替换确认、失败重试和无词本进入练习后的导入可达性。

### 3. 整理返回与守卫

- **Files or discovery point:** `src/routes/exam.tsx`、`src/routes/summary.tsx`、`src/routes/favorites.tsx`、`src/routes/mistakes.tsx`、`src/routes/import.tsx`、`src/App.tsx`，以及返回按钮所属组件。
- **Change:** 核对所有原先指向 `/` 的设置返回路径：按按钮语义导向新练习设置页或首页选择页；保持已启动考试和练习的离开保护，失去内存中的练习轮次时转回设置而非进入作答空白页。避免触及未完成的考试业务变更。
- **Outcome:** 从导入、词库和考试设置可回首页，练习结算返回设置仍有明确去向，没有误触发开始或丢失进行中作答。
- **Verification:** 覆盖考试设置返回、练习结算返回、导入/集合浏览返回、刷新/直达练习作答路径及离开保护。

### 4. 更新测试与静态验证

- **Files or discovery point:** `src/App.test.tsx`、`src/features/wordbooks/WordbookImportFlow.test.tsx`、路由相关测试、路由生成配置。
- **Change:** 将旧测试中“打开 `/` 立即见到练习设置”的前提改为先选择练习；保留既有练习、考试、导入回归断言，新增首页五入口与独立导入路由测试。运行 `bun run test`、`bun run check`、`bun run build`。
- **Outcome:** 路由分离与入口视觉层级可验收，不改变原有练习/考试/导入规则。
- **Verification:** 测试与类型、构建通过；手工确认桌面/小屏两张大卡片、三个次要按钮的布局与可操作性。

## Edge cases

| Condition | Expected behavior | Owning step or verification |
| --- | --- | --- |
| 尚无词本但收藏或错题仍有内容 | 五个入口仍可访问；进入各页面后沿用现有来源可用性规则 | 步骤 1、4 |
| 无词本且集合也为空 | 首页导入按钮可用；练习设置提示无可练习单词并提供返回首页按钮 | 步骤 1、2、4 |
| 文件选择取消、导入失败或同名冲突 | 保持在导入路由并沿用现有错误/确认机制，可重试或返回首页 | 步骤 2、4 |
| 直接访问 `/practice` 而无进行中轮次 | 守卫转至有效设置界面，不出现空白作答页 | 步骤 3、4 |
| 考试答题或练习进行中尝试离开 | 仍遵守原来的离开限制与错题保存保护 | 步骤 3、4 |
| 从结算页面点击“返回设置” | 到达练习设置；从收藏/错题/导入页面返回能回首页 | 步骤 3、4 |

## Risks and open questions

- **Risk:** 当前 `src/routes/index.tsx` 和考试相关文件已有未提交改动；修改时只迁移入口/导航相关逻辑，先核对本地差异，避免覆盖正在进行的考试功能。
- **Implementation discovery:** 现有返回按钮有不同的文案与语义；逐处核对后选择入口或练习设置路径，不把路径建议当作新的领域要求。
