# 词书词条与收藏同步

**Ticket ID:** T0002
**Source:** [Spec 0004](../../spec/0004-offline-lan-learning-data-sync.md) §Requirement 1–3；[ADR 0001](../../adr/0001-store-imported-wordbooks-in-sqlite.md)、[ADR 0002](../../adr/0002-preserve-favorites-independent-of-wordbooks.md)、[ADR 0008](../../adr/0008-keep-practice-entry-deletion-alongside-central-management.md)、[ADR 0009](../../adr/0009-support-offline-multi-device-lan-sync.md)
**Status:** Done

## Goal

从全新数据开始，不同设备对词书、词条和收藏作离线修改后，可通过交换可重放操作收敛，同时不破坏导入替换、删除及收藏独立生命周期。

## Affected Surfaces

- **词书仓储/导入：** `src-tauri/src/wordbooks/repository.rs` 与既有 Excel 导入命令；词书及词条操作的稳定目标身份。
- **收藏持久化：** 收藏增删与词义配对身份，不随源词书删除/替换而消失。
- **数据契约：** 与 T0001 的原子变更及重放接口对接；既有 Tauri 词书命令行为保持可用。

## Approach

将导入（包括同名确认替换）、整书删除、词条删除、收藏增删纳入变更记录与重放路径。按已有英文规范化值和中文释义保持词条/收藏的配对语义，但不能将本机行号误当跨设备稳定身份；对并发替换、修改和删除遵循已确认的确定性重放规则。复用既有 Excel 解析与确认约束，保持收藏从原词书独立存续；操作失败与记录失败一起回滚。

## Dependencies and Coordination

- **Blocked by:** T0001，业务写入必须消费其持久身份、原子记录和重放接口。
- **Blocks:** T0005，完整数据同步端到端验收必须包括词书、词条和收藏。
- **Coordination risks:** T0003 可能触及相同 `repository.rs`；保持操作身份及重复应用契约一致，但可并行处理各自行为。

## Acceptance

- [ ] 双库从空数据开始分别导入词书、收藏不同配对并交换操作后，两端词书与词条一致，收藏仍独立于词书存在。
- [ ] 同名词书替换仍需原有确认且原子生效；词书或词条删除不会误删收藏/错题，重放后两端结果一致。
- [ ] 对同一词书/词条的并发修改、替换与删除按确定性重放结果收敛；不因本地自增行号不同而作用到另一词条。
- [ ] 收藏重复添加/移除、变更重复接收与失败回滚不会产生重复成员或不一致记录；既有导入和词书管理行为测试继续通过。

## Out of Scope

- 错题、复习事件与目标设置的同步；首次配对时合并功能发布前两份旧库。
