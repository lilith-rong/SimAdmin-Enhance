# SimAdmin 扩展开发文档：多路径语音/短信统一接入与 SIP Trunk 网关（进度更新版）

> **文档性质**：进度快照 + 四版本对比 + VoWiFi 代码回迁 + VoLTE 逆向重构 + 二次开发扩展 + 分阶段实施路线图 + TodoList
> **本版说明**：本文在原规划文档基础上，依据**当前代码库真实状态**更新进度、重排 TodoList（已完成阶段前置、未完成阶段后置）。
> **撰写依据**：对以下项目的实际代码/二进制对比分析
> - `SimAdmin`（**当前开发工作树**，基线为旧上游 1.1.3 + 完整 vowifi + AI 语音信令 + 本次新增 volte + ims 共享核心 + 领域化目录重构）
> - `SimAdmin-main`（最新上游 1.1.5，vowifi 已被原作者移除，无 volte）
> - `SimAdmin-main-vowifi`（旧上游 1.1.3，**含完整 vowifi 脚手架**）
> - `SimAdmin-VoLTE`（未开源的已编译二进制 1.1.6-dev18，含独立 `src/volte.rs`，经 clean-room 静态分析）
>
> ⚠️ **基线校正（重要）**：原规划以"迁移到最新上游 1.1.5"为前提（含阶段 0 回迁手册）。但**实际二次开发是在 `SimAdmin`（1.1.3 + vowifi）工作树上直接进行的**，并未迁移到 1.1.5。因此原"阶段 0：VoWiFi 回迁到 1.1.5"在当前工作树中**不适用/已跳过**（vowifi 本就在树内）。迁移到 1.1.5 作为一项独立的、尚未执行的可选任务保留在文末。
>
> **合规声明**：本文档对 `SimAdmin-VoLTE` 的描述均基于对已编译二进制的**行为级静态分析**（字符串锚点 + 前端 API），不含任何反编译的原始源码。VoWiFi 代码回迁使用的是用户已合法持有的旧版本 GPLv3 源码。所有重构实现基于公开 3GPP/RFC 规范独立完成（clean-room），遵循 GPLv3。

---

## 目录

0. [进度快照（截至本版更新）](#零进度快照截至本版更新)
1. [文档目标与背景](#一文档目标与背景)
2. [四个版本对比分析](#二四个版本对比分析)
3. [关键发现：1.1.5 不是 1.1.3 删 vowifi](#三关键发现115-不是-113-删-vowifi)
4. [目标能力与统一架构](#四目标能力与统一架构)
5. [阶段 0：VoWiFi 代码回迁到 1.1.5（移植手册）](#五阶段-0vowifi-代码回迁到-115移植手册)
6. [可复用资产清单](#六可复用资产清单)
7. [从 VoLTE 二进制还原的实现要点](#七从-volte-二进制还原的实现要点)
8. [配置模型设计](#八配置模型设计)
9. [收信去重与活跃监听者机制](#九收信去重与活跃监听者机制)
10. [SIP Trunk / Asterisk 对接设计](#十sip-trunk--asterisk-对接设计)
11. [分阶段实施路线图](#十一分阶段实施路线图)
12. [完整 TodoList](#十二完整-todolist)
13. [合规与可验证性边界](#十三合规与可验证性边界)
14. [工程规范与交付标准](#十四工程规范与交付标准)

---

## 零、进度快照（截至本版更新）

> **本节是本次更新新增的活动状态区**，用于快速对齐"计划 vs 已落地"。下方各章的技术方案保持原样（作为设计蓝图），**实际完成状态以本节 + 第十二章 TodoList 的勾选为准**。
>
> ⚠️ **重要基线澄清**：本文档原计划以 **1.1.5** 为开发底座（先回迁 vowifi 再扩展）。但**实际的二次开发是在 `SimAdmin`（1.1.3 + vowifi + voice 基线）工作树上直接进行的**，并未迁移到 1.1.5。因此："阶段 0 VoWiFi 回迁"在当前工作树中**不适用**（vowifi 本就在树内）；迁移到 1.1.5 作为一项**独立的、尚未进行**的任务另列。下方状态均针对**当前 `SimAdmin` 工作树的真实代码**。

### 0.1 阶段完成状态总览

| 阶段 | 目标 | 状态 | 说明 |
|------|------|:---:|------|
| **目录/架构重构** | 领域化分层 + 共享核心抽离 | 🟢 **已完成** | `ims/` 共享核心 + `access/{vowifi,volte}` 接入腿 + 8 个业务领域目录；378 单测全绿；已建 git 基线 |
| **A. 共享 IMS 核心** | VoWiFi/VoLTE 合并 SIP/AKA 层 | 🟢 **部分完成** | `ims/` 已抽出 `digest_aka`（AKAv1/v2-MD5 + HMAC-MD5）+ `sip_frame`（组帧/解析原语），volte 与 vowifi/live.rs **均已复用**；但**未做**完整 `ImsChannel` trait / `AccessLeg` enum / `context.rs` 中立参数层 |
| **B. VoLTE SMS 腿** | VoLTE 收发短信离线层 | 🟢 **部分完成** | `access/volte/` 全套离线层已就位（identity/bearer/pcscf/ipsec/sip/sms/runtime）+ 单测；**未做** `channel.rs`（`ImsChannel` 实现）、`live.rs` 真机 IO、真机验证 |
| **E. 语音（部分）** | VoLTE 语音编排 + 信令 | 🟢 **部分完成** | `voice.rs` 已参数化（解 `CarrierProfile` 耦合）；`access/volte/voice.rs` 语音编排 + SIP INVITE/ACK/BYE/CANCEL + `rtp_relay.rs` 骨架 + 单测；**未做** relay 接入真实呼叫、编排器选路、真机通话 |
| **C. 三层 SMS 编排器** | 可配置优先级 + 活跃监听者 + 去重 | 🔴 **未开始** | `orchestrator/` 目录尚不存在 |
| **D. SIP Trunk 网关** | 对外 SIP endpoint + RTP relay | 🔴 **未开始** | `trunk/` 目录尚不存在 |
| **F. ViLTE 视频** | SDP video + H.264 relay | 🔴 **未开始** | — |
| **迁移到 1.1.5** | 把成果搬到最新上游底座 | 🔴 **未开始** | 当前在 1.1.3 基线；含 Email/ServerChan3 等 1.1.5 新功能的合并 |

### 0.2 当前工作树实际目录结构（已重构）

```
SimAdmin/backend/src/
├── main.rs / state.rs          # 入口 + 全局 AppState
├── ims/                        # 🟢 共享 IMS 核心（digest_aka + sip_frame）
│   ├── digest_aka.rs           #   AKAv1/v2-MD5 + HMAC-MD5 + nonce 解码（RFC 向量单测）
│   └── sip_frame.rs            #   SIP 组帧/解析原语（parse_status/body/frame_len/header_*）
├── access/                     # 🟢 接入腿分组
│   ├── vowifi/                 #   VoWiFi 腿（ike*/ims/sms/qmi_uim/live/voice/... 31 文件，复用 ims/）
│   └── volte/                  #   VoLTE 腿（identity/bearer/pcscf/ipsec/sip/sms/runtime/voice/rtp_relay，复用 ims/）
├── automation/                 # 自动化任务调度
├── api/                        # handlers + models + auth
├── cellular/                   # modem_manager + cell_lock_store + serial
├── messaging/                  # sms_listener + verification_code
├── notify/                     # notification + notification_queue
├── network/                    # device_network + iptables
├── system/                     # ota + system_event + system_event_monitor + device_status
├── sim/                        # esim
└── infra/                      # config + db + utils
```

> 注意：与原第 4.2 节"规划目录"的差异——实际把 vowifi/volte **收进了 `access/` 伞下**（而非顶层平级），共享核心 `ims/` 目前是**轻量版**（只含 digest_aka + sip_frame 两个纯逻辑文件），尚未实现规划中的 `context.rs`/`access.rs`（`ImsChannel`/`AccessLeg`）/`register.rs`/`sms_codec.rs`。这些是阶段 A 的剩余工作。

### 0.3 下一步建议顺序

1. **补齐阶段 A 剩余**：`ims/context.rs`（中立参数）+ `ims/access.rs`（`ImsChannel`/`AccessLeg`），把 volte/vowifi 的通道差异真正收敛到 trait/enum 后（目前两腿仍各自持有通道逻辑）。
2. **阶段 B 收尾**：`access/volte/channel.rs` + `live.rs` 真机 IO；真机验证 VoLTE 注册/收发短信。
3. **阶段 C 编排器**：新建 `orchestrator/`，实现三层 SMS 选路 + 活跃监听者选举 + 去重。
4. **阶段 D/E/F**：Trunk 网关 → 语音编排收尾 → ViLTE。

---

## 一、文档目标与背景

### 1.1 情况变化（重要）

自上一版文档以来，项目基线发生了关键变化：

1. **原 `SimAdmin-main`（1.1.3，含 vowifi）已被重命名为 `SimAdmin-main-vowifi`**，作为 VoWiFi 源码的保留副本。
2. **下载了最新上游 `SimAdmin-main`（1.1.5）**，原作者已**主动移除 vowifi 模块**（据称出于合规性顾虑）。这个 1.1.5 既无 vowifi 也无 volte，是纯净的 SIM 管理系统。
3. **开发对象变更为最新的 1.1.5**。目标是在 1.1.5 上：先把被删除的 VoWiFi 功能**回迁**，再逆向重构 VoLTE，最后做多路径 Trunk 的二次开发扩展。

> **用户立场**：用户所在地不存在原作者所顾虑的合规性问题，且用户合法持有旧版 GPLv3 源码，因此有权将 VoWiFi 功能加回自己的衍生版本。

### 1.2 1.1.5 当前具备的能力

- **CS（电路交换）短信**：通过 ModemManager 收发（`sms_listener.rs`）。
- **CS 语音通话**：通过 ModemManager Voice 接口，接口完整（`/api/call/dial` `/answer` `/hangup` `/hangup-all` `/volume` `/forwarding` `/settings` `/history`）。
- **IMS/Voicemail 状态查询**：`/api/ims/status`、`/api/voicemail/status`（只读，依赖 ModemManager 暴露能力）。
- **新增邮件通知渠道**：1.1.5 相比 1.1.3 新增了 Email（`lettre` SMTP）与 ServerChan3 通知渠道。

### 1.3 最终扩展目标

将 SimAdmin 从"SIM 管理工具"升级为**"SIM 多路径接入网关 / SIP Trunk"**：

1. **短信**支持三条接入路径，按**用户可自定义优先级**回退：VoWiFi / VoLTE / CS。
2. **语音**支持三条接入路径，同样可配置优先级；语音与短信的优先级**独立设置**。
3. 每条路径（层级）可**独立启用/禁用**。
4. 新增 **VoLTE**（SMS/语音）与 **ViLTE**（视频通话）能力。
5. 对外提供标准 **SIP Trunk endpoint**，可对接 FreePBX/Asterisk，或内嵌 Asterisk 桥接外部 Linphone 软电话拨打电话。

### 1.4 关键设备约束（必须正视）

目标设备之一是**高通 410 随身 WiFi**（MSM8916 精简型），**无音频硬件**（无 mic/speaker/codec/PCM 接口）：

- **本地话机模式不可行**：设备无法本地采集/播放音频。
- **网关模式可行**：设备只做 SIP 信令 + RTP relay（转发），真正的音频端点是外部软电话（Linphone）或 PBX 后的话机。
- **CS 语音的音频无法软件 relay**：CS 通话音频走基带内部 PCM/模拟通路，无 IP 包可转发；在无音频接口的设备上，CS 语音这一层**对语音网关无效**（仅在带 USB-Audio/PCM 的设备如部分 EC20 方案上可用）。

> **结论**：项目定位为 SIP Trunk 网关，恰好化解了"无音频硬件"的矛盾——网关本就不该放音。

---

## 二、四个版本对比分析

### 2.1 版本基本信息

| 版本 | 目录 | 版本号 | 形态 | vowifi/ | voice.rs | volte.rs |
|------|------|--------|------|:---:|:---:|:---:|
| **最新上游（开发对象）** | `SimAdmin-main` | **1.1.5** | 源码 | ✗ | ✗ | ✗ |
| 旧上游 + VoWiFi | `SimAdmin-main-vowifi` | 1.1.3 | 源码 | ✓ | ✗ | ✗ |
| VoWiFi 语音版 | `SimAdmin` | 1.1.3 | 源码 | ✓ | ✓ | ✗ |
| VoLTE 版 | `SimAdmin-VoLTE` | 1.1.6-dev18 | **仅二进制** | ✗ | ✗ | ✓ (独立文件) |

### 2.2 能力矩阵对比

| 能力 | 1.1.5 (新main) | 1.1.3+vowifi | voice 版 | VoLTE 版 (二进制) |
|------|:---:|:---:|:---:|:---:|
| CS 短信（ModemManager） | ✓ | ✓ | ✓ | ✓ |
| CS 语音（ModemManager Voice） | ✓ | ✓ | ✓ | ✓ |
| Email/ServerChan3 通知 | ✓ | ✗ | ✗ | ? |
| VoWiFi 短信（IMS over WiFi/ePDG） | ✗ | ✓(脚手架) | ✓(脚手架) | ✓(脚手架) |
| VoWiFi 语音（呼叫状态机/SDP/RTP） | ✗ | ✗ | ✓(信令层+单测) | ✗ |
| **VoLTE 短信（IMS over LTE）** | ✗ | ✗ | ✗ | **✓(完整实现)** |
| VoLTE 语音 | ✗ | ✗ | ✗ | ✗ |
| 内核 IPsec（`ip xfrm`） | ✗ | ✗ | ✗ | **✓** |
| ModemManager IMS bearer | ✗ | ✗ | ✗ | **✓** |
| SIP Trunk / Asterisk 对接 | ✗ | ✗ | ✗ | ✗ |
| ViLTE 视频 | ✗ | ✗ | ✗ | ✗ |

### 2.3 各版本作为"素材来源"的定位

| 版本 | 在本项目中的角色 |
|------|-----------------|
| `SimAdmin-main` (1.1.5) | **开发底座**。所有新代码最终落在这里 |
| `SimAdmin-main-vowifi` (1.1.3) | **VoWiFi 源码供体**。阶段 0 从这里回迁 `vowifi/` 及其接线 |
| `SimAdmin` (voice 版) | **语音信令供体**。`vowifi/voice.rs` 呼叫状态机/SDP/RTP 供阶段 E 复用 |
| `SimAdmin-VoLTE` (二进制) | **VoLTE 行为规格**。逆向出的实现要点供阶段 A 参考（非源码） |

---

## 三、关键发现：1.1.5 不是 1.1.3 删 vowifi

> 这是本次分析最重要的结论，直接决定阶段 0 的做法。

经逐文件对比，**1.1.5 不是简单地在 1.1.3 上删掉 vowifi**，而是一条**独立演进过的分支**：一边移除了 vowifi 接线，一边引入了与 vowifi 无关的新功能（Email/ServerChan3 通知、OTA 模板治理、eSIM 字段扩展、SIM 详情刷新接口等）。

**因此回迁不能靠覆盖文件，必须做三方合并（3-way merge）**：把 vowifi 代码搬进 1.1.5，同时保留 1.1.5 的新演进。

### 3.1 1.1.5 相对 1.1.3 的独立演进（与 vowifi 无关，必须保留）

| 文件 | 1.1.5 的新变化 | 对回迁的影响 |
|------|---------------|-------------|
| `config.rs` | 新增 `EmailConfig`/`ServerChan3Config`；`NotificationChannel` 加 `Email`/`ServerChan3`；**`NotificationRule` 新增 `title_template` 字段** | ⚠️ 任何构造 `NotificationRule` 的 vowifi 代码/测试必须补 `title_template` |
| `config.rs` | OTA 模板治理：`migrate_update_templates` 取代 `migrate_templates_to_remove_md5`；默认模板移除 `Commit:` 行 | 回迁时用 1.1.5 的迁移函数，不要带回旧的 |
| `models.rs` | `EsimEuiccInfo`/`EsimProfile` 新增 `updated_at`；`VersionUpdateEvent` **移除 `commit` 字段** | 与 vowifi 基本无关，保留 1.1.5 现状 |
| `main.rs` | 新增 `/api/sim/details/refresh` 路由；stats sampler 调用时机变化；shutdown 逻辑简化 | 保留 1.1.5 现状，vowifi 路由另外插入 |
| `Cargo.toml` | 新增 `lettre 0.11`（Email 依赖） | **必须保留**，与 vowifi 依赖不冲突 |

### 3.2 1.1.5 相对 1.1.3+vowifi 缺失的（需回迁）

| 文件 | 缺失内容 | 行数差 |
|------|---------|-------|
| `Cargo.toml` | `num-bigint` / `num-traits` / `aes` / `cbc` 四个加密依赖 | — |
| `src/vowifi/` | 整个目录（31 个文件：ike*/ims/sms/qmi_uim/runtime/diagnostics/restore/live/dataplane/...） | 新增 |
| `config.rs` | `VowifiConfig` + 3 默认值函数 + `AppConfig.vowifi` + 3 个 ConfigManager 方法 + 2 单测 | — |
| `state.rs` | `vowifi_runtime` / `vowifi_connect_lock` 字段 + `new` 入参 + `FromRef` 实现 | ~12 |
| `db.rs` | `SmsMessage.transport` 字段；`insert_sms_with_transport`/`insert_sms_at_with_transport`/`sms_id_by_pdu`；**7 张 `vowifi_*` 表**（`vowifi_runtime_events`/`vowifi_runtime_snapshots`/`vowifi_sms_delivery`/`vowifi_sms_parts`/`vowifi_esim_restore`/`vowifi_soak_runs`/`vowifi_soak_samples`）及其索引/结构/方法/脱敏 | ~1026 |
| `sms_listener.rs` | `start_sms_listener` 的 `config_manager` 参数；`maybe_scan_sms_paths` 门控逻辑 | ~61 |
| `handlers.rs` | 13 个 vowifi handler + 辅助函数 + eSIM 切换钩子 | ~1261 |
| `main.rs` | `mod vowifi;`、`vowifi_runtime` 构造、`AppState::new` 传参、`spawn_vowifi_auto_restore`、13 条 `/api/vowifi/*` 路由 | ~60 |

### 3.3 两个移植点（务必重点验证）

1. **`db.rs` 的 `SmsMessage.transport` 列索引同步（中危，改动点集中）** — 加回 `transport` 会使 `SmsMessage` 从 7 字段变 8 字段，若结构体/SELECT 二者不同步，`row.get(索引)` 会错位（编译期不报错、运行时才炸）。**但实际改动面收敛**：两版代码都把行解析集中在**单个私有函数 `sms_message_from_row`**，SmsMessage 的读取只经由它，加上 `get_sms_messages`、`get_sms_conversation` 两个函数共 3 条 SELECT。因此只需精确同步这 3 处：
   - `sms_message_from_row`：加 `transport: row.get(7)?`（1.1.5 该函数在 `db.rs:318` 附近，vowifi 版在 `db.rs:909` 附近）。
   - `get_sms_messages` 的 2 条 SELECT、`get_sms_conversation` 的 1 条 SELECT：列清单末尾补 `transport`（须落在第 8 位 / 索引 7）。
   - 无需担心散落各处的手写 `row.get`——不存在。`get_sms_stats` 只做 `COUNT(*)` 聚合、`sms_id_by_pdu`/`sms_exists_by_pdu`/`update_sms_notification_status` 不映射结构体，均不受影响。

2. **`handlers.rs` 的 eSIM profile 切换钩子** — 旧版在 eSIM 切换 handler 里植入了 vowifi 的"切换前拆除 + 切换后恢复"逻辑。1.1.5 该 handler 可能已随新功能演进，**植入前必须先比对 1.1.5 该函数的当前实现**，不能盲目粘贴。

---

## 四、目标能力与统一架构

### 4.1 目标架构总览

```
┌───────────────────────────────────────────────────────────┐
│                    对外接口层                                 │
│  Web UI  │  REST API  │  SIP Trunk endpoint (对接FreePBX)    │
├───────────────────────────────────────────────────────────┤
│                     应用服务层（传输无关）                      │
│    短信服务 SmsService     │     语音服务 VoiceService          │
├───────────────────────────────────────────────────────────┤
│                     编排层 Orchestrator                       │
│  - 短信选路(可配置优先级)   - 语音选路(可配置优先级, 独立)         │
│  - 就绪监测  - 故障回退  - 活跃监听者选举  - 收信去重             │
├──────────────────────────┬────────────────────────────────┤
│   IMS 接入层（共享 SIP/注册/AKA）  │        CS 接入层             │
│ ┌──────────┐ ┌──────────┐ │  (ModemManager)                │
│ │ VoWiFi腿  │ │ VoLTE腿   │ │  传统 SMS / CS 语音              │
│ │ IKEv2/ESP │ │ 内核xfrm  │ │                                │
│ └──────────┘ └──────────┘ │                                │
└──────────────────────────┴────────────────────────────────┘
                        QMI / AT / D-Bus / USB-Audio
                              基带 + SIM
```

**核心设计原则**：把"IMS 接入"抽象成 trait，VoWiFi 腿与 VoLTE 腿各实现一份（差异仅在"如何建立受保护的 SIP 通道"），而 **REGISTER / SMS-MESSAGE / Voice-INVITE / ViLTE 信令只写一遍**，两条腿共用。

### 4.2 新增模块目录规划（`backend/src/`）

> **模块三分原则**（用户明确要求，且经 `live.rs` 代码核对后确认可行）：
> 1. **共享抽象独立成 `ims/`**，与 `vowifi/`、`volte/` **三者平级**。
> 2. **两条腿的私有实现各留各家**：VoWiFi 的 IKEv2/ESP/ePDG 留在 `vowifi/`，VoLTE 的 bearer/xfrm/P-CSCF 留在 `volte/`。
> 3. **依赖方向单向**：`vowifi/ → ims/`、`volte/ → ims/`，两条腿**互不依赖**。绝不能让 `volte/` 反向依赖 `vowifi/`（否则 VoLTE 拖着一整套 ePDG/IKE 代码，语义错误）。

```
backend/src/
├── ims/                       # 【阶段C，共享核心】传输无关的 IMS 信令层
│   ├── mod.rs                 #   模块声明 + clean-room 声明 + 依赖方向注释
│   ├── context.rs             # ★ 中立上下文（抽离的关键）：
│   │                          #   ImsIdentity(IMPI/IMPU/contact) / ImsRoute(本地+远端 addr:port)
│   │                          #   / ImsRegisterParams(realm/domain/local_port/supported/sec-agree 开关)
│   │                          #   —— 取代 live.rs 对 CarrierProfile + ImsClientTcpRoute 的直接依赖
│   ├── sip_message.rs         #   REGISTER/MESSAGE/RP-ACK/INVITE/ACK 真实报文构造
│   │                          #   （从 vowifi/live.rs 的 build_register_request /
│   │                          #    build_live_sms_message_request / build_live_sms_rp_ack_request /
│   │                          #    build_live_invite_request / build_live_ack_request 抽出，
│   │                          #    入参从 &CarrierProfile+ImsClientTcpRoute 改为 &ImsContext）
│   ├── sip_parse.rs           #   响应解析 + TCP 粘包处理
│   │                          #   （parse_sip_status / sip_body / sip_message_complete /
│   │                          #    sip_complete_frame_len / sip_header_values / sip_header_uri）
│   ├── digest_aka.rs          #   Digest-AKA：AKAv1/v2-MD5 + hmac_md5 + nonce 解码
│   │                          #   （从 live.rs 抽出 aka_digest_password / hmac_md5 /
│   │                          #    digest_nonce_shape / digest_scheme_start_after_comma，
│   │                          #    已带单测 akav2_md5_digest_uses_res_ik_ck / digest_* 系列）
│   ├── register.rs            #   REGISTER 事务骨架（initial→401→AKA→auth→200 的纯逻辑编排）
│   ├── sms_codec.rs           #   短信编解码入口：把 sms.rs 迁来或 pub 重导出
│   │                          #   （TPDU/RP-DATA/GSM7/UCS2/UDH + build_single_part_mo_submission /
│   │                          #    parse_mt_rp_data / classify_rp_ack / build_network_rp_ack）
│   └── access.rs              # ★ 通道抽象：trait ImsChannel + enum AccessLeg（见 §4.3）
│                              #   两条腿差异**只**收敛在这里：如何 send_sip/recv_sip
│
├── vowifi/                    # VoWiFi 私有（阶段0回迁；阶段C瘦身）
│   │                          #   保留：ike*/dataplane/epdg/tun_gateway/eap_aka/executor/
│   │                          #        profiles/diagnostics/restore/soak/stability
│   ├── channel.rs             #   【阶段C新增】impl ImsChannel：SIP 字节走 ESP over ePDG
│   └── live.rs                #   瘦身：报文构造搬到 ims/，本文件只留"建 ePDG 通道 + 调 ims::"
│
├── volte/                     # 【阶段A，VoLTE 私有】LTE 侧接入腿
│   ├── mod.rs
│   ├── config.rs              #   VolteConfig
│   ├── identity.rs            #   IMSI + USIM AID 读取（复用 vowifi::qmi_uim 的 AKA 运算）
│   ├── bearer.rs              #   ModemManager IMS APN bearer 建立/删除陈旧/重建
│   ├── pcscf.rs               #   P-CSCF 发现（从 bearer PCO/IP 设置取）
│   ├── ipsec.rs               #   内核 ip xfrm SA/策略拼装 + SPI/端口生成 + 依赖检查
│   ├── channel.rs             # ★ impl ImsChannel：SIP 字节走内核 xfrm 保护的 socket
│   └── runtime.rs             #   VoLTE 状态机（stage 对齐前端 volteStatus.js）→ 调 ims:: 收发
│
├── orchestrator/              # 【阶段B新增】编排层
│   ├── sms_router.rs
│   ├── voice_router.rs
│   └── listener_election.rs
└── trunk/                     # 【阶段D新增】SIP Trunk 网关
    ├── sip_endpoint.rs
    ├── rtp_relay.rs
    └── bridge.rs
```

#### 4.2.1 抽离边界分析（基于对 `vowifi/live.rs` 的实际代码核对）

`live.rs`（6585 行）已实现一套**真实可上线**的 IMS SIP 客户端，与 VoLTE 所需高度重合。抽离的难点不在 SIP 逻辑本身，而在**解开三类耦合**：

| `live.rs` 现有依赖 | 性质 | 归属 | 抽离处理 |
|---|---|---|---|
| `&'static CarrierProfile`（`profile.ims.realm`/`local_port`/`register.supported_header`/`require_sec_agree_headers`…） | VoWiFi 运营商静态配置 | → `ims/context.rs` | ⚠️ **核心动作**：把报文构造函数入参从 `&CarrierProfile` 改为中立的 `&ImsRegisterParams`；VoWiFi 侧从 profile 填充，VoLTE 侧从 bearer 发现结果填充 |
| `tun_gateway::ImsClientTcpRoute`（ePDG 隧道内 TCP 路由） | **VoWiFi 专有**（ESP over ePDG） | 留 `vowifi/` | 这就是"通道差异"，封进 `vowifi/channel.rs` 的 `ImsChannel` 实现 |
| `LiveImsRegisterIdentity`（IMPI/IMPU/contact）、`sms::MoSmsSubmission`、`qmi_uim::UsimAkaApduResult`、`voice::MoCallInvite` | 传输中立的数据类型 | → `ims/`（identity/codec）；qmi_uim 保留 vowifi 但对 volte pub | ✅ 直接进共享层或跨模块复用 |

**一句话**：报文构造/解析/AKA 计算 → 全进 `ims/`；"如何把这些字节安全送出去" → 各腿的 `channel.rs`。这正好印证 §4.1 的核心原则——差异仅在受保护通道，信令只写一遍。

#### 4.2.2 为什么共享层放独立 `ims/`（而非塞进 `vowifi/`）

- **依赖方向干净**：`volte/` 若要复用留在 `vowifi/` 里的 SIP 代码，就得 `use crate::vowifi::…`，等于 VoLTE 反向依赖 VoWiFi，会把整套 IKE/ESP/ePDG 代码拖进 VoLTE 的编译单元与心智模型。独立 `ims/` 让两条腿平级、互不牵连。
- **符合用户诉求**：用户明确要"共享抽象抽离出来、两功能私有内容各留各家"，`ims/` 就是那个抽离出来的中立层。
- **可测试性**：`ims/` 不含任何 IO（不碰 xfrm、不碰 ePDG socket），是纯逻辑，Windows CI 可全量单测，无需 `#[cfg(unix)]` 门控。

#### 4.2.3 `vowifi::qmi_uim` 的跨腿复用

SIM 侧 AKA 运算（`execute_usim_authenticate_via_proxy_reason_with_retry`）VoLTE 也要用。它**已是 `pub`**、逻辑传输无关（只跟 QMI/USIM 打交道，与接入网无关）。两种处理，二选一：
- **方案甲（推荐，改动小）**：`qmi_uim.rs` 留在 `vowifi/`，`volte/identity.rs` 直接 `use crate::vowifi::qmi_uim`。缺点：轻微违反"腿间不依赖"，但仅此一个纯工具函数、无状态、无接入网语义，可接受。
- **方案乙（更纯，改动大）**：把 `qmi_uim.rs` 提升到顶层 `sim/` 或 `ims/`，两条腿都从共享处引用。若阶段 C 顺手，建议做方案乙。

### 4.3 核心接入腿抽象设计

> ⚠️ **实现约束（务必先读）**：接入腿抽象天然需要 `async`（`establish`/`teardown` 都是 IO 密集）。但 Rust 原生的 `async fn in trait`（RPIT，rustc 1.75+）**目前不是 dyn-compatible（对象安全）** 的——无法直接写 `Box<dyn ImsAccessLeg>` 或 `Vec<Box<dyn ImsAccessLeg>>`。而编排器需要"统一持有并按优先级遍历多条腿"，必然要某种运行时多态。这是本设计里最容易在编译期翻车的点，必须提前决策。
>
> 三种可选方案：
> | 方案 | 做法 | 优劣 |
> |------|------|------|
> | **A. enum 分发（推荐）** | `enum AccessLeg { VoWiFi(VoWifiLeg), VoLTE(VolteLeg) }`，方法内 `match` 分发 | 腿的种类是**封闭集合**（就 VoWiFi/VoLTE 两种，无开放扩展需求），enum 零虚表开销、可穷尽匹配、`async fn` 直接可用、无需额外 crate。**首选。** |
> | B. `#[async_trait]` | 引入 `async-trait` 宏，把 async fn 脱糖为 `Pin<Box<dyn Future>>` | 可 `dyn`，但每次调用一次堆分配；引入额外依赖 |
> | C. 手写 RPITIT + 关联类型 | trait 返回 `impl Future`，用泛型静态分发 | 无堆分配，但无法 `dyn`，编排器需泛型化，代码复杂 |
>
> **本项目采用方案 A（enum 分发）**。腿只有两种且封闭，用 trait + dyn 是过度设计。下面的 trait 仅作为"两条腿共同契约"的**文档化描述**，真实代码以 enum 承载。

本设计有**两层抽象**，别混为一谈：

1. **接入腿 `AccessLeg`（编排器视角）**：编排器统一持有、按优先级遍历的对象。用**封闭 enum + match 分发**（理由见上表）。
2. **受保护通道 `ImsChannel`（信令层视角）**：`ims/` 里的报文构造/事务逻辑只依赖"能收发 SIP 字节"这一契约，不关心底层是 ESP-over-ePDG 还是内核 xfrm。这是 §4.2.1 抽离的落点。

```rust
// ================== ims/access.rs —— 编排器视角 ==================
// 接入腿的共同契约（文档化；运行时用下方 enum 承载分发）
pub trait ImsAccessLegBehavior {
    fn kind(&self) -> AccessLegKind;                        // VoWiFi / VoLTE
    async fn establish(&mut self) -> Result<AccessLegChannel, AccessError>;
    fn readiness(&self) -> LegReadiness;
    fn pcscf(&self) -> Option<SocketAddr>;
    fn local_addr(&self) -> Option<IpAddr>;
    async fn teardown(&mut self);
}

// 编排器实际持有的类型：封闭 enum，match 分发，无 dyn、无堆分配
pub enum AccessLeg {
    VoWiFi(VoWifiLeg),   // 定义在 vowifi/，内部封装 ePDG/ESP 通道建立
    VoLTE(VolteLeg),     // 定义在 volte/，内部封装 bearer + xfrm 通道建立
}

impl AccessLeg {
    pub fn kind(&self) -> AccessLegKind {
        match self { AccessLeg::VoWiFi(l) => l.kind(), AccessLeg::VoLTE(l) => l.kind() }
    }
    pub async fn establish(&mut self) -> Result<AccessLegChannel, AccessError> {
        match self {
            AccessLeg::VoWiFi(l) => l.establish().await,
            AccessLeg::VoLTE(l)  => l.establish().await,
        }
    }
    // readiness / pcscf / local_addr / teardown 同样 match 分发...
}

// ================== ims/access.rs —— 信令层视角 ==================
// 受保护 SIP 通道：REGISTER/MESSAGE 事务只依赖这个，不碰底层加密细节。
// 建立完成后，两条腿都产出一个满足 ImsChannel 的对象（同样用 enum 收口，避免 dyn）。
pub trait ImsChannel {
    fn send_sip(&mut self, frame: &[u8]) -> Result<(), ImsError>;
    fn recv_sip(&mut self, timeout: Duration) -> Result<Vec<u8>, ImsError>;
    fn route(&self) -> ImsRoute;                 // 中立：本地/远端 addr+port（见 ims/context.rs）
    fn security_verify(&self) -> Option<String>; // sec-agree 协商结果，注入到请求头
}

pub enum AccessLegChannel {
    VoWiFi(vowifi::channel::EpdgSipChannel),  // send/recv 走用户态 ESP over ePDG
    VoLTE(volte::channel::XfrmSipChannel),    // send/recv 走内核 xfrm 保护的 socket
}
// impl ImsChannel for AccessLegChannel { ... match 分发 ... }
```

```rust
// ================== ims/context.rs —— 报文构造的中立入参 ==================
// 取代 live.rs 里对 &CarrierProfile 的直接依赖。VoWiFi 从 profile 填，VoLTE 从 bearer 发现结果填。
pub struct ImsRegisterParams<'a> {
    pub realm: &'a str,
    pub domain: &'a str,
    pub registrar: Option<&'a str>,
    pub local_port: u16,
    pub supported_header: &'a str,
    pub require_sec_agree: bool,
    pub user_agent: &'a str,
    pub pani: PaniValue,          // VoWiFi: IEEE-802.11...; VoLTE: 3GPP-E-UTRAN-FDD
    // ...其余从 ImsPolicy 归纳出的中立字段
}
pub struct ImsIdentity {          // 取代 LiveImsRegisterIdentity
    pub private_user: String,     // IMPI
    pub public_uri: String,       // IMPU
    pub contact_user: String,
    pub contact_user_phone: bool,
}
pub struct ImsRoute { pub local: SocketAddr, pub remote: SocketAddr }  // 取代 ImsClientTcpRoute
```

> 编排器持有 `Vec<AccessLeg>`，按 `sms_path.priority` 顺序遍历即可，全程无 trait object、无堆分配。信令函数签名从 `fn build_register_request(profile: &CarrierProfile, route: &ImsClientTcpRoute, ...)` 改成 `fn build_register_request(params: &ImsRegisterParams, id: &ImsIdentity, route: &ImsRoute, ...)`——这就是把 live.rs 抽进 ims/ 的**唯一实质改动**（其余是搬运 + 提升可见性）。

---

## 五、阶段 0：VoWiFi 代码回迁到 1.1.5（移植手册）

> 这是所有后续工作的前置。目标：让 1.1.5 重新拥有 1.1.3 的 VoWiFi 能力，且不破坏 1.1.5 的新功能。**按依赖顺序执行，每步后编译。**

### 步骤 0.1 — Cargo.toml 依赖合并

在 1.1.5 的 `[dependencies]` 中**加回**（保留已有的 `lettre`）：

```toml
num-bigint = "0.4"
num-traits = "0.2"
aes = "0.8"
cbc = { version = "0.1", features = ["alloc"] }
```

> 验证：`cargo tree` 确认无版本冲突（`aes 0.8`/`cbc 0.1` 与 `ring 0.17` 共存无已知问题）。

### 步骤 0.2 — 拷入 vowifi 子模块目录

将 `SimAdmin-main-vowifi/backend/src/vowifi/` 整个目录（31 个文件）拷贝到 `SimAdmin-main/backend/src/vowifi/`。此目录内部自洽，通常不需改动。

> ⚠️ 若打算同时回迁语音信令，则从 `SimAdmin/backend/src/vowifi/voice.rs` 额外拷入 `voice.rs`，并在 `mod.rs` 加 `pub mod voice;`（语音属阶段 E，阶段 0 可暂不带）。

### 步骤 0.3 — db.rs（**最高危**）

1. `SmsMessage` 加回字段：`#[serde(default = "default_sms_transport")] pub transport: String`，并加 `default_sms_transport()` 返回 `"modem"`。
2. **逐一核对所有读取 `SmsMessage` 的 SELECT 语句与 `row.get(索引)`** —— 新字段改变列索引，必须同步（这是静默 bug 的唯一来源）。
3. 加回方法：`insert_sms_with_transport`、`insert_sms_at_with_transport`、`sms_id_by_pdu`，以及 `insert_sms` 内的 transport 归一化逻辑。
4. 搬回全部 `vowifi_*` 表、结构体、方法、脱敏辅助（`redact_vowifi_event_detail` 等），并把建表语句并入 `Database::new` 的迁移流程。
5. 加回 `ALTER TABLE ... ADD COLUMN transport`（照搬旧版列迁移方式）。
6. 搬回 vowifi 相关单测。

### 步骤 0.4 — config.rs

1. 加回 `VowifiConfig` + 3 个默认值函数 + `AppConfig.vowifi` 字段 + `ConfigManager` 的 `get_vowifi_config`/`set_vowifi_feature_enabled`/`set_vowifi_connection_enabled` + 2 单测。
2. ⚠️ **检查所有构造 `NotificationRule` 的代码补 `title_template` 字段**（1.1.5 新增，否则编译不过）。
3. 迁移函数保留 1.1.5 的 `migrate_update_templates`，不要带回旧的 `migrate_templates_to_remove_md5`。

### 步骤 0.5 — state.rs

加回：`use crate::vowifi::runtime::VowifiRuntime;`、`vowifi_runtime: Arc<VowifiRuntime>` 与 `vowifi_connect_lock: Arc<Mutex<()>>` 两个字段、`AppState::new` 的 `vowifi_runtime` 入参、`FromRef<AppState> for Arc<VowifiRuntime>` 实现。

> 注意：`vowifi_connect_lock` 是内部 `Arc::new(Mutex::new(()))` 初始化，**不是**构造参数；别和 `vowifi_runtime` 入参搞混。

### 步骤 0.6 — sms_listener.rs

1. `start_sms_listener` 签名加回 `config_manager: Arc<ConfigManager>` 参数。
2. 把 1.1.5 的 `scan_sms_paths(...)` 调用点改回 `maybe_scan_sms_paths(...)`，并搬回该函数及其"IMS 通路激活时跳过/去重 modem 扫描"的门控逻辑。

### 步骤 0.7 — handlers.rs

1. 顶部加回 `use crate::vowifi::{...}` 相关引入。
2. 搬回 13 个 vowifi handler + 全部辅助函数（`persist_vowifi_mt_deliveries`/`spawn_vowifi_*`/`reset_vowifi_runtime` 等）。
3. ⚠️ **先比对 1.1.5 的 eSIM profile 切换 handler 当前实现**，再在其中重新植入 vowifi 拆除/恢复钩子（`get_vowifi_config` → `reset_vowifi_runtime` → `spawn_vowifi_profile_switch_restore`）。

### 步骤 0.8 — main.rs

1. 加 `mod vowifi;`。
2. 构造 `vowifi_runtime`（参考旧版构造方式）。
3. `AppState::new` 按旧版参数顺序补传 `vowifi_runtime`（位置在 `airplane_mode_requested` 和 `cell_monitoring_active` 之间）。
4. `AppState::new` 之后补 `spawn_vowifi_auto_restore(app_state.clone())`。
5. `start_sms_listener` 调用处补传 `config_manager`（准备 `sms_config_clone`）。
6. 加回 13 条 `/api/vowifi/*` 路由（插在 `/api/ims/status` 和 `/api/voicemail/status` 之间）。
7. 保留 1.1.5 新增的 `/api/sim/details/refresh` 与 stats sampler 调用。

### 步骤 0.9 — 前端回迁（可选）

从 `SimAdmin-main-vowifi/frontend/` 回迁 VoWiFi 相关页面/组件（SIM 页的 WiFi Calling 连接状态、诊断时序图等）。前端可后置到功能联调阶段。

### 步骤 0.10 — 编译与验证

- `cargo check`（Windows + MinGW GNU 工具链，见附录环境说明）。
- `cargo test`：vowifi 相关单测 + db transport 单测全过。
- 重点回归：SMS 收发（transport 列索引）、eSIM 切换、通知渠道（Email/ServerChan3 不受影响）。

### 阶段 0 移植 Checklist（速查）

- [ ] 0.1 Cargo.toml 加回 4 个加密依赖，保留 lettre
- [ ] 0.2 拷入 `src/vowifi/` 目录
- [ ] 0.3 db.rs：transport 字段 + **列索引同步** + vowifi 表/方法
- [ ] 0.4 config.rs：VowifiConfig + **NotificationRule 补 title_template**
- [ ] 0.5 state.rs：2 字段 + 入参 + FromRef
- [ ] 0.6 sms_listener.rs：config_manager 参数 + maybe_scan 门控
- [ ] 0.7 handlers.rs：13 handler + **eSIM 钩子重新植入**
- [ ] 0.8 main.rs：mod + 路由 + AppState 传参 + auto_restore
- [ ] 0.9 前端 VoWiFi 页面（可后置）
- [ ] 0.10 编译 + 单测 + 回归验证

---

## 六、可复用资产清单

> 阶段 0 完成后，1.1.5 将拥有以下可复用资产（均来自回迁的 vowifi 模块）。

### 6.1 SIM AKA 运算 —— `vowifi/qmi_uim.rs` ⭐ 直接可用

```rust
pub fn execute_usim_authenticate_via_proxy_reason_with_retry(
    proxy_socket: &str, device_path: &str, slot: u8, aid: &[u8],
    rand: &[u8], autn: &[u8],
    attempts: usize, timeout: Duration, retry_delay: Duration,
) -> Result<UsimAkaApduResult, &'static str>
pub struct UsimAkaApduResult { pub res: Vec<u8>, pub ck: Vec<u8>, pub ik: Vec<u8>, pub auts: Option<Vec<u8>> }
pub const USIM_AID_PREFIX: &[u8] = &[0xa0,0x00,0x00,0x87,0x10,0x02];
```
- 已处理抽象/文件 socket、`0x61`(GET RESPONSE)、`0x6C`(长度重发)、`0xDB`/`0xDC` 标签。
- 约束：仅 `#[cfg(unix)]`；需设备运行 `qmi-proxy` 且 ModemManager 释放 QMI 端口。

### 6.2 3GPP SMS 编解码 —— `vowifi/sms.rs` ⭐ 逻辑可用（部分私有）

```rust
pub fn build_single_part_mo_submission(recipient, text, service_center) -> Result<MoSmsSubmission, SmsEncodingError>
pub fn parse_mt_rp_data(body: &[u8]) -> Result<MtSmsDeliver, SmsEncodingError>
pub fn classify_rp_ack(body, expected_reference) -> RpduAckState
pub fn build_network_rp_ack(reference: u8) -> Vec<u8>
```
- `MtSmsDeliver` 含 `segment_reference: Option<u16>` / `segment_sequence: u8` / `segment_total: u8`（真实字段名带 `segment_` 前缀，勿写成裸 `sequence`/`total`），`is_duplicate_delivery()` 可去重。
- **需改造**：GSM7/UCS2/BCD 编解码与 UDH 解析为私有，需提升 `pub` 或抽取共享；MO 只支持单段，长短信分段需自写。

### 6.3 SIP 响应解析 —— `vowifi/ims.rs`

```rust
pub fn parse_sip_response(response: &str, expected_realm: &str) -> Result<SipResponseSummary, ImsRegisterError>
```
- **需改造**：当前丢弃真实 nonce。`SipResponseSummary` 不含 nonce 明文字段，`DigestChallengeSummary` 只记录 `challenge_token_present: bool` 且 `values_redacted:true`（`ims.rs`）；做 Digest-AKA 必须改造为保留真实 nonce/realm/qop/opaque。
- ⚠️ **注意**：`ims.rs` 的 `build_initial_register`/`build_authenticated_register` 返回的是 `SipMessageSummary`（dry-run 计划），**不是可上线字节**。

### 6.3b SIP 请求真实报文构造 —— `vowifi/live.rs`（⭐ 之前遗漏，工作量重估关键）

真实的线格式（wire-format）SIP 请求构造**已存在于 `vowifi/live.rs`**（均为 live.rs 内私有函数）：

```
build_live_sms_message_request     // 构造 SIP MESSAGE（承载 SMS）
build_live_sms_rp_ack_request      // 构造 RP-ACK 的 MESSAGE
build_live_invite_request          // 构造 INVITE（语音）
build_live_ack_request             // 构造 ACK
build_register_request             // 构造 REGISTER
```

- **影响**：VoLTE 腿的真实 SIP 报文**不必从零写**，可参考/提升 `live.rs` 的实现。但这些函数处于 VoWiFi 语境（SMS over IPsec/ePDG），需适配 VoLTE 语境（VoLTE 特有的 `P-Access-Network-Info: 3GPP-E-UTRAN-FDD`、Contact accesstype 等头）。
- **定位修正**：资产表里"SIP 请求构造"应从"✗ 自写"改为"⚠️ live.rs 有 vowifi 版真实实现，需适配 + 提升 pub"。

### 6.4 VoWiFi 语音信令 —— `SimAdmin` 版 `vowifi/voice.rs`

呼叫状态机 + SDP + RTP/AMR 编解码 + 双腿选路，10 单测已验证。阶段 E 移植到 `ims/` 共享层。

### 6.5 配置/DB 持久化模式

回迁后 `VowifiConfig`/`ConfigManager` 门禁模式、`SmsMessage.transport`（`"modem"`/`"vowifi_ims"`，可扩展 `"volte_ims"`）可直接仿用。

### 6.6 CS 语音/短信 —— 1.1.5 已完整

```
/api/call/dial /answer /hangup /hangup-all /volume /forwarding /settings /history
/api/sms/send /list /conversation /stats /batch-delete /clear
```

### 6.7 可复用资产优先级速查表

| 需求 | 直接可用 | 需改造/自写 |
|------|---------|------------|
| SIM AKA 运算 | ⭐ `qmi_uim::execute_usim_authenticate_*` | 仅 unix |
| 读 IMSI | ✗ | `modem_manager.rs`/D-Bus 封装 |
| SMS-SUBMIT/RP-DATA 编码(MO) | ⭐ `sms::build_single_part_mo_submission` | 长短信分段 |
| SMS-DELIVER/RP-DATA 解码(MT) | ⭐ `sms::parse_mt_rp_data` | — |
| GSM7/UCS2/BCD 编解码 | 逻辑完整但私有 | 提升 pub/抽取 |
| RP-ACK 处理 | ⭐ `classify_rp_ack`/`build_network_rp_ack` | — |
| SIP 响应解析 | `ims::parse_sip_response` | 保留真实 nonce |
| SIP 请求构造 | ⚠️ `live.rs` 有 vowifi 版真实实现 | 适配 VoLTE 语境 |
| IMS Digest-AKA 计算 | ✗ | 用 CK/IK/RES 自写 |
| 语音信令(INVITE/SDP/RTP) | ⭐ voice 版 `voice.rs` | 移植共享层 |
| 内核 IPsec(ip xfrm) | ✗ | 自写（参考 VoLTE 行为） |
| IMS bearer | ✗ | 自写（ModemManager D-Bus） |
| 配置/DB 持久化 | ⭐ 现有模式 | 仿写 VolteConfig |

---

## 七、从 VoLTE 二进制还原的实现要点

> 对 `SimAdmin-VoLTE` 二进制的静态分析结论，作为 VoLTE 腿的**行为规格参考**（非源码）。

### 7.1 运行阶段（对齐前端 `volteStatus.js`）

```
disabled → starting → identity(读USIM) → identity_aka(读鉴权材料)
    → radio(等LTE) → pcscf(发现P-CSCF) → modem(等ModemManager)
    → bearer(建立IMS bearer) → register_ipsec / register_udp(IMS注册)
    → registered(短信已接管)
```

### 7.2 `src/volte.rs` 函数分布（由行号锚点聚类还原）

| 行号区间 | 推断职责 |
|----------|---------|
| ~760 | IPsec 上下文清理（`ip xfrm policy/state flush`） |
| ~1360-1650 | IMS data path probe、bearer 就绪判定、SIP 头常量、xfrm 安装 |
| ~1580-1850 | IMS bearer 连接/删除陈旧 bearer、P-CSCF 发现前清理 |
| ~1960-2020 | bearer 重建（匹配漫游策略）、连接重试 |
| ~2220-2500 | IPsec 注册链路：REGISTER→401→AKA→200 OK；失败降级 UDP |
| ~2570-2950 | IPsec 运行时：MO/MT 短信、RP-ACK、非 MESSAGE 请求应答 |
| ~3050-3400 | Digest 鉴权：realm/nonce/qop/opaque、AKAv1/v2-MD5、Security-Server、USIM AID |
| ~3590-3720 | MO SMS 多变体发送（IPv4/IPv6 多尝试）、SIP 响应处理 |
| ~5370-5650 | MT SMS 落库：去重标记、多段拼接缓存、随机端口/SPI 生成 |

### 7.3 关键技术锚点（二进制字符串证据）

- **IPsec**：`ip xfrm`、`policy/flush`、`src/dst/proto/esp/spi`、`alg=hmac-md5-96;ealg=null`、`Native VoLTE IPsec xfrm installed`。
- **IMS bearer**：`--wds-start-network=apn=ims,3gpp-profile=`、`SIMADMIN_MM_IMS_BEARER`、`/org/freedesktop/ModemManager1/Bearer/`。
- **SIP 短信头**：`Content-Type: application/vnd.3gpp.sms`、`P-Preferred-Service: urn:urn-7:3gpp-service.ims.icsi.sms`、`Accept-Contact: *;+g.3gpp.smsip`、`P-Access-Network-Info: 3GPP-E-UTRAN-FDD`、`User-Agent: SimAdmin VoLTE`。
- **鉴权**：`AKAv1-MD5`、`AKAv2-MD5`、`http-digest-akav2-password`、`AKA returned AUTS, requesting resync`。
- **MT 处理**：`MT SMS received`、`MT RP-ACK SIP response`、`MT multipart SMS assembled`、`Stored native VoLTE MT SMS`、`Skipped duplicate native VoLTE MT SMS`。
- **协同**：`SMS listener paused while VoLTE IMS SMS path is registered`（CS 监听器与 IMS 路径互斥）。
- **降级**：`IPsec registration failed, falling back to plain UDP SIP`。
- **依赖检查**：`volte_dependency_missing:ip`（缺 `ip` 命令提示 OpenWrt 装 `ip-full`）。

### 7.4 对外 API（前端还原）

```
GET  /api/volte/control     # 查 VoLTE 运行状态（stage/注册模式/收发计数）
POST /api/volte/feature     # 开关（body: {enabled: bool}）
```
配置结构：`VolteConfig { feature_enabled, sms_enabled }`。

---

## 八、配置模型设计

### 8.1 短信/语音独立的分层优先级配置

```rust
// config.rs —— 新增（全部 #[serde(default)] 保证向后兼容）

pub struct PathLayerConfig { pub kind: AccessPathKind, pub enabled: bool }  // VoWiFi/VoLTE/Cs

pub struct SmsPathPolicy {
    #[serde(default = "default_sms_path_order")]
    pub priority: Vec<PathLayerConfig>,          // 顺序=优先级，用户可自定义
    #[serde(default = "default_true")] pub dedupe_enabled: bool,
    #[serde(default = "default_true")] pub cs_fallback_receiver: bool,
}

pub struct VoicePathPolicy {
    #[serde(default = "default_voice_path_order")]
    pub priority: Vec<PathLayerConfig>,
    #[serde(default)] pub gateway_mode: bool,    // 无音频硬件设备置 true
}

pub struct VolteConfig {
    #[serde(default)] pub feature_enabled: bool,
    #[serde(default)] pub connection_enabled: bool,
    #[serde(default = "default_true")] pub sms_enabled: bool,
    #[serde(default)] pub voice_enabled: bool,
    #[serde(default)] pub prefer_ipsec: bool,    // true=优先IPsec失败降级UDP
    #[serde(default = "d_restore_delay")] pub auto_restore_initial_delay_secs: u64,
    #[serde(default = "d_restore_attempts")] pub auto_restore_attempts: u8,
    #[serde(default = "d_restore_retry")] pub auto_restore_retry_delay_secs: u64,
}

pub struct VilteConfig {
    #[serde(default)] pub feature_enabled: bool,
    #[serde(default = "default_h264")] pub codec: String,
}

pub struct AppConfig {
    // ... 1.1.5 现有字段（含 vowifi 回迁后）...
    #[serde(default)] pub volte: VolteConfig,
    #[serde(default)] pub vilte: VilteConfig,
    #[serde(default)] pub sms_path: SmsPathPolicy,
    #[serde(default)] pub voice_path: VoicePathPolicy,
    #[serde(default)] pub trunk: TrunkConfig,
}
```

### 8.2 默认优先级（可被用户覆盖）

```rust
fn default_sms_path_order() -> Vec<PathLayerConfig> {
    vec![
        PathLayerConfig { kind: VoWiFi, enabled: true },
        PathLayerConfig { kind: VoLTE,  enabled: true },
        PathLayerConfig { kind: Cs,     enabled: true },
    ]
}
```

### 8.3 配置联动规则

- 关 `feature_enabled` 连带关 `connection_enabled`（沿用 `set_vowifi_*` 门禁模式）。
- 某层 `enabled=false` 时编排器跳过。
- 语音 `gateway_mode=true` 时禁用本地放音，只走 RTP relay。

---

## 九、收信去重与活跃监听者机制

### 9.1 问题本质

- **发信**：每条消息临时选一条腿，失败换下一条 —— 简单。
- **收信**：必须有腿处于注册/监听态；但不能同时在多条 IMS 腿上注册同一号码，否则**重复收信**。

### 9.2 活跃监听者选举

```
规则：同一时刻，IMS 侧只有【一条】腿作为"活跃监听者"。
1. 按 sms_path.priority 顺序，取第一个 enabled 且 readiness=Registered 的 IMS 腿。
2. 该腿成为活跃监听者，在其上注册并监听 MT 短信。
3. CS 监听器受 cs_fallback_receiver 控制：
   - 某 IMS 腿活跃 → 暂停 CS 监听（对齐二进制 "SMS listener paused while VoLTE IMS SMS path is registered"）
   - 或保持 CS 监听但强制去重。
4. 活跃腿掉线 → 切换到下一优先级腿，重新注册。
```

### 9.3 去重实现

```rust
// 复用 vowifi::sms::MtSmsDeliver::is_duplicate_delivery()
// 增强跨传输去重：DB 按指纹判重
// 注意真实字段名：segment_reference: Option<u16> / segment_sequence: u8 / segment_total: u8
fn message_fingerprint(msg) -> String {
    hash(scts, originator, content_hash, segment_reference, segment_sequence)
}
```
- DB 加去重标记列/唯一索引，插入前查指纹（对齐二进制 `Failed to check native VoLTE MT SMS duplicate marker`）。
- 长短信按 `segment_reference` 缓存各段收齐再落库（对齐 `MT multipart SMS assembled`）。

---

## 十、SIP Trunk / Asterisk 对接设计

### 10.1 定位

```
Linphone/软电话 ──SIP/RTP──► Asterisk/FreePBX ──SIP/RTP──► SimAdmin(Trunk)
                                                              │
                                                    内部编排：IMS/CS 腿
                                                              │
                                                     VoWiFi/VoLTE/CS ──► 运营商
```

### 10.2 两种对接形态

1. **外部 PBX 模式（推荐先做）**：SimAdmin 实现 SIP UAS/UAC，注册为 FreePBX 一条 trunk；RTP 在 SimAdmin 与 PBX 间转发。
2. **内嵌 Asterisk 模式（可选/后期）**：设备内运行精简 Asterisk，SimAdmin 作 channel driver 后端。复杂度高。

### 10.3 RTP Relay（无音频硬件设备的关键）

```rust
// trunk/rtp_relay.rs —— 设备不解码音频，只在两个 RTP 端点间转发 UDP 包
pub struct RtpRelay { ims_side: UdpSocket, trunk_side: UdpSocket }
// 可选：codec 不匹配时转码（AMR↔G711/opus），否则透传由 PBX 转码
```
- 纯转发：设备只搬运 UDP 包，无需音频硬件 —— **随身 WiFi 可行**。
- 复用 voice 版 `RtpPacket` 做包头处理；`SipEndpointBridge` seam 作对外接入点。

### 10.4 安全提醒 ⚠️

- 对外 SIP endpoint 必须强制鉴权（SIP Digest / IP 白名单 / TLS-SRTP）。
- **不要照搬** VoLTE 二进制 SMS 端点的开放模式（其 `/api/volte/*` 仅受功能开关保护，无端点级鉴权）。
- 网关模式默认绑定内网接口，显式配置才对公网开放。

---

## 十一、分阶段实施路线图

> 每阶段独立可编译、带单元测试。触碰上游代码的阶段标注 ⚠️。
>
> **进度标记（本次更新）**：🟢 已完成/部分完成 · 🔴 未开始。详见第零节快照与第十二章 TodoList。

### 阶段 0：VoWiFi 回迁到 1.1.5 ⚠️（前置，见第五章）— 🟢 不适用/已具备
> **状态说明**：当前工作树 `SimAdmin` 本就是 1.1.3+vowifi 基线，VoWiFi 能力已在树中，**无需回迁**。本章手册保留，用于**未来迁移到 1.1.5 上游底座**时参考（见 TodoList「迁移到 1.1.5」）。

### 阶段 A：抽取共享 IMS 核心 ⚠️（**前置于 VoLTE，取代旧“可选阶段C”**）— 🟢 部分完成
> **状态说明**：已轻量抽出 `ims/digest_aka.rs` + `ims/sip_frame.rs`，volte 与 vowifi/live.rs 均已复用（消除 AKA/HMAC/SIP 组帧重复）。**未做**完整的 `ImsChannel` trait / `ImsRegisterParams` 中立入参 / `live.rs` 报文构造上移（见 TodoList A2/A3/A6/A7）。
> **决策修正（相对旧版）**：经核对 `live.rs`（6585 行）后确认，VoWiFi 与 VoLTE 在 IMS 层高度重合（REGISTER 事务、Digest-AKA、SIP MESSAGE/RP-ACK 构造、响应解析/粘包全部一致），差异只在“受保护通道”。因此**先抽共享层再写 VoLTE**，比“VoLTE 独立重写再回头抽取”省 3000+ 行重复代码、少一次回归。旧版把这步列为“可选/后置”的结论已作废（当时误判 `ims.rs` 是唯一 SIP 载体，实际真实报文在 `live.rs`）。
- A1 建 `ims/` 模块骨架 + `context.rs`（`ImsIdentity`/`ImsRoute`/`ImsRegisterParams` 中立类型）
- A2 从 `live.rs` 抽 `ims/sip_message.rs`（`build_register/message/rp_ack/invite/ack`，入参改吃 `ImsRegisterParams` 而非 `&CarrierProfile`）
- A3 从 `live.rs` 抽 `ims/sip_parse.rs`（`parse_sip_status`/`sip_body`/`sip_complete_frame_len`/`sip_header_values`）
- A4 从 `live.rs` 抽 `ims/digest_aka.rs`（`aka_digest_password`+`hmac_md5`+nonce 解码；**已带单测，一并搬**）
- A5 `ims/register.rs` REGISTER 事务骨架（initial→401→auth→200，传输无关）
- A6 `ims/access.rs` 定义 `ImsChannel` trait + `AccessLeg` enum（见 §4.3）
- A7 `vowifi/channel.rs` 让 VoWiFi 实现 `ImsChannel`（内部仍走 ESP over ePDG/`ImsClientTcpRoute`）；`live.rs` 瘦身改调 `ims::`
- A8 **VoWiFi 全量回归**（10 个 voice 单测 + ims/live 单测全过，行为不变）
- 验证：全离线单测，Windows CI 可跑（`ims/` 无 IO）

### 阶段 B：VoLTE SMS 腿（复用阶段 A 的 `ims/`）— 🟢 离线层完成
> **状态说明**：`volte/` 下 identity/bearer/pcscf/ipsec/sip/sms/runtime 全套离线层已实现并单测（离线可验证部分）。**未做**：`channel.rs` 的 `ImsChannel` 实现（当前 volte 直接用自己的 sip.rs 而非经统一 ImsChannel）、`live.rs` 真机 IO 装配、真机注册收发验证。
- B1 `volte/identity.rs` IMSI（`AT+CIMI`）+ USIM AID（复用 `vowifi::qmi_uim`，见 §4.2.3）
- B2 `volte/bearer.rs` ModemManager IMS bearer 建立/删除陈旧/重建
- B3 `volte/pcscf.rs` P-CSCF 发现（data-path probe）
- B4 `volte/ipsec.rs` 内核 `ip xfrm` SA/策略/清理 + 依赖检查 ⚠️
- B5 `volte/channel.rs` 实现 `ImsChannel`（xfrm 保护的 socket；IPsec 优先/降级 UDP）
- B6 `volte/runtime.rs` VoLTE 状态机（stage 对齐前端契约）；REGISTER 走 `ims::register`
- B7 `volte/sms.rs` MT/MO：调 `ims::sip_message` + 复用 `sms::parse_mt_rp_data`/`build_single_part_mo_submission`；MO 长短信分段自写
- B8 `config.rs` `VolteConfig` + `handlers.rs` 两个 handler + `main.rs` 两条路由
- 验证：离线单测；真机需目标设备（抓包比对）

### 阶段 C：三层 SMS 编排器 ⚠️ — 🔴 未开始
> `orchestrator/` 目录尚不存在。
- C1 `config.rs` `SmsPathPolicy`（独立于语音的优先级 + 各层 enabled）
- C2 `orchestrator/sms_router.rs` 优先级发送 + 回退
- C3 `orchestrator/listener_election.rs` 活跃监听者选举（同一时刻仅一条 IMS 腿注册收信）
- C4 收信去重（DB 指纹列/唯一索引 + 跨传输判重）
- C5 改造 `sms_listener.rs` IMS 活跃时暂停/去重 CS
- C6 API + 前端策略 UI

### 阶段 D：SIP Trunk 网关 — 🔴 未开始
> `trunk/` 目录尚不存在。这是让内网软电话经设备拨打运营商电话的关键一步。
- D1 `trunk/sip_endpoint.rs` UAS/UAC
- D2 `trunk/rtp_relay.rs` RTP 转发
- D3 `trunk/bridge.rs` 内外腿桥接
- D4 强制鉴权 + 内网默认 ⚠️安全
- 与本地 Linphone/Asterisk 联调

### 阶段 E：语音编排（网关模式） — 🟡 部分完成（信令骨架已就位）
> 已完成：`access/volte/voice.rs`（VoLTE 语音编排层，复用参数化后的 `voice.rs` 信令）、SIP INVITE/ACK/BYE/CANCEL/200OK 报文构造、`access/volte/rtp_relay.rs`（RTP 双向转发纯逻辑核心 + `#[cfg(unix)]` UDP relay 循环骨架）。未完成：`VoicePathPolicy` 配置、`voice_router` 选路、relay 接入真实呼叫流程、经 Trunk 桥接外部软电话、真机通话。
- E1 `config.rs` `VoicePathPolicy`
- E2 移植呼叫状态机/SDP 到共享层 `ims/`（依赖阶段 A；voice.rs 的 INVITE/SDP/RTP 逻辑）
- E3 VoLTE 语音腿 INVITE over IMS（复用阶段 A `ims/` + 阶段 B `volte/channel.rs`）
- E4 `orchestrator/voice_router.rs` 语音选路（低延迟）
- E5 通过 Trunk 桥接外部软电话
- 注意：CS 语音在无音频硬件设备不纳入网关

### 阶段 F：ViLTE 视频
- F1 SDP video m-line（H.264）
- F2 视频 RTP 转发（纯转发不转码）
- F3 `VilteConfig` + API + UI

### 依赖关系图

```
阶段0 (VoWiFi回迁)
  └──► A (抽取共享 ims/ 核心 + VoWiFi 改用 ImsChannel)
        └──► B (VoLTE SMS 腿, 复用 ims/)
              ├──► C (三层 SMS 编排器)
              ├──► D (SIP Trunk 网关)
              └──► E (语音编排, 复用 ims/ 的 INVITE/SDP) ──► F (ViLTE)
```

> **顺序变更说明**：旧版顺序是 “A=VoLTE核心 → B=编排 → C=共享层(可选)”。新版把**共享层提到最前（阶段 A）**，VoLTE 腿（阶段 B）直接建立在共享层之上。这样 VoWiFi 与 VoLTE 的 SIP/AKA 逻辑从一开始就是同一份代码，避免先写两份再合并。语音（E）与 ViLTE（F）也天然复用 `ims/` 里的 INVITE/SDP，不再单独依赖一个“可选阶段”。

---

## 十二、完整 TodoList

> **重排序说明（本次更新）**：按"已完成 → 部分完成 → 未开始"排序。勾选状态针对**当前 `SimAdmin`（1.1.3+vowifi+voice）工作树**的真实代码。`[x]`=已完成、`[~]`=部分完成、`[ ]`=未开始。
>
> ⚠️ 标注差异：实际实现与原规划有偏差处以 `→ 实际:` 标出。

---

### ✅ 已完成 — 目录/架构重构（原文档无此阶段，实施中新增）
- [x] R1 领域化分层：22 个平铺文件 → 2 根文件 + 11 领域目录（api/cellular/messaging/notify/network/system/sim/infra + automation）
- [x] R2 接入腿分组：vowifi/volte 收进 `access/` 伞下（→ 实际比规划的"顶层平级"更进一步，归到 access/ 下）
- [x] R3 共享核心 `ims/` 抽出并被两腿复用
- [x] R4 每领域一个 git commit 检查点；全程 378 单测回归全绿；已建 git 基线
- [x] R5 `backend/src/README.md` 源码结构说明文档

### 🟢 部分完成 — 阶段 A：共享 IMS 核心
> → 实际：采用了**轻量抽离**（先抽最高价值、字节级重复的部分），而非一次性完成规划中的完整 `ImsChannel`/`context` 抽象。
- [x] A1 新建 `ims/` 模块（`mod.rs` + clean-room 声明 + 中立 `ImsError`），加入 `mod ims;`
- [x] A5 `ims/digest_aka.rs`：`aka_digest_password`/`hmac_md5`/nonce 解码（AKAv1/v2-MD5，**RFC 2617/2104/3310 测试向量单测**）
- [x] A4' `ims/sip_frame.rs`：`parse_status`/`body`/`complete_frame_len`/`header_values`/`header_uri`/`sip_host`/`quote_param`（组帧/解析原语，含粘包处理）
- [x] A9' volte 与 vowifi/live.rs **均已改调 `ims::`**（消除 AKA/HMAC/SIP 组帧的重复实现）；**VoWiFi 10 voice 单测 + 全量回归全绿**
- [ ] A2 `ims/context.rs`：中立 `ImsRegisterParams`/`ImsIdentity`/`ImsRoute`（**未做**；volte/vowifi 目前仍各自持有参数）
- [ ] A3 `ims/sip_message.rs`：从 `live.rs` 抽出 `build_*_request` 到共享层（**未做**；volte 的 SIP 报文构造目前在 `access/volte/sip.rs` 自有一份）
- [ ] A6 `ims/register.rs`：REGISTER 事务骨架（**未做**）
- [ ] A7 `ims/access.rs`：`trait ImsChannel` + `enum AccessLeg`（**未做**；这是把两腿通道差异真正收敛的关键，目前尚未抽象）
- [ ] A8 `ims/sms_codec.rs`：短信编解码上移/重导出（**未做**；仍在 `access/vowifi/sms.rs`，volte 通过 `crate::access::vowifi::sms` 复用）

### 🟢 部分完成 — 阶段 B：VoLTE SMS 腿
> → 实际：离线信令层全部就位并单测通过；真机 IO 与 `ImsChannel` 抽象未做。
- [x] B1 `access/volte/mod.rs` + `config.rs`（`VolteConfig`：feature/sms/voice/connection + auto-restore 三元组）加入接线
- [x] B2 `access/volte/identity.rs` IMSI + USIM AID（复用 `access::vowifi::qmi_uim`）
- [x] B3 `access/volte/bearer.rs` ModemManager IMS bearer 建立/重建/探测
- [x] B4 `access/volte/pcscf.rs` P-CSCF 发现
- [x] B5 `access/volte/ipsec.rs` `ip xfrm` SA/策略/清理 + 依赖检查 ⚠️
- [x] B7 `access/volte/runtime.rs` VoLTE 状态机（stage 对齐前端契约）
- [x] B8 `access/volte/sms.rs` MT/MO 链路（复用 `parse_mt_rp_data`/`build_single_part_mo_submission`）+ 单测
- [x] B9 `handlers.rs` handler + `main.rs` 路由（`/api/volte/control` `/api/volte/feature` `/api/ims/status` 等）
- [x] B' `access/volte/sip.rs` 真实 SIP 报文构造（REGISTER/MESSAGE/RP-ACK/INVITE/ACK/BYE/CANCEL）+ 单测
- [x] B'' `access/volte/digest_aka.rs` 适配器（复用 `ims::digest_aka`，映射 `VolteError`）
- [ ] B6 `access/volte/channel.rs` 实现 `ImsChannel`（**未做**；依赖阶段 A7 的 trait）
- [ ] B-live `access/volte/live.rs` `#[cfg(unix)]` 真机 IO 装配（**未做**）
- [ ] B-mo-seg MO 长短信分段（>160 GSM7 / >70 UCS2）自写 UDH（**未做/待确认**）
- [ ] B-真机 真机验证：真实 VoLTE 注册 + 收发短信 + 抓包比对（**未做**）

### 🟢 部分完成 — 阶段 E：语音（信令 + 编排骨架）
> → 实际：本轮提前做了语音的信令层与编排骨架（原计划 E 在 C/D 之后）。
- [x] E-voice-param `voice.rs` 参数化：抽出中立 `VoiceParams`，解开对 `CarrierProfile` 的耦合；vowifi 10 单测回归全绿
- [x] E-volte-voice `access/volte/voice.rs` VoLTE 语音编排（呼叫状态机驱动 + SDP offer/answer + 腿就绪）+ 单测
- [x] E-sip-invite `access/volte/sip.rs` INVITE/ACK/BYE/CANCEL/200OK 报文构造 + 单测
- [x] E-rtp-relay `access/volte/rtp_relay.rs` RTP 双向转发骨架（对称 RTP 学习 + 计数器 + `#[cfg(unix)]` UDP relay 循环）+ 单测
- [ ] E1 `config.rs` `VoicePathPolicy`（独立优先级 + gateway_mode）（**未做**，仅有 `voice_enabled` 开关）
- [ ] E2 呼叫状态机/SDP 移植到共享层 `ims/`（**未做**；目前在 `access/vowifi/voice.rs`）
- [ ] E3 VoLTE 语音腿真正 INVITE over IMS（**未做**；依赖 channel + 真机会话）
- [ ] E4 `orchestrator/voice_router.rs` 语音选路（**未做**）
- [ ] E5 relay 接入真实呼叫流程 + Trunk 桥接外部软电话 + 真机通话（**未做**）

---

### 🔴 未开始 — 阶段 C：三层 SMS 编排器 ⚠️
- [ ] C1 `config.rs` SmsPathPolicy + serde 默认值 + 门禁
- [ ] C2 `orchestrator/sms_router.rs` 优先级发送 + 回退（持有 `Vec<AccessLeg>`）
- [ ] C3 `orchestrator/listener_election.rs` 活跃监听者选举
- [ ] C4 DB 去重：指纹列/唯一索引 + 插入前查重 + 跨传输去重
- [ ] C5 改造 `sms_listener.rs` IMS 活跃时暂停/去重 CS
- [ ] C6 API 策略读写 + 前端策略 UI（严格对齐 volteStatus.js 契约）
- [ ] C7 编排选路/回退/去重单测

### 🔴 未开始 — 阶段 D：SIP Trunk 网关
- [ ] D1 `trunk/sip_endpoint.rs` UAS/UAC
- [ ] D2 `trunk/rtp_relay.rs` RTP 转发（可复用 `access/volte/rtp_relay.rs` 的 relay 核心）
- [ ] D3 `trunk/bridge.rs` 内外腿桥接
- [ ] D4 强制鉴权 + 内网默认绑定 ⚠️安全
- [ ] D5 API + UI Trunk 配置页 + Linphone/Asterisk 联调

### 🔴 未开始 — 阶段 F：ViLTE 视频
- [ ] F1 SDP video m-line（H.264）协商
- [ ] F2 视频 RTP 转发（纯转发不转码）
- [ ] F3 VilteConfig + API + UI + SDP video 单测 + 真机联调

### 🔴 未开始 — 迁移到 1.1.5 上游底座
> 当前工作树是 1.1.3 基线。若要合入最新上游，需做三方合并（见第五章手册），把 Email/ServerChan3、OTA 模板治理、eSIM 字段扩展等 1.1.5 新功能与本 fork 的 volte/ims/access 成果合并。
- [ ] U1 以 1.1.5 为底座，迁入 `ims/` + `access/` + 领域重构成果
- [ ] U2 合并 1.1.5 独立演进（Email/ServerChan3、`NotificationRule.title_template`、OTA 模板治理、`/api/sim/details/refresh`）
- [ ] U3 db.rs transport 列索引三方合并核对
- [ ] U4 全量回归 + 真机验证

### 贯穿性任务
- [x] X-重构安全：每步编译 + 单测 + git 检查点（重构阶段已贯彻）
- [~] 文档：每阶段更新开发/API 文档（本文档 + `backend/src/README.md` 已更新；其余待补）
- [~] 合规：clean-room，基于公开规范，标注修改内容与日期（GPLv3）（代码注释已标注，待系统化核对）
- [ ] 安全：所有对外网络端点审查鉴权（阶段 D 前必做）
- [ ] 提交上游：分阶段 PR，附单测与验证说明

---

## 十三、合规与可验证性边界

### 13.1 Clean-room 合规

- 对 `SimAdmin-VoLTE` 的描述基于**已编译二进制的行为观察**（字符串、前端 API、日志锚点），**不含反编译源码**。
- VoWiFi 回迁使用用户**合法持有的旧版 GPLv3 源码**（`SimAdmin-main-vowifi`），是 GPLv3 明确允许的行为。
- 所有 VoLTE 重构基于公开规范独立编写：
  - 3GPP TS 24.229（IMS SIP）、TS 24.341（SMS over IP）、TS 24.301（EPS/bearer）
  - RFC 3261（SIP）、RFC 3310（HTTP Digest AKA）、RFC 4566（SDP）、RFC 3550（RTP）、RFC 4867（AMR）
- GPLv3 义务：保留版权声明、衍生作品继续 GPLv3、标注修改内容与日期、附完整许可证。

### 13.2 可验证性边界（诚实声明）

**离线可保证（Windows + MinGW 单测即可验证）**：SIP 报文构造/解析、Digest AKA-MD5（对照规范向量）、TPDU/RP-DATA/GSM7/UCS2 编解码、`ip xfrm` 命令拼装、SDP 协商、RTP 包框帧、编排器选路/回退/去重逻辑。

**离线无法保证（需目标设备真机验证）**：真实连接运营商 IMS 的注册成败、真实 SIM AKA 运算（需 qmi-proxy + 硬件）、真实 LTE/P-CSCF 收发短信、特定运营商报文怪癖与定时器、内核 xfrm 在目标内核的行为、RTP relay 实际音质/时延。

**建议**：真机阶段务必抓包（SIP/RTP）比对，补齐二进制中不可见的字节级细节与定时参数。

### 13.3 已知风险点（简表）

> 完整的风险登记册（含可能性/影响/等级/触发条件/回退方案）见 §十四·4。此处为速查。

| 风险 | 说明 | 缓解 |
|------|------|------|
| **回迁三方合并** | 1.1.5 是独立演进分支，非 1.1.3 删 vowifi | 按第五章手册逐文件合并 |
| transport 列索引 | db.rs 加字段致 SELECT 错位。**改动面收敛**：仅 `sms_message_from_row` + `get_sms_messages` + `get_sms_conversation` 三处 | 同步这 3 个函数 + 覆盖往返单测 |
| **eSIM 切换钩子** | 1.1.5 该 handler 已演进 | 植入前先比对当前实现 |
| **title_template** | 1.1.5 新增字段致 vowifi 测试编译失败 | 构造 NotificationRule 处补字段 |
| 版本基线差异 | VoLTE 版基于 1.1.6-dev18 | 参考行为规格，不依赖其源码 |
| **共享层抽离改动 vowifi** | 从 `live.rs` 抽 SIP/AKA 到 `ims/`，解 `CarrierProfile`/ePDG 耦合，可能引入 VoWiFi 回归 | 每步编译；VoWiFi 全量单测回归；分小 commit |
| async trait 非对象安全 | `ImsChannel`/`AccessLeg` 无法直接 `dyn` | 用 enum 分发（见 §4.3） |
| 无音频硬件 | CS 语音无法网关化 | 仅网关模式 + IMS 腿 |
| 对外 SIP 安全 | endpoint 暴露风险 | 强制鉴权 + 内网默认 |
| unix-only | AKA/xfrm 仅 Linux | Windows 仅逻辑单测 |

---

## 十四、工程规范与交付标准

> 本章将前面偏"技术方案"的内容，补齐为一份可执行的**工程实施文档（Engineering Design / TDD）**所需的规范外壳，对齐常见大厂研发流程。技术内核见前十三章，本章约束"怎么落地、怎么算完成、怎么回退"。

### 14.1 版本与分支策略

**派生版本号**：本 fork 基于上游 `1.1.5`，同时行为参考未开源的 `1.1.6-dev18`。为避免与上游版本号冲突、又能表达血缘，采用 SemVer 的构建元数据后缀：

```
1.1.5+lilith.<n>       # n 为本衍生线的递增序号
```

- 不冒用 `1.1.6`（那是上游/二进制的版本，含义不同）。
- 每次对外可用的里程碑递增 `lilith.n`，并在 `CHANGELOG` 记录对应完成的阶段（0/A/B/…）。
- `VERSION` 文件与 `Cargo.toml` 的 `package.version` 保持一致。

**分支模型**（trunk-based，轻量）：

| 分支 | 用途 |
|------|------|
| `main` | 始终可编译、单测全绿；每个阶段合入一次 |
| `stage/<x>-<slug>` | 单个阶段的开发分支（如 `stage/0-vowifi-backport`、`stage/a-ims-core`、`stage/b-volte-sms`） |
| `fix/<slug>` | 缺陷修复 |

- 每个阶段一个 PR，**小步合入**；⚠️ 触碰上游代码的阶段（0=回迁、A=`live.rs` 瘦身、C=改 `sms_listener`）PR 描述必须列出改动的上游文件与理由。
- 提交信息遵循 Conventional Commits：`feat(volte): ...` / `fix(db): ...` / `chore: ...`；GPLv3 要求的"修改内容与日期"在提交历史中天然满足，另在文件头注释标注。

### 14.2 每阶段验收标准（Definition of Done）

每个阶段必须**全部**满足以下通用 DoD 才算完成：

- [ ] `cargo check` + `cargo build` 通过（Windows GNU 工具链，见附录 A）
- [ ] `cargo test` 全绿；新增代码有对应单测
- [ ] `cargo clippy -- -D warnings` 零告警
- [ ] `cargo fmt --check` 通过
- [ ] 新增/变更的对外 API 与前端契约（stage/phase/字段名）核对一致（见 §14.7）
- [ ] 涉及 DB 的改动附带迁移，且**幂等**（重复启动不报错）
- [ ] 新功能默认 `feature_enabled=false`（灰度，见 §14.5）
- [ ] PR 描述含：改动文件清单、测试结果、真机验证指引（若有）、回退方式

**各阶段专属验收项**（示例，实施时补全）：

| 阶段 | 关键验收项（除通用 DoD 外） |
|------|---------------------------|
| 0 | SMS 收发往返单测过；eSIM 切换回归过；Email/ServerChan3 通知不受影响；7 张 `vowifi_*` 表全部建成 |
| A（共享 IMS 核心） | `ims/` 从 `live.rs` 抽出后 **VoWiFi 全量回归通过**（10 voice 单测 + ims/live 单测）；`ims/` 无 IO、Windows CI 全量单测；AKAv1/v2-MD5 通过 **RFC 3310 测试向量**；SIP 请求构造/解析往返单测 |
| B（VoLTE SMS 腿） | `volte/channel.rs` 实现 `ImsChannel`；`ip xfrm` 命令参数序列断言；MT 多段拼接/去重单测；MO 分段单测；VoLTE 走 `ims::` 复用而非自写 SIP |
| C（编排器） | 选路优先级/回退/活跃监听者选举单测；跨传输去重指纹单测（同一短信经不同腿只落库一次） |
| D（Trunk） | 对外 SIP endpoint 鉴权用例（无凭据被拒）；默认绑定内网接口的用例 |

### 14.3 测试策略（测试金字塔）

| 层次 | 范围 | 运行环境 | 门槛 |
|------|------|---------|------|
| 单元测试 | 编解码/AKA/SIP 报文/xfrm 命令拼装/选路逻辑 | Windows CI + 真机 | 新增核心模块行覆盖率 ≥ 70% |
| 集成测试 | 状态机 dry-run 全流程、配置联动、DB 迁移幂等 | Windows CI | 每阶段至少 1 条 happy-path + 1 条降级路径 |
| 契约测试 | `/api/*` 响应字段与前端枚举对齐 | Windows CI | 字段/枚举值快照断言 |
| E2E / 真机 | 真实 IMS 注册、真实收发短信、RTP relay | **目标 aarch64 设备（用户执行）** | 抓包比对，人工签核 |

**`#[cfg(unix)]` 代码在 Windows CI 的处理**：AKA/xfrm 等 unix-only 逻辑，把"可离线验证的纯逻辑部分"（命令字符串拼装、报文编解码、SPI/端口生成算法）抽到**平台无关函数**里单测；真正的 syscall/进程调用留在 `#[cfg(unix)]` 薄封装层，Windows 上编译跳过、真机验证。避免"整块 unix-only 导致 Windows 完全测不到"。

### 14.4 风险登记册（Risk Register）

> 标准字段：可能性 (L/M/H) × 影响 (L/M/H) = 等级；含触发条件与回退方案。§13.3 是速查简表。

| ID | 风险 | 可能性 | 影响 | 等级 | 触发条件 | 缓解 / 回退 |
|----|------|:---:|:---:|:---:|---------|-----------|
| R1 | 回迁三方合并引入回归 | M | H | **高** | 阶段 0 合并后单测/回归失败 | 按 §五逐文件合并；每步编译；回退到合并前 tag |
| R2 | transport 列索引错位 | L | H | 中 | 读到的短信字段错乱/类型 panic | 只改 3 处集中函数；往返单测；回退该 commit |
| R3 | eSIM 钩子与 1.1.5 新逻辑冲突 | M | M | 中 | eSIM 切换后 vowifi 未拆除/恢复 | 植入前 diff 当前 handler；灰度关钩子可禁用 |
| R4 | AKA/Digest 计算与运营商不符 | M | H | **高** | 真机 REGISTER 401 二次挑战仍失败 | RFC 向量单测兜底；真机抓包迭代；降级 UDP |
| R5 | IPsec xfrm 与端口绑定不一致 | M | H | **高** | 内核丢包、注册超时 | 命令层单测；真机 tcpdump；降级明文 UDP SIP |
| R6 | IPv6 缺失导致 IPsec 不可用 | M | M | 中 | `ipsec_requires_ipv6` | 双模：无 IPv6 自动走 UDP 降级 |
| R7 | 对外 SIP endpoint 被滥用 | L | H | 中 | 公网暴露 + 无鉴权 | 强制鉴权 + 默认内网绑定 + 用例覆盖 |
| R8 | 上游后续演进与本 fork 冲突 | H | M | 中 | 上游发布新版本 | 保持模块边界清晰；定期 rebase；隔离改动 |
| R9 | async trait 非对象安全编译失败 | L | M | 低 | 用 `dyn ImsAccessLeg` | 用 enum 分发（§4.3） |
| R10 | 抽离 `ims/` 破坏 VoWiFi 现有行为 | M | H | **高** | 阶段 A 抽 `live.rs` 后 VoWiFi 单测/真机回归 | 抽离为**纯搬移+改入参**、不改逻辑；先让 `live.rs` 调 `ims::` 保持行为等价；VoWiFi 10 voice 单测 + ims/live 单测作回归门；分小 commit 便于二分定位 |
| R11 | 解 `CarrierProfile` 耦合时丢字段 | M | M | 中 | `ImsRegisterParams` 漏映射某运营商头（如 sec-agree/P-Visited-Network） | 逐字段对照 `ImsPolicy`/`RegisterPolicy` 建映射表；VoWiFi 回归覆盖各 header variant 单测（`register_header_variants_*`） |

### 14.5 灰度、开关与回滚

- **所有新功能默认关闭**：VoLTE/ViLTE/Trunk/多路径策略的 `feature_enabled` 默认 `false`，`#[serde(default)]` 保证旧配置文件平滑升级。
- **联动门禁**：关 `feature_enabled` 连带关 `connection_enabled`（沿用 `set_vowifi_*` 模式）。
- **回滚层级**：
  1. 运行时回滚：前端/API 关闭 feature 开关，退回 CS 路径，无需重启。
  2. 版本回滚：每阶段合入打 git tag（`v1.1.5+lilith.n`），出问题回退到上一 tag。
  3. DB 回滚：迁移只**加列/加表**，不删不改既有列；降级时新列被忽略，旧版本仍可读库。

### 14.6 数据库 Schema 迁移规范

- 所有建表/加列语句用 `CREATE TABLE IF NOT EXISTS` / `ALTER TABLE ... ADD COLUMN`，保证**幂等**。
- 加列一律带默认值（如 `transport` 默认 `"modem"`、去重指纹列可空），保证旧行可读。
- 迁移集中在 `Database::new` 的迁移流程里顺序执行；新增迁移**只追加不修改**历史迁移。
- 回迁涉及的对象清单（阶段 0 必须逐一核对）：
  - `sms_messages` 加 `transport` 列（+ 索引若有）
  - 7 张 `vowifi_*` 表：`vowifi_runtime_events`（含 2 索引）、`vowifi_runtime_snapshots`、`vowifi_sms_delivery`（含 2 索引）、`vowifi_sms_parts`（外键 → delivery）、`vowifi_esim_restore`、`vowifi_soak_runs`（含 2 索引）、`vowifi_soak_samples`（外键 → soak_runs）
  - 阶段 B 新增：短信去重指纹列 + 唯一索引

### 14.7 可观测性与接口契约

**结构化日志与错误码命名**：沿用 VoLTE 二进制观测到的 `volte_*` 错误码族风格（作为**行为参考清单**，clean-room 下用语义等价的 SimAdmin 风格自拟命名，不逐字照抄）。每个错误码语义单一、可 grep。stage 推进有明确日志锚点。

**前端契约（硬约束，阶段 B 验收项）**：`/api/volte/control` 的响应字段与 stage/phase/registration_mode 枚举值，必须与前端 `volteStatus.js` **完全一致**，否则现有 UI 不显示。完整枚举与字段表见 `SimAdmin/VOLTE_SMS_MODULE_DESIGN.md` §4（stage 12 值、phase 5 值、control 响应 ~18 字段、last_error 子串映射）。本文档不重复维护该表，以设计文档为准，实现时以契约快照测试锁定。

> 合规注：前端 JS 是 GPL 产物的一部分（可合法读取），保留其字段/枚举是**互操作性需要**，不构成对二进制的抄写。

### 14.8 代码评审 Checklist

每个 PR 评审时逐条核对：

- [ ] 是否满足本阶段 DoD（§14.2）
- [ ] ⚠️ 触碰上游文件的改动是否最小化、是否列明理由
- [ ] 是否有对应单测，覆盖 happy-path 与至少一条失败/降级路径
- [ ] DB 改动是否幂等、是否只增不改
- [ ] 对外网络端点是否鉴权、是否默认内网
- [ ] 是否可能读到/记录敏感值（IMSI/nonce/密钥）——需脱敏
- [ ] 新功能是否默认关闭、可运行时回滚
- [ ] `clippy`/`fmt` 是否通过

---

## 附录 A：编译环境

- Rust 工具链：`stable-x86_64-pc-windows-gnu`（GNU 版，自带链接器），rustc 1.97.0，位于 `D:\Program\Dev\Languages\Rust`。
- C 编译器：MinGW-W64 GCC 16.1.0，位于 `D:\Program\Dev\Languages\GCC\mingw64`（编译 rusqlite 内置 SQLite、ring 等 C 依赖）。
- 生产目标：`aarch64-unknown-linux-musl`（Debian ARM 蜂窝设备），Windows 本机仅做逻辑编译/单测验证。

## 附录 B：相关文档索引

- `SimAdmin-VoLTE/VOLTE_SMS_逆向分析.md` — VoLTE 二进制行为还原详析
- `SimAdmin/VOLTE_SMS_MODULE_DESIGN.md` — VoLTE 短信模块详细设计
- `SimAdmin/VOLTE_TRUNK_PROJECT_PLAN.md` — 综合架构与路线图
- `SimAdmin/VOWIFI_VOICE_MODULE.md` — VoWiFi 语音模块改造说明（voice 版）

---

*本文档为面向最新上游 `SimAdmin-main`（1.1.5）的扩展开发规划，随实施推进应持续更新。*
