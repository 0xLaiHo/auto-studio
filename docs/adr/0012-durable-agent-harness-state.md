---
status: accepted
date: 2026-08-24
---

# 分离推理记录、Provider 连续性状态、授权凭据与运行预算

> 实施状态（2026-08-27）：规范化 Transcript、完整 Tool pair、SSE partial-call assembler、固定 Planning 多轮链路、crash-safe resume 与 CM-2 Continuity Planning slice 已实现。当前 Vault 使用 Project 外的 XChaCha20-Poly1305 密文、独立本地密钥、精确 binding、TTL、启动/周期清理；OpenAI Responses 与 Anthropic Messages contract fixture 已证明捕获和原样回传，`gpt-5-mini` 也已通过两轮真实 Continuity Planning 测试。CM-3/CM-4 已实现 automatic compaction/spill/overflow recovery 与 Run 内 source-linked retrieval。Approval Grant / Run Budget machine slice 现已实现不可变精确 binding、独立 system ceiling、累计 ledger、Execution Reservation/settlement/cancel、稳定三类拒绝、SQLite CAS、故障零发布、重启恢复和损坏失败关闭；等待 Creator、程序退出和跨日暂停不计入 active wall-clock。当前 Planning composition root 尚未签发 Grant，通用 AgentStep/ToolExecution、Music Project 独立 revision、统一 `Needs Attention` 投影、Anthropic exact-model live、OS Credential Vault、Provider-specific tokenizer/overflow live 与真实音乐 Tool 长 Run 质量仍未完成。本 ADR 的“背景”保留决策发生时的历史上下文。

## 背景

Auto Studio 已交付 LLM Connection、Model Catalog、Thinking Level 与一次 typed Planning Turn，但尚无通用 `Turn`、`Message`、`ToolCall`、Tool Registry 或多轮 Tool loop。现有设计要求“不持久化 private reasoning”，同时又要求 Agent Run 可暂停和恢复。

这两个要求在 Provider 的连续工具调用协议上存在直接冲突：部分 OpenAI Responses 推理项和 Anthropic extended thinking block 必须在后续请求中按协议继续携带；如果只保存 Creator 可见消息和 Tool Result，同一条工具调用链可能无法继续；如果把完整 Provider payload 混入 Project Event、日志或聊天，又会扩大敏感数据暴露面，并让 provider-specific wire format 污染领域模型。

现有 `CostApproval` 只表达货币上限，也不能表示“只允许在 revision 42 新增 4 条轨道并渲染 3 次 Preview”。把 Creator 的同意、系统安全上限和单工具资源限制压成一个 budget，会产生越权与审计歧义。

## 决策

1. Agent Harness 持久化规范化 `InferenceTranscript`。它由带稳定 identity 和顺序的 `InferenceItem` 构成，至少表达 Creator/Agent 可见消息、完整 `ToolRequest`、对应 `ToolResult`、usage、finish reason 与中断事实。
2. 流式 token、partial JSON 和 partial tool call 只是内存中的组装状态。只有完整、通过 schema 校验的 Tool Request 才能写入 Transcript 并进入 Tool Runtime；中断的 partial call 不猜测、不执行、不补写成工程事实。
3. Provider Adapter 单独拥有 `ProviderContinuityState`。它是 opaque、provider/model/protocol-bound 的 wire payload 或安全引用，Harness 只处理 version、binding、created-at、expiry 和 opaque reference，不解释其中的 reasoning 内容。
4. Continuity State 写入 Project Package 之外的加密 `ContinuityVault`。它不进入 Project Snapshot、Project Event、Run Event 文本、普通日志、可见 Transcript、compaction summary、backup/export 或 telemetry。
5. Continuity State 只在 active Run 内保留。Provider、model、protocol、tool schema fingerprint 或安全策略不兼容时必须失效；`Awaiting Selection`、`Cancelled`、`Failed` 等终态完成最终语义提交后立即 purge。不能 purge 或密文损坏时 Run 进入可见 `Needs Attention`，不得伪装为已安全清理。
6. 只有精确兼容的 Continuity State 才能继续未完成的推理链。缺失或失效时可以从已提交 Transcript 开始一个新的 Inference Turn，但不得把它称为“恢复原 tool-use chain”，也不得重放未完成 Tool Request。
7. `ApprovalGrant` 取代 money-only 授权表达。它绑定 Creator、Project/Revision、Plan 或 Agent Step、Tool Descriptor fingerprint、目标实体/区域、允许的 side-effect class、数量上限、费用上限和失效条件。
8. `RunBudget` 是独立的系统 ceiling，至少限制 turns、tool executions、tokens、cost、累计 active wall-clock、render count、side effects、asset bytes 与并发；Creator 的 Grant 或客户端配置不能提高 host-owned ceiling。等待 Creator、进程退出和跨日暂停不消耗 active wall-clock。
9. `ToolResourceLimit` 仍属于单个 Tool Descriptor。执行前必须按 Approval Grant、Run Budget、Tool Resource Limit 三类独立错误依次检查，并用稳定 identity 创建保守 `ExecutionReservation`；相同绑定重放不重复扣减，CAS 结果丢失后用旧 revision 重放同一 identity 也必须返回已提交状态。完成时只能用不高于预留的实际量结算；仅在 durable ToolExecution 证明从未启动或没有产生影响时才能取消并释放 effects/cost/assets/render/concurrency，ToolExecution 次数仍保持消耗。Unknown Outcome 必须保留预留并先对账。未来 Tool Runtime 必须把 reservation 与 durable `ToolExecution`/receipt 原子关联。
10. `AgentStep`、`InferenceTurn` 与 `ToolExecution` 是不同身份：一个 Step 可以包含一个或多个 Turn；一个 Turn 可以产生零个或多个完整 Tool Request；每个 Tool Request 绑定独立 ToolExecution。它们不能复用 ID 或用聊天顺序推断执行完成。
11. Context compaction 只压缩可重建的对话语义，不改变 Project facts、Transcript 中完整 Tool Request/Result、Approval Grant、budget ledger、ToolExecution receipt 或 Continuity binding。
12. Context Manager 在每次 Provider 调用前，从 canonical instructions/messages/Tool schema 加上 Adapter 提供的 opaque continuity allowance 生成 `RequestFootprint`。在没有精确 tokenizer 的通用边界使用保守且版本化的估算；已知输入预算按 `75% soft / 90% hard / 超预算 overflow` 分级，hard/overflow 不得调用 Provider。
13. 超过固定阈值的 Tool Result 只在 Context Surface 中替换为 source item、hash、原始字节数和有界预览。完整 Tool Result 继续保留在 Transcript，并以 content-addressed spill blob 与对应 Context Manifest/事件同事务提交；revision 冲突必须一起回滚，读取、重启和备份时必须重新校验 hash。
14. automatic compaction 只在完整 Inference Turn 边界选择连续前缀，必须推进上一 cut、不拆 Tool Request/Result、保留本轮新输入并至少保留最近两个 Turn。结构化摘要由 Context Module 确定性生成；只有 prepared surface 实际变短并回到 `Normal` 才可提交。
15. Creator 新输入、Compaction Checkpoint、Context Manifest 与 spill blob 必须由同一 Context journal transaction 原子发布。压缩不调用外部收费服务，因此事务前失败不写 attempt 事实；相同 source facts 的安全重试必须得到相同 checkpoint content hash。
16. Provider 只有返回明确的 context-window 机器码/短语才进入 `ContextOverflow`。Harness 必须先记录 Finish 并清除旧 Continuity，再执行一次必须推进 surface 的恢复；同一 Run 第二次 overflow 必须停止，不能无限重提。
17. Planning 的 `16,384` token context window 是 host-owned 保守 safety ceiling，不是模型能力声明。Provider-specific tokenizer 可以校准 footprint，但不能绕过 hard/overflow、effectiveness 或单次恢复合同。
18. Long-Run Retrieval 只查询同一 Run 的完整 Transcript。第一版支持精确 source item 和本地 FTS5/BM25；不引入跨 Project 人格记忆、向量知识库或远端索引。
19. 每个 Retrieval Hit 必须携带 source item id/type/time、Project revision、内容 hash、可选 Tool execution/error provenance、稳定排名与注入 token 成本。实际 Selection 连同 query fingerprint 和选择原因进入 Context Manifest，并参与 canonical input hash。
20. Retrieval index 是可删除、可从 Transcript 重建的 SQLite projection，不是事实源。current raw tail 与 summary 已显式引用的 source 必须排除；retrieved Creator/Tool 内容在所有 Provider wire 上都映射为 untrusted user context，不能覆盖 system、policy 或 Project facts。

## Considered Options

### 完全不保存 Provider-specific 状态

隐私面最小，但 Provider 要求 continuity 的工具调用链无法可靠恢复，Thinking Level 只在单轮有效。否决。

### 把 Provider payload 原样写入 Transcript 或 Project Event

实现简单，却让敏感/private reasoning 数据进入可见记录、备份和导出，也把 wire protocol 变成领域事实。否决。

### 只保存 provider response ID

对部分协议足够，但不能覆盖 signed/encrypted block、client-side continuation 与协议版本差异。采用“opaque payload 或 reference”的一般形式，由 Adapter 决定精确表示。

### 用一个 Budget 同时表达同意与安全上限

无法区分“Creator 是否同意”和“系统是否允许”，也无法绑定 revision、target 与 tool fingerprint。否决。

## Consequences

- Agent Harness 新增深 Module：`InferenceTranscript` 负责规范化可审计语义，`ContinuityVault` 只负责加密保存和生命周期；Provider Adapter 是唯一理解 opaque payload 的 Adapter。
- TUI/GUI/Web 可以一致显示 Transcript、Grant 和预算使用，但永远不显示 Continuity payload 或 private reasoning。
- 切换 Provider/Model 可能开始新 Turn 并损失原链 continuity，这是可见且可解释的行为，不做隐式协议转换。
- 本地存储需要独立密钥、原子写入、TTL、启动清理和 purge 失败处理；这是 M3 Tool loop 的前置工作，不是后补日志功能。
- 调试时必须依赖规范化 items、hash、receipt 与安全诊断，不能通过记录 private reasoning 解决问题。

## 验证

1. OpenAI 与 Anthropic 协议 fixture 分别证明 continuity payload 可保存、加载、原样回传并在终态 purge；Provider/Model/Protocol mismatch 被拒绝。
2. Continuity payload 的已知 sentinel 不出现在 Project SQLite、Event、Transcript、日志、backup、Export、SSE 或 TUI snapshot 中。
3. partial tool call、损坏密文和缺失 continuity 不产生 Project mutation，也不盲目重试。
4. 一个完整 Tool Request/Result 在重启和 compaction 后仍能从 Transcript 重建上下文，identity 与顺序保持不变。
5. `execution_control_contract` 证明 revision、subject、tool fingerprint、target、effect count、cost、issue/expiry 任一超出 Approval Grant 时请求被拒绝；扩大 Grant 需要新的 Creator action。
6. 即使 Grant 允许，Run Budget 或 Tool Resource Limit 超限仍以不同稳定错误停止；系统 ceiling 不可由 configured budget 提高，相同 inference/reservation/settlement replay 不重复扣减；CAS 结果丢失后，以旧 revision 重放同一 identity 能恢复已提交 revision。
7. Candidate/Cancelled/Failed 的最终语义提交与 continuity purge 具备故障注入测试，清理失败不会被误报为成功。
8. 相同 canonical input 产生相同 footprint/pressure；超过阈值的 Tool Result 在模型视图中变短，但完整 Transcript 和 content-addressed blob 可恢复，篡改 hash 被拒绝。
9. stale Context revision 不留下孤儿 spill；Project backup 恢复后，相同 hash 必须读出相同完整内容。
10. 删除 FTS projection 后重新打开 Project，精确 source query 与内容 hash 必须从 Transcript 重建为相同结果；BM25 命中必须携带完整 provenance 并写入 Manifest。
11. 冻结 machine corpus 必须完成至少 100 个 inference step、10 次 compaction、3 次重启和一次跨日恢复，并召回旧约束、Creator 决定、artifact 与未解决 Tool Result；真实音乐 Tool 正确率另由 Music Project 纵切验证。
12. `execution_control_persistence` 必须证明 SQLite CAS、不同请求 stale revision 冲突、同一请求 ambiguous-commit 重放、拒绝零升版、重启恢复和篡改失败关闭；fault store 必须证明 commit 失败不会发布部分 Grant。该 machine Gate 不冒充尚未存在的 ToolExecution 或 Music Project 写入。

## 关联

- [共同语言](../../CONTEXT.md)
- [产品设计](../product/ai-creative-agent-product-design.md)
- [技术设计](../design/auto-studio-technical-design.md)
- [Roadmap](../roadmap.md)
- [ADR-0011：由 LLM 通过本地工具创作音乐](0011-llm-authored-local-music.md)
