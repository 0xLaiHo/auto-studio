# 历史调研：Supabase 架构可行性（已被取代）

> 原调研日期：2026-08-21  
> 当前状态：**Superseded by [ADR-0001](../adr/0001-local-first-byok-desktop.md) and [ADR-0002](../adr/0002-independent-local-core-service.md)**  
> 本文不再代表 Auto Studio 当前技术架构。

## 1. 原问题

在 Auto Studio 仍采用 Cloudflare Web/SaaS 架构时，本报告比较了三种 Supabase 用法：

1. 只使用 Supabase PostgreSQL；
2. PostgreSQL + Supabase Auth；
3. PostgreSQL + Auth + Storage + Realtime 全栈。

当时推荐的是“Supabase PostgreSQL 为核心、Auth 可选，继续使用 Cloudflare Workflows 和 R2”，而不是 Supabase 全家桶。

## 2. 当时的推荐拓扑

```text
Hono on Cloudflare Workers
  ├─ Hyperdrive (query cache disabled)
  │    └─ Supabase PostgreSQL Direct connection
  ├─ Cloudflare Workflows
  ├─ R2
  └─ optional Supabase Auth
```

主要证据：

- Supabase 提供完整 PostgreSQL，适合事务、行锁、JSONB 和 Agent 执行账本。[Supabase Database](https://supabase.com/docs/guides/database/overview)
- Cloudflare 官方要求 Hyperdrive 连接 Supabase Direct connection，而不是再连接池化端点形成双池。[Cloudflare Hyperdrive + Supabase](https://developers.cloudflare.com/hyperdrive/examples/connect-to-postgres/postgres-database-providers/supabase/)
- Hyperdrive 默认查询缓存不适合权限、预算、Agent 状态和写后读取，权威状态需要关闭缓存。[Hyperdrive Query Caching](https://developers.cloudflare.com/hyperdrive/concepts/query-caching/)
- Supabase Auth JWT 可以通过项目 JWKS 验证，但产品权限仍需独立维护。[Supabase JWT](https://supabase.com/docs/guides/auth/jwts)
- Supabase Storage 支持 TUS/S3-compatible，但数据库备份不包含实际 Storage 对象，且同时使用 R2 会形成双媒体事实源。[Supabase Storage S3](https://supabase.com/docs/guides/storage/s3/compatibility)、[Supabase Backups](https://supabase.com/docs/guides/platform/backups)
- Realtime Postgres Changes 不应替代持久事件和任务恢复。[Supabase Realtime](https://supabase.com/docs/guides/realtime/subscribing-to-database-changes)

## 3. 为什么不再采用

新的产品基线不需要 Auto Studio 云端关系库或身份系统：

- Project Package 是本地事实中心；
- SQLite 保存 Project、Asset、Agent Run、Job、Event 和 Export；
- Provider Credential 保存在设备安全存储；
- 用户不需要 Auto Studio 云端账户即可使用核心产品；
- 媒体保存在本地 Project Package；
- Agent Run 由本地 Runtime 执行，应用关闭后停止并在重开时对账。

因此 Supabase PostgreSQL、Auth、Storage、Realtime 和 Edge Functions 均不进入当前 MVP。继续引入它们会重新带回多租户、数据地域、连接、云端备份和用户账号复杂度。

## 4. 可迁移的研究结论

| 原结论 | 当前继承方式 |
|---|---|
| 完整事务适合 Agent 状态账本 | 本地 SQLite transaction 承载单机 Project 状态 |
| Auth 身份与产品权限应分离 | Provider 身份只建立 Connection，不成为 Project 所有权 |
| 媒体与结构化状态分离 | Project Package 文件与 SQLite 分离 |
| Realtime 不是可靠任务队列 | Local Job Runner 以数据库状态和 external job 对账 |
| Storage 备份与数据库备份不同 | Project backup 必须覆盖 SQLite 和整个资产目录 |
| 云查询缓存不适合权威状态 | 本地 Project Session 直接读取 SQLite 权威状态 |

## 5. 当前替代方案

```text
Electron Desktop
  ├─ local Project SQLite + Drizzle
  ├─ local Project assets
  ├─ OS-backed Credential Vault
  ├─ Local Agent Runtime + Job Runner
  └─ user-configured Provider APIs
```

现行设计见：

- [产品设计文档](../product/ai-creative-agent-product-design.md)
- [技术设计文档](../design/auto-studio-technical-design.md)
- [当前 Roadmap](../roadmap.md)
- [本地优先 BYOK ADR](../adr/0001-local-first-byok-desktop.md)

## 6. 重新评估 Supabase 的触发条件

只有出现明确且已验证的可选云需求，才重新调研：

- 跨设备 Project Library 或云备份；
- 团队账户、共享项目和中心化权限；
- 用户自愿上传的质量评测数据；
- 许可证或付费权益需要账号，但仍不能阻止本地 Project 打开；
- 云能力可以通过 Sync/Auth Adapter 引入，而不会污染 Project Session Interface。

重新评估时必须使用届时的官方文档、价格和数据条款；本历史报告只记录曾经的决策过程。
