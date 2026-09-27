# 收藏夹移除按钮简短显示

- **Input:** 对话确认：收藏页面每项按钮只显示“移除”，点击仍移除对应收藏。
- **Status:** Proposed

## Summary

仅调整 `FavoritesList` 的按钮可见文字；保留带词义的无障碍名称和现有点击、加载、禁用行为。

## Implementation steps

1. 在 `src/features/favorites/FavoritesList.tsx` 中保留按钮的完整 `label` 作为可访问名称，使用 Astryx Button 的 `children` 将可见文字覆盖为“移除”。不修改收藏数据逻辑。
2. 针对两个不同收藏项渲染组件测试，检查可见文字一致、可访问名称能区分词条，点击仍传入对应项。

## Verification

- 运行针对组件的测试、类型检查与范围内格式检查。
- 审阅差异，确认无其他页面或持久化逻辑变化。

## Risks

- 同名按钮不易区分：保留现有完整 `label` 供辅助技术使用；测试验证显示文字和可访问名称各自正确。
