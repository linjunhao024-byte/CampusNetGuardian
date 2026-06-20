<div align="center">
<img src="https://capsule-render.vercel.app/api?type=waving&color=0:000000,100:0b0f0b&height=220&section=header&text=CampusNetGuardian&fontSize=45&fontColor=00FF41&fontAlignY=35&desc=Automated%20Campus%20Network%20Authentication%20Engine&descSize=14&descColor=00FF41&descAlignY=55&animation=twinkling" width="100%"/>

<br/>

![Rust](https://img.shields.io/badge/Rust-1.96+-DEA584?style=flat&logo=rust&logoColor=white)
![Platform](https://img.shields.io/badge/Platform-Windows-0078D6?style=flat&logo=windows&logoColor=white)
![GUI](https://img.shields.io/badge/GUI-egui-4CAF50?style=flat)
![CLI](https://img.shields.io/badge/CLI-Terminal-FFA500?style=flat)
![Release](https://img.shields.io/badge/Release-V1.0.0-blue?style=flat)
![License](https://img.shields.io/badge/License-MIT-green?style=flat)

</div>

---

## 📡 项目简介 | Overview

**CampusNetGuardian** 是一款针对 ePortal Portal 认证协议校园网环境的**无感自动认证引擎**，专为 **广东培正学院** 校园网定制开发。

部署后以守护进程形态常驻后台，周期性探测链路状态。一旦检测到认证失效或物理断网，立即触发重连认证流程，全程无需人工干预。通过潮汐式指数退避策略平衡重连速度与资源消耗，并内置 USB 共享热感知机制，自动切换网络出口避免流量泄漏。

基于 Rust 构建，提供 **CLI 后台守护** 和 **GUI 图形界面** 两种模式，单文件分发，无需运行时依赖。

> 默认配置适配广东培正学院 ePortal 网关。其他高校需修改网关地址和 AC_IP。

---

## ⚡ 核心特性 | Features

| 能力维度 | CLI 模式 | GUI 模式 |
|:---|:---|:---|
| 交互方式 | 终端标准输出，适合无人值守 | 图形窗口，适合桌面日常使用 |
| 配置方式 | 首次运行交互式向导，后续手动编辑 `config.json` | 内置配置 Tab，支持运行时热更新无需重启 |
| 状态反馈 | 终端字符流 | 状态栏实时着色 + 日志面板 |
| 部署依赖 | 无外部依赖 | 无外部依赖 |
| 独立分发 | 单文件 `.exe`（~5MB） | 单文件 `.exe`（~5MB） |

**通用核心能力：**

- 🔄 **Portal 自动认证** — 断网后自动向 ePortal 网关发起认证请求，无需打开浏览器
- 📉 **潮汐式退避重连** — 2s → 4s → 8s → ... → 300s 指数退避 + ±0.5s 随机抖动，防止认证风暴
- 📱 **USB 共享感知** — 检测到手机/平板/随身 WiFi 等 USB 共享设备，自动禁用校园网卡避免流量泄漏，断开后自动恢复
- 🌐 **地理感知** — TCP 探测网关端口判断是否在校园网内，异地环境自动进入低功耗待机
- 🔒 **单实例保护** — 本地端口绑定互斥锁，杜绝重复启动造成资源冲突
- 📝 **静默日志** — 关键状态切换写入 `guardian_activity.log`，故障可追溯
- ⏸ **守护控制** — 支持关闭/开启/重启守护，关闭时若 USB 正在提供网络则保持现状不断网
- 🔀 **有线优先** — 双网卡同时在线时提醒用户，建议使用有线连接
- 👁 **智能排除** — 自动排除 Tailscale、WireGuard、VMware 等虚拟网卡，避免误判
- 🐕 **看门狗** — 心跳检测守护线程，卡死时自动重启
- 🔍 **自动检测** — 首次运行自动检测网卡名称、网关地址、AC_IP，用户只需输入学号密码
- ⚡ **网络测速** — 内置延迟测试（Ping 网关/百度/DNS）和下载测速（国内镜像源）
- 🎨 **主题切换** — 支持暗色/亮色主题，配置中可切换
- 🚀 **开机自启** — 可选写入 Windows 注册表实现开机自动启动
- 🗑 **一键卸载** — 删除配置、日志、清理注册表，干净卸载

---

## 🖥️ CLI 终端运行实况 | CLI Runtime Preview

```
╔══════════════════════════════════════════════════════════════════╗
║  CampusNet Guardian V1.0.0 — CLI Daemon Mode                    ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  [2026-06-20 08:00:01] CampusNet Guardian V1.0.0 已启动。        ║
║  [2026-06-20 08:00:02] 链路中断，发送认证请求。                   ║
║  [2026-06-20 08:00:03] 认证通过。                                ║
║  ..............................                                   ║
║  [2026-06-20 09:32:17] 检测到USB共享设备，禁用 以太网。            ║
║  [2026-06-20 09:32:19] USB网络就绪 (2s)。                        ║
║  [2026-06-20 10:15:44] USB设备已断开，启用 以太网。                ║
║  [2026-06-20 10:15:47] 以太网 就绪 (3s)。                        ║
║  [2026-06-20 10:15:48] 链路中断，发送认证请求。                   ║
║  [2026-06-20 10:15:49] 认证通过。                                ║
║  ..............................                                   ║
║                                                                  ║
║  [命令] stop=关闭守护 | start=开启守护 | restart=重启 | quit=退出  ║
╚══════════════════════════════════════════════════════════════════╝
```

---

## 🚀 快速开始 | Quick Start

### GUI 模式（默认）

双击 `CampusNetGuardian.exe`，首次运行自动弹出配置向导，只需输入学号和密码。

### CLI 模式

```bash
CampusNetGuardian.exe --cli
```

---

## ⚙️ 配置说明 | Configuration

首次运行自动生成 `config.json`，也可手动编辑：

| 字段 | 类型 | 说明 | 默认值 |
|:---|:---|:---|:---|
| `student_id` | `string` | 校园网学号 | `"000000000000"` |
| `password` | `string` | 认证密码 | `"000000"` |
| `ethernet_name` | `string` | 有线网卡名称（自动检测） | `"以太网"` |
| `wifi_name` | `string` | 无线网卡名称（自动检测） | `"WLAN"` |
| `gateway` | `string` | 网关地址（自动检测） | `"10.20.3.1"` |
| `ac_ip` | `string` | AC 控制器 IP（自动检测） | `"10.20.3.254"` |
| `force_ethernet_priority` | `bool` | 有线优先（双网卡在线时提醒） | `true` |
| `auto_start` | `bool` | 开机自动启动 | `false` |
| `theme` | `string` | 主题（`"dark"` / `"light"`） | `"light"` |
| `base_retry_interval` | `int` | 初始重试间隔（秒） | `2` |
| `max_retry_interval` | `int` | 最大重试间隔（秒） | `300` |
| `normal_check_interval` | `int` | 正常状态巡检间隔（秒） | `5` |

---

## 📁 项目结构 | Project Structure

```
CampusNetGuardian/
├── CampusNetGuardian.exe    # 编译产物（~5MB）
├── Cargo.toml               # Rust 项目配置
├── Cargo.lock               # 依赖版本锁定
├── README.md                # 项目说明
└── src/
    ├── main.rs              # 程序入口，模式分发
    ├── config.rs            # 配置管理，注册表操作
    ├── network.rs           # 网卡检测、启禁用、连通性
    ├── auth.rs              # Portal 认证协议
    ├── guardian.rs          # 守护线程核心逻辑
    ├── gui.rs               # egui 图形界面
    ├── cli.rs               # CLI 命令行模式
    ├── tray.rs              # Windows 系统托盘
    ├── speedtest.rs         # 延迟测试 + 下载测速
    └── theme.rs             # 主题定义（暗色/亮色）
```

---

## 🔨 从源码构建 | Build

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 克隆仓库
git clone https://github.com/YOUR_USERNAME/CampusNetGuardian.git
cd CampusNetGuardian

# 编译 release 版本
cargo build --release
```

产物位于 `target/release/campus_net_guardian.exe`（~5MB）。

---

## 🏗️ 架构原理 | Architecture

```
                    ┌──────────────────────────────┐
                    │         主控循环 (while)       │
                    └──────────────┬───────────────┘
                                   │
                    ┌──────────────▼───────────────┐
                    │  P1. USB 共享检测              │
                    │      优先级最高，阻断校园网卡    │
                    │      排除虚拟网卡(Tailscale等)   │
                    └──────────────┬───────────────┘
                                   │ 未检测到
                    ┌──────────────▼───────────────┐
                    │  P2. HTTPS 链路探测            │
                    │      HEAD baidu.com (3s超时)   │
                    └──────────────┬───────────────┘
                                   │ 链路中断
                    ┌──────────────▼───────────────┐
                    │  P3. 地理感知 + Portal 认证     │
                    │  ┌─ 在校园网 → 发起认证         │
                    │  │   成功 → 恢复正常巡检         │
                    │  │   失败 → 退避重试 (指数递增)   │
                    │  └─ 在异地 → 低功耗待机 30s      │
                    └──────────────────────────────┘
```

---

## ⚠️ 注意事项 | Notes

- 默认配置适配 **广东培正学院** ePortal 网关，其他高校需自行修改网关地址和 AC_IP
- 仅适用于 **ePortal Portal 认证协议** 的校园网环境
- 网卡启用/禁用操作需要 **管理员权限**
- 认证协议参数（`dr1004`、`jsVersion:3.3.3`）可能因网关厂商及版本不同需要调整
- 日志文件 `guardian_activity.log` 持续追加写入，建议定期清理
- 已知虚拟网卡（Tailscale、WireGuard、OpenVPN、VMware 等）会被自动排除
- 首次运行会自动检测网卡和网关，AC_IP 若检测不到会使用默认值，不影响认证

---

## 📄 许可证 | License

本项目基于 [MIT License](LICENSE) 开源。

---

<div align="center">

**无感 · 稳定 · 轻量**

`CampusNetGuardian` — 让校园网认证成为底层基础设施，而非你的日常负担。

</div>
