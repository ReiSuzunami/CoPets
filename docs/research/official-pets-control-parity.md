# 官方 Pets 控制能力兼容记录

> Status: Research snapshot
> Owns: 2026-07-19 official Pets control-parity evidence; not current product contract
> Update when: Preserve this snapshot; add a dated re-verification section or a new snapshot
> Last verified: 2026-07-19

快照：2026-07-19。证据来自本机当前 `ChatGPT.app` 的 `app.asar`、运行中的用户私有 IPC Router，以及 Sidecar 实机联调。

## 结论

本地 Codex 任务不需要 WebView 注入。Sidecar 可以作为 thread follower 加入官方进程间同步层：

1. 监听 `thread-stream-following-changed`，发现 Codex App 当前正在跟随的 conversation/host。
2. 广播自己的 `following:true`，任务 owner 会定向返回 `thread-stream-state-changed` snapshot。
3. 只从 snapshot 的 `requests` 提取等待操作；完整任务 prompt、回答正文和命令输出不发送到 Sidecar WebView。
4. 用户点击后，Sidecar 通过 Router 把 follower request 定向发给 snapshot 的 owner client。

这条路径已经在不重启 Codex App 的情况下实机收到 snapshot。它不修改 Codex 页面，也不接管 app-server。

## 已实现控制

| UI 操作 | Follower method | 关键参数 |
|---|---|---|
| 运行/拒绝命令 | `thread-follower-command-approval-decision` | `conversationId`, `requestId`, `decision` |
| 允许/拒绝网络 | 同上 | `decision=accept/decline` |
| 应用/拒绝文件变更 | `thread-follower-file-approval-decision` | `conversationId`, `requestId`, `decision` |
| 允许/拒绝权限 | `thread-follower-permissions-request-approval-response` | `response.permissions`, `scope=turn` |
| 回答问题 | `thread-follower-submit-user-input` | `response.answers[questionId].answers[]` |
| MCP/tool 确认 | `thread-follower-submit-mcp-server-elicitation-response` | `response.action/content/_meta` |
| 停止任务 | `thread-follower-interrupt-turn` | `conversationId` |
| 运行中追问 | `thread-follower-steer-turn` | text input + restore message |
| 已结束任务追问 | `thread-follower-start-turn` | steer 无 active turn 时回退 |

审批和输入只能由用户点击或提交触发。没有自动批准、自动回答或后台停止逻辑。

## UI 同步

- 等待请求以紧凑卡片显示，支持 command、network、patch、permission、question、tool。
- 多条请求可滚动；关闭通知只影响 Sidecar 展示，不替用户作出决定。
- 提交期间按钮禁用；失败保留卡片和输入并显示错误。
- Working 状态显示停止按钮；可控任务显示追问入口。
- 拖动宠物本体可移动窗口；右下角缩放热点仅在指针进入时浮现。
- 位置和大小写入 WebView 本地存储。重启恢复；原显示器不存在时回到主屏可见区域。

## 内容边界

为做出知情审批，Sidecar 会显示官方 Pets 同类的紧凑请求内容：问题文字/选项、命令摘要、权限原因或工具说明。以下内容不进入 WebView：

- 完整用户任务 prompt；
- 完整 assistant 回答；
- command stdout/stderr；
- 原始 conversation/request/question ID；
- snapshot 中与等待操作无关的 turn/history 内容。

原始 ID、精确 permissions 和用户刚输入的追问只在 Rust 内存中用于构造当前请求，不写入 Sidecar 日志或数据库。

## 尚未完全对齐

| 项目 | 当前状态 |
|---|---|
| Plan 卡片的 `Implement plan` | 尚未接。官方会先更新 collaboration mode，再发送包含 plan 的 follow-up。 |
| Patch 的文件数、增删行和 `Review` 跳转 | 审批已可用；当前卡片只有紧凑原因，未复刻 review 跳转。 |
| MCP 表单型 elicitation | 官方悬浮层本身也只显示摘要；Sidecar 已支持 accept/decline，未在悬浮层填写结构化表单。 |
| Option picker / setup context picker | 不属于当前官方紧凑 question 卡路径，尚未接。 |
| 云端任务 stop/reply | 官方走 cloud mutation，不走本地 follower IPC；Sidecar 当前只控制本机 app-server conversation。 |
| 稳定 ABI | IPC 和 snapshot 都是 App 私有协议；需要按 Codex App 版本维护 fixture/fingerprint。 |

## 协议安全检查

- Router request 必须定向到 owner client；owner 仍通过自己的 thread ownership 校验。
- Sidecar 对 Router 的 discovery request 始终返回 `canHandle:false`，不会冒充 app-server owner。
- action kind 与 request method 在 Rust 后端再次匹配；前端不能自行提供 conversationId、requestId、permissions 或任意 method。
- 权限允许沿用请求中的 permissions，拒绝固定为空 permissions；scope 只使用官方 overlay 已观察到的 `turn`。
- MCP accept/decline response 与官方 helper 一致：`{action, content, _meta}`。
