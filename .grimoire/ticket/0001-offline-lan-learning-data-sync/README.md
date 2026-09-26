# 局域网离线学习数据同步 · 票据关系

**Source:** [需求规格 0004](../../spec/0004-offline-lan-learning-data-sync.md)；[ADR 0009](../../adr/0009-support-offline-multi-device-lan-sync.md)、[ADR 0010](../../adr/0010-discover-lan-peers-with-mdns-and-display-pairing-code.md)
**Ticket folder:** `.grimoire/ticket/0001-offline-lan-learning-data-sync/`

## Overview

从新数据开始，让各设备离线独立学习，首次核对配对码后在 LAN 相遇时安全地自动交换、按因果及确定性并发顺序重放变更；所有持久学习数据收敛，设备本地的当前词书和未完成练习不迁移。不自动删除旧数据，不声称可合并功能发布前的历史汇总。

## Delivery Surfaces

- Rust/SQLite：`src-tauri/src/wordbooks/repository.rs`、`schedule.rs` 中的持久写入、变更记录、稳定身份及复习状态。
- 桌面边界：`src-tauri/src/lib.rs` 的同步服务和 Tauri 命令、`src/data/wordbooks.ts` 与同步界面的状态/配对操作。
- 局域网：mDNS 发现、Noise 加密配对、已授权对端的增量交换与重连。
- 验证：Rust/SQLite 双端与三端重放测试、桌面配对/离线/重连演练。

## Dependency Graph

| Ticket | Blocks | Concrete reason |
| --- | --- | --- |
| T0001 | T0002, T0003, T0005 | 词书与学习结果需要持久变更身份、顺序、去重和重放约定；交换也需读取并应用此约定。 |
| T0002 | T0005 | 完整学习数据交换验收需要词书、词条和收藏的可重放写入。 |
| T0003 | T0005 | 完整学习数据交换验收需要错题、复习和设置的可重放写入。 |
| T0004 | T0005 | 未经安全配对与授权不得连接并交换数据。 |

## Coordination Risks

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0002, T0003 | 可能共同修改 `repository.rs`、`schedule.rs` 和操作格式 | 先遵循 T0001 的记录/重放契约，各自提交可测试的行为；共享文件按模块职责协调合并，不视为业务阻塞。 |
| T0004, T0005 | 共享 Rust 同步服务、Tauri 状态与前端入口 | 对齐已确认设备身份、首次确认及连接状态契约；T0004 先保证鉴权，T0005 再接交换。 |
| T0002, T0003, T0005 | 同一练习动作可能同时涉及错题与复习记录 | 同一原始操作保持原有事务/去重语义；端到端测试核对两种结果不会重复。 |

## Parallel Candidates

- T0004 与 T0001、T0002、T0003 无行为前置依赖，可单独验证发现、配对及拒绝未授权连接。
- T0002 与 T0003 在 T0001 契约可用后可并行，但需协调共用持久化文件。

## Recommended Order

T0001 和 T0004 可并行启动；T0001 后完成 T0002、T0003；T0002、T0003、T0004 均验收后整合 T0005。该顺序不引入额外依赖。

## Coverage

| Spec 0004 requirement | Tickets |
| --- | --- |
| 1. 离线修改、相遇自动同步、无常在线主机 | T0001、T0002、T0003、T0005 |
| 2. 全部持久学习数据及本机状态边界 | T0002、T0003、T0005 |
| 3. 因果/并发排序、重放、时间戳、去重 | T0001、T0002、T0003、T0005 |
| 4. mDNS、Noise、配对码、明确配对、不传递信任 | T0004、T0005 |
| 5. 从全新数据开始、不自动清除旧数据 | T0001、T0005 |

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [持久变更与确定性重放](./T0001-durable-change-replay.md) | 可测试的离线变更、因果排序与幂等重放边界 |
| T0002 | [词书词条与收藏同步](./T0002-wordbooks-and-favorites.md) | 内容操作与收藏跨设备稳定收敛 |
| T0003 | [错题复习与设置同步](./T0003-learning-history-and-settings.md) | 学习事件、复习调度及目标设置保持语义并收敛 |
| T0004 | [局域网发现与安全配对](./T0004-lan-discovery-and-pairing.md) | mDNS、Noise、首次配对码和明确授权的设备关系 |
| T0005 | [自动交换与端到端同步](./T0005-automatic-peer-sync.md) | 已配对设备相遇时自动安全交换、断线恢复与界面反馈 |
