# 历史调研：ORM 与 Cloudflare 数据库路线（已被取代）

> 原调研日期：2026-08-21  
> 当前状态：**Superseded by [ADR-0001](../adr/0001-local-first-byok-desktop.md) and [ADR-0002](../adr/0002-independent-local-core-service.md)**  
> 本文不再代表 Auto Studio 当前技术架构。

## 1. 原问题

在 Auto Studio 仍被定义为托管 Web/SaaS 产品时，需要在 Hono + Cloudflare Workers 下选择关系数据库、ORM、持久任务和媒体存储。原候选包括：

- Cloudflare D1 + Drizzle；
- 外部 PostgreSQL + Hyperdrive + Drizzle；
- Cloudflare Workflows 作为 Agent Run 执行驱动；
- R2 保存媒体；
- Prisma、Kysely 与裸 D1 Binding 作为 ORM/访问替代。

## 2. 当时的结论

当时的有条件路线是：

```text
Hono on Workers
  ├─ Drizzle + D1       产品与 Agent 状态
  ├─ Cloudflare Workflows
  ├─ R2                 音视频和大型结果
  └─ external media worker + FFmpeg
```

主要判断：

- Drizzle 同时支持 D1 和 PostgreSQL 路线，SQL 透明度适合 Agent 状态账本。[Drizzle D1](https://orm.drizzle.team/docs/sqlite/connect-cloudflare-d1)、[Cloudflare Drizzle + Hyperdrive](https://developers.cloudflare.com/hyperdrive/examples/connect-to-postgres/postgres-drivers-and-libraries/drizzle-orm/)
- D1 `batch()` 可提交预构造原子语句，但不等同于 PostgreSQL 的交互式事务；Agent Run、预算和 Outbox 必须先做并发与故障 Spike。[D1 batch](https://developers.cloudflare.com/d1/worker-api/d1-database/#batch)
- Prisma 的 D1 事务限制不适合作为执行账本默认方案。[Prisma D1 transactions](https://www.prisma.io/docs/orm/v6/overview/databases/cloudflare-d1#transactions-not-supported)
- Workflows 可承载持久 Step、等待和重试，但不能替代产品状态与外部副作用幂等。[Cloudflare Workflows](https://developers.cloudflare.com/workflows/)
- R2 适合音视频对象，关系数据库只保存 metadata、hash 和对象引用。[R2 consistency](https://developers.cloudflare.com/r2/reference/consistency/)

这些结论在“托管 SaaS”前提下仍具有历史参考价值，但该前提已经失效。

## 3. 为什么不再采用

产品已经转为本地优先 BYOK 桌面工作站：

- Project、Asset、Agent Run、Job 和 Export 保存在用户设备；
- 用户使用自己的 Provider Credential 直接调用外部模型；
- Auto Studio 不建设云端账户、多租户数据库、对象存储、生成计费或持久任务平台；
- 应用关闭后停止本地执行，重新打开时通过 Provider external job 对账；
- 媒体保存在 Project Package，而不是平台 R2。

因此 Hono、Workers、D1、Hyperdrive、Workflows、Queues 和 R2 都不属于当前 MVP 核心架构。继续实现这些能力会把精力从内容质量、桌面恢复、Credential 安全和专业导出转移到一个已被取消的 SaaS 控制面。

## 4. 可迁移的研究结论

旧调研仍保留以下通用约束，并已转入当前技术设计：

| 原结论 | 本地架构中的继承方式 |
|---|---|
| 媒体不进入关系数据库 | SQLite 保存 metadata，Project Package 保存媒体文件 |
| 外部调用必须幂等和可对账 | Local Job Runner + external job ref + Unknown Outcome |
| 业务状态和事件原子提交 | SQLite transaction + `project_events` |
| ORM 不能替代数据库语义验证 | Drizzle + SQLite 故障与 Migration Spike |
| 大型响应只保存引用和 hash | 本地 response 文件 + SQLite relative path/hash |
| Provider 临时 URL 不是资产 | 下载、probe、hash 后才提交 Asset Version |
| Export 不应重新生成源媒体 | 本地 immutable Asset Version + staging Export |

## 5. 当前替代方案

```text
Electron Desktop
  ├─ Project Session
  ├─ Drizzle + local SQLite
  ├─ Project Package filesystem
  ├─ Local Agent Runtime + Job Runner
  ├─ BYOK Provider Adapters
  └─ FFmpeg / ffprobe
```

当前持久化选择是嵌入式 SQLite，而不是“无数据库 JSON”。SQLite 不依赖外部服务，并能提供 Agent Run、Job、Selection、Event 和 Migration 所需的事务与约束。

完整现行方案见：

- [技术设计文档](../design/auto-studio-technical-design.md)
- [当前 Roadmap](../roadmap.md)
- [本地优先 BYOK ADR](../adr/0001-local-first-byok-desktop.md)

## 6. 重新评估 Cloudflare 的触发条件

只有出现以下经过验证的产品需求，才重新打开本路线：

- 用户愿意为跨设备项目同步和云备份付费；
- 多人实时协作成为核心使用场景；
- 用户明确要求应用关闭后继续执行 Agent Run；
- 需要团队级权限、中心化审计或统一 Provider 计费；
- 本地 Project Package 已验证成功，云能力可以作为可选 Adapter 而不是核心前置。

届时必须重新调研当前平台能力和价格；本历史报告不能直接作为新的采用依据。
