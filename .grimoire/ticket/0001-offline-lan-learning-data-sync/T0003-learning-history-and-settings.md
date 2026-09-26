# 错题复习与设置同步

**Ticket ID:** T0003
**Source:** [Spec 0004](../../spec/0004-offline-lan-learning-data-sync.md) §Requirement 1–3；[ADR 0003](../../adr/0003-preserve-mistake-records-independent-of-wordbooks.md)、[ADR 0004](../../adr/0004-schedule-practice-with-coverage-and-due-review.md)、[ADR 0006](../../adr/0006-keep-exam-scoring-separate-from-practice-review.md)、[ADR 0009](../../adr/0009-support-offline-multi-device-lan-sync.md)
**Status:** Done

## Goal

离线设备交换错题提交、练习调度与完成、复习目标更改后，错题次数、每来源覆盖、每配对/方向的记忆状态与设置一致，且不重复计算学习结果。

## Affected Surfaces

- **学习持久化：** `src-tauri/src/wordbooks/repository.rs` 的错题/提交记录，`src-tauri/src/wordbooks/schedule.rs` 的覆盖、完成反馈与复习目标。
- **练习与考试命令：** 原有练习结果、考试错题的写入和去重契约；不改变前端未结束轮次的本机状态。
- **重放接口：** T0001 的变更记录与学习事件发生时间，不依赖两台设备的系统时间来决定操作因果。

## Approach

在原有业务写入边界持久记录可去重的错题提交、练习调度/完成及目标设置变更；交换原始事件而非相加现成错题计数或覆盖现成记忆稳定度。按既有分来源覆盖、同一英语/中文配对同方向跨来源共享记忆、考试错题不改变复习的规则重放。保留实际学习发生时间供间隔计算；并发时按已确认稳定顺序收敛。避免新设备重放时复用本地临时 token 或重复提交身份造成学习次数重复。

## Dependencies and Coordination

- **Blocked by:** T0001，学习事件要有可持久去重的身份、顺序及原子应用接口。
- **Blocks:** T0005，完整学习数据交换验收必须包含错题、复习进度和目标设置。
- **Coordination risks:** 与 T0002 共享仓储文件及配对身份，与 T0005 共享完成后的界面刷新时机；联测保证一次练习的错题和复习写入都按原有语义生效。

## Acceptance

- [ ] 两个空库设备各自离线完成相同/不同词义配对的题目并交换学习事件，两端错题累计与练习复习状态一致；重复传输、重启或重复完成不再增加次数。
- [ ] 来源覆盖分别维护，同一配对同方向共享记忆，反方向分离；不同中文释义不误并，跨词书替换/删除后收藏及错题历史独立保留。
- [ ] 考试中的错误和空题仍按原有规则各记一次错题但不更新复习；直接跳过不额外计错，练习完成与错题提交保留原有区分。
- [ ] 离线各自修改复习目标后按确定性重放两端设置一致；事件发生时间参与复习间隔计算，时钟偏差不破坏操作因果顺序。
- [ ] 当前选中的词书及未结束的练习轮次不因学习历史同步而被远端替换；已有调度与考试测试继续通过。

## Out of Scope

- 同步进行中的练习会话，或从旧数据库汇总状态重建过去学习事件。
