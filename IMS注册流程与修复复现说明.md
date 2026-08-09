# SimAdmin IMS（VoWiFi/VoLTE）注册流程与修复复现说明

> 状态：**真机验证通过**（2026-08-09，410 设备，Maxis / Hotlink 50212）
> 本文档说明标准 IMS 注册流程、本项目在 410 上从"受保护 REGISTER 静默超时"到"200 OK 注册成功"的修复过程，以及如何完整复现。

## 1. 结论摘要

| 项目 | 结果 |
|---|---|
| VoWiFi IMS REGISTER（3GPP IPsec，经 ePDG 隧道） | ✅ `200 OK`，`runtime_registered=true`，`voice_ready` |
| VoLTE IMS REGISTER（LTE IPv6 P-CSCF） | ✅ 200 OK，Service-Route / P-Associated-URI 均收到 |
| 注册身份 | `sip:+60174231067@ims.mnc012.mcc502.3gppnetwork.org` |
| 关键修复 | **自动分片**（默认）：IMS ESP 包在隧道内软件切成 ≤1356B 分片，物理链路永远不分片；完整 REGISTER 头保留。**紧凑 REGISTER**（`SIMADMIN_COMPACT_REGISTER=1`）作为备选 |

**一句话结论**：ESP 帧、SPI/端口映射、密钥派生从头到尾都是正确的；真正导致 P-CSCF 静默丢弃的原因是**外层 ESP-in-UDP 包超过 WiFi 路径 MTU 被 IP 分片，分片在 NAT 路由器/ePDG 路径上丢失**。两种解法都已在真机验证：

1. **自动分片（推荐，默认）**：保留完整 REGISTER 头（1608B 内层），在隧道内把 IMS ESP 包软件切成 1356+272 两个 IP 分片，各自套外层 ESP（1416B/344B，物理不分片）→ P-CSCF 标准 IP 重组 → `200 OK`。
2. **紧凑 REGISTER（备选）**：删可选头把外层压到 1432B 不分片 → `200 OK`。

---

## 2. 标准 IMS 注册流程（3GPP）

参考规范：

- 3GPP TS 33.203：IMS 接入安全，IPsec ESP 密钥派生（Annex I）、SA 对与端口语义（§7.1/§7.3.2）
- RFC 3329：`Security-Client / Security-Server / Security-Verify` 协商（spi-c/spi-s/port-c/port-s）
- 3GPP TS 24.229 §5.1.1.2.2：受保护 REGISTER 的 Via/Contact 端口、`Require/Proxy-Require: sec-agree`
- RFC 4303：ESP 帧格式、显式 IV、ICV 覆盖范围
- RFC 3948 / TR 33.802：ESP-in-UDP 封装（NAT 场景，本项目 VoWiFi 默认不用）

### 2.1 四步消息流

```text
UE                                    P-CSCF
 │ ① REGISTER (无安全，带 Security-Client)   │
 │  + Security-Client: ipsec-3gpp; alg; ealg;  │
 │    prot=esp; mod=trans; spi-c; spi-s;       │
 │    port-c(5064); port-s(5063)               │
 │ ───────────────────────────────────────────>│
 │ ② 401 Unauthorized                          │
 │  + Security-Server: ...spi-c=...;spi-s=...; │
 │    port-c(7807); port-s(7777)               │
 │  + WWW-Authenticate: Digest, AKAv1-MD5      │
 │ <───────────────────────────────────────────│
 │ ③ REGISTER (带 Security-Verify, 走 ESP)      │
 │  + Authorization: Digest(aka)               │
 │  + Security-Verify: 原样回显 Security-Server │
 │  + ESP: SPI=spi_ps, 从 port_uc 发到 port_ps │
 │ ───────────────────────────────────────────>│
 │ ④ 200 OK (ESP, 从 port_pc 回到 port_us)     │
 │ <───────────────────────────────────────────│
```

### 2.2 关键语义（TS 33.203 §7.1 / §7.3.2）

- UE 两个受保护端口：`port_uc`（发）与 `port_us`（收）；P-CSCF 两个：`port_ps`（收）与 `port_pc`（发）。
- 四对 SA 方向：
  - UE `port_uc` → P-CSCF `port_ps`，ESP 头 SPI = **spi_ps**（P-CSCF 侧入站 SA）
  - P-CSCF `port_ps` → UE `port_uc`，SPI = **spi_uc**（UE 侧入站 SA）
  - P-CSCF `port_pc` → UE `port_us`，SPI = **spi_us**
  - UE `port_us` → P-CSCF `port_pc`，SPI = **spi_pc**
- 受保护 REGISTER 用第 1 对（`port_uc`→`port_ps`，SPI=spi_ps）；200 OK 用第 3 对（`port_pc`→`port_us`，SPI=spi_us）。
- Via/Contact 通告 `port_us`（=5063），但数据报实际从 `port_uc`（=5064）发出。
- 密钥（TS 33.203 Annex I）：CK 直接作 AES-CBC 密钥；IK 后补 32 个 0 得 HMAC-SHA-1 密钥；ICV 为 HMAC-SHA1-96 截断 12 字节，覆盖 SPI|seq|IV|ciphertext。

### 2.3 VoWiFi 特殊点：两层 IPsec

```text
wlan0/wwan0 物理包
  └─ 外层 ESP（IKEv2 CHILD SA，UE↔ePDG，ESP-in-UDP 4500/raw ESP）
       └─ 内层 IP 包（src=隧道内 IP，dst=P-CSCF）
            └─ IMS ESP（TS 33.203，proto=50，SPI=spi_ps）
                 └─ UDP(5064→7777) + SIP REGISTER
```

外层隧道负责 UE↔ePDG；内层 IMS ESP 负责 UE↔P-CSCF（Gm 接口）。两者密钥无关（外层用 IKEv2 密钥，内层用 USIM AKA 的 CK/IK）。

---

## 3. 故障根因：外层 IP 分片丢失

### 3.1 现象

- 初始（明文）REGISTER 小（外层 ≈800B，不分片）→ 能收到 401。
- 受保护 REGISTER 大（完整头时内层 1608B → 外层 1672B → IP 1700B）→ 物理链路切成 1500+220 两片。
- 抓包确认两片都发出、UDP 校验和正确，但 ePDG/P-CSCF 无任何回应（无 ICMP、无 200 OK）。
- 关闭连接重试、换 ePDG、换 SPI/端口/密钥组合（8 个候选）全部静默超时。

### 3.2 排除项（都有证据）

| 假设 | 结论 |
|---|---|
| ESP 帧构造错误 | ❌ 已离线验证：AES-CBC 解密正确、padding 正确、ICV 与 RFC 4303 一致 |
| SPI/端口映射错误 | ❌ 与 siphon-sip/TS 33.203 逐项一致，两个端口配对 × 两种 SPI 映射都试过 |
| 密钥派生错误 | ❌ CK/IK 来自同一轮 AKA 挑战，Annex I 派生正确 |
| ICV 不含 IV 的变体 | ❌ 候选已试，非主因 |
| ESP-in-UDP 封装（RFC 3948） | ❌ 候选已试，非主因 |
| 内层 ESP 分片（TUN MTU 太小） | ❌ 早期 MTU=1360 时试过，同样被丢（IPsec 栈默认丢弃分片 ESP） |
| 外层大包 IP 分片 | ✅ **实锤**：压缩后外层 1432B 不分片，立即 200 OK |

### 3.3 修复 A（推荐）：自动分片（默认开启）

在 `tun_gateway.rs` 外层封装前，把 IMS ESP 后的内层 IPv4 包软件分片（`fragment_ipv4_packet`）：

```text
内层 IP（完整头 REGISTER）1608B
  └─ 分片 1：IP(proto=50, off=0, MF=1, len=1356) ──外层 ESP──> 物理 1436B ✅不分片
  └─ 分片 2：IP(proto=50, off=169, MF=0, len=272) ──外层 ESP──> 物理 ~370B ✅不分片
  └─ P-CSCF 按 IP id 重组 1608B ESP 包 → 验 ICV → 解密 → SIP
```

- 默认开启；`SIMADMIN_AUTO_FRAGMENT=0` 可关闭。
- 分片保持 IP id、offset 8 字节对齐、MF 标志、重算校验和。
- 实测：首个候选即 `200 OK`（`outer_frame_bytes=1416/344`）。

**IPv6 支持**：`fragment_inner_packet` 同时处理 IPv4 与 IPv6（`fragment_ipv6_packet` 按 RFC 8200 §4.5 生成 Fragment Header：next header=44、13 位 offset、M 标志、32 位 identification 计数器）。IPv6-only 运营商（内层为 IPv6 地址、REGISTER 走 IPv6）时同样在外层封装前分片，物理包不超 MTU；外层传输套接字本身已是双栈。

### 3.5 入向大包重组（IPv4 + IPv6，已实现）

出向分片解决"我们发大包"；入向（P-CSCF → UE，如带 SDP 的 INVITE / 短信）同样可能被运营商侧分片。`reassemble_inbound_ip_fragment` 在解外层 ESP 之后、解内层 IMS ESP 之前完成重组：

- **IPv4**：按 `(src, dst, id)` + MF/offset 重组，重组后清标志位并重算校验和。
- **IPv6**：识别 Fragment Header（next header=44），按 `(src, dst, identification)` 重组，还原原始 next header 与 payload length。
- 支持**乱序到达**（后片先到会缓存，等片 0 到达后组装）、**重叠分片拒绝**（RFC 8200 §4.5.4）、3 秒超时清理、最多 32 个重组缓冲。
- 单元测试：IPv4/IPv6 乱序重组往返一致、重叠拒绝。

### 3.4 修复 B（备选）：紧凑 REGISTER（SIMADMIN_COMPACT_REGISTER=1）

删除 3 组**可选**头，把 SIP 从 1537B 压到 ~1290B：

1. `Cellular-Network-Info`（VoWiFi 下不需要，P-Access-Network-Info: IEEE-802.11 才是相关项）
2. Contact 的 3 个 SRVCC feature tag（`+g.3gpp.mid-call` 等，仅 SRVCC 能力声明）
3. `+sip.instance` / `reg-id`（RFC 5626 可选，注册不需要）

效果：

```text
                   修复前       修复后
SIP 报文            1537B        ≈1290B
内层 IP             1608B        1368B
外层 ESP frame      1672B        1432B
物理包（含 IP/UDP） 1700B → 分片 1472B → 不分片 ✅
```

代码位置（`SimAdmin/backend/src/connectivity/modems/softstack/vowifi/live.rs`）：

- `LiveRegisterHeaderProfile.compact_register`：env `SIMADMIN_COMPACT_REGISTER` 门控
- `build_contact_header()`：compact 时跳过 contact_param_order 与 always_add_sip_instance
- `include_cellular_network_info`：compact 时置 false

> 说明：该模式仅移除 3GPP 可选头。若某运营商 profile 显式要求 `always_add_sip_instance`（部分 iOS 库），不要开启 compact；优先用自动分片。

---

## 4. 实测报文（Maxis 50212，抓包证据）

### 4.1 401 挑战（P-CSCF → UE）

```text
SIP/2.0 401 Unauthorized
Security-Server: ipsec-3gpp;q=0.5;alg=hmac-sha-1-96;prot=esp;mod=trans;
                 ealg=aes-cbc;spi-c=3759706211;spi-s=2594505554;
                 port-c=7807;port-s=7777
WWW-Authenticate: Digest realm="ims.mnc012.mcc502.3gppnetwork.org",
                 nonce="...",algorithm=AKAv1-MD5,qop="auth"
```

### 4.2 受保护 REGISTER（UE → P-CSCF，明文观察点=隧道内）

```text
REGISTER sip:ims.mnc012.mcc502.3gppnetwork.org SIP/2.0
Via: SIP/2.0/UDP 2.30.238.251:5063;branch=...;rport
Contact: <sip:502122039563670@2.30.238.251:5063;transport=udp>
Authorization: Digest username="...@ims.mnc012.mcc502.3gppnetwork.org",
               realm="...",nonce="...",response="...",
               algorithm=AKAv1-MD5,qop=auth,nc=00000001,cnonce="..."
Supported: sec-agree
Require: sec-agree
Proxy-Require: sec-agree
P-Access-Network-Info: IEEE-802.11
Security-Client: ipsec-3gpp; alg=hmac-sha-1-96; ealg=aes-cbc; prot=esp;
                 mod=trans; spi-c=26157651; spi-s=49840557;
                 port-c=5064; port-s=5063
Security-Verify: ipsec-3gpp;q=0.5;alg=hmac-sha-1-96;prot=esp;mod=trans;
                 ealg=aes-cbc;spi-c=3759706211;spi-s=2594505554;
                 port-c=7807;port-s=7777
```

实际线包：内层 IP proto=50 + ESP（SPI=2594505554=spi_ps，seq=1，IV，AES-CBC，HMAC-SHA1-96），外层经 ePDG 隧道送达。

### 4.3 200 OK（P-CSCF → UE，从 port_pc=7807 回到 port_us=5063）

```text
SIP/2.0 200 OK
Contact: <sip:502122039563670@2.30.238.251:5063;transport=udp>;
         expires=3600;+g.3gpp.accesstype="wlan"
P-Associated-Uri: <sip:+60174231067@ims.mnc012.mcc502.3gppnetwork.org>
P-Associated-Uri: <tel:+60174231067>
P-Charging-Function-Addresses: ccf="aaa://prepaid01.emm.ims.mnc012.mcc502.3gppnetwork.org"
```

### 4.4 时间线（日志）

```text
01:55:01 REGISTER (initial, 1055B)  → 401 (Security-Server)
01:55:01 policy_candidate="client_server_flow_primary"  ← 第一个候选即成功
01:55:01 outer_frame_bytes=1432 (不分片)
01:55:01 IMS REGISTER authenticated response status_code=200
01:55:01 IMS REGISTER final response 200 OK, associated_uri_count=2
```

---

## 5. 复现步骤

### 5.1 前置

- 410 设备（Debian trixie + ModemManager + QMI），WiFi 已连、LTE 已注册
- Hotlink/Maxis 50212 SIM（USIM 支持 AKA）
- iOS carrier bundle 数据库在设备 `/root/simadmin-codex/carrier-bundles-iphone16promax-26.6.sqlite3`

### 5.2 构建

```bash
cd SimAdmin/backend
cargo zigbuild --release --target aarch64-unknown-linux-musl
```

### 5.3 部署（自动分片为默认，无需环境变量）

```bash
sshpass -p '1313144' scp target/aarch64-unknown-linux-musl/release/simadmin \
  root@192.168.100.13:/root/simadmin-codex/simadmin.new
sshpass -p '1313144' ssh root@192.168.100.13 '
  mv /root/simadmin-codex/simadmin.new /root/simadmin-codex/simadmin
  chmod +x /root/simadmin-codex/simadmin
  systemctl daemon-reload
  systemctl restart simadmin'
```

若某网络不接受自动分片（受保护 REGISTER 超时），改用紧凑 REGISTER 回退：

```bash
printf "[Service]\nEnvironment=SIMADMIN_COMPACT_REGISTER=1\n" \
  > /etc/systemd/system/simadmin.service.d/compact.conf
```

调试（可选）：

```bash
Environment=SIMADMIN_AUTO_FRAGMENT=1
Environment=SIMADMIN_DEBUG_ESP_KEYS=1
Environment=SIMADMIN_DEBUG_ESP_FRAMES=1
```

### 5.4 触发连接

```bash
COOKIE=$(sed 's/^simadmin_session=//' /tmp/simadmin_cookie)
curl -s -X POST -H "Cookie: simadmin_session=$COOKIE" \
  -H "Content-Type: application/json" \
  -d '{"enabled":true}' \
  http://127.0.0.1:3300/api/vowifi/lines/line-eb362e8e7db3496c653dd74836cd418c/connection
```

### 5.5 验证

```bash
# 1) 日志出现 200 OK
journalctl -u simadmin --since "2 minutes ago" | grep -E "status_code=200|final response"

# 2) 运行时状态
curl -s -H "Cookie: simadmin_session=$COOKIE" \
  http://127.0.0.1:3300/api/vowifi/lines/line-eb362e8e7db3496c653dd74836cd418c \
  | python3 -c "import json,sys; d=json.load(sys.stdin)['data'];
print(d['runtime_phase'], d['runtime_registered'])"
# 期望：voice_ready True

# 3) 抓包确认外层包不分片（IP 总长 ≤1500 且无 MF 分片）
tcpdump -i any -n "host 202.75.146.43" -c 200 -w /tmp/ims.pcap
```

### 5.6 回归检查

- 断开再连接（enable=false→true）应再次 200 OK（已验证）。
- 自动分片模式日志应出现 `IMS ESP inner packet software-fragmented ... fragment_count=2`，且外层包 `outer_frame_bytes` 均 <1500（实测 1416/344）。
- 若禁用自动分片且不设 compact，受保护 REGISTER 会重新静默超时（1700B 物理分片被丢）。

---

## 6. 相关代码改动清单（2026-08-09）

| 文件 | 改动 |
|---|---|
| `vowifi/live.rs` | `SIMADMIN_COMPACT_REGISTER` 紧凑头模式（备选）；新增 ICV 变体 / UDP 封装 / null-加密候选（互操作探测） |
| `vowifi/tun_gateway.rs` | **`fragment_inner_packet` 自动分片（默认，IPv4+IPv6）**；**`reassemble_inbound_ip_fragment` 入向重组（IPv4+IPv6，乱序/重叠防护）**；`SIMADMIN_DEBUG_ESP_FRAMES` 出/入向帧 dump；ICV 模式与 UDP 封装支持 |
| `vowifi/dataplane.rs` | `protect/unprotect_inner_packet_for_esp_with_mode`（ICV 是否含 IV）；测试 |

> 参考实现：`SimAdmin-VoLTE/simadmin_1.1.7-beta8.tar.gz`（beta8 用 `alg=hmac-md5-96;ealg=null` 的仅完整性 ESP，对 Maxis 同样有效，说明该网 P-CSCF 对 null/AES-CBC 都接受；本项目按协商的 AES-CBC 走，已验证）。

---

## 7. 遗留事项

1. **外层包体积**：已由自动分片解决（默认开启，IPv4+IPv6，保留完整头）。物理层分片路径仍是已知不可靠场景，未来可加 IKEv2 MTU 通告做更精细的协商。
2. **ePDG 路径分片**：分片丢失发生在 NAT 路由器还是 ePDG 未最终定位；自动分片使该路径不再触发，问题规避。
3. **TCP 传输**：profile 显式 `ims.transport=tcp` 时仍走 TCP 候选，未在本轮覆盖（Maxis 为 UDP）。
4. **compact 模式与 iOS 库**：`always_add_sip_instance` 的运营商不要开 compact，用自动分片即可。
5. **瞬态入向校验失败**：个别会话首个候选后出现一次 `IMS ESP inbound unprotect failed`（P-CSCF 已回包但入向 ICV 校验失败），随后候选/重试仍 200 OK。已加入向帧 dump（`SIMADMIN_DEBUG_ESP_FRAMES`）以便后续定位。
6. **VoLTE / VoWiFi 双路径**：均已在 Maxis 真机注册成功（VoLTE 走 LTE IPv6 P-CSCF `[2001:d08:12:a::3c0]`，VoWiFi 走 ePDG 隧道 IPv4 P-CSCF）。
