# Auto Studio：真实乐器采样、音色内容与 Rust 音频栈调研

> 日期：2026-08-21  
> 决策范围：后端、Agent Runtime 与专业音频引擎统一切换为 Rust；FFmpeg 和必要的非 Rust 音频运行库作为受控依赖；真实乐器采样与音色通过受控 Tool 提供给 LLM。  
> 证据标准：仅采用项目官网、官方仓库、官方 LICENSE/EULA、规范与第一方开发者文档。  
> 法律说明：本文是工程与产品准入调研，不是法律意见。正式商业发布前仍需律师复核最终归档的许可证、素材版本和分发方式。

> 当前决策：本文是目标架构与许可研究快照。[ADR-0011](../adr/0011-llm-authored-local-music.md) 已取消 Music Provider，并把 Music Project、MIDI、Sampler、Factory Pack、Audio Engine 与受限 VST3 路径纳入本地音乐 MVP；具体内容和库仍必须分别通过许可、音质、实时、目标 OS 和分发 Gate。

## 1. 结论

切换到 **Rust Core + Rust Audio Engine** 与“专业实时音乐工作站”的目标一致，但必须同时接受两个事实：

1. Rust 可以统一服务端、Agent Runtime、实时调度、DSP、MIDI、采样器控制和插件宿主的业务代码；它不会自动带来专业级乐器音色。音色质量主要取决于采样录制、velocity layers、round robins、articulations、麦克风位、映射、播放算法和最终混音。
2. “全部切换到 Rust”不等于“依赖必须 100% pure Rust”。FFmpeg 是 C 项目，当前成熟的 SFZ 播放器 `sfizz` 是 C++/C API。若禁止所有非 Rust 依赖，短期内会牺牲长尾媒体兼容性和 SFZ 表现力，不符合“内容质量优先”。推荐把它们作为版本锁定、进程隔离、许可可审计的媒体运行时，而不是业务后端。

首批内容建议：

- **默认内置基础包**：从 VCSL 中挑选的 CC0 真乐器子集；每个文件进入自己的资产清单、哈希和来源记录。VCSL 官方明确允许把声音用于商业软件，但官方也说明多数音色只有一个 round robin 和 2–3 个 velocity layers，因此它适合“可立即演奏的基础音色”，不能作为旗舰级管弦乐写实度承诺。[VCSL 官方说明](https://github.com/sgossner/VCSL/blob/master/README.md?plain=1)
- **可选高质量下载包**：Salamander Grand Piano、VSCO 2 CE 全量或精选包、明确标注 CC BY 4.0 的 DrumGizmo 套件，以及逐包审核后的 FreePats 真乐器。
- **需单独商业合同的旗舰包**：自行组织录音、从权利人直接取得独家或 OEM 再分发授权；或分别与 UVI、Native Instruments、Decent Samples、Steinberg 等洽谈引擎/平台与内容合同。平台许可和声音内容许可必须分开签。
- **不得默认内置**：Philharmonia、Pianobook、University of Iowa、Freesound 批量内容、GeneralUser GS，以及普通购买得到的 Kontakt/Spitfire/UVI/Steinberg 内容。原因不是一定不能用于成品音乐，而是没有足够权利把原始或转换后的采样随 Auto Studio 再分发。

音源技术建议：

- SF2 基础播放优先验证 `rustysynth`；其官方仓库明确为 pure Rust、实时/离线 SoundFont 合成，并采用 MIT 许可证。[rustysynth](https://github.com/sinshu/rustysynth)
- 专业 SFZ 先使用隔离的 `sfizz-worker`；`sfizz` 是 BSD-2-Clause 的 C++ 库并提供 C/C++ API，不能称为 Rust 库。[sfizz](https://github.com/sfztools/sfizz)
- 若产品要求所有依赖也必须 pure Rust，目前没有获得官方证据支持的成熟 SFZ 等价实现；该路线应标记为 **NOT APPROVED**，直到自研 sampler 或候选实现通过兼容性和听感 Gate。
- VST3 宿主若在 Ship 2 准入，使用 Steinberg 官方 SDK/C API 的窄 Rust FFI，并把扫描与运行放入独立 worker；SDK 当前为 MIT，C API 按其仓库内许可证归档，插件 Profile、PDC、state、GUI fallback 和隔离仍需自建。[Steinberg VST3 SDK](https://github.com/steinbergmedia/vst3sdk)、[VST3 C API](https://github.com/steinbergmedia/vst3_c_api)
- `nih-plug` 是插件开发框架，不是第三方插件宿主；原项目处于 maintenance mode，且其现有 VST3 bindings 仍标为 GPLv3，不能直接作为闭源宿主方案。[NIH-plug](https://github.com/robbert-vdh/nih-plug)

## 2. 最重要的许可原则：commercial use 不等于 redistribution

“允许用于商业音乐”通常只表示用户可以把音色演奏并混入一首歌、配乐或成品视频。它**不自动包含**以下权利：

- 把原始 WAV/FLAC/SF2/SFZ 随商业软件安装包交给所有用户；
- 把采样转成另一格式后再分发；
- 把采样封装成新的虚拟乐器、音色库、preset 或 sampler pack；
- 让用户通过 Tool 提取、另存、批量试听或重新打包底层样本；
- 用普通单用户许可证替代 OEM、平台授权或批量终端用户许可。

Native Instruments 的官方 EULA 是典型例子：它允许声音用于商业或非商业音乐作品，同时严格禁止把声音用于创建 sound library、virtual instrument 或 sample-based product，并禁止独立分发或重新打包。[NI EULA](https://www.native-instruments.com/de/support/downloads/open-source-drivers/end-user-license-agreement/)

因此本报告采用“分发权”三分法。颜色表示 Auto Studio 的准入状态，而不是素材本身的音质评分：

| 级别 | 含义 | 产品动作 |
|---|---|---|
| **GREEN：可随软件再分发** | 官方条款明确允许复制/商业再分发，且能落实署名、许可文本和修改说明等义务 | 通过法律 Gate 后可内置或做官方可选包 |
| **YELLOW：仅可用于成品音乐** | 当前公开条款只足以支持把声音用于歌曲、配乐或视频成品，不允许 Auto Studio 提供底层样本；或再分发必须先取得书面/OEM 合同 | 不进入安装包或官方内容 CDN；至多允许用户自行加载其合法持有的本地插件/内容；取得新合同后重新审核 |
| **RED：不批准** | 商业成品或再分发权利没有可验证的第一方证据，来源链不足，或逐文件/ShareAlike 准入尚未完成 | Tool 不发现、不安装、不渲染；完成文件级审核后才能改变状态 |

规则：**证据不足即 NOT APPROVED**。免费、可下载、开源软件附带、网站写有 royalty-free，均不能替代再分发授权。

## 3. 候选库红黄绿矩阵

### 3.1 开放或社区内容

| 候选 | 内容/格式与官方质量信息 | 内容许可 | 商业成品 | 原始或转换样本随商业应用再分发 | 结论 |
|---|---|---|---|---|---|
| **VCSL** | 真乐器为主，官方 SFZ release；多数乐器原始 WAV 约 20–75 MB，通常 1 RR、2–3 velocity layers | CC0 | 允许 | 官方明确写明可做商业软件，无署名要求 | **GREEN**；默认精选基础包首选，但不是旗舰写实度 |
| **VSCO 2 CE** | 管弦乐 SFZ/WAV；官网标注约 3 GB、1,952 samples 加其他内容、基础 articulations | CC0 | 允许 | 官方明确“无规则、版税或限制”，可改成自有格式 | **GREEN**；适合可选完整管弦包或从官方原始 WAV 重建精选包 |
| **Salamander Grand Piano V3** | Yamaha C5；48 kHz/24-bit、16 velocity layers、release/共鸣样本；FreePats 官方页面提供 707 MiB SFZ/FLAC、1.18 GiB SFZ/WAV 等版本 | CC BY 3.0 | 允许 | 允许复制和改编，但必须正确署名、附许可链接并说明修改 | **GREEN WITH OBLIGATIONS**；高质量钢琴可选包首选 |
| **FreePats** | SFZ/FLAC、SFZ/WAV、SF2；逐乐器来源、版本和许可不同 | CC0、CC BY，历史上也有 GPL+exception | 取决于每个包 | 不能对“FreePats 整站”做统一判断；只能逐包准入 | **RED collection / GREEN per approved pack** |
| **MuseScore General** | 官方手册列出 SF3 35.9 MB、SF2 208 MB，定位完整 GM 回放 | 官方手册称 MIT | 允许 | MIT 原则上允许分发，但发布前需要归档确切 artifact 内的许可、贡献者/上游样本清单和哈希 | **RED pending artifact audit**；通过后仅做 GM compatibility pack，不承担旗舰音质 |
| **GeneralUser GS** | 官方页提供 GM/GS SoundFont 下载 | 官方页要求查看包内 LICENSE | 本次未从第一方网页取得可稳定归档的完整条款 | 未取得覆盖精确 artifact、样本来源链和商业应用再分发的充分第一方证据 | **RED / NOT APPROVED** 默认内置 |
| **DrumGizmo 官方套件** | 多麦克风、多通道原声鼓；例如 CrocellKit 5.5 GB、15 channels；DRSKit 13 channels；MuldjordKit 16 channels | 套件页分别标 CC BY 4.0 | 允许 | CC BY 4.0 可再分发，但要按每套官方要求署名、附许可和修改说明 | **GREEN WITH OBLIGATIONS**；逐套件独立下载与署名 |
| **AVL Drumkits** | 官方许可证列 SFZ、SF2、h2drumkit | CC BY-SA 3.0 + 特别例外 | 成品音乐可自由许可 | 可分发；修改样本/制作新库触发 BY-SA，修改版还要改名，必须保留作者信息和 readme | **RED pending pack-level Gate**；通过后可变为隔离的 GREEN WITH SHAREALIKE OBLIGATIONS |
| **Freesound CC0/CC BY** | 单文件格式和质量不统一；API 提供逐文件 license 字段 | 逐文件 CC0/CC BY/CC BY-NC | CC0/CC BY 可商业，NC 不可 | 单个 CC0/CC BY 可能允许，但上传者来源风险仍存在；商业使用 Freesound API 需另行联系许可 | **RED / NOT APPROVED for built-in bulk pack**；逐文件审核后另行准入 |
| **Philharmonia samples** | 标准管弦乐、吉他和打击乐真采样 | 官网自定义条款 | 允许用于商业作品 | 官网明确禁止把它们“as is”出售或作为 samples/sampler instrument 提供 | **YELLOW：仅成品音乐** |
| **University of Iowa MIS** | 官方页称 1997 年起录制；早期为逐音符 16-bit/44.1 kHz、pp/mf/ff，后期含更高规格和多麦克风内容；未给总磁盘规模 | 官网自定义说明称可下载并用于任何项目、无额外限制，但没有标准许可证文本 | 广泛项目使用有官方依据 | 未明确写出把原始/转换样本随第三方商业应用、内容 CDN 再分发和转授权的权利 | **RED pending written redistribution confirmation** |
| **Pianobook** | 社区 sample packs，格式与来源各异 | Pianobook EULA | 可用于符合条款的商业作品 | EULA 明确禁止转售、再分发、转换为另一 sampler 或创建竞争性 sample product | **YELLOW：仅成品音乐**；社区来源风险另做用户提示 |

官方依据：

- VCSL 官方称整个集合为 CC0，目标是用于软件和媒体，并直接写明可制作 commercial software。[VCSL README](https://github.com/sgossner/VCSL/blob/master/README.md?plain=1)
- VSCO 2 CE 官网称 3 GB 素材采用 CC0，提供官方 SFZ 和 44.1 kHz 16/24-bit 原始 WAV，并允许任意使用。[VSCO 2 CE](https://versilian-studios.com/vsco-community/)
- Salamander 的官方 FreePats 页面记录了录音规格、层数、作者、许可和各格式大小。[FreePats Salamander](https://freepats.zenvoid.org/Piano/acoustic-grand-piano.html)
- FreePats 明确说明集合内使用多种许可，同时要求来源已知、原始录音、许可适合再分发；因此只能逐包审核，不能从项目名称推断许可。[FreePats about](https://freepats.zenvoid.org/about.html)、[FreePats licenses](https://freepats.zenvoid.org/licenses.html)
- MuseScore 3 官方手册把 MuseScore General 标为 MIT，并给出 SF2/SF3 大小；这能支持初步判断，但产品仍需保存下载包内部的原始许可证，而不是只依赖网页摘要。[MuseScore SoundFonts](https://musescore.org/en/handbook/3/soundfonts-and-sfz-files)
- GeneralUser GS 的官方页面要求查看下载包内 LICENSE；本次没有从第一方网页取得可稳定归档、覆盖精确 artifact 与样本来源链的完整许可证据。按照“证据不足即 NOT APPROVED”，不得把第三方镜像或口耳相传的许可摘要替代发布审计。[GeneralUser GS 官方页](https://schristiancollins.com/generaluser.php)
- CrocellKit 官方页面给出 5.5 GB、15 channel 和 CC BY 4.0；DRSKit、MuldjordKit 等也分别提供自己的许可与 attribution 规则。[CrocellKit](https://drumgizmo.org/wiki/doku.php?id=kits%3Acrocellkit)、[DRSKit](https://drumgizmo.org/wiki/doku.php?id=kits%3Adrskit)、[MuldjordKit](https://drumgizmo.org/wiki/doku.php?id=kits%3Amuldjordkit)
- AVL 的官方许可要求：修改样本或创建新 sample library 时适用 BY-SA，修改版必须使用不同名称，发行中保留作者信息和 readme。[AVL license](https://bandshed.net/pdf/AVL-Drumkits%20CC-BY-SA%20License.pdf)
- Freesound API 返回逐声音的 license；官方 FAQ 同时警告用户上传可能侵权，API 免费使用只面向非商业用途，商业 API 使用需联系 UPF。[API resource](https://freesound.org/docs/api/resources_apiv2.html)、[FAQ](https://freesound.org/help/faq/)、[API terms](https://freesound.org/docs/api/terms_of_use.html)
- Philharmonia 官网允许商业作品但明确禁止样本或 sampler instrument 再分发。[Philharmonia samples](https://philharmonia.co.uk/resources/sound-samples/)
- University of Iowa 官方页提供了广泛的项目使用表述和录音规格，但缺少标准许可证及针对 installer/CDN、转格式和终端用户转授权的明确条款；在取得权利人的书面澄清前仍不能内置。[Iowa MIS](https://theremin.music.uiowa.edu/MIS.html)、[Iowa post-2012](https://theremin.music.uiowa.edu/MISPost2012Intro.html)
- Pianobook EULA 只给个人、不可转让/再许可的使用权，并禁止原始、转换、处理后素材作为 sampler/sample library 再分发；官网还说明不能保证社区上传内容无版权问题。[Pianobook EULA](https://www.pianobook.co.uk/terms-conditions/)、[Pianobook FAQ](https://www.pianobook.co.uk/faq/are-pianobook-sample-packs-royalty-free-or-free-for-commercial-use/)

### 3.2 商业平台和厂商内容

| 厂商/平台 | 普通终端用户许可 | 官方商业合作证据 | 对 Auto Studio 的结论 |
|---|---|---|---|
| **Native Instruments / Kontakt** | Factory/商购声音可用于商业作品，但禁止再分发或创建 sample library/virtual instrument | 官方提供 Kontakt Player developer licensing，按产品编码、serial 和数量定价；这是发布**自己内容**的平台协议，不自动授予 NI Factory 声音权利 | **厂商内容 YELLOW：仅成品音乐；平台合作待合同**。只能经 NI 与内容权利人书面合同后内置 |
| **Decent Samples / DecentSampler** | 官网 EULA 允许作品使用，但禁止单独再分发样本，除非 Decidedly, LLC 书面许可 | 未找到可直接适用于 Auto Studio 的公开 OEM/嵌入许可 | **内容 YELLOW：仅成品音乐；引擎/OEM NOT APPROVED，待书面合同**。必须分别取得引擎嵌入与每个 pack 的书面再分发许可 |
| **Spitfire Audio / LABS** | 可用于商业录音；官方明确一般不得创建另一 sample library | 未找到允许把 LABS/Spitfire 原始内容随第三方商业应用再分发的公开 OEM 条款 | **YELLOW：仅成品音乐**；除非取得专门书面合同，不得内置 |
| **UVI** | 普通 EULA 允许商业录音，但禁止原始、转换、混合或再合成后的声音作为 samples/programs 再分发 | 官方有 “licensing the UVI Engine, contact us” 页面 | **厂商内容 YELLOW：仅成品音乐；UVI Engine 合作待合同**。需要平台合同和独立内容合同 |
| **Steinberg / HALion** | 购买的 Steinberg/第三方内容不等于可向 Auto Studio 用户转授权 | HALion 提供 Library Creator；官方称合作伙伴可申请 Steinberg Licensing 并销售自己的 instrument library | **厂商内容 YELLOW：仅成品音乐；平台合作待合同**。自有采样可谈平台分发，现有 Steinberg 内容不得默认内置 |

证据：

- Native Instruments 明确禁止使用其 samples/instruments/presets 创建 sound library 或 sample-based product；Kontakt Player licensing 页面则面向开发者自己的库，包含编码费和数量定价。[NI EULA](https://www.native-instruments.com/de/support/downloads/open-source-drivers/end-user-license-agreement/)、[Kontakt Player licensing](https://www.native-instruments.com/en/specials/komplete/this-is-nks/licensing/)
- Decent Samples EULA 说明任何再分发必须取得 Decidedly, LLC 的书面许可；DecentSampler 格式本身是 XML 加 WAV/AIFF/FLAC，但文件格式开放不表示播放器或商店内容开放。[Decent Samples EULA](https://www.decentsamples.com/decent-samples-end-user-license-agreement/)、[Developer Guide](https://decentsampler-developers-guide.readthedocs.io/en/stable/)
- Spitfire 官方帮助中心允许商业录音，同时把“用声音创建另一个 sample library”列为通常不允许。[Spitfire licensing FAQ](https://support.spitfireaudio.com/en/articles/11815239-are-spitfire-audio-sample-libraries-royalty-free-and-can-i-use-them-on-commercial-recordings)
- UVI EULA 禁止各种重新格式化或处理后用于 sampler 的再分发；UVI 另有引擎授权联系入口。[UVI EULA](https://www.uvi.net/index.php/end-user-license-agreement)、[UVI Engine licensing](https://www.uvi.net/licensing-third-parties)
- HALion 的 Library Creator 可封装用户自己有权分发的 samples/presets；合作伙伴可申请 Steinberg Licensing，但这不是 HALion factory content 的转授权。[HALion Library Creator](https://www.steinberg.help/r/halion/7.1/en/halion/topics/library_creator/library_creator_c.html)、[HALion for developers](https://www.steinberg.net/es/vst-instruments/halion/)

## 4. 推荐内容分层

### 4.1 默认内置包：Auto Studio Core Instruments

安装包只包含小型、来源清晰、无需把署名义务转嫁给每个用户作品的内容：

1. 从 VCSL 挑选 12–20 个真乐器基础音色，例如钢琴、吉他/拨弦、少量管弦独奏、基础打击乐；不直接复制整个仓库。
2. 使用官方原始 WAV/官方 SFZ release 作为来源；转换为内部格式时保留上游路径、提交或 release、SHA-256、转换程序版本和参数。
3. 生成 `content-pack.json`、`LICENSES/CC0-1.0.txt`、来源说明与逐文件 inventory。
4. 产品 UI 仍显示作者和来源，即使 CC0 不强制署名。

为什么不把 MuseScore General 或 GeneralUser GS 设为默认：GM 覆盖广但不是高质量制作音色的核心优势；GeneralUser 的来源链不能通过严格 Gate，MuseScore General 还需要对确切发布包完成文件级归档与上游审计。

### 4.2 官方可选下载包

| 建议包 | 内容 | 许可执行 | 产品定位 |
|---|---|---|---|
| **Grand Piano HD** | Salamander Grand Piano V3 SFZ/FLAC | 自动写入 Alexander Holm、CC BY 3.0、来源和修改信息 | 写实钢琴首选 |
| **Community Orchestra** | VSCO 2 CE 官方 SFZ/WAV 全量或精选 | CC0 清单；若使用第三方 conversion，必须另审 conversion 代码/文件 | 管弦草图和多乐器覆盖；不宣称旗舰电影配乐品质 |
| **Acoustic Drums – Crocell/DRS/Muldjord** | 选一至多个 DrumGizmo 多麦克风套件 | 每套独立 CC BY 4.0 署名、许可、变更记录；保留多通道结构 | 专业原声鼓与 mic mix |
| **FreePats Verified Instruments** | 只收录官方页面明确为 CC0/CC BY 且来源完整的真乐器 | 每个 instrument 单独 manifest；禁止把全站当一个许可 | 补齐乐器覆盖 |
| **GM Compatibility** | 经审计的固定版本 MuseScore General | 归档 exact artifact 内 MIT 文本、哈希、贡献者/来源 | MIDI 兼容回放，不作为专业旗舰音色 |
| **AVL Drums** | AVL SFZ/SF2/h2drumkit | 独立下载、独立目录；保留 readme；修改版改名并按 BY-SA 发布 | 可选社区鼓包，不混入默认专有内容包 |

可选包必须允许用户在下载前看到：大小、许可、署名要求、来源、支持格式、安装位置和卸载按钮。

### 4.3 需商务/OEM合同的包

优先级：

1. **自建录音库**：直接与演奏者、录音棚、录音师签署表演、录音、编辑、全球商业分发、再格式化、终端用户使用、机器辅助编曲/渲染以及必要宣传使用权。这是建立差异化和长期权利确定性的最佳方案。
2. **收购或独家授权独立采样库**：合同必须明确可把可恢复的 multisample 交付给终端用户，以及用户作品是否需要署名。
3. **UVI/DecentSampler/Kontakt/HALion 平台合作**：只在其运行时或生态能显著提升内容供应时采用；引擎合同和内容合同分别审核。
4. **Spitfire 或其他顶级厂商**：公开 EULA 不够，必须取得明确允许 Auto Studio installer/content CDN/离线缓存/升级/地区分发的书面 OEM 条款。

合同至少覆盖：OS/CPU、CLI/GUI/Web、本地与云渲染、并发设备数、离线授权、DRM、内容 CDN、增量更新、撤回和下架、用户工程可移植性、生成成品的永久权利、样本提取防护、AI/LLM Tool 操作、模型训练是否明确排除、审计与赔偿。

### 4.4 禁止默认内置

- Philharmonia：官网直接禁止作为 sample/sampler instrument 提供。
- Pianobook：个人不可转让许可且禁止再分发/重新格式化。
- University of Iowa：官方虽称可用于任何项目，但没有明确覆盖 installer/CDN、转格式和终端用户转授权；需书面澄清后再准入。
- Freesound 批量包：逐文件许可、上传来源和商业 API 三重风险。
- GeneralUser GS：来源链无法达到商业内置内容的证明标准。
- Kontakt、Spitfire、UVI、Steinberg 的普通商购/免费终端用户内容：成品音乐许可不是 OEM。
- 任何只有“free”“royalty-free”“public domain source”描述，却没有权利人、确切许可文本、版本和文件清单的包。

## 5. Rust 播放、采样与插件技术评估

### 5.1 SF2/SF3 与 SFZ 的产品定位

- **SF2/SF3**：适合 GM、轻量音色、简单 velocity/key zone、envelope/filter 和便携分发。FluidSynth 官方文档把 SF2/SF3 描述为包含所有乐器 waveforms 的 SoundFont；SF3 是压缩样本变体。[FluidSynth Getting Started](https://www.fluidsynth.org/wiki/GettingStarted)
- **SFZ**：文本映射加外部 WAV/FLAC，适合复杂 velocity、round robin、articulation、release、CC、mic position 和流式加载；但实现之间存在 opcode 兼容差异。Auto Studio 必须声明自己的 supported SFZ profile，而不能声称支持所有 SFZ。
- **内部格式**：建议将通过法律 Gate 的 SFZ/SF2 编译为版本化 `Auto Studio Instrument Manifest`，保留源文件、zone、articulation、RR、velocity、mic/channel、loop、tuning、license 和 attribution 元数据。编译是缓存/索引，不得丢失可验证来源。

### 5.2 合成/采样播放器

| 组件 | 官方定位与许可 | 优点 | 风险 | 决策 |
|---|---|---|---|---|
| **rustysynth** | pure Rust SoundFont MIDI synth，实时/离线，MIT | 无 C ABI、许可宽松、易嵌入与测试 | 只解决 SoundFont 层；专业音质仍受库与实现兼容性限制 | **首选 SF2 Spike** |
| **OxiSynth** | pure safe Rust SoundFont synth，带 chorus/reverb，Cargo 标记 LGPL-2.1 | 架构接近 FluidSynth，支持实时展示 | LGPL 静态链接/替换能力需法律设计；官方 crate 仍为 0.1.0 | **备选/对照，不默认发布** |
| **FluidSynth** | C 的实时 SoundFont synth，LGPL；官方明确商业闭源应保持可替换动态库并履行 LGPL | 成熟、可作参考渲染器 | 非 Rust、LGPL 分发义务、平台动态库 | **验证基线或隔离 fallback** |
| **sfizz** | C++ SFZ parser/synth，BSD-2-Clause，提供 C/C++ API | 当前范围内最可信的开放 SFZ 引擎，FreePats 也推荐它 | 非 Rust；需审查其依赖组合和每个平台构建 | **隔离 `sfz-worker` 首选** |

依据：[rustysynth](https://github.com/sinshu/rustysynth)、[OxiSynth](https://github.com/PolyMeilex/OxiSynth)、[OxiSynth Cargo license](https://github.com/PolyMeilex/OxiSynth/blob/master/oxisynth/Cargo.toml)、[FluidSynth licensing FAQ](https://www.fluidsynth.org/wiki/LicensingFAQ/)、[sfizz](https://github.com/sfztools/sfizz)、[FreePats 推荐软件](https://freepats.zenvoid.org/links.html)

建议演进：

1. 首发用 `rustysynth` 支持 GM/SF2，用 `sfizz-worker` 支持复杂 SFZ。
2. 建立同一 MIDI/automation 输入下的 reference render suite，以 FluidSynth、sfizz 和 Auto Studio 输出做峰值、频谱、包络、loop、voice stealing 与主观听感对比。
3. 长期自研 Rust `auto-sampler`，先实现 Auto Studio Instrument Manifest，而不是承诺完整 SFZ；逐个 opcode 通过测试后再替换 `sfizz-worker`。

### 5.3 现有 Rust 音频组件的正确位置

| 能力 | 组件 | 许可/注意事项 | 模块定位 |
|---|---|---|---|
| FFT/STFT 原语 | `rustfft` | MIT OR Apache-2.0；只提供 FFT | `dsp-analysis`，另实现 window/hop/OLA |
| 多维离线计算 | `ndarray` | MIT OR Apache-2.0；不要作为实时 callback 默认 buffer | 特征、谱图、离线算法 |
| 解码/demux/tag | `symphonia` | MPL-2.0，纯 Rust；不是完整编码/导出器 | 可信格式的快速导入和采样解码 |
| 设备 I/O | `cpal` | Apache-2.0；低层 API | 仅 `audio-engine` 实时 callback |
| DSP graph/prototype | `fundsp` | MIT OR Apache-2.0 | 基础效果与算法原型；transport/PDC 仍自研 |
| 自有插件开发 | `nih-plug` | 框架 ISC、maintenance mode；现有 VST3 bindings GPLv3 | 不用于 host；必要时评估社区 fork |
| MIDI 端口 | `midir` | MIT；低层实时 I/O | `midi-service`，高层 MIDI/tempo map 自研 |
| 重采样 | `rubato` | MIT OR Apache-2.0；实时处理需预分配并离开设备 callback | `render-worker`/处理线程 |
| WAV | `hound` | Apache-2.0；只处理 WAV PCM/float | 中间文件、测试 fixture |
| 长尾媒体 | FFmpeg/ffprobe | 默认 LGPL 2.1+；启用 GPL 部件会使整个 FFmpeg build 变为 GPL，`--enable-nonfree` 可能导致不可分发 | 签名、固定版本的独立 media worker |

官方依据：[RustFFT](https://github.com/ejmahler/rustfft)、[ndarray](https://github.com/rust-ndarray/ndarray)、[Symphonia](https://github.com/pdeljanov/symphonia)、[CPAL](https://github.com/RustAudio/cpal)、[FunDSP](https://github.com/SamiPerttu/fundsp)、[midir](https://github.com/Boddlnagg/midir)、[rubato](https://github.com/HEnquist/rubato)、[hound](https://github.com/ruuda/hound)、[FFmpeg legal](https://www.ffmpeg.org/legal.html)

### 5.4 插件宿主

1. **VST3 进入 Ship 2 的条件设计**：生产 Adapter 使用 Steinberg 官方 SDK/C API 的窄 Rust FFI；`autostudio-vst3-sys` 内部拥有 unsafe，Domain、Audio Graph、Agent Tool 和客户端不接触 ABI、pointer、binary path 或 raw state。[Steinberg VST3 SDK](https://github.com/steinbergmedia/vst3sdk)、[VST3 C API](https://github.com/steinbergmedia/vst3_c_api)
2. **宿主能力不等于 binding**：产品仍需实现官方目录扫描、UID/hash Catalog、processor/controller 生命周期、audio/MIDI bus、parameter/state/preset、sample-accurate automation、processing context、latency/tail、PDC、realtime/offline、generic editor、freeze 和缺失插件恢复。[VST3 API](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical%2BDocumentation/API%2BDocumentation/Index.html)
3. **Scan/Runtime 分离**：Scan Worker 短生命周期加载未知模块；Runtime Worker 使用预分配共享内存/ring buffer 传 audio/event block，控制数据使用有界 IPC。插件不得与 Project SQLite、Agent Runtime 或主 Audio Engine 共享崩溃域。
4. **Agent 需要 Plugin Profile**：只有 Approved 且具有语义 Profile 的插件可被 LLM 自动调参；未知插件允许用户手动 generic control，但 Agent 不能猜 parameter index 的含义。
5. **CLAP 后置**：`clack` 是未来 CLAP host 候选，不进入 MVP；AU、VST2 同样排除。不要把 NIH-plug 当 host，它用于开发插件且原仓库处于 maintenance mode。[Clack](https://github.com/prokopyl/clack)、[NIH-plug](https://github.com/robbert-vdh/nih-plug)

VST3 SDK 当前采用 MIT；官方 C API 使用仓库内许可证，二者都必须随确切 commit 归档。SDK 许可不会改变旧 Rust bindings 的既有 GPL 声明，也不会授予第三方插件二进制再分发权或自动解决 VST 商标使用。

## 6. 推荐 Rust 模块和进程边界

```text
TUI / GUI / future Web clients
          │ versioned HTTP/WebSocket API
          ▼
┌──────────────── auto-studio-core (Rust) ────────────────┐
│ Agent Runtime / Provider Adapters / Project / Jobs      │
│ Tool Registry / Policy / Content Catalog / Provenance   │
└──────────────┬───────────────────┬───────────────────────┘
               │ commands/events   │ offline jobs
               ▼                   ▼
┌──────── auto-audio-engine ─┐  ┌──── auto-render-worker ────┐
│ CPAL callback              │  │ deterministic offline graph│
│ transport / scheduler      │  │ sample streaming / bounce  │
│ realtime DSP / MIDI        │  │ Symphonia/Rubato/Hound     │
│ sampler voices             │  │ FFmpeg child process       │
└──────────┬─────────────────┘  └─────────────┬──────────────┘
           │ shared memory/ring buffer         │ staged files
           ▼                                   ▼
┌──── plugin-worker(s) ──────┐        Project content store
│ VST3 Ship 2, CLAP/AU later │        + license manifests
│ scan/quarantine/watchdog   │
└────────────────────────────┘

Optional isolated runtimes:
  sfz-worker (sfizz C++/C API)
  fluidsynth validation worker
```

所有后端业务模块使用 Rust；非 Rust 媒体组件只存在于明确的 worker/FFI 边界。主要规则：

- 音频 callback 不分配大对象、不访问网络/数据库/文件、不等待 mutex、不记录普通日志、不调用 LLM。
- 控制线程编译不可变 audio graph，通过 lock-free queue 或双缓冲切换。
- 大型样本由 streaming/prefetch 线程读取；音频线程只消费已准备好的 page。
- 离线 render 和实时 preview 共享 graph semantics，但使用不同 execution policy。
- 第三方插件、FFmpeg、sfizz 的 crash/timeout 不得破坏 Project transaction。
- PCM 不走 JSON/HTTP；命令和事件版本化，音频数据走共享内存、映射文件或 staged asset。
- 内容包安装、升级和卸载是事务：下载到 staging，验签/哈希/许可检查，通过后 atomic commit。

## 7. LLM Semantic Tool 目录

LLM 只能调用领域工具，不得直接调用 crate、shell、FFmpeg 参数、任意文件路径或插件 ABI。

### 7.1 内容发现与安装

| Tool | 作用 | 关键约束 |
|---|---|---|
| `instrument.search` | 按乐器、articulation、风格、音域、质量等级查找 | 只返回已安装或可合法下载的 catalog entries |
| `instrument.inspect` | 返回层数、RR、articulations、mic positions、大小、许可摘要 | 不暴露底层样本绝对路径 |
| `instrument.audition` | 用受限 MIDI phrase 生成短试听 | 时长、音量、并发、缓存配额；不可导出 isolated raw sample |
| `content_pack.list` | 显示官方包、许可、大小和安装状态 | RED/NOT APPROVED 永不进入候选 |
| `content_pack.install` | 安装明确 pack/version | 需要用户确认下载大小与许可；验签、哈希、空间和 policy |
| `content_pack.remove` | 卸载未被项目依赖的包 | 项目引用检查；可恢复 trash/staging |

### 7.2 编曲、采样器与渲染

| Tool | 作用 | 关键参数 |
|---|---|---|
| `track.assign_instrument` | 将 catalog instrument 分配给 MIDI track | `track_id`, `instrument_id`, `preset_id` |
| `instrument.set_articulation` | 设置 keyswitch/CC/区域的语义 articulation | 只接受 manifest 中声明的 articulation id |
| `instrument.set_mic_mix` | 控制多麦克风套件 | 归一化 channel ids 和安全增益范围 |
| `instrument.set_expression` | 写入 dynamics/expression/pedal/aftertouch | 有界 automation，sample-accurate 编译 |
| `midi.humanize` | 对 timing/velocity 做可复现扰动 | seed、最大偏移、力度范围；保留原 clip |
| `instrument.layer` | 构造多个音色层 | voice/CPU/memory budget；禁止许可不兼容内容合并成新 distributable pack |
| `instrument.render_stem` | MIDI/automation 离线渲染为 stem | 固定 sample rate、bit depth、seed、engine version |
| `mix.render` | 正式 bounce | deterministic graph、loudness/peak policy、atomic asset commit |

### 7.3 插件工具

- `plugin.search/inspect`: 只返回已扫描、已批准的 Catalog 与 Profile 摘要；未知或 quarantined 插件不可被 Agent 选择。
- `plugin.add/apply_preset`: 只使用 Catalog ID 和 Profile preset；添加新实例、显著 CPU/latency 变化可触发审批。
- `plugin.set_parameter/write_automation`: 只接受 Profile semantic parameter id、单位、合法范围与 automation policy。
- `plugin.bypass/render_preview/freeze/remove`: 通过受限 Runtime/Render Worker；崩溃时保留诊断并回滚 staged output。

### 7.4 Tool 安全边界

- `allowlist tool + typed schema + policy engine + executor`，模型没有第二条执行路径。
- 所有路径由 `AssetId/PackId/PluginId` 解析，拒绝 `..`、symlink escape、URL 和 shell token。
- 禁止 LLM 生成原始 FFmpeg filter graph、插件二进制路径、SFZ include path 或 SQL。
- Tool 请求在执行前计算 CPU、RAM、磁盘、时长、下载量和潜在许可义务。
- 安装内容、启用第三方插件、覆盖工程、导出带署名材料等动作要求显式用户确认。
- 每次 render 记录 `engine_version`、pack/version/hash、preset、MIDI、automation、随机 seed、插件与参数、FFmpeg build id，支持复现和审计。
- 许可策略属于运行时数据：例如 CC BY pack 自动进入项目 credits；RED pack 即使文件意外出现在磁盘也不可由 Tool 发现。

## 8. 质量与法律验收 Gates

### 8.1 内容法律 Gate（任何一项失败即 NOT APPROVED）

每个 pack 必须归档：

1. 权利人、录音者、演奏者和贡献者；原始官方 URL；下载时间；确切 release/commit/version；文件 SHA-256。
2. 许可证原文的本地不可变副本；明确区分代码、映射文件、UI/图片、原始 recordings 和衍生 samples。
3. 明确允许商业使用、installer/CDN 再分发、格式转换、压缩、映射修改、可选下载与离线缓存。
4. Attribution、ShareAlike、改名、许可链接、修改说明、source/preferred form、relink 或 notice 等义务。
5. 终端用户能否复制底层样本；是否需要防提取；撤销/下架后现有项目如何继续渲染。
6. 人工法律批准人、批准日期和适用产品版本。

对于 CC BY/CC BY-SA，release pipeline 必须自动生成 `THIRD_PARTY_CONTENT.md` 和 GUI Credits；不能把署名义务留给 LLM 临时生成。

### 8.2 采样内容 Gate

- 文件完整、无 clipping、NaN、损坏 header、意外 DC、异常静音或截断 release。
- pitch/root key、音域、loop point 和 crossfade 经自动检测加人工抽查。
- velocity layer 音量/音色过渡、RR 重复感、articulation 切换、pedal/release/共鸣行为可接受。
- 多麦克风套件检查相位、channel alignment、bleed、mono compatibility；DrumGizmo 某些官方套件已公开相位或坏样本注意事项，不能忽略。
- 记录 sample rate/bit depth/channel/layout，转换只做一次并保留原始 master 的哈希。
- 专业音乐人盲听：按乐器、力度、独奏/合奏、干声/混音四组场景评分；低于产品基线不得标 HD/Pro。

### 8.3 引擎 Gate

- reference sampler 对比：音高、包络、loop、filter、modulator、voice stealing、pedal、keyswitch/CC、RR、release。
- 实时目标：明确首发 OS/设备、sample rate、buffer size、voices、tracks 和效果图；统计 p50/p95/p99 callback、underrun、CPU 和内存，不以“听起来没问题”验收。
- callback 在稳态无 heap allocation、磁盘 I/O、网络、等待锁和 panic；通过长时间 soak test。
- 离线 render 可复现；相同输入、版本、seed 和内容 hash 得到等价输出。
- 大包测试 cold start、首音延迟、streaming page miss、快速 seek、pack 热切换、卸载中引用。
- 插件 scan fuzz、恶意/崩溃/挂死插件、GUI 关闭、state restore、latency change、sample-rate change 和离线 faster-than-realtime。
- 导入 archive 防 zip-slip、symlink escape、超大解压比、畸形 WAV/SF2/SFZ、递归 include 和路径穿越。

### 8.4 软件供应链 Gate

- `cargo deny`/SBOM/许可证 allowlist；MPL/LGPL/GPL 依赖单独审查。
- FFmpeg build 记录 configure flags，禁止 `--enable-nonfree`；若商业闭源发行目标不接受 GPL，则禁止 `--enable-gpl` 以及会改变 build 许可的库。遵循 FFmpeg 官方 compliance checklist。[FFmpeg legal](https://www.ffmpeg.org/legal.html)
- `sfizz`、FFmpeg、VST3 SDK/C API、固定 VST3 corpus 和系统 audio backends 固定版本、构建可复现、签名、提供第三方 notice。
- VST3 SDK/C API 代码许可证、VST 商标/logo、User-owned Plugin 和 Bundled/OEM Plugin 权利分开处理。

## 9. 推荐落地顺序

### Phase 0：四周准入 Spike

1. 完成 VCSL 精选包、Salamander、一个 DrumGizmo 包的文件级 license/provenance manifests。
2. `rustysynth` 渲染固定 GM/MIDI corpus；`sfizz-worker` 渲染 VCSL/VSCO/Salamander；建立 reference audio 与主观试听。
3. CPAL 实现 48 kHz、128/256 buffer 的无分配 callback；sampler/streaming 在独立处理线程。
4. 完成 VST3 private FFI、Scan/Runtime Worker、一个 instrument/effect fixed corpus、generic editor、state、automation、PDC、freeze 与故障隔离。
5. 为 fixed corpus 建 Plugin Profile；Agent 只开放语义插件 Tool，不开放 ABI、路径或 raw state。
6. 验证 Windows/macOS/Linux 的构建、签名、安装、VST3 官方目录和内容包事务。

### Phase 1：专业采样 MVP

- 默认 VCSL Core Instruments。
- 可选 Salamander Grand Piano、VSCO 2 CE、单个 DrumGizmo 套件。
- Rust Audio Engine 支持 transport、sample-accurate MIDI/automation、基础 mixer/DSP、offline bounce。
- VST3 支持 Approved instrument/effect、generic editor、Plugin Profile、Plugin Lock、PDC、realtime/offline 与 freeze；只承诺发布的兼容矩阵。
- FFmpeg 只做长尾媒体、交付编码和视频合成；采样器正式 render 不依赖 shell command 拼接。
- 自动 credits、SBOM、content provenance 和 reproducible render manifest。

### Phase 2：VST3 兼容扩展与商业音色

- 扩展 VST3 fixed corpus、native GUI compatibility、multi-bus、dynamic latency 和更多 Approved Plugin Profile。
- 自录或 OEM 第一套旗舰乐器，优先钢琴、原声鼓或弦乐中的一个，不同时铺开。
- 商业引擎/内容谈判只有在明确提升覆盖或质量、且许可允许 CLI/GUI/Web/本地渲染时才进入产品。

### Phase 3：更多插件格式

- CLAP 只在独立需求和 ADR 通过后评估 `clack-host`；AU 需要独立平台边界；VST2 不进入新宿主计划。
- 新格式必须复用 Plugin Catalog/Profile/Lock 和隔离合同，不能把 ABI 泄漏到 Domain 或 Agent Tool。

### Phase 4：自有 Rust Sampler

- 以内部 Instrument Manifest 为稳定接口，逐步替换 `sfizz-worker`。
- 支持 disk streaming、mic mixes、RR、velocity crossfade、release/resonance、keyswitch/CC、MPE、purge/预载策略。
- 只有在完整 compatibility/quality/real-time Gate 通过后，才能把外部 SFZ worker 从产品中移除。

## 10. 最终决策

1. **批准**：后端、Agent Runtime、核心媒体服务和实时 Audio Engine 统一为 Rust。
2. **批准**：FFmpeg 继续作为独立受控媒体运行时；它不改变“业务后端全 Rust”的技术决策。
3. **批准（阶段性）**：SFZ 使用隔离的 `sfizz-worker`，SF2 先验证 `rustysynth`；不要为了 pure Rust 标签牺牲专业音色支持。
4. **条件批准**：VST3 Plugin Host 仅在 Ship 2 进入，采用官方 SDK/C API 的窄 Rust FFI、Scan/Runtime Worker、Plugin Profile/Lock 和 fixed compatibility corpus；CLAP/AU 后置，VST2 不支持。
5. **批准**：VCSL 精选 CC0 包作为默认内容；Salamander、VSCO 2 CE、逐套审核的 DrumGizmo/FreePats 作为可选下载。
6. **条件批准**：MuseScore General、AVL、Freesound 单文件和所有商业厂商候选；只有文件级证据或书面合同通过 Gate 后才能改变状态。
7. **不批准**：Philharmonia、Pianobook、Iowa、GeneralUser GS 和普通商业终端用户音色作为默认内置内容。
8. **产品质量结论**：开放内容能提供合法的专业制作基础和少数高质量单项，但不足以形成全面的旗舰音色库。要达到与主流商业工作站竞争的真实乐器质量，必须把“自录/OEM 旗舰内容包”列为独立产品计划，而不能仅靠搜集免费库完成。
