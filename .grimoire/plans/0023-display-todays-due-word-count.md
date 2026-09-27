# 展示今日待复习词数

- **Input:** `.grimoire/spec/0005-display-todays-due-word-count.md`、`.grimoire/CONTEXT.md`、ADR 0004/0006
- **Date:** 2026-09-27
- **Status:** Completed

## Summary

在 SQLite 复习持久层基于当前单词本成员和方向性到期时间查询今日待复习的不同英文—中文组合数量，并通过现有 Tauri/service 边界供首页和练习设置使用。中等风险：跨 Rust、TypeScript 与两处 UI，但不改 schema 或复习调度。

## Source intent and acceptance

- 首页汇总所有单词本，练习设置只看选定单词本；收藏夹、错题本不参与（步骤 1、2；Rust 和页面测试）。
- 已逾期或今日结束前到期、且已有完成复习记录的组合计数；未练词不计数。两个方向到期或跨词本重复仍只计一次；不同释义分别计数（步骤 1；可控时间 Rust 测试）。
- 练习设置数字不随练习方向变化（步骤 2；页面测试）。

## Design impact

### 复习持久层计数
- **Action:** Modify
- **Location:** `src-tauri/src/wordbooks/schedule.rs`，`src-tauri/src/wordbooks/mod.rs`，`src-tauri/src/lib.rs`
- **Responsibility:** 复用 `entries` 与 `review_memory` 的身份/到期事实，查询全部词本或指定词本的去重数量。
- **Relationships:** 保持与现有复习结果及同步投影使用同一数据库视图；只读，不调用会创建 pending review 的调度请求。
- **Rationale:** 把去重和到期判断放在已有持久层，避免前端复制数据与时序逻辑；以可控截止时间测试。

### 前端服务与展示
- **Action:** Modify
- **Location:** `src/data/wordbooks.ts`，`src/routes/index.tsx`，`src/routes/practice-setup.tsx`，`src/features/practice/PracticeSetup.tsx`，测试用服务桩。
- **Responsibility:** 提供计数调用并在两处展示正确范围的数值。
- **Relationships:** 沿用 route context 的 WordbookService、词本选择状态；使用 Astryx 组件和 token，不添加原生布局元素。
- **Rationale:** 把 UI 的范围切换与现有页面生命周期绑定；不影响练习抽题。

## Implementation steps

1. 在 Rust 增加只读计数查询与 Tauri command：对当前词本成员与已到期的任一方向 review_memory 求不同配对数，传入今日截止值；全部/指定词本查询均支持。用可控时钟的仓库测试覆盖边界、跨词本/方向去重、未练、无词本和无关来源。
2. 在 WordbookService 添加调用，首页与练习设置页各自请求范围；练习设置切换词本时更新而切换模式不改变值；对异步失败与过期结果沿用相邻页面的处理模式。通过页面测试验证。
3. 跑受影响 Rust/前端测试、静态检查；review 与 intent check 后修复阻断项，最后做集成验证与定向格式化。

## Edge cases

| Condition | Expected behavior | Owning step |
| --- | --- | --- |
| 跨词本相同配对和双方向同时到期 | 首页一次；指定词本内一次 | 1 / Rust 测试 |
| 已抽到但未完成、或尚无 review_memory | 不作为已到期复习词计数 | 1 / Rust 测试 |
| 切换词本时旧查询较晚返回 | 不覆盖新选择的数字 | 2 / 页面测试 |
| 无词本或无到期词 | 显示零 | 1、2 / 测试 |

## Verification strategy

- Rust 定时边界/当前成员/去重与不改变数据库状态的仓库测试；同步开启下共享投影的读取路径检查。
- Vitest 页面用 WordbookService 测试桩覆盖首页、词本切换、模式变化、无词本及请求失败。
- `cargo test`（相关测试/全量可行时）、`bun run test`、`bun run typecheck`、`bun run check`，检查完整 diff 与格式。

## Revision log
- **2026-09-27:** 独立代码审查发现页面驻留期间同步更新或跨越本地午夜会使显示过期；计数查询现在订阅 `syncedDataVersionAtom` 并在下一个本地午夜重新请求，新增确定性 hook 测试。
