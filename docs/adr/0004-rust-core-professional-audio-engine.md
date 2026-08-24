---
status: accepted
supersedes: 0003-typescript-core
---

# 采用 Rust Core 与专业实时音频引擎

Auto Studio 将 Core、TUI、Provider Adapter、Agent Runtime、项目状态、内容目录以及实时和离线 Audio Engine 统一为 Rust。Core 使用 Axum、Tokio、Serde 和 Reqwest 提供本机 HTTP/JSON、SSE 与受限实时控制接口；GUI 采用 Tauri 2 壳层和薄 React/TypeScript Renderer，本机 Web 复用同一客户端契约。Renderer 中不包含权威业务逻辑、Provider 凭证或音频调度，因此不属于后端技术栈。

> 实例化说明：[ADR-0011](./0011-llm-authored-local-music.md) 已取消外部 Music Provider，并把 Music Project、MIDI、Sampler、Factory Pack、Audio Engine 与受限 VST3 路径纳入本地音乐 MVP。TUI 仍是主发布 Client；Desktop 保留为开发界面，本机 Web 后置。

该决策不是为了把 Rust 当作内容质量的替代品，而是因为产品范围已经从“外部生成加简单编辑”扩展为专业音乐制作：需要低延迟播放、sample-accurate MIDI、真实乐器采样器、Mix Graph、确定性离线渲染以及未来的第三方插件宿主。这些能力必须由一个具有明确实时线程模型、资源上界和原生分发路径的音频数据面承载。

“Rust 技术栈”指 Auto Studio 自有业务与音频代码统一使用 Rust，不表示所有依赖都必须用 Rust 重写。FFmpeg、系统音频 API，以及在 Rust sampler 达到质量 Gate 前隔离运行的 sfizz，可以作为固定版本、可审计、可替换的原生运行时。它们不得侵入领域模型或 Agent Tool 合同。

## Considered Options

- TypeScript/Hono Core + FFmpeg/Web Audio：不再采用。它适合 Provider 编排和简单试听，但不能作为专业实时 Audio Engine、采样器和插件宿主的质量与时序保证。
- TypeScript Control Plane + Rust Audio Adapter：不采用。它形成两个业务 Runtime、跨语言状态和打包边界，而当前专业音频已经是核心产品能力，不再是可选 Adapter。
- Rust Core + Rust Audio Engine：采用。Provider REST 接入需要自行维护 schema 和流式协议，但获得单一权威 Runtime、原生 TUI、统一故障模型和可测的实时数据面。
- 完全 pure Rust，包括替换 FFmpeg 和成熟 SFZ 引擎：不采用。短期会牺牲格式兼容和采样表现；只有替代实现通过质量、许可、实时和兼容性 Gate 后才能移除受控依赖。

## Consequences

- Core、TUI、Agent Runtime、Provider Adapter、SQLite 持久化、Content Catalog 和 Audio Engine 使用 Rust；不再保留 Hono、Node Runtime、Drizzle 或 TypeScript Core。
- GUI 使用 Tauri 2 + React/TypeScript，但仅通过版本化 Core API 工作；本机 Web 也是普通 Client Surface。
- Tokio 网络/任务线程与实时音频线程严格隔离。音频 callback 不得访问网络、数据库或文件，不得普通日志、动态分配、等待阻塞锁或调用 LLM。
- Core 仍是独立服务。Audio Engine 首版可在同一 Rust Core 进程内使用专用线程；第三方插件、FFmpeg 和 sfizz 必须处于 worker 或可隔离边界，崩溃不得损坏项目事实。
- SQLite 使用 rusqlite bundled 候选和专用 DB actor；数据库操作不得占用实时线程。
- Provider 官方 Rust SDK 缺口由 Adapter 内的 Reqwest REST/SSE/WebSocket client 承担；供应商类型不能穿透领域或 API。
- SF2 优先验证 rustysynth。复杂 SFZ 可阶段性使用隔离 sfizz worker，同时发展版本化 Auto Studio Instrument Manifest 与自有 Rust sampler；不得宣称未实现的完整 SFZ 兼容。
- 插件隔离和安全边界最初由 [ADR-0005](./0005-vst3-plugin-host-in-mvp.md) 定义；当前交付顺序由 [ADR-0011](./0011-llm-authored-local-music.md) 调整为 Factory Path 先完成本地闭环，再用一个 OS 和固定 corpus 验证受限 VST3 MVP。nih-plug 只可用于开发自有插件，不能当第三方插件宿主。
- FFmpeg 继续负责视频、交付编码和长尾媒体兼容；正式音乐播放、MIDI 调度、采样器与 Mix Graph 由 Audio Engine 负责。
- 真实乐器内容按精确 Pack 版本完成法律、来源和音质 Gate。允许商业成品音乐使用，不等于允许随软件再分发。
- 旧 [ADR-0003](./0003-typescript-core.md) 保留为历史记录，但状态改为 superseded。

## Acceptance Gates

1. 目标 OS 在 48 kHz、128/256 buffer 下通过 callback 时延、underrun 和长时间 soak test；稳态 callback 无 heap allocation 和阻塞 I/O。
2. 同一 Render Plan、engine version、pack hash 和 seed 得到等价离线输出；实时与离线共享 graph semantics。
3. rustysynth 与 SFZ 路径通过音高、包络、loop、RR、articulation、pedal、voice stealing 和专业盲听测试。
4. 每个内置或官方可选 Content Pack 具有不可变许可证、来源、文件 hash、转换记录和批准人；RED 内容不进入 Catalog。
5. Core 恢复、SQLite 事务、ToolExecution 幂等、render/worker 崩溃、插件隔离和项目文件原子提交通过故障注入。
6. Windows、macOS 与首发 Linux 目标完成 Core、Tauri、FFmpeg、内容包与签名分发 Spike；不要求用户安装 Rust、Node 或系统级开发环境。
