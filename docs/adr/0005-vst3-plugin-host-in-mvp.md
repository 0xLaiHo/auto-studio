---
status: superseded
superseded_by: 0011-llm-authored-local-music
---

# 在 MVP 中提供隔离的 VST3 Plugin Host

> 历史状态：隔离、Profile、信任和许可边界仍有效；[ADR-0011](./0011-llm-authored-local-music.md) 重新把受限 VST3 路径纳入本地音乐 MVP，但以 Factory Path 先行、一个 OS 和固定 corpus 收敛，未恢复本文原有的完整兼容矩阵。

Auto Studio 将 VST3 Plugin Host 纳入音乐 MVP，使 Creative Agent 能在创作者现有插件生态上完成乐器分配、效果链、参数、自动化、试听、freeze 和正式渲染。插件不是 LLM 可直接调用的函数：模型只能通过版本化 Semantic Tool 与已批准 Plugin Profile 操作 PluginId、InstanceId、preset 和有界参数；VST3 ABI、binary path、原始 state 和文件系统不暴露给模型。

MVP 只支持 VST3，不支持 VST2、AU 或 CLAP。VST2 新宿主存在历史许可限制；AU 会扩大 macOS 专用实现；CLAP 虽适合 Rust，但不能替代目标创作者已持有的 VST3 生态。Auto Studio 自有基础 DSP 仍直接实现为 Audio Engine 节点，不为了内部调用额外包成 VST3。

## Considered Options

- 只提供内置 DSP：实现风险最低，但不能使用创作者现有虚拟乐器和效果器，不满足专业工作站定位。
- 在 Audio Engine 进程内直接加载 VST3：延迟最低，但扫描或运行中的坏插件可拖垮 Core、项目状态和 Agent Run。
- 隔离 VST3 Plugin Host：采用。扫描在短生命周期 Scan Worker 中进行，运行实例进入受监督 Plugin Worker；音频和事件使用固定容量共享内存/ring buffer，控制与状态使用版本化有界 IPC。

## Consequences

- Rust 自有业务代码通过窄 FFI 使用官方 [VST3 SDK](https://github.com/steinbergmedia/vst3sdk) 与 [VST3 C API](https://github.com/steinbergmedia/vst3_c_api)；所有 unsafe ABI、对象生命周期和线程约束封装在 Plugin Host Module 内，不能泄漏到 Audio Graph、Agent Tool 或 Project Domain。
- MVP 支持官方目录扫描、稳定 Plugin UID、乐器/效果分类、audio/MIDI bus、sample-accurate parameter、preset/state、latency/PDC、实时和离线 processing、freeze、缺失插件报告及 Generic Parameter Editor。
- 原生插件 GUI 不是 Agent Tool 的前置条件；首版可作为兼容性增强，但不能阻塞参数、state、渲染和 freeze 闭环。
- User-owned Plugin 由创作者自行安装和授权，Auto Studio 不重新分发。Bundled Plugin 只能是自有插件或取得 installer、更新、离线使用和终端用户分发权的 OEM 内容。
- 只有 Plugin Trust Status 为 Approved 且具有已批准 Plugin Profile 的精确版本可由 Agent 自动实例化和调参；其他已验证插件只能由用户手动使用 Generic Parameter Editor 或 preset。
- Plugin Lock 记录 UID、vendor、版本、binary hash、Profile、state、latency、I/O 与 render mode；缺失或版本不匹配时不得静默替换。
- Plugin Worker crash、hang、超时或 deadline miss 只使相关实例失败或降级，不能破坏 Project transaction；正式输出通过 staged render 和 atomic asset commit 提交。
- 引入 VST3 会扩大三系统构建、签名、GUI、扫描、兼容矩阵、实时调度和许可工作，因此 Phase 0 延长并把固定插件 corpus 设为发布 Gate。

## Acceptance Gates

1. Windows、macOS 与首发 Linux 上至少通过一组固定 VST3 乐器和效果 corpus，明确每个 plugin/version 的 Supported、Limited 或 Quarantined 状态。
2. 扫描畸形、崩溃和挂死插件不会终止 Core；重复扫描使用 binary hash 缓存，升级后重新验证。
3. 48 kHz、128/256 buffer 下，Plugin Worker IPC、参数自动化、MIDI、PDC 和 graph swap 达到 Audio Engine deadline；30 分钟和 8 小时 soak 无不可解释 underrun。
4. preset/state 在关闭项目、重启 Core 和离线 render 后恢复；sample rate、block size、bus layout 和 latency change 可对账。
5. Agent 不能加载任意路径或无 Profile 参数；Tool 只能使用 PluginId、InstanceId、允许的 preset/parameter 和已批准预算。
6. VST3 SDK/C API、商标、第三方插件 EULA、Bundled Plugin OEM 和发行 notice 通过法律与供应链审核；不包含 VST2 SDK 文件。
