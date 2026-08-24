---
status: accepted
date: 2026-08-22
supersedes: ADR-0007 中的 Desktop-first 客户端优先级
---

# TUI 作为默认入口并由 Core 管理 LLM Connection

## 背景

Auto Studio 需要同时保留 TUI、GUI 和未来本机 Web，但 Ship 0 只能维护一个主发布 Client。创作者期望像 pi、OpenCode 一样输入 `autostudio` 直接进入持续交互界面，并在界面内配置自备 Provider Key。要求用户先启动 `core-daemon`、寻找 discovery 文件并设置环境变量，会把实现细节暴露为产品流程。

Provider Credential 又不能进入 Project Package、Run Event、日志、Export、WebView 或可读 API。首发 OS 尚未冻结，因此当前不能诚实宣称已完成跨平台 OS Vault。

## 决策

1. Ship 0 的默认产品入口是名为 `autostudio` 的 Ratatui binary；它是 TUI，不是一次性 CLI。
2. TUI 先读取应用数据目录的私有 discovery record。健康 Core 存在时复用，否则启动同目录或 `PATH` 中的 `core-daemon`。
3. TUI 托管的 Core 绑定动态 loopback 端口，继承独立 Project、runtime 和 LLM Connection 路径，并通过私有 heartbeat 管理生命周期。
4. Core 未配置 LLM Connection 时仍可启动，TUI 不显示强制首次向导。主界面始终保留 Composer；输入 `/` 打开命令 Overlay，`/connect` 打开 Provider 搜索与 Key 输入 Overlay，`/model` 打开模型选择 Overlay。普通文本是 Creative Agent 请求：没有当前 Project 时先创建 `Untitled Project`，再保存首个 Brief 并调用 Agent Plan；已有 Project 时更新 Brief 后调用 Plan。`/brief` 保留为只修改 Brief、不触发推理的显式命令。
5. `/connect` 只完成 Provider 与 write-only Key 保存，不要求同时填写 Model，也不自动选默认 Model。保存成功后关闭 Overlay，由 Core 在后台刷新目录；TUI 投影 `refreshing/ready/failed`，用户再从 `/model` 显式选择。
6. `/model` 使用全屏选择界面：输入文本过滤，上下键移动模型，左右键只切换当前模型 `ThinkingCapability` 中的合法档位。Enter 原子保存 model + level，Esc 丢弃完整草稿；切换模型时恢复该模型偏好，不合法或缺失时使用 capability default。`Provider default` 不等于 `Off`。
7. Core 提供认证的 Provider/Connection/Catalog API：`GET /v1/providers/llm`、`GET|PUT /v1/provider-connections/llm`、`GET|POST|PUT /v1/provider-connections/llm/models`。Key 不可读；状态返回 configured、provider、selected model、per-model Thinking 偏好、source 与含 capability 的非秘密 Catalog snapshot。
8. `autostudio-core` 只定义 `LlmConnectionControl` application interface 和强类型 Thinking 领域模型。`autostudio-provider` 负责 Provider 注册、当前存储、目录刷新、模型/Thinking 选择与协议 encoder；Client 不读取 Connection 文件，也不直接调用 Provider。
9. 当前开发 bootstrap 使用 Project 外的原子私有 JSON：Unix 强制 `0600`，拒绝 symlink、非普通文件、宽权限、超限文件和未知 schema。连接由 `connectionId` 标识，迟到的旧目录刷新不能覆盖新连接；Key value、配置 DTO 和 HTTP adapter 在 drop 时尽量 zeroize，`Debug` 与界面统一脱敏。
10. 私有 JSON 不是加密 Vault。正式分发必须在首发 OS 冻结后，用 Keychain/Credential Manager/Secret Service 实现替换存储后端，保持 `LlmConnectionControl` 和 Client 流程不变。
11. 环境变量只在私有 Connection 文件不存在时作为开发回退，不再是默认产品入口。
12. `/exit` 是命令菜单的一等动作；直接输入 `/exit`、从菜单选择 Exit 或按 `Ctrl+C` 都执行本地安全退出，不产生工程命令或隐藏副作用。

## 结果

- 安装 `core-daemon` 与 `autostudio` 后，创作者只需输入 `autostudio`。
- 创作者首次直接输入创作要求时，不需要先理解或执行 `/new`；Project 仍由 Core 创建并保持为工程事实。
- TUI、Desktop 和未来 Web 继续共享一个权威 Core、Project 状态机和 Provider Adapter。
- 修改 Provider 后无需重启 Core；只有模型目录确认并由用户选择后，下一次 Plan 才使用原子保存的 model + Thinking level。
- Ship 0 的主客户端矩阵从 Desktop 改为 TUI；Desktop 保留为次要开发界面。
- OS Vault、安装器把两个 binary 成对交付、签名和干净机验证仍是正式发布 Gate。

## 不采用

- 在 TUI 进程内保存 Key 或直接调用 Provider：会复制业务边界并绕过 Core 审计。
- 把 Credential 写入 Project Package：会随工程复制、备份和导出。
- 继续要求用户手工启动 Core：不符合单命令产品入口。
- 在首发 OS 未冻结时抽象一个名义上的跨平台 Vault 并宣称安全完成：缺少目标平台验证证据。

## 验证

- 全新应用数据目录下，`autostudio` 自动启动 Core，未配置状态仍停留在可输入的主 Composer；
- TUI reducer 测试证明 `/`、`/connect`、Key 掩码、`/model` 上下选择、按模型左右切换、per-model 恢复、Esc 原子取消，以及 API Key 不进入 `Debug` 输出；
- API 合同证明 Provider list、status/configure、Catalog refresh/select 需要 Core session，响应不包含 Key；
- Provider 合同证明私有配置被下一次真实 HTTP inference 使用；
- Unix 权限测试证明 Connection 文件不向 group/other 开放；
- 正式发布前补 OS Vault、双 binary 安装、升级/卸载和干净机证据。
