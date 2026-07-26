# rpmsg_wwan_ctrl_multi — 为 IMS 提供独立 QMI 端点

## 这是什么 / 为什么需要

ModemManager 独占主 QMI 端口（`/dev/wwan0qmi0`）来跑普通移动数据。在同一个端口上再建第二条数据会话（IMS/VoLTE bearer）会：

- 直接报 `QMI protocol error (14): CallFailed - interface-in-use-config-match`，或者
- 在 MSM8916 这类固件上**激活 IMS PDP context 时直接崩基带**（`mmcli -L` 变 `No modems were found`）

解决办法是给 IMS 一个**物理独立的 QMI 端点**。基带固件本身提供了一堆备用 SMD 控制通道（`DATA6_CNTL`、`DATA7_CNTL`…），但内核自带的 `rpmsg_wwan_ctrl` 驱动只认三个通道：

```c
{ "DATA5_CNTL", WWAN_PORT_QMI }   // -> wwan0qmi0（ModemManager 在用）
{ "DATA4",      WWAN_PORT_AT  }   // -> wwan0at1
{ "DATA1",      WWAN_PORT_AT  }   // -> wwan0at0
```

用 `driver_override` 强行把 `DATA6_CNTL` 绑给它是**不行的**：驱动匹配不到，`driver_data` 为 0（`WWAN_PORT_UNKNOWN`），内核会把它发布成一个类型不对的端口（实测得到 `wwan0at2`，`type=AT`）。这种端口的表现是"半通"：

| 操作 | 结果 |
|---|---|
| `qmicli --get-service-version-info` | ✅ 能返回 `wds (1.36)` `dms (1.14)` |
| 分配 WDS 客户端 | ⚠️ 不稳定，常报 `CID allocation failed ... endpoint hangup` |
| `--wds-start-network` | ❌ **崩基带** |

本模块把这些备用通道以**正确的 `WWAN_PORT_QMI` 类型**注册，于是它们被发布成真正的 QMI 端口（`wwan0qmi1`、`wwan0qmi2`…），数据面完整可用。

## 与内核自带驱动共存

本模块**故意不接管** `DATA1` / `DATA4` / `DATA5_CNTL`，所以可以和内核自带的 `rpmsg_wwan_ctrl` 同时加载，不会抢 ModemManager 正在用的端口。加载顺序无所谓。

## 编译

### 前提

```sh
uname -r                                    # 记下内核版本
ls /lib/modules/$(uname -r)/build/Makefile  # 需要内核头文件
which gcc make                              # 需要工具链
```

设备上没有 gcc/make 时，装一下（Debian/Ubuntu 系）：

```sh
apt-get update && apt-get install -y build-essential
```

模块签名检查需为关闭状态（本项目目标设备上 `CONFIG_MODULE_SIG_FORCE` 未开启，可直接加载自编译模块）。

### 设备上本地编译

```sh
cd kernel/rpmsg_wwan_ctrl_multi
make
make install     # 装到 /lib/modules/$(uname -r)/extra/simadmin/ 并 depmod
make load        # modprobe
```

### 从主机交叉编译

```sh
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- \
     KDIR=/path/to/linux-headers-6.17.0-rc6-lkiuyu-compile+
```

然后把 `rpmsg_wwan_ctrl_multi.ko` 拷到设备的
`/lib/modules/<内核版本>/extra/simadmin/`，执行 `depmod -a` 再 `modprobe`。

## 验证

```sh
# 1. 模块已加载
lsmod | grep rpmsg_wwan_ctrl_multi

# 2. 备用通道被识别为 QMI（关键：type 必须是 QMI，不是 AT）
for p in /sys/class/wwan/*; do
  echo "$(basename $p) type=$(cat $p/type 2>/dev/null)"
done
# 期望出现：wwan0qmi1 type=QMI

# 3. 新端点确实能跑 WDS
qmicli -d /dev/wwan0qmi1 --device-open-qmi --get-service-version-info | grep wds
qmicli -d /dev/wwan0qmi1 --device-open-qmi \
       --device-open-net='net-raw-ip|net-no-qos-header' \
       --client-no-release-cid --wds-noop
```

如果通道没自动绑定（`driver_override` 之前被写过），手动绑一次：

```sh
D=remoteproc0:smd-edge.DATA6_CNTL.-1.-1
echo rpmsg_wwan_ctrl_multi > /sys/bus/rpmsg/devices/$D/driver_override
echo "$D" > /sys/bus/rpmsg/drivers/rpmsg_wwan_ctrl_multi/bind
```

## 让 ModemManager 不要碰这个端点

IMS 端点必须由 SimAdmin 独占，否则 ModemManager 会把它当额外端口接管。装一条 udev 规则（端口名按实际情况替换）：

```
# /etc/udev/rules.d/99-simadmin-secondary-qmi.rules
SUBSYSTEM=="wwan", KERNEL=="wwan0qmi1", ENV{ID_MM_PORT_IGNORE}="1"
```

```sh
udevadm control --reload-rules
systemctl restart ModemManager
```

## 扩展：增加更多通道

先看固件实际提供哪些通道：

```sh
for d in /sys/bus/rpmsg/devices/*; do cat $d/name; done | sort -u
```

然后在 `rpmsg_wwan_ctrl_multi.c` 的 `rpmsg_wwan_multi_id_table[]` 里加一行：

```c
{ .name = "DATA10_CNTL", .driver_data = WWAN_PORT_QMI },
```

规则：`*_CNTL` 控制通道用 `WWAN_PORT_QMI`；只有固件确实当 AT 控制台驱动的通道才用 `WWAN_PORT_AT`。**不要**加 `DATA1`/`DATA4`/`DATA5_CNTL`，那是内核自带驱动和 ModemManager 的地盘。

## 多基带说明

`wwan_create_port()` 的 parent 取的是**第一个 platform 设备祖先**，在 Qualcomm 上就是代表该基带的 remoteproc 设备。因此生成的端口和同基带的主 QMI 端口**共享同一个 `<addr>.remoteproc` sysfs 祖先**——SimAdmin 后端正是用这个祖先关系把"数据端口"和"IMS 端口"配对，保证多基带/多读卡器时不会跨基带操作。

对应后端实现：`backend/src/cellular/secondary_qmi.rs`
（`baseband_key_for_device()` 做祖先解析，`probe_qmi_capability()` 做能力探测）。

## 来源与许可

改编自内核自带驱动 `drivers/net/wwan/rpmsg_wwan_ctrl.c`，
Copyright (c) 2021 Stephan Gerhold，GPL-2.0-only。本模块沿用同一许可。
