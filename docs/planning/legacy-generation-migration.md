# Legacy Generation 迁移清单

> 基线日期：2026-08-27
> 状态：`LEGACY / FROZEN`
> 适用范围：旧外部 Music Provider、Provider Job、Audio-only Candidate Fixture 路线

## 1. 冻结结论

Auto Studio 的目标执行路径是：真实 LLM 调用本地 Semantic Tool，修改可恢复的 Music Project，再由本地 Rust 路径渲染 Preview。旧 `GenerationAdapter → Provider Job → 下载 WAV` 路线不再属于 production runtime，也不得用于证明产品可以生成音乐。

旧实现暂不删除，因为现有 Project Snapshot、备份与回归合同仍包含这些形状。它只能通过 `autostudio-provider/legacy-generation` 非默认 Cargo feature 编译；`core-daemon`、TUI 和 Desktop production application 均不得启用或暴露该 feature。

## 2. 冻结资产

| 位置 | Legacy 资产 | 当前处理 | 最终迁移目标 |
|---|---|---|---|
| `autostudio-core::agent` | `GenerationAttempt`、`GenerationJob`、`Submitting/Submitted/UnknownOutcome` | 保留反序列化和不变量验证，不新增业务能力 | 新 Run/Step/ToolExecution projection 能读取旧 Run 为只读历史 |
| `autostudio-core::project` | `prepare/record/mark/reconcile_generation_*` 与 Generation events | 冻结写路径，仅供 legacy contract | 新 migration 不把 Provider Job 转换成成功 Tool receipt |
| `autostudio-provider` | `GenerationAdapter`、`GenerationCoordinator`、确定性 WAV fixture | 仅 `legacy-generation` feature；默认构建不可见 | Music Tool 进入版本化 Tool Registry/Policy/ExecutionControl |
| `autostudio-api` | `/execute`、`/refresh`、`/reconcile` 与旧 DTO/OpenAPI 字段 | v1 compatibility 冻结，不再由 TUI/Desktop 调用 | 新版本提供 Agent Step/ToolExecution/NeedsAttention 接口 |
| Provider/API tests | fake generation、known failure、Unknown Outcome reconcile、audio workflow | 只验证旧数据与兼容语义，不计入 production capability | 被 Music Project/ToolExecution/reconciliation 合同替代后删除 |
| `autostudio-storage` tests | Generation lifecycle、旧 Snapshot 恢复 | 保留 backup/reopen compatibility | 增加新 schema migration 后继续验证旧工程可读 |
| TUI/Desktop | 旧 `/generate`、Provider 查询、Unknown Outcome 操作 | 已移除；legacy Run 只读展示 | 展示 Approval Grant、Run Budget、ToolExecution 与 NeedsAttention |

## 3. 冻结规则

1. 不得为 `GenerationAdapter` 增加新的 production Adapter、Provider、模型或配置项。
2. 不得在 `core-daemon`、TUI 或 Desktop production source 中注册或调用 legacy Generation runtime。
3. 不得把 Fixture WAV、legacy Candidate 或 compatibility test 报告为真实音乐生成能力。
4. 旧 `CostApproval` 不能升级、转换或别名为新的 `ApprovalGrant`；两者语义和持久化域不同。
5. 旧 Unknown Outcome 只保留兼容对账合同；新 Tool Runtime 使用 durable ToolExecution、Execution Reservation 与 receipt，不复用 Provider Job 状态机。
6. legacy contract 只允许修复安全、数据可读性和确定性回归；任何产品扩展必须进入 M3-B/M3-C 新路径。

## 4. 删除条件

只有同时满足以下条件才能删除 legacy 代码：

- Music Project schema 与独立 revision 已发布并能从旧 Project Package 安全打开；
- durable ToolExecution、Unknown Outcome reconciliation 与 Candidate Project Snapshot 已替代旧写路径；
- TUI、Desktop 与新 API 不再依赖旧状态或字段；
- 已定义 v1 API/Project schema 的兼容期限和升级失败回滚策略；
- 旧备份 corpus 的 reopen/migration Gate 通过；
- legacy tests 已被等价的新路径合同替代，而不是直接丢失覆盖。

## 5. 架构守护

`scripts/check-crate-boundaries.sh` 同时验证：

- `autostudio-provider` 的 default feature 为空；
- `core-daemon` 未启用 `legacy-generation`；
- production application source 不出现 legacy Adapter/Coordinator 或旧执行、刷新、对账 client command；
- workspace 仍保持 5 个共享 crate 与 3 个 application entry。

默认 production Gate 与兼容 Gate 分开运行：

```text
cargo check -p core-daemon --release
cargo test --workspace --all-targets
cargo test -p autostudio-provider --features legacy-generation \
  --test fake_generation_contract \
  --test generation_failure_contract \
  --test unknown_outcome_reconciliation
```

第二组只表示 legacy compatibility 仍可回归，不改变其 `LEGACY` 状态。
