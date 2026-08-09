# SimAdmin 1.1.7-beta8：VoLTE 与数据连接逆向说明

> **作者标注：本说明由 OpenAI Codex 于 2026-07-31 独立完成。**
>
> 分析时直接使用原始 Beta8 ELF、发布包内前端资源与 systemd/udev 脚本；未读取或引用同目录的 `beta8_逆向工作笔记.md`，也未采用现有 IDA 数据库中的人工命名。

## 1. 结论摘要

Beta8 不是把 VoLTE 简单挂在普通上网连接上，而是实现了一个“普通数据面 + IMS 专用数据面”的资源分配器：

1. 普通移动数据可以由 NetworkManager/ModemManager 管理，也可以通过 DATA6 暴露出的 secondary QMI WDS 独立承载。
2. VoLTE 模块会创建 APN 为 `ims` 的专用 PDP/bearer，获取 IMS 地址和 P-CSCF，再自行完成 SIP REGISTER、USIM AKA、3GPP IPsec 和 IMS 短信收发。
3. 主 qmi0 与 DATA6 不能同时被普通数据占用，否则没有空闲槽位放置 IMS，程序返回 `both_data_slots_active`/`volte_data_slot_conflict`。
4. 双栈均采用“IPv4v6 优先、单栈降级”的策略；IPv6 分支失败时尽量保留已经可用的 IPv4。
5. SIP 安全注册优先使用 3GPP IPsec；建立或注册失败后，才退回明文 UDP SIP。
6. Beta8 的原生 IMS 实现重点是 **IMS 短信**，不是完整的自研 VoLTE 语音媒体栈。电话拨号走 ModemManager `CreateCall`；样本中没有 SDP、RTP、AMR/AMR-WB 媒体协商字符串或相应实现证据。

## 2. 样本与证据范围

| 项目 | 值 |
|---|---|
| 文件 | `simadmin beta8` |
| 版本 | `1.1.7-beta8` |
| Git commit | `930365d` |
| 构建时间 | `2026-07-27T10:47:10+08:00` |
| 架构 | AArch64, ELF64, little-endian |
| 链接方式 | 静态链接，无 dynamic section |
| 符号 | 已剥离 |
| SHA-256 | `210C35B11F54DD240A83E90DD08D5E8A8F4F2CEA227CE3A0503A9CED4140F9B7` |
| 发布元数据 MD5 | `d0903ceab475bacaf00e7ef45d1403c5` |

证据等级：

- **A（直接证据）**：ELF/前端/发布脚本中的原文，或反汇编中的明确分支和字符串引用。
- **B（强推断）**：多个命令、状态字段、错误路径和相邻控制流能互相印证，但没有硬件动态跟踪。
- **C（待验证）**：仅能确定意图，具体参数或时序仍需真机抓包/日志确认。

本说明不执行目标二进制，避免在非目标 AArch64 设备上产生副作用。分析工具包括 `readelf`、GNU `strings`、`rg`、Rust LLVM `llvm-objdump` 和发布包内静态资源。

## 3. 功能边界和模块划分

二进制保留了 Rust 源文件路径，相关模块可明确划分为：

| 模块 | 职责 |
|---|---|
| `src/modem_manager.rs` | ModemManager/NetworkManager 控制、普通数据开关、APN、拨号和 watchdog |
| `src/managed_mm_data.rs` | 绕过 NM、直接创建和配置 ModemManager data bearer |
| `src/secondary_qmi.rs` | 将 RPMSG `DATA6_CNTL` 绑定为第二 QMI 控制端点 |
| `src/secondary_qmi_data.rs` | 在 secondary QMI 上直接运行 WDS 数据会话 |
| `src/volte.rs` | IMS profile/bearer、SIP REGISTER、IPsec、运行时和短信调度 |
| `src/ims_uim.rs` | QMI UIM/APDU 与 USIM AKA |
| `src/ims_sms.rs` | 3GPP SMS RPDU/TPDU 编解码 |
| `src/handlers.rs` | HTTP API 与配置持久化入口 |

对外接口（A 级证据）：

| API | 方法 | 作用 |
|---|---|---|
| `/api/data` | GET | 返回 `DataConnectionResponse`，前端至少读取 `active` |
| `/api/data` | POST | 请求体 `{"active": bool}`，切换普通移动数据 |
| `/api/apn` | GET | 读取 APN context 列表 |
| `/api/apn` | POST | 写入 `context_path/apn/protocol/username/password/auth_method` |
| `/api/volte/control` | GET | 返回开关及 VoLTE runtime 状态 |
| `/api/volte/feature` | POST | 请求体 `{"enabled": bool}`，启停 VoLTE 功能 |
| `/api/volte/diagnostics/upload` | POST | 采集并上传 VoLTE/数据面诊断信息 |
| `/api/sms/send` | POST | 运行时已注册则优先 IMS SMS，否则回退 ModemManager SMS |

## 4. 总体架构

```text
HTTP /api/data
      |
      +--> 数据协调层 ----> NetworkManager GSM profile
      |        |                 + ModemManager bearer
      |        |
      |        +------------> managed_mm_data
      |                          直接创建 MM bearer + 配置 IP/DNS
      |
      +---------------------> secondary_qmi_data
                                 DATA6 / WDS + 配置 IP/DNS

HTTP /api/volte/feature
      |
      +--> VoLTE supervisor
              |
              +--> USIM identity / AKA
              +--> qmi0 与 DATA6 资源分配
              +--> IMS PDP profile + IMS bearer
              +--> P-CSCF 发现与路由
              +--> SIP REGISTER
                       +--> 3GPP IPsec（优先）
                       +--> plain UDP（降级）
              +--> SIP MESSAGE / 3GPP IMS SMS
```

## 5. 普通数据连接实现

### 5.1 `/api/data` 的控制语义

前端直接调用：

```text
GET  /data
POST /data  body={"active": true|false}
```

后端的 `DataConnectionRequest` 只有 `active` 字段。切换成功后记录 `Data connection updated`，同时存在系统事件 `cellular.data_enabled_changed`。配置结构中另有持久化字段 `data_enabled`，说明 API 不只是瞬时拨号，还会影响启动后的自动连接策略。（A）

启用逻辑不是无条件重复拨号：它会识别 `connected/connecting/disconnecting` 等状态，分别输出“已活动”“转换中，等待/跳过重复连接”或进入激活；关闭逻辑会清理当前受管 bearer，并在 VoLTE 独立模式下处理 secondary 数据面。（A/B）

### 5.2 NetworkManager 路径

常规路径使用 NetworkManager GSM profile。可见的 profile 属性包括：

```text
gsm.apn
gsm.username
gsm.password
gsm.home-only
gsm.auto-config
ipv4.method
ipv4.may-fail
ipv6.method
ipv6.may-fail
ipv6.never-default
```

流程重建如下（B）：

1. 查找现有 GSM connection profile；没有时创建 `simadmin-modem...` profile。
2. 将 UI 中的 APN、认证和漫游设置写回 profile。
3. 若 NM 已报告数据活动，记录 `Data connection already active in NetworkManager` 并避免重复 connect。
4. 若状态正在切换，跳过第二次激活。
5. 必要时调用 NM 激活，成功记录 `Data connection activated via NetworkManager`。
6. VoLTE 已启用时，允许保留既有 NM bearer，日志为 `Existing NetworkManager data bearer retained for VoLTE coexistence`。

### 5.3 直接 ModemManager bearer：`managed_mm_data`

该模块是 NM 之外的受管数据路径，使用 operation lock 避免并发创建/销毁 bearer。（A）

激活流程（A/B）：

1. 检查 APN；缺失返回 `managed_mm_data_apn_missing`。
2. 调用 `mmcli --create-bearer=apn=...,ip-type=...,allow-roaming=...`，认证参数按 `none/pap/chap` 转换。
3. 调用 bearer `--connect`。
4. 读取：
   - `bearer.status.connected`
   - `bearer.status.interface`
   - `bearer.ipv4-config.{address,prefix,gateway,dns}`
   - `bearer.ipv6-config.{address,prefix,gateway,dns}`
5. 校验地址、前缀和网关后，用 `ip` 配置接口、地址、路由，再更新蜂窝 DNS。
6. 双栈失败时记录 `Dual-stack ModemManager data activation failed; falling back to IPv4`，重新走 IPv4；IPv6 配置失败时保留 IPv4。

停用流程会 `--disconnect` 并 `--delete-bearer=<path>`；已停用时直接返回。错误码区分 bearer 未连接、接口异常、地址/网关无效和 bearer path 缺失。（A）

数据协调层中还存在 `Managed data cleanup before NM activation failed`，说明从直接 MM 路径切回 NM 前会先清理旧 bearer，避免两个管理者同时控制同一数据上下文。（A/B）

### 5.4 DATA6 secondary QMI 数据路径

发布包安装了以下 udev 规则：

```udev
SUBSYSTEM=="wwan", KERNEL=="wwan0qmi1", ENV{ID_MM_PORT_IGNORE}="1"
```

这表明第二 QMI 端点被明确排除在 ModemManager 自动探测之外，由 SimAdmin 自己控制。（A）

`simadmin-secondary-qmi.service` 在 ModemManager 和主服务之前启动：

1. 加载内核 `rpmsg_wwan_ctrl`。
2. 在 `/sys/bus/rpmsg/devices/*/name` 查找 `DATA6_CNTL`。
3. 执行 `simadmin secondary-qmi-init`。
4. 将 DATA6 绑定到 stock RPMSG WWAN driver。
5. 以 `--device-open-qmi --device-open-net=net-raw-ip|net-no-qos-header` 初始化 raw-IP/no-QoS 数据格式。
6. 把实际 QMI device/netdev 写入 `/run/simadmin`，并支持环境变量覆盖。

secondary 数据激活流程（A/B）：

1. 获取全局 operation lock。
2. 确认 secondary QMI device 存在，检查 ModemManager 的 `modem.3gpp.registration-state`。
3. 非 home 状态下根据 `roaming_allowed` 决定是否拒绝，错误码包括 `registration_not_home` 和 `roaming_forbidden`。
4. IPv4：
   - `--wds-set-ip-family=4`
   - `--wds-start-network=apn=...,3gpp-profile=1,ip-type=4`
   - 解析 WDS client CID 和 `Packet data handle`
   - `--wds-get-current-settings`
   - 校验 IPv4、mask、gateway、DNS，配置 netdev
5. IPv6 使用独立 CID/handle，按同样方式设置 family 6 并启动。
6. 双栈 IPv6 失败时记录 `Secondary DATA6 IPv6 unavailable; retaining IPv4 data`，恢复 IPv4 DNS，不撤销已经工作的 IPv4。
7. 健康检查使用 `--wds-get-packet-service-status` 并匹配 `Connection status: 'connected'`。

停用时使用保存的 CID/handle 执行 `--wds-stop-network=<handle>`；IPv4/IPv6 会话分别持有资源。`--client-no-release-cid` 表明 CID 生命周期由程序显式管理，而不是每条 qmicli 命令结束后自动释放。（A/B）

### 5.5 主数据与 IMS 的槽位分配

这是 Beta8 最关键的共存逻辑。运行时状态明确包含：

```text
data_requested
primary_data_active
secondary_data_active
data_path_mode
```

反汇编确认了两个互斥布局：

| 普通数据当前占用 | IMS 分配 | 状态模式 |
|---|---|---|
| 主 qmi0 | DATA6/独立 netdev | `independent_wwan1` |
| secondary DATA6 | 主 qmi0 | `secondary_qmi_data` |
| 两边都被普通数据占用 | 无可用 IMS 槽位，拒绝 | `both_data_slots_active` |

等价伪代码（B，分支本身为 A）：

```rust
fn allocate(data_requested: bool, primary_data_active: bool,
            secondary_data_active: bool) -> Result<Allocation> {
    if primary_data_active && secondary_data_active {
        return Err("both_data_slots_active");
    }

    if primary_data_active {
        // 普通数据保留 qmi0，IMS 移到 DATA6。
        return Ok(Allocation {
            ims: DATA6,
            data_path_mode: "independent_wwan1",
        });
    }

    if secondary_data_active {
        // 普通数据保留 DATA6，IMS 使用 qmi0。
        return Ok(Allocation {
            ims: QMI0,
            data_path_mode: "secondary_qmi_data",
        });
    }

    // 无活动数据时再结合 data_requested 和可用端点选主布局；
    // 静态证据不足以断言所有空闲场景的优先级。
    choose_available_slot(data_requested)
}
```

证据地址：

- `0x5A8928`：根据数据活动位选择布局分支。
- `0x5A899C`：引用 VA `0x941D15`，内容为 `IMS allocated to primary qmi0; DATA6 is reserved for data`。
- `0x5A8B78`：引用 VA `0x941D4E`，内容为 `IMS allocated to DATA6; primary qmi0 is reserved for data`。
- `0x5A8D74`：根据结果选择 `secondary_qmi_data` 或 `independent_wwan1`。
- `0x5A8DB0`：引用 `VoLTE and data path allocation selected`。

## 6. VoLTE/IMS 实现

### 6.1 Supervisor 与状态机

前端展示的 runtime 阶段直接来自二进制：

```text
phase: disabled -> starting -> registered/degraded -> stopping

stage:
  starting
  identity
  identity_aka
  radio
  pcscf
  modem
  bearer
  register_ipsec
  register_udp
  registered
  stopping
```

`VolteRuntimeStatus` 字段包括：

```text
phase, stage, registration_mode,
session_started_at, registered_at, last_rx_at, last_tx_at,
last_error, last_failure_at, next_retry_at,
sent_count, received_count, duplicate_count, reconnect_count,
data_path_mode
```

Supervisor 在配置关闭时停止 worker；失败时进入 `degraded`，记录错误和下次重试时间，再重新建立 bearer/注册。运行时与 HTTP handler 之间有命令 channel，错误包含 `runtime_not_running`、`send_timeout`、`reply_closed`、`command_closed`。（A）

### 6.2 IMS 身份与 USIM 数据

启动时按以下顺序准备身份（A/B）：

1. 通过 ModemManager AT 通道执行 `AT+CIMI` 读取 IMSI；失败时回退到 SIM 缓存的 IMSI。
2. 获取 EF_AD 以确定 MNC 长度：
   - QMI UIM：`--uim-get-card-status` 找到 primary ready USIM application；
   - QMI 读文件：`--uim-read-transparent=0x3F00,0x7FFF,0x6FAD`；
   - AT 回退：`AT+CRSM=176,28589,0,0,4`。
3. MNC 长度来源可能是 `modemmanager_home_operator`、`sim_ef_ad`、`three_digit_fallback` 或 `china_compatibility_fallback`。
4. 从 card status 获取 USIM AID；失败时使用内置 fallback AID。
5. 读取 SMSC；失败不会阻止注册，而是使用空 SMSC 继续。

随后按 MCC/MNC/IMSI 构造 3GPP IMS realm、私有身份和公开 SIP URI。字符串中有 `P-Visited-Network-ID`、`P-Associated-URI`、`sip:`/`tel:` 处理和 `ims.mnc...mcc...3gppnetwork.org` 相关片段。（A/B）

### 6.3 IMS PDP profile 和 bearer

程序先检查 modem readiness；还会观察 `/run/qmi_auto_activate.ready`，给初始 QMI UIM provisioning 留出稳定时间。marker 超时不会立刻终止，而是继续做 ModemManager readiness 检查。（A）

IMS profile 配置命令包括：

```text
AT+CGDCONT=<cid>,"IPV4V6|IP|IPV6","ims"
AT$QCPDPIMSCFGE=<cid>,1,1,1
AT+CGCONTRDP=<cid>
```

关闭/回收时使用：

```text
AT+CGACT=0,<cid>
AT$QCPDPIMSCFGE=<cid>,0,0,0
AT+CGPADDR=<cid>
```

程序会扫描已定义和已活动 profile，租用一个未占用 IMS PDP profile；若 profile 更新失败，会清掉 stale context 后重试。`QCPDPIMSCFGE` 用于打开 P-CSCF 报告。（A/B）

IMS bearer 有两种后端：

- **主 qmi0/ModemManager**：`--create-bearer=profile-id=<cid>,apn=ims,ip-type=<...>,allow-roaming=<...>`，随后连接并读取 bearer IP 配置。
- **DATA6/secondary QMI**：`--wds-start-network=apn=ims,3gpp-profile=<cid>,ip-type=<...>`，保存 WDS CID/handle，并读取 current settings。

两种后端都优先尝试双栈。若双栈未获完整 family 或激活失败，程序根据实际获配地址构造单栈候选，逐一尝试 IPv4/IPv6。关键日志为：

```text
Native VoLTE ... dual-stack ... falling back to single-stack attempts
Native VoLTE selected single-stack fallback families from granted IMS addresses
Native VoLTE runtime trying IMS address family
volte_runtime_all_ip_families_failed
```

### 6.4 P-CSCF 发现和主机路由

P-CSCF 候选来源按可用性组合（A/B）：

1. 当前活动 IMS bearer 中预取的 P-CSCF。
2. 使用保存的 QMI WDS CID 直接查询 current settings。
3. AT `CGCONTRDP` 回退。
4. IMS profile 中已有的 P-CSCF 信息。

程序会逐个尝试 P-CSCF；地址族必须与当前 IMS family 一致。接口地址、网关和到 P-CSCF 的主机路由使用 `/bin/ip` 或 `/usr/bin/ip` 安装，IPv6 使用 `nodad`、`noprefixroute` 等选项。所有候选失败返回 `volte_runtime_all_pcscf_failed`。（A）

### 6.5 SIP REGISTER 与 AKA

plain UDP 注册主流程（A/B）：

1. 向 P-CSCF 发送初始 REGISTER，Authorization 使用“空 AKA”形式以触发挑战。
2. 收到 401 后解析 Digest challenge：`realm`、`nonce`、`qop`、`opaque`、`algorithm`。
3. 要求 nonce 为 AKA 格式，解出 RAND/AUTN。
4. 通过 `ims_uim` 的 QMI UIM/APDU 请求 USIM AKA。
5. 正常结果得到 RES/CK/IK，计算 Digest response；支持 `AKAv1-MD5`、`AKAv2-MD5` 和 `MD5` 分支。
6. 若 USIM 返回同步失败材料，构造 `auts=` Authorization 发起重同步。
7. 服务器返回 423 时解析 `Min-Expires`，确认值有效且确实增大后重发。
8. 200 OK 时保存 `P-Associated-URI`、`Service-Route`、feature caps、`Security-Verify` 和 contact/smsip 能力。
9. 注册到期前 refresh；refresh 失败由 supervisor 重建运行时。

REGISTER 中可确认的能力头包括：

```text
Supported: path, gruu
Allow: INVITE,ACK,CANCEL,BYE,UPDATE,PRACK,MESSAGE,REFER,NOTIFY,INFO,OPTIONS
Accept: application/vnd.3gpp.sms
Contact: ...;+g.3gpp.smsip
P-Access-Network-Info: 3GPP-E-UTRAN-FDD...
```

### 6.6 3GPP IPsec 与 UDP 降级

安全注册优先级为 IPsec，然后才是 plain UDP。（A）

客户端在 REGISTER 中声明：

```text
Security-Client: ipsec-3gpp;prot=esp;mod=trans;
                 spi-c=...;spi-s=...;port-c=...;port-s=...;
                 alg=hmac-md5-96;ealg=null
```

收到 `Security-Server` 后：

1. 校验 mechanism、SPI 和端口。
2. 使用 AKA 派生的 IK 作为 IPsec integrity key。
3. 通过 `ip xfrm state` 和 `ip xfrm policy` 安装入/出方向 ESP transport 规则。
4. `auth-trunc hmac(md5)` 截断到 96 bit，`ealg=null`，即只做完整性保护、不做 ESP 加密。
5. 在协商端口上重新发送带 `Security-Verify` 的鉴权 REGISTER。
6. 200 OK 后进入 IPsec runtime，并定时 refresh。

如果 IPsec 注册失败，代码明确记录 `Native VoLTE IPsec registration failed, falling back to plain UDP SIP`，清理相关状态后重新走 UDP 注册；不是直接判定整个 VoLTE 失败。（A）

关键反汇编引用：

- `0x59CD90` 附近：引用 IPsec 注册失败并回退 UDP 的字符串（VA `0x943404`）。
- `0x5A0F84`：引用 `registered with 3GPP IPsec and listening`（VA `0x9440F5`）。
- `0x5A526C`：引用 `registered with plain UDP SIP and listening`（VA `0x9447D3`）。

### 6.7 IMS 短信

`/api/sms/send` 的选择逻辑很明确：VoLTE runtime 已注册时走 IMS；未注册时记录 `VoLTE SMS requested but runtime is not registered; falling back to ModemManager SMS`。（A）

MO 短信流程（A/B）：

1. `ims_sms` 将文本编码为 SMS TPDU/RP-DATA，必要时拆分 multipart。
2. 根据 SMSC 和目标号码构造多个 `sip:`/`tel:` route variant。
3. 生成 SIP MESSAGE：

```text
MESSAGE <target> SIP/2.0
Via: SIP/2.0/UDP ...;branch=...;rport
Route: <sip:<pcscf>;lr>
P-Preferred-Identity: <...>
P-Preferred-Service: urn:urn-7:3gpp-service.ims.icsi.sms
Accept-Contact: *;+g.3gpp.smsip
Content-Type: application/vnd.3gpp.sms
Content-Length: ...

<RPDU/TPDU bytes>
```

4. 依次尝试 route variant；记录 SIP status。IPsec runtime 和 UDP runtime 各有一套发送入口。
5. 全部失败返回 `volte_sms_message_all_variants_failed` 或 `volte_ipsec_sms_all_variants_failed`。

MT 短信流程（A/B）：

1. 收到 SIP MESSAGE 后先按 Via/Call-ID/CSeq 返回 SIP 响应。
2. 解码 RPDU/TPDU；支持普通短信、状态报告和 multipart。
3. multipart 先按 reference/total/part 缓存，齐全后组装。
4. 通过数据库 marker 去重，避免 SIP 重传导致重复入库。
5. 状态报告和不支持的 TPDU 仍会被确认，防止网络持续重传。
6. 对有效 MT 短信构造 RP-ACK，再发送反向 SIP MESSAGE，并等待 SIP response。
7. 等待 RP-ACK 响应时若收到原 MT 重传，会再次确认但不重复存储。

### 6.8 运行期健康检查、停止与清理

运行期会检查：

- ModemManager modem 是否仍存在/ready。
- 当前 bearer path 是否仍是预期 bearer。
- bearer 的连接状态和地址是否仍有效。
- secondary QMI device 是否存在，WDS packet status 是否 connected。
- command channel、UDP socket、REGISTER refresh 是否健康。

secondary QMI packet status 查询失败或结果不明确时，程序不会立即拆掉仍存在的主机 IMS 状态，而是记录 `retaining live host IMS state`，降低 qmicli 瞬态失败造成误重连的概率。（A）

停止顺序（B）：

1. 停止 runtime command loop 和 SIP socket。
2. 删除 IPsec xfrm state/policy（若使用）。
3. 删除 IMS 地址、路由和 DNS 影响。
4. 断开并删除 ModemManager IMS bearer，或按 CID/handle 停止 secondary WDS。
5. `CGACT=0`，关闭 P-CSCF reporting。
6. 通过 `CGPADDR` 确认释放；不支持时组合使用 `CGACT`/`CGCONTRDP` 验证。
7. 释放租用的 IMS PDP profile。

系统退出时还会释放 secondary QMI data。若 ModemManager 冷启动时持续报告 SIM missing，但 QMI UIM 连续确认 USIM present/ready，发布脚本只重启一次 ModemManager；恢复后重建 DATA6，并要求多次稳定采样。脚本明确拒绝自动重启 MPSS 或操作系统。（A）

## 7. “VoLTE 语音”边界

需要特别避免把 UI 名称推导成不存在的实现：

- 电话 API 最终使用 ModemManager Voice `CreateCall`、answer、hangup 等接口。（A）
- SIP 字符串中虽然 `Allow` 列出 INVITE/ACK/BYE/CANCEL，但运行时对非 MESSAGE SIP request 的证据仅是“acknowledged”。（A）
- 整个 ELF 中未找到 `m=audio`、`application/sdp`、`RTP/AVP`、`AMR` 或 `AMR-WB`。（A）
- 未发现 RTP socket、SDP offer/answer、codec negotiation、jitter buffer 或音频设备桥接证据。（B）

因此更准确的命名是：**Beta8 内置原生 IMS 注册与 SMS over IMS 数据面；语音通话由 modem/ModemManager 的语音能力完成。** 它可能借助 modem 已有的 IMS/语音能力实现运营商意义上的 VoLTE 通话，但不是本二进制自行实现 SIP INVITE + RTP 媒体栈。（B）

## 8. 失败与降级矩阵

| 故障 | 行为 |
|---|---|
| 普通数据双栈失败 | 回退 IPv4 |
| DATA6 IPv6 WDS 失败 | 保留 IPv4，恢复 IPv4 DNS |
| IMS 双栈未完整获配 | 根据实际地址尝试单栈 |
| 单个 P-CSCF 失败 | 尝试下一个候选 |
| IPsec 注册失败 | 清理后回退 plain UDP SIP |
| SIP 423 | 按合法且增大的 Min-Expires 重试 |
| AKA 同步失败 | 使用 AUTS 发起重同步 |
| VoLTE runtime 未注册时发短信 | 回退 ModemManager SMS |
| bearer/QMI 健康检查确认失效 | runtime 失败，supervisor 重建 |
| secondary packet status 暂时不明确 | 保留现有 host IMS 状态 |
| 主、次数据槽位都被占用 | 拒绝 IMS 分配，不强拆普通数据 |
| 配置关闭/进程退出 | 有序停止 SIP、IPsec、路由、bearer 和 profile |

## 9. 可复核的静态证据

| 证据 | 文件偏移/VA 或代码地址 |
|---|---|
| `src/volte.rs` | file `0x51814E`, VA `0x91814E` |
| `/api/...` 路由表起点 | file `0x5408D6`, VA `0x9408D6` |
| `data_requested/primary_data_active/secondary_data_active` | VA `0x941CC8/0x941CD6/0x941CE9` |
| IMS→qmi0、DATA6→data 文案 | VA `0x941D15`，xref `0x5A899C` |
| IMS→DATA6、qmi0→data 文案 | VA `0x941D4E`，xref `0x5A8B78` |
| `both_data_slots_active` | VA `0x949801` |
| 普通数据 dual→IPv4 文案 | VA `0x937DB7`，xref block `0x554610` |
| IPsec→UDP 降级文案 | VA `0x943404`，xref block `0x59CD90` |
| IPsec 注册成功文案 | VA `0x9440F5`，xref `0x5A0F84` |
| UDP 注册成功文案 | VA `0x9447D3`，xref `0x5A526C` |
| 数据/IMS allocation event | VA `0x944AA5`，xref `0x5A8DB0` |

复核命令示例：

```powershell
Get-FileHash -Algorithm SHA256 '.\simadmin beta8'
readelf -h -l -S -d '.\simadmin beta8'
strings -a -n 4 -t x '.\simadmin beta8' | rg -i 'volte|ims|secondary_qmi|managed_mm_data|wds'

$llvmObjdump = Join-Path (rustc --print sysroot) `
  'lib\rustlib\x86_64-pc-windows-gnu\bin\llvm-objdump.exe'
& $llvmObjdump -d --no-show-raw-insn `
  --start-address=0x5A8600 --stop-address=0x5A8F30 '.\simadmin beta8'
```

## 10. 尚未由静态分析证明的内容

以下项目不应当作已确认事实：

1. 无普通数据活动时，所有 modem/固件组合下 qmi0 与 DATA6 的最终优先级；静态控制流能确认冲突规则，但部分 Rust future 已内联，需真机日志补齐所有输入组合。
2. 各 timeout、backoff 和 refresh 的精确秒数；状态与行为明确，但本说明未逐个还原常量。
3. 不同运营商对 IPsec/UDP 的真实接受情况以及 NAT/防火墙影响。
4. 具体 modem 固件是否把 ModemManager Voice 通话实际承载在这里建立的 IMS context 上；需要 modem 日志或空口/接口抓包验证。
5. IPsec xfrm 的完整 argv 顺序和所有 selector；算法、SPI/端口、方向和 transport mode 已确认，逐参数表仍建议用运行日志复核。

## 11. 最终判断

Beta8 的设计核心不是一个单独的“VoLTE 开关”，而是一个资源协调系统：先确保普通数据和 IMS 不争用同一 QMI/PDP 槽位，再为 IMS 建立独立 bearer，完成 P-CSCF 发现、AKA、IPsec/UDP 注册，最后在该 runtime 上承载 SMS over IMS。其可靠性策略以“保留已工作的 IPv4/host state、逐级降级、由 supervisor 重建”为主，并通过 DATA6、udev 忽略规则和启动/恢复服务绕开 ModemManager 对第二 QMI 端点的管理。

---

**完成者：OpenAI Codex（独立逆向）**  
**完成日期：2026-07-31**

---

## 12. 附录：DATA6 初始化与端点保活深度分析（2026-07-31 补充）

> 本补充章节基于 IDA Pro 对 `simadmin beta8`（SHA-256 `210C35B11F54DD240A83E90DD08D5E8A8F4F2CEA227CE3A0503A9CED4140F9B7`）的交互式反编译，聚焦 DATA6 的创建、raw-IP 初始化、端点身份保活及失败回滚顺序。

### 12.1 核心差异：Beta8 使用单一 stock 驱动 + 单通道 DATA6

存在于当前开源项目中的 `rpmsg_wwan_ctrl_multi` 自定义内核模块**在 Beta8 中不存在，也未被使用**。Beta8 在 systemd unit 的 `ExecCondition` 中显式执行 `modprobe rpmsg_wwan_ctrl`（A 级证据，VA `0x935450` 附近 `[Unit]` 字符串），随后只查找名为 `DATA6_CNTL` 的 RPMSG 设备（shell 脚本 grep `DATA6_CNTL`，A 级证据）。

不再使用的常量对比：

| 项目 | 当前开源实现 | Beta8 实际行为 |
|---|---|---|
| 驱动列表 | `rpmsg_wwan_ctrl_multi` → `rpmsg_wwan_ctrl` | 仅 `rpmsg_wwan_ctrl` |
| 备选通道 | DATA6/7/8/9/5 | 仅 `DATA6_CNTL` |
| port-rank 排序 | 0/1/2 三级 | 无排序——只接受唯一端口 |
| 自定义模块 | `kernel/rpmsg_wwan_ctrl_multi/` | 未包含 |

证据字符串（A 级）：

- `stock RPMSG WWAN driver is unavailable`（VA `0x935481`）
- `DATA6_CNTL RPMSG device is unavailable`（VA `0x935565`）
- `DATA6 driver_override is unavailable`

反汇编关键函数：

| 地址 | 作用 |
|---|---|
| `0x547B64` | 检查 stock 驱动可用性 → 调用 `0x547E0C` 查找 `DATA6_CNTL` → systemctl 失败/重试循环 |
| `0x547E0C` | 遍历 `/sys/bus/rpmsg/devices`，按 name 匹配 `DATA6_CNTL`，记录 device_id |
| `0x548684` | 端口发现后的 `driver_override` 写入 + 等待端口出现 |
| `0x5478D8` | 通用错误/状态日志（多处 xref） |

### 12.2 迁移旧驱动绑定的安全顺序

当 `DATA6_CNTL` 仍被旧 `rpmsg_wwan_ctrl_multi` 驱动持有时，Beta8 不是直接覆盖绑定，而是：

1. 枚举旧驱动已创建的 WWAN 端口（通过 RPMSG device 目录解析）。
2. 执行 `unbind`。
3. 轮询等待旧端口设备节点消失（最多 `PORT_APPEAR_TIMEOUT`）。
4. 若旧端口未消失，返回 `old secondary QMI endpoint did not disappear` 错误。（A 级证据，VA `0x916464`）

字符串证据：

- VA `0x916464`：`old secondary QMI endpoint did not disappear:`

### 12.3 端点出现后的唯一性校验

当前实现在绑定后收集所有新出现的端口并按 rank 排序。Beta8 更严格：

- 先通过 RPMSG device 目录解析该 `DATA6_CNTL` 精确关联的 WWAN 端口。
- 若精确关联结果为空且新出现端口有且仅有一个，才退一步接受。
- 精确关联结果为多个、或无精确关联但新端口数 ≠ 1，均判定为失败并立即 unbind 回滚。

这保证了 MSM8916 上不会因竞态误拿其他 baseband 的端口，也不会在 DATA7/8/9 被 firmware 占用时破坏主 modem 库存。

### 12.4 raw-IP/no-QoS 初始化与网口拉起

Beta8 在 qmicli 能力探测**之前**先拉起数据网口：

1. 在 `/sys/class/net` 中枚举属于同一基带的 netdev。
2. 从绑定后新出现的 netdev 中选一个非主端口 netdev（例如 `wwan0qmi0` → 对应 `wwan0`，DATA6 → `wwan1`）。
3. 执行 `ip link set <netdev> up`。
4. 成功后调用 `qmicli -d /dev/<port> --device-open-qmi --device-open-net=net-raw-ip|net-no-qos-header --get-service-version-info` 探测 `wds` 服务。

证据字符串（A 级）：

- `--device-open-net=net-raw-ip|net-no-qos-header`（VA `0x935C89` 附近）
- `DATA6 stock RPMSG raw-IP/no-QoS initialization completed`（VA `0x935C89`）
- `secondary QMI endpoint did not become ready`（VA `0x935B92`）

### 12.5 端点身份轮询保活（hold_endpoint）

初始化成功后，Beta8 的 `secondary-qmi-init` **不会退出**。它：

1. 将 QMI device + netdev + channel + rpmsg_device 写入 `/run/simadmin/secondary-qmi-device`。
2. 读取 `/dev/<port>` 的 `(dev_t, inode)` 身份。
3. 每 3 秒轮询同一路径的 `(dev_t, inode)`。
4. 若 inode 变化 → 报告 `secondary QMI endpoint was replaced` 并退出（→ systemd restart）。
5. 若路径不存在 → 报告 `secondary QMI endpoint disappeared` 并退出。
6. 若 stat 失败 → 报告 `failed to inspect secondary QMI endpoint` 并退出。

只有在身份不变的情况下，进程才持续运行，systemd `Type=notify` 的 ready 信号才被认为是有效的。这也是为什么前一轮设备测试中 `secondary QMI endpoint disappeared` 会导致整个初始化失败。

证据字符串（A 级）：

- `secondary QMI endpoint was replaced:`（VA `0x916344`）
- `secondary QMI endpoint disappeared:`（VA `0x91636C`）
- `failed to inspect secondary QMI endpoint`（VA `0x916393`）
- `failed to hold secondary QMI endpoint`（VA `0x9164D3`）

### 12.6 运行期 WDS 会话：保留 CID 而非 --wds-follow-network

对 `sub_55A14C`（DATA6 数据激活主函数，约 0x2780 字节）的深度反编译揭示：

- **不存在** `--wds-follow-network` 字符串（IDA 全量搜索确认为 0 条命中）。
- 实际会话模式为：单次 `qmicli` 启动 network → 解析 CID + packet_data_handle → 后续操作（current-settings、packet-status、stop）均通过同一 CID + `--device-open-proxy` 复用。

启动命令参数（A 级，`sub_559930` 反编译）：

```text
qmicli --verbose -d <device>
       --device-open-qmi --device-open-proxy
       --device-open-net=net-raw-ip|net-no-qos-header
       --client-no-release-cid
       --wds-set-ip-family=<family>
       --wds-start-network=apn=...,ip-type=<family>
```

注意 `--device-open-qmi` 和 `--device-open-proxy` 同时出现（`sub_559930` 行 65-66 引用 VA `0x935C30` 和 `0x9381A8`），这不是笔误——Beta8 先用 `--device-open-qmi` 强制 QMI 模式（因为 stock 驱动将 DATA6 报告为 UNKNOWN/AT 类型），再用 `--device-open-proxy` 将 QMUX 注册到 `qmi-proxy`，使得后续不同 `qmicli` 进程可以通过同一 CID 寻址已保留的 WDS client。

活性检测（A 级，`sub_55A14C` 行 2058 引用 VA `0x91706E`）：

```text
qmicli -d <device> --device-open-qmi --device-open-proxy
       --device-open-net=net-raw-ip|net-no-qos-header
       --client-cid=<cid> --client-no-release-cid
       --wds-get-packet-service-status
```

匹配 `Connection status: 'connected'` 的行即视为会话存活。

停止流程（A 级）：

```text
qmicli -d <device> --device-open-qmi --device-open-proxy
       --device-open-net=net-raw-ip|net-no-qos-header
       --client-cid=<cid> --client-no-release-cid
       --wds-stop-network=<handle>

qmicli -d <device> --device-open-qmi --device-open-proxy
       --device-open-net=net-raw-ip|net-no-qos-header
       --client-cid=<cid> --wds-noop
```

第二个 `--wds-noop` 命令不带 `--client-no-release-cid`，让 qmicli 在退出时归还 WDS CID，与 `--client-no-release-cid` 的启动命令形成配对的 CID 生命周期管理。（B 级推断，反编译 `sub_55A14C` 行 790-825 存在两条独立 qmicli 调用路径。）

### 12.7 当前源代码与 Beta8 的关键实现偏差（已修正）

| 偏差项 | 当前源代码（修正前） | Beta8 实际行为 |
|---|---|---|
| 驱动选择 | 优先 `rpmsg_wwan_ctrl_multi`，回退 `rpmsg_wwan_ctrl` | 仅 `rpmsg_wwan_ctrl` |
| 备选通道 | 遍历 DATA6/7/8/9/5 | 仅 `DATA6_CNTL` |
| 初始化后行为 | 写入 JSON state 文件后退出 | 常驻进程，每 3 秒校验设备节点身份 |
| 数据会话模式 | `--wds-follow-network` 子进程长驻 | 单次 `--wds-start-network` + `--client-no-release-cid` 保留 CID |
| 会话复用 | 子进程存活即认为会话有效 | 主动 `--wds-get-packet-service-status` 查询 connectivity |
| qmicli 打开模式 | `--device-open-qmi` 或 `--device-open-proxy` 二选一 | 同时使用 `--device-open-qmi --device-open-proxy` |

以上所有偏差已在 `codex/volte-beta8-fix` 分支中修正，修正文件为：

- `backend/src/hardware/cellular/secondary_qmi.rs`
- `backend/src/hardware/cellular/secondary_qmi_data.rs`
- `backend/src/main.rs`

---

**补充完成者：OpenAI Codex（独立逆向补充）**  
**补充日期：2026-07-31**
