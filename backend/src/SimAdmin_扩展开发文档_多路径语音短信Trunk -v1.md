# SimAdmin 扩展开发文档：多路径语音/短信统一接入与 SIP Trunk 网关

> **开发对象**：`SimAdmin-main`（**最新上游，版本 1.1.5**，无 vowifi、无 volte）
> **文档性质**：四版本对比 + VoWiFi 代码回迁 + VoLTE 逆向重构 + 二次开发扩展 + 分阶段实施路线图 + TodoList
> **撰写依据**：对以下四个项目的实际代码/二进制对比分析
> - `SimAdmin-main`（**最新上游 1.1.5**，vowifi 已被原作者移除，无 volte）
> - `SimAdmin-main-vowifi`（旧上游 1.1.3，**含完整 vowifi 脚手架**，原 `SimAdmin-main` 重命名而来）
> - `SimAdmin`（旧上游 1.1.3 + AI 编写的 `vowifi/voice.rs` 语音信令层）
> - `SimAdmin-VoLTE`（未开源的已编译二进制 1.1.6-dev18，含独立 `src/volte.rs`，经 clean-room 静态分析）
>
> **合规声明**：本文档对 `SimAdmin-VoLTE` 的描述均基于对已编译二进制的**行为级静态分析**（字符串锚点 + 前端 API），不含任何反编译的原始源码。VoWiFi 代码回迁使用的是用户已合法持有的旧版本 GPLv3 源码。所有重构实现基于公开 3GPP/RFC 规范独立完成（clean-room），遵循 GPLv3。

---

## 目录

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
| `db.rs` | `SmsMessage.transport` 字段；`insert_sms_with_transport`/`insert_sms_at_with_transport`/`sms_id_by_pdu`；全部 `vowifi_*` 表/结构/方法/脱敏 | ~1026 |
| `sms_listener.rs` | `start_sms_listener` 的 `config_manager` 参数；`maybe_scan_sms_paths` 门控逻辑 | ~61 |
| `handlers.rs` | 13 个 vowifi handler + 辅助函数 + eSIM 切换钩子 | ~1261 |
| `main.rs` | `mod vowifi;`、`vowifi_runtime` 构造、`AppState::new` 传参、`spawn_vowifi_auto_restore`、13 条 `/api/vowifi/*` 路由 | ~60 |

### 3.3 两个高危移植点（务必重点验证）

1. **`db.rs` 的 `SmsMessage.transport` 列索引同步** — 这是**静默运行时错误**风险。1.1.5 的 `SmsMessage` 只有 7 个字段，加回 `transport` 后，所有读取 SmsMessage 的 `SELECT` 语句和 `row.get(索引)` 映射都会因列位移而错乱，编译期不报错、运行时才炸。必须逐一核对所有 `insert_sms`/`get_sms_messages`/`get_sms_conversation` 等的列索引。

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

```
backend/src/
├── vowifi/                    # 【阶段0回迁】从 SimAdmin-main-vowifi 移植
├── ims/                       # 【阶段C新增】共享 IMS 核心
│   ├── sip_message.rs         # SIP 请求构造 + 响应解析（真实报文）
│   ├── digest_aka.rs          # IMS Digest AKAv1/v2-MD5 计算
│   ├── register.rs            # REGISTER 事务（401→AKA→200）
│   └── access.rs              # trait ImsAccessLeg（受保护通道抽象）
├── volte/                     # 【阶段A新增】VoLTE 接入腿
│   ├── config.rs              # VolteConfig
│   ├── bearer.rs              # ModemManager IMS bearer
│   ├── pcscf.rs               # P-CSCF 发现
│   ├── ipsec.rs               # 内核 ip xfrm SA/策略
│   ├── register.rs            # VoLTE 注册（IPsec优先/降级UDP）
│   ├── sms.rs                 # MT/MO 短信
│   └── identity.rs            # IMSI/USIM AID 读取
├── orchestrator/              # 【阶段B新增】编排层
│   ├── sms_router.rs
│   ├── voice_router.rs
│   └── listener_election.rs
└── trunk/                     # 【阶段D新增】SIP Trunk 网关
    ├── sip_endpoint.rs
    ├── rtp_relay.rs
    └── bridge.rs
```

### 4.3 核心 trait 设计

```rust
// ims/access.rs —— IMS 接入腿抽象（VoWiFi / VoLTE 各实现一份）
pub trait ImsAccessLeg: Send + Sync {
    fn kind(&self) -> AccessLegKind;                        // VoWiFi / VoLTE
    async fn establish(&mut self) -> Result<SipTransport, AccessError>;
    fn readiness(&self) -> LegReadiness;
    fn pcscf(&self) -> Option<SocketAddr>;
    fn local_addr(&self) -> Option<IpAddr>;
    async fn teardown(&mut self);
}
// 信令层只依赖 SipTransport，不关心底层是 ESP 还是 xfrm
pub struct SipTransport { /* TCP/UDP socket + 加密上下文 */ }
```

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
- `MtSmsDeliver` 含 `segment_reference/sequence/total`，`is_duplicate_delivery()` 可去重。
- **需改造**：GSM7/UCS2/BCD 编解码与 UDH 解析为私有，需提升 `pub` 或抽取共享；MO 只支持单段，长短信分段需自写。

### 6.3 SIP 响应解析 —— `vowifi/ims.rs`

```rust
pub fn parse_sip_response(response: &str, expected_realm: &str) -> Result<SipResponseSummary, ImsRegisterError>
```
- **需改造**：当前丢弃真实 nonce（`values_redacted:true`），做 Digest-AKA 需保留；不构造真实 SIP 请求，需自写。

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
| SIP 请求构造 | ✗ | 自写真实报文 |
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

### 阶段 0：VoWiFi 回迁到 1.1.5 ⚠️（前置，见第五章）
让 1.1.5 重获 VoWiFi 能力且不破坏新功能。**所有后续阶段的基础。**

### 阶段 A：VoLTE SMS 核心
- A1 `ims/sip_message.rs` 真实 SIP 请求构造 + 改造 `parse_sip_response` 保留真实 nonce
- A2 `ims/digest_aka.rs` 用 CK/IK/RES 实现 AKAv1/v2-MD5
- A3 `volte/bearer.rs` ModemManager IMS bearer
- A4 `volte/pcscf.rs` P-CSCF 发现
- A5 `volte/ipsec.rs` 内核 `ip xfrm` SA/策略 ⚠️
- A6 `volte/register.rs` REGISTER→401→AKA→200（IPsec优先/降级UDP）
- A7 `volte/sms.rs` MT（MESSAGE→复用 parse_mt_rp_data→RP-ACK→拼接→去重）+ MO
- A8 `volte/identity.rs` IMSI/USIM AID 读取
- 验证：离线单测；真机需目标设备

### 阶段 B：三层 SMS 编排器 ⚠️
- B1 `config.rs` `SmsPathPolicy`/`VolteConfig`
- B2 `orchestrator/sms_router.rs` 选路+回退
- B3 `orchestrator/listener_election.rs` 活跃监听者
- B4 收信去重（DB 指纹）
- B5 改造 `sms_listener.rs` IMS 活跃时暂停/去重 CS
- B6 API + 前端策略 UI

### 阶段 C：抽取共享 IMS 核心 ⚠️（评估性价比）
- C1 抽取 `vowifi/ims.rs`+voice 信令到 `ims/`
- C2 定义并实现 `ImsAccessLeg`（VoWiFi/VoLTE 腿）
- C3 `vowifi/` 依赖 `ims/`；VoWiFi 全回归

### 阶段 D：SIP Trunk 网关
- D1 `trunk/sip_endpoint.rs` UAS/UAC
- D2 `trunk/rtp_relay.rs` RTP 转发
- D3 `trunk/bridge.rs` 内外腿桥接
- D4 强制鉴权 + 内网默认 ⚠️安全
- 与本地 Linphone/Asterisk 联调

### 阶段 E：语音编排（网关模式）
- E1 `config.rs` `VoicePathPolicy`
- E2 移植呼叫状态机/SDP 到共享层（依赖 C）
- E3 VoLTE 语音腿 INVITE over IMS（复用 A 地基）
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
  └──► A (VoLTE SMS核心)
        ├──► B (SMS编排器)
        ├──► C (共享IMS核心) ──► E (语音编排) ──► F (ViLTE)
        └──► D (SIP Trunk) ───────────┘
```

---

## 十二、完整 TodoList

### 阶段 0 — VoWiFi 回迁（前置）
- [ ] 0.1 Cargo.toml 加回 num-bigint/num-traits/aes/cbc，保留 lettre
- [ ] 0.2 拷入 `src/vowifi/` 目录（31 文件）
- [ ] 0.3 db.rs：transport 字段 + **列索引同步** + vowifi 表/方法/建表迁移
- [ ] 0.4 config.rs：VowifiConfig + ConfigManager 方法 + **NotificationRule 补 title_template**
- [ ] 0.5 state.rs：vowifi_runtime/connect_lock 字段 + 入参 + FromRef
- [ ] 0.6 sms_listener.rs：config_manager 参数 + maybe_scan_sms_paths 门控
- [ ] 0.7 handlers.rs：13 handler + 辅助函数 + **eSIM 钩子重新植入**
- [ ] 0.8 main.rs：mod vowifi + 13 路由 + AppState 传参 + auto_restore + listener 传参
- [ ] 0.9 前端 VoWiFi 页面回迁（可后置）
- [ ] 0.10 cargo check/test + SMS/eSIM/通知回归验证

### 阶段 A — VoLTE SMS 核心
- [ ] A1 `ims/sip_message.rs` 真实 SIP 请求构造（REGISTER/MESSAGE/ACK）
- [ ] A1 改造 `parse_sip_response` 保留真实 nonce/realm/qop/opaque
- [ ] A2 `ims/digest_aka.rs` AKAv1-MD5/AKAv2-MD5（输入 CK/IK/RES）
- [ ] A2 AKA 计算单测（对照 3GPP 测试向量）
- [ ] A3 `volte/bearer.rs` IMS bearer 建立/删除陈旧/重建
- [ ] A4 `volte/pcscf.rs` P-CSCF 发现（data path probe）
- [ ] A5 `volte/ipsec.rs` `ip xfrm` SA/策略/清理 + 依赖检查
- [ ] A6 `volte/register.rs` REGISTER 事务（IPsec 优先/降级 UDP）
- [ ] A7 `volte/sms.rs` MT：MESSAGE 解析→RP-ACK→多段拼接→去重落库
- [ ] A7 `volte/sms.rs` MO：构造 MESSAGE + 长短信分段
- [ ] A8 `volte/identity.rs` IMSI + USIM AID 发现
- [ ] A 提升 `vowifi/sms.rs` 私有编解码为 pub 或抽取共享
- [ ] A SIP/TPDU/xfrm 离线单测 + Windows 编译 + 真机验证清单

### 阶段 B — 三层 SMS 编排器
- [ ] B1 `config.rs` SmsPathPolicy + VolteConfig + serde 默认值 + 门禁
- [ ] B2 `orchestrator/sms_router.rs` 优先级发送 + 回退
- [ ] B3 `orchestrator/listener_election.rs` 活跃监听者选举
- [ ] B4 DB 去重：指纹列/唯一索引 + 插入前查重 + 跨传输去重
- [ ] B5 改造 `sms_listener.rs` IMS 活跃时暂停/去重 CS
- [ ] B6 API `/api/volte/control` `/api/volte/feature` + 策略读写 + 前端 UI
- [ ] B 编排选路/回退/去重单测

### 阶段 C — 共享 IMS 核心（可选）
- [ ] C1 抽取 `vowifi/ims.rs` + voice `voice.rs` 到 `ims/`
- [ ] C2 定义 `ImsAccessLeg` trait + 实现两条腿
- [ ] C3 `vowifi/` 依赖 `ims/` + VoWiFi 全回归

### 阶段 D — SIP Trunk 网关
- [ ] D1 `trunk/sip_endpoint.rs` UAS/UAC
- [ ] D2 `trunk/rtp_relay.rs` RTP 转发（复用 RtpPacket）
- [ ] D3 `trunk/bridge.rs` 内外腿桥接
- [ ] D4 强制鉴权 + 内网默认绑定
- [ ] D API + UI Trunk 配置页 + Linphone/Asterisk 联调

### 阶段 E — 语音编排（网关模式）
- [ ] E1 `config.rs` VoicePathPolicy（独立优先级 + gateway_mode）
- [ ] E2 移植呼叫状态机/SDP 到共享层（依赖 C）
- [ ] E3 VoLTE 语音腿 INVITE over IMS
- [ ] E4 `orchestrator/voice_router.rs` 语音选路（低延迟）
- [ ] E5 Trunk 桥接外部软电话 + API/UI + 呼叫状态机单测

### 阶段 F — ViLTE 视频
- [ ] F1 SDP video m-line（H.264）协商
- [ ] F2 视频 RTP 转发（纯转发不转码）
- [ ] F3 VilteConfig + API + UI + SDP video 单测 + 真机联调

### 贯穿性任务
- [ ] 文档：每阶段更新开发/API 文档
- [ ] 合规：clean-room，基于公开规范，标注修改内容与日期（GPLv3）
- [ ] 安全：所有对外网络端点审查鉴权
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

### 13.3 已知风险点

| 风险 | 说明 | 缓解 |
|------|------|------|
| **回迁三方合并** | 1.1.5 是独立演进分支，非 1.1.3 删 vowifi | 按第五章手册逐文件合并 |
| **transport 列索引** | db.rs 加字段致 SELECT 错位（静默 bug） | 逐一核对所有行映射 |
| **eSIM 切换钩子** | 1.1.5 该 handler 已演进 | 植入前先比对当前实现 |
| **title_template** | 1.1.5 新增字段致 vowifi 测试编译失败 | 构造 NotificationRule 处补字段 |
| 版本基线差异 | VoLTE 版基于 1.1.6-dev18 | 参考行为规格，不依赖其源码 |
| 无音频硬件 | CS 语音无法网关化 | 仅网关模式 + IMS 腿 |
| 对外 SIP 安全 | endpoint 暴露风险 | 强制鉴权 + 内网默认 |
| unix-only | AKA/xfrm 仅 Linux | Windows 仅逻辑单测 |

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
