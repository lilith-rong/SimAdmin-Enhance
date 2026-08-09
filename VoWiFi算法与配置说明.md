# SimAdmin VoWiFi 算法与配置说明（合并文档）

> 合并日期：2026-08-09
> 本文合并自三份文档：`VoWiFi配置来源与算法扩展说明.md`、`算法缺口-AKAv2与IKE-ESP提案.md`、`SimAdmin_未实现算法说明-AES-CTR与AES-XCBC.md`，内容按主题重排，未删减。

# VoWiFi 配置来源与算法扩展说明

> 本文说明 Pixel/Android 与 Qualcomm/三星 modem 中 VoWiFi 配置的“提取/解码”边界，
> 并列出 SimAdmin 需要由后续 AI 继续扩展的 IKE/ESP 算法与解析缺口。
> 关联：`carrier_Bundles/android/pixel/README.md`、`算法缺口-AKAv2与IKE-ESP提案.md`。

## 1. 结论：`iwlan.*` 不是解密

`carrier_Bundles` 上一轮新增的 `iwlan.*` 提取（方案 A）**不是新的 VoWiFi 解密方式**。
它读取的是 Android 官方 `CarrierConfigManager.Iwlan` 的公开配置键，这些键以明文形式存放在
Google 官方固件 `product.img/etc/CarrierSettings/*.pb` 中，解析依赖仓库已有的
`carrier_settings.proto` 定义，不存在加密或混淆。

因此：

- 方案 A 已经实现并验证：`carrier_Bundles/android/pixel/catalog.py` 新增
  `_encryption_candidates` / `_sa_proposals` / `_iwlan_ike_policy`；
- 线上构建已重新生成 `carrier-bundles-pixel-mustang.sqlite3`，VoWiFi ready 从 0 提升到 984；
- mustang 的 CarrierSettings 里只有 4 个 profile 显式写了 IKE 提案（child 3 个），
  其余 980 条走 SimAdmin 基线默认提案。

## 2. 方案 A 的提取映射（已完成）

Android 键（`CarrierConfigManager.Iwlan`）→ catalog `access.vowifi.ike` 字段：

| Android 配置键 | catalog 字段 |
|---|---|
| `iwlan.supported_ike_session_encryption_algorithms_int_array` + `iwlan.ike_session_encryption_aes_cbc/ctr/gcm_key_size_int_array` | `ike_sa_proposals[].encryption`（如 `AES-128`、`AES-CTR-128`、`AES-GCM-16`） |
| `iwlan.supported_child_session_encryption_algorithms_int_array` + `iwlan.child_session_aes_cbc/ctr/gcm_key_size_int_array` | `child_sa_proposals[].encryption` |
| `iwlan.supported_integrity_algorithms_int_array` | `ike_sa_proposals[].integrity` / `child_sa_proposals[].integrity` |
| `iwlan.supported_prf_algorithms_int_array` | `ike_sa_proposals[].prf` |
| `iwlan.diffie_hellman_groups_int_array` | `ike_sa_proposals[].dh_group`（数字，SimAdmin 约定） |
| `iwlan.natt_keep_alive_timer_sec_int` | `nat_keepalive_seconds` |
| `iwlan.dpd_timer_sec_int` | `dpd_interval_seconds` |
| `iwlan.ike_rekey_soft_timer_sec_int` / `ike_rekey_hard_timer_in_sec` | `ike_rekey_soft_seconds` / `ike_rekey_hard_seconds` |
| `iwlan.child_sa_rekey_soft_timer_sec_int` / `child_sa_rekey_hard_timer_sec_int` | `child_sa_rekey_soft_seconds` / `child_sa_rekey_hard_seconds` |
| `iwlan.max_retries_int` / `retransmit_timer_sec_int_array` | `max_retries` / `retransmit_timer_seconds` |
| `iwlan.ike_local_id_type_int` / `ike_remote_id_type_int` | `local_id_type` / `remote_id_type` |
| `iwlan.epdg_authentication_method_int` | `epdg_authentication_method` |
| `iwlan.epdg_address_priority_int_array` / `epdg_plmn_priority_int_array` | `epdg_address_priority` / `epdg_plmn_priority` |
| `iwlan.supports_ike_session_multiple_sa_proposals_bool` / `supports_child_session_multiple_sa_proposals_bool` | `supports_multiple_ike_sa_proposals` / `supports_multiple_child_sa_proposals` |

## 3. 真正需要“解码/逆向”的来源

### 3.1 Qualcomm MCFG MBN（`mcfg_sw.mbn`）

- 位置：高通机型 `vendor.img` 中 `rfs/msm/mpss/readonly/vendor/mbn/mcfg_sw/**/mcfg_sw.mbn`。
- 现状：`carrier_Bundles` 只做 inventory（路径/SHA-256），未语义解码。
- MBN 是 Qualcomm 专有容器，内部为压缩 XML/二进制配置；其中可能包含 IWLAN/ePDG 的
  IKE/ESP proposals、P-CSCF、PDN、认证策略等。
- 已有开源解析工具（可让另一个 AI 接入）：
  - `sbaresearch/mbn-mcfg-tools`（Python，解析/打包 Qualcomm MBN MCFG）；
  - `Biktorgj/mcfg_tools`（PinePhone modem SDK 使用，`extract_mcfg` / `convert_mcfg`）；
  - `fenrir-naru/mbn_utils`（Ruby，MBN 解包/重打包）。
- 目标：从 MBN 的 IWLAN XML 中提取每个 carrier 的 IKE SA / Child SA proposals、
  ePDG FQDN、EAP 方法与 ID 类型，并写入与方案 A 相同的 catalog 字段。

### 3.2 三星/Tensor modem provisioning（Pixel 10 实测为空）

- Pixel 10 Pro XL（mustang，CP2A.260805.005）实测 `mcfg_files_inventoried = 0`：
  vendor.img 中没有 `mcfg_sw.mbn`。
- Pixel 10 使用 Tensor（三星 Exynos modem），其 provisioning 不在 Google Factory Image 中公开；
  需要从 modem 固件镜像（NV/EFS/特定分区）逆向，或从其它 Pixel 10 设备 dump 获取。
- 结论：当前 mustang 的 VoWiFi 显式提案来源只有 CarrierSettings 的 4 个 profile；
  其余依赖 SimAdmin 基线。

### 3.3 CarrierConfig APK XML assets（无需解密，待接入）

- 位置：`product.img` 中 `priv-app/CarrierConfig/CarrierConfig.apk` 的 `assets/*.xml`。
- 键名与 `iwlan.*` 完全一致，只是 XML 格式；解包 APK 后可直接解析，不需要逆向。
- 可作为 CarrierSettings PB 之外的第二个配置来源，目前提取器尚未接入。

## 4. SimAdmin 需要扩展的算法/解析缺口（供另一个 AI）

### 4.1 提案 token 解析器

文件：`SimAdmin/backend/src/connectivity/modems/softstack/vowifi/carrier_catalog_v7.rs`
（`algorithm_token` / `dh_group_token` / `structured_ike_proposals` / `structured_esp_proposals`）。

当前已支持：`aes128`、`aes256`、`md5`、`sha1`、`sha256`、`sha384`、`sha512`、
`aes-xcbc`；DH 组 1/2/5/14/15/16/18。

mustang 显式提案中已出现、但解析器不支持的 token：

| token | 类型 | 现状 |
|---|---|---|
| `AES-CBC` | 加密（无密钥长度） | 未识别；需要决定默认密钥长度或跳过 |
| `AES-CTR-128` / `AES-CTR-192` / `AES-CTR-256` | 加密 | 未识别；IKE 引擎也未实现 |
| `AES-XCBC-96` | 完整性 | 解析器只认 `aes-xcbc`，`aes-xcbc-96` 未识别 |
| `AES128-XCBC` | PRF | 未识别 |
| `AES-GCM-8` / `AES-GCM-12` / `AES-GCM-16` | AEAD 加密 | 未识别；IKE 引擎也未实现 |
| `3DES` | 加密 | 未识别；IKE 引擎也未实现 |
| `AES-CMAC-96` | 完整性 | 未识别 |
| ECP DH 组 19/20/21 | DH | 未识别；iOS 已有 1 条 ECP-521 属于此类 |

建议行为（与现有“跳过单个 profile”策略一致）：

1. `structured_ike_proposals` / `structured_esp_proposals` 改为过滤不支持的提案，
   而不是对整个 profile 返回错误；
2. 过滤后为空时回落到 `BASELINE_IKE_PROPOSALS` / `BASELINE_ESP_PROPOSALS`；
3. 在日志中记录被过滤的 token 与 profile，方便后续补实现。

### 4.2 IKE/ESP 加密原语

文件：`SimAdmin/backend/src/connectivity/modems/softstack/vowifi/ike_encrypted.rs`、
`ike_keys.rs`、`ike_dh.rs`。

需要扩展的原语（按优先级）：

1. `AES-192-CBC` 密钥长度（当前只实现 128/256）；
2. `AES-XCBC-96` 完整性（当前实现 `hmac_md5_96` / SHA 系列 / 可能已有 XCBC，需核对）；
3. `AES128-XCBC` PRF；
4. `AES-CTR` 加密模式；
5. `AES-GCM` AEAD；
6. `3DES-CBC`；
7. ECP（256/384/521）DH 组（当前只支持 MODP）。

### 4.3 验收口径

- catalog 契约已放宽：`/access/vowifi/ike/ike_sa_proposals` 与
  `/access/vowifi/ike/child_sa_proposals` 不再属于 VoWiFi 必需路径；
- SimAdmin 对“无提案”profile 已能使用基线提案；
- 目标是“有显式提案且算法被实现时使用显式提案，否则安全回落基线”，不让任何单条
  提案词汇导致整个 profile 无法加载。

## 4.4 SIP Security-Client 算法（第 8 节实测问题的数据结论）

410 实机测试记录第 8 节指出三份数据库 `/sip/common/security_client` 全空。核查结果：

- **iOS（IPSW + IPCC）**：真实 Maxis IPCC 的 `carrier.plist` 中 `IMSConfig.Signaling` 只有
  `UseIPSec`（布尔）和认证算法（`DefaultAuthAlgorithm`），**不存在** `alg/ealg/prot/mod`
  等 Security-Client 算法字段。Apple 的 IMS 栈使用固定默认
  `ipsec-3gpp; alg=hmac-sha-1-96; ealg=aes-cbc; prot=esp; mod=trans`，
  与 SimAdmin 基线一致，因此 iOS 侧没有可提取的 per-carrier 差异。
- **Pixel / Android**：框架定义了
  `ims.ipsec_authentication_algorithms_int_array`（0=HMAC-MD5、1=HMAC-SHA1）与
  `ims.ipsec_encryption_algorithms_int_array`（0=NULL、1=3DES-CBC、2=AES-CBC），
  可映射为 `/sip/common/security_client` 对象
  `{mechanism: ipsec-3gpp, integrity_algorithm, encryption_algorithm, protocol: esp, mode: trans}`。
  提取器已接入（`carrier_Bundles/android/pixel/catalog.py _security_client`），
  实际覆盖数量由下次固件重建的 `profiles_with_security_client` 统计给出。
- **协议固定项**：`prot=esp`、`mod=trans` 是 3GPP TS 33.328 固定值，三份数据源都不需要也不能提供。
- SimAdmin 校验边界：VoWiFi 只接受 `hmac-sha-1-96/aes-cbc/esp/trans`；
  LTE 接受 `hmac-md5-96|hmac-sha-1-96 / null|aes-cbc / esp / trans`。
  若运营商声明 3DES-CBC 等超出范围组合，SimAdmin 会跳过该 profile（不污染其他 profile）。

## 5. 数据来源与安全边界

- 以上所有配置均来自公开固件静态制品（Factory Image / CarrierSettings / CarrierConfig APK）；
- 数据库只读、schema v7、不保存 IMSI/ICCID/MSISDN/IMEI/AKA 材料等用户身份数据；
- 网络会话中动态分配的 P-CSCF、IPsec SPI/端口、entitlement token 不属于 catalog 范围。


---

# 第二部分：AKAv2 与 IKE/ESP 提案（自《算法缺口-AKAv2与IKE-ESP提案》）

# 算法缺口：AKAv2 与 IKE/ESP 提案（已修复 + vowifi-go 参考分析）

本文档记录两件事：

1. 从开源项目 `vowifi-go`（`SimAdmin/vowifi-go-21eb46189e0ab82c56c791c4834a9383078f9c5f.zip`）中能/不能拿到哪些算法实现；
2. 基于该分析，SimAdmin 对 IMS AKA 与 IKE/ESP 算法缺口做了什么修复、修复后的真实数据库审计结果，以及仍然存在的缺口。

---

## 1. vowifi-go 分析结论

### 1.1 AKAv2-MD5：vowifi-go 没有实现，SimAdmin 自己已实现

- `runtimehost/simauth/simauth.go` 只实现 RFC 3310 **AKAv1-MD5**：`nonce = base64(RAND || AUTN)`，`HA1 = MD5(username:realm:hex(RES))`，以及同步失败时的空密码 + `auts=` 重同步响应。
- `runtimehost/voiceclient/register.go` 的 `scoreDigestChallenge` 虽然能识别 `AKAv2-MD5` 并打较低分，但最终仍调用 `simauth.ComputeDigest`，而该函数只把密码当作 RFC 3310 的 RES，**不按 RFC 4169 计算 AKAv2 密码**。
- SimAdmin 的 `backend/src/connectivity/core/digest_aka.rs` 已经实现 RFC 4169 `AKAv2-MD5`：

  ```rust
  if algorithm.eq_ignore_ascii_case("AKAv2-MD5") {
      key = RES || IK || CK;
      digest = hmac_md5(key, b"http-digest-akav2-password");
      password = base64(digest);
  }
  ```

- 结论：**AKAv2 不需要从 vowifi-go 移植**。此前真正挡住的只是 catalog 适配器加载 profile 时只放行 `AKAv1-MD5`（`carrier_catalog_v7.rs`），运行链路本身早已支持。现已放行。

### 1.2 IKE/ESP：vowifi-go 可参考，但不能直接移植

- `runtimehost/crypto/diffiehellman.go` 只实现了 **DH group 14（modp2048）**；`runtimehost/ikev2/constants.go` 虽声明了 1/2/5/14/15/16/17/18 等组号，`proposal_match.go` 的默认列表也宣称支持 modp1536/3072/4096，但**没有这些组的素数/数学实现**。
- `internal/vowifi/ipsec3gpp/algorithms.go` 有 **HMAC-MD5-96** 的参考实现（`hmac.New(md5.New, key)` 截断 12 字节），但它是给 **3GPP TS 33.203 SIP Security-Client 机制**用的，不参与 IKEv2 的 PRF/完整性密钥运算。其数学形式与 IKE 所需一致，可作为参考。
- 结论：**IKE 侧的 MD5/SHA2-384 与 MODP 1/5/15/16/18 需要 SimAdmin 自己实现**（基于 RFC 2403 / RFC 2104 / RFC 2409 / RFC 3526），vowifi-go 提供不了现成代码。

---

## 2. SimAdmin 已实现的修复（2026-08-08）

### 2.1 IMS AKA

- `carrier_catalog_v7.rs` 的算法白名单改为接受 `AKAv1-MD5` 与 `AKAv2-MD5`；空字符串视为未指定（默认 AKAv1-MD5），兼容真实数据库中的空 `algorithm` 字段。
- 运行链路 `live.rs` / `digest_aka.rs` 早已支持 `AKAv2-MD5`，无需改动。

### 2.2 IKE/ESP 完整性 + PRF

新增 IANA transform ID 与实现：

| 算法 | 类型 | Transform ID | 实现位置 |
|---|---|---:|---|
| HMAC-MD5-96 | IKE/ESP 完整性 | 1 | `ike_payloads.rs` / `ike_keys.rs` / `ike_encrypted.rs` / `dataplane.rs` |
| PRF HMAC-MD5 | IKE PRF | 1 | `ike_keys.rs` |
| HMAC-SHA2-384-192 | IKE/ESP 完整性 | 13 | 同上 |
| PRF HMAC-SHA2-384 | IKE PRF | 6 | `ike_keys.rs` |

- HMAC-MD5 由项目已有 `md5` crate 实现（ring 不提供 MD5），并已用 RFC 2202 测试向量 `9294727a3638bb1c13f48ef8158bfc9d` 验证。
- catalog 映射新增：`MD5-96`/`MD5-128` → `md5`，`SHA2-384` → `sha384`。

### 2.3 MODP DH group

新增 group 与 RFC 素数（素数逐字从 RFC 2409 / RFC 3526 官方文本提取并核对字节长度）：

| Group | 名称 | 素数来源 | public value 长度 |
|---:|---|---|---:|
| 1 | modp768 | RFC 2409 §6.1 | 96 B |
| 2 | modp1024 | RFC 2409 §6.2（原有） | 128 B |
| 5 | modp1536 | RFC 3526 §2 | 192 B |
| 14 | modp2048 | RFC 3526 §3（原有） | 256 B |
| 15 | modp3072 | RFC 3526 §4 | 384 B |
| 16 | modp4096 | RFC 3526 §5 | 512 B |
| 18 | modp8192 | RFC 3526 §7 | 1024 B |

- `ike_dh.rs` 的 `DhGroup` 枚举、`ike_payloads.rs` 的 proposal token、`carrier_catalog_v7.rs` 的 `dh_group_token` 已同步扩展。
- 8192 位模幂在 debug 构建下较慢（单测约 60s），release 构建可用。

---

## 3. 修复后的真实数据库审计结果

运行 `SIMADMIN_CATALOG_AUDIT_DIR=<SimAdmin> cargo test carrier_catalog -- --nocapture`：

| 数据库 | 接入 | ready 总数 | 可解析 | 剩余错误 |
|---|---|---:|---:|---|
| `carrier-bundles-ios-ipcc.sqlite3` | LTE | 226 | **226** | 无 |
| `carrier-bundles-ios-ipcc.sqlite3` | VoWiFi | 101 | **101** | 无 |
| `carrier-bundles-iphone16promax-26.6.sqlite3` | LTE | 322 | **322** | 无 |
| `carrier-bundles-iphone16promax-26.6.sqlite3` | VoWiFi | 141 | **140** | 1（DH group 21） |
| `carrier-bundles-pixel-mustang.sqlite3` | LTE | 1348 | **1348** | 无 |
| `carrier-bundles-pixel-mustang.sqlite3` | VoWiFi | 0 | 0 | 数据库本身无 ready VoWiFi profile（见 carrier_Bundles 兼容性文档） |

修复前 iOS IPCC 只有 LTE 217 / VoWiFi 63 可解析，16PM 为 LTE 314 / VoWiFi 109。

---

## 4. 仍然存在的缺口

### DH group 21（ECP-521）

唯一剩余被拒 profile：

```text
profile-coppervalleytelecom-lte-us-base-312380-ab1d1bbbe1
（carrier-bundles-iphone16promax-26.6.sqlite3，VoWiFi）
```

- group 21 是 **椭圆曲线组 secp521r1**，不是 MODP 模幂，需要单独的 EC 实现（如 RustCrypto `p521` crate）或 strongSwan 式 ECP 支持。
- 目前影响 1 条 profile；建议后续按需引入 `p521` crate，或确认该运营商的 ePDG 是否接受降级到 MODP 提案后再放开。

---

## 5. 关键文件

- `SimAdmin/backend/src/connectivity/modems/softstack/vowifi/carrier_catalog_v7.rs`（算法/DH 白名单与映射）
- `SimAdmin/backend/src/connectivity/modems/softstack/vowifi/ike_payloads.rs`（transform ID 与 proposal 解析）
- `SimAdmin/backend/src/connectivity/modems/softstack/vowifi/ike_keys.rs`（PRF/完整性密钥调度，含 HMAC-MD5）
- `SimAdmin/backend/src/connectivity/modems/softstack/vowifi/ike_encrypted.rs`、`dataplane.rs`（IKE/ESP 完整性标签）
- `SimAdmin/backend/src/connectivity/modems/softstack/vowifi/ike_dh.rs`（MODP 768/1536/3072/4096/8192 素数）
- `SimAdmin/backend/src/connectivity/core/digest_aka.rs`（AKAv2-MD5 密码派生，原已存在）


---

# 第三部分：未实现算法说明（自《SimAdmin_未实现算法说明-AES-CTR与AES-XCBC》）

# SimAdmin 未实现算法说明：AES-CTR / AES-XCBC（2026-08-08）

## 1. 背景

更新后的 Pixel 数据库里，部分运营商的显式 IKE 提案（来自 Android IWLAN `iwlan.*` 键）包含以下
我们目前**没有实现**的算法：

| 算法 | 用途 | 标准 | 出现于 |
|---|---|---|---|
| AES-CTR-128 | IKE/ESP 加密（计数器模式） | RFC 5930 | Singtel（SG 52501）等 |
| AES-XCBC-96 | IKE/ESP 完整性 | RFC 3566 | Singtel 等 |
| PRF AES128-XCBC | IKE PRF | RFC 4434 | Singtel 等 |

另有 `AES-CBC`（不带密钥长度）这类提取格式问题，已按 128 位默认值归一化处理。

## 2. 处理决定：不实现算法本体，改为"提案过滤 + 基线兜底"

- `carrier_catalog_v7.rs` 的提案映射现在会**跳过**它不认识的算法条目（AES-CTR、AES-XCBC、PRF-XCBC、
  ECP-521 等），只保留能支持的部分（AES-CBC / SHA1 / SHA2 / MODP 组）；
- 若某 profile 的提案全部被跳过，则回落源码基线
  `aes128-sha256-modp2048 / aes128-sha1-modp2048 / aes128-sha256-modp1024`；
- 效果：这些运营商 profile 现在都能正常加载（Pixel 984/984、16PM 141/141），
  加载后 IKE 只携带受支持的提案子集去协商。

## 3. 代价与风险

- 如果某运营商的 ePDG **只接受** AES-CTR/AES-XCBC（不太可能，因为其提案列表里同时包含 AES-CBC/SHA1），
  协商会以 `NO_PROPOSAL_CHOSEN` 失败——这是正常的协商失败，日志可查，不会静默错连；
- 因此当前行为是"能加载、能尝试"，不等于"保证能注册成功"。

## 4. 如果以后要实现这些算法，改动点

- `ike_payloads.rs`：新增 `ENCR_AES_CTR = 13`、`AUTH_AES_XCBC_96 = 5`、`PRF_AES128_XCBC = 4`
  及 proposal token 解析；
- `ike_keys.rs` / `ike_encrypted.rs`：密钥调度与完整性标签支持 AES-CTR（计数器模式加密）
  与 AES-XCBC-MAC（RFC 3566，K1/K2/K3 派生）；
- `dataplane.rs`：ESP 数据面支持 AES-CTR 加密与 AES-XCBC-96 完整性；
- `carrier_catalog_v7.rs`：`algorithm_token` 增加对应映射；
- 引入的加密原语可用现有 `aes` crate 实现（XCBC 的 K1/K2/K3 需要额外 ~50 行）。

## 5. 相关记录

- 真实审计（2026-08-08）：iOS IPCC LTE 226/226、VoWiFi 101/101；
  iPhone 16 Pro Max LTE 322/322、VoWiFi 141/141；Pixel LTE 1348/1348、VoWiFi 984/984。
- 涉及文件：`SimAdmin/backend/src/connectivity/modems/softstack/vowifi/carrier_catalog_v7.rs`
  （提案过滤与 AES-CBC 归一化）。
