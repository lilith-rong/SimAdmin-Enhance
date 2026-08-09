# SimAdmin-Enhance（VoLTE / VoWiFi 工作留档）

SimAdmin 项目中 VoLTE / VoWiFi 相关已完成工作的文档与参考二进制归档。

## 文档

| 文件 | 内容 |
|---|---|
| [IMS注册流程与修复复现说明.md](IMS注册流程与修复复现说明.md) | 标准 IMS 注册流程（TS 33.203 / RFC 3329）、410 真机修复（自动分片 IPv4/IPv6 + 入向重组）、复现步骤 |
| [VOLTE_逆向与重构总文档.md](VOLTE_逆向与重构总文档.md) | VoLTE 逆向与重构单一权威参考（beta2 IDA 结论、QMI 端点实测、重构落地状态） |
| [VOLTE_beta2_IDA逆向差异与后续修改指导.md](VOLTE_beta2_IDA逆向差异与后续修改指导.md) | beta2 IDA 逐项差异与建议实施阶段 |
| [VoWiFi算法与配置说明.md](VoWiFi算法与配置说明.md) | iwlan 提取边界、AKAv2/IKE/ESP 缺口与修复、未实现算法取舍 |
| [SimAdmin_多路径语音短信Trunk开发全程记录.md](SimAdmin_多路径语音短信Trunk开发全程记录.md) | 多路径语音/短信/SIP Trunk 开发历程（历史归档） |
| [beta8_VoLTE与数据连接_逆向说明_Codex.md](beta8_VoLTE与数据连接_逆向说明_Codex.md) | beta8 成品 VoLTE/数据连接逆向说明 |

## 参考二进制与源码

- `Volte/`：simadmin 1.6 / 1.7 / beta2 / beta8 参考二进制
- `SimAdmin-main_*`：SimAdmin 源码打包
- `LTE_manager-main.zip`、`vowifi-go-*.zip`：参考项目
