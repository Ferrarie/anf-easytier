# ANFAGENT-30 部署说明（骨架）

> 状态：M3 里程碑细化中。当前记录拓扑与端口规划。

## 拓扑

```text
Ubuntu 26.04 VM (Proxmox)
├── easytier-web（魔改版，embed 前端 + config server + REST API，sqlite 卷）  :11211（仅 mesh 内网）
└── easytier-core（中心/兜底中继节点，固定虚拟 IP）                          :22020 UDP / :11010 UDP+TCP
```

## 端口

| 端口 | 协议 | 用途 | 公网 |
| --- | --- | --- | --- |
| 11211 | HTTP | web 控制台 | 否（仅 mesh 内网） |
| 22020 | UDP | config server（设备注册/下发） | 是 |
| 11010 | UDP/TCP | peer/中继 | 是 |

## 注意事项

- 上线前必须先在 VM 上验证与 `easytier@default`（官方 2.6.4，anidev 网络）的端口不冲突；
- 替换运行中服务前先起新栈验证 mesh 连通，再逐步切换（见方案稿 Q21）；
- 镜像推送到 gitea registry（`registry.example.com/anidev/...`），tag 遵循 `v2.6.4-anf.N`。
