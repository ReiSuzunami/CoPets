# Codex App 无重启寄生附着研究

> Status: Research snapshot
> Owns: 2026-07-19 running-app attachment evidence; not current product contract
> Update when: Preserve this snapshot; add a dated re-verification section or a new snapshot
> Last verified: 2026-07-19

快照：2026-07-19。目标：桌宠在 Codex App 已经运行后直接附着，不代理用户请求、不接管 app-server、不要求重启，也不依赖官方 Pets UI。

## 结论

**可行，而且不应再把 Hook 当主通道。** 本机 Codex App 主进程暴露了一个用户私有 Unix socket：`~/.codex/ipc/ipc.sock`。静态分析确认它是多客户端 `IpcRouter`；实机原型已完成协议初始化，并在不重启 App 的情况下收到当前会话广播。

最合适的结构是：

1. **App IPC Router：** 负责 App 级 thread stream、following、read/unread、archive、queued follow-up 等状态变化。
2. **Session JSONL 增量尾读：** 负责 turn、reasoning、tool、review、subagent、审批和完成/失败等细粒度事件。
3. **SQLite 只读对账：** 启动时补当前 thread 清单、归档状态和 spawn edges；不当实时总线。
4. **Hook：** 只作为 CLI 或旧版本 Codex 的可选增强，不再是 App 寄生模式的前提。
5. **App activity log：** `thread_stream_view_activity_changed` 补当前可见 task；必须与 SQLite thread index 对账，过滤同一 view 内部的非 thread conversation。
6. **Accessibility：** 只补前台窗口等纯 UI 状态；需要用户授权，可完全关闭。
7. **Follower 控制：** 显式用户操作通过同一 IPC Router 定向发送给 thread owner；不需要 CDP/WebView 注入。
8. **CDP 深度模式：** 仅作显式实验模式；当前已运行 Prod 实例没有 endpoint，不能热附着。

这样可以透明观察已经运行的 Codex App。代价从“托管另一套 app-server”降为一个本地只读 sidecar；真正的代价是兼容私有协议，而不是运行两套 Codex。

## 已验证证据

### 1. 运行中 App 拓扑

- ChatGPT/Codex App 主进程启动内嵌 `codex ... app-server`，两者使用匿名 socketpair/stdin/stdout；第三方不能附着该 app-server 的 stdio。
- 主进程另行监听 `~/.codex/ipc/ipc.sock`，文件权限为当前用户私有；本机没有 ChatGPT/Codex TCP listener。
- 因此，app-server stdio 不是寄生入口，主进程 IPC Router 才是。

### 2. IPC 协议

本机 `/Applications/ChatGPT.app/Contents/Resources/app.asar` 的当前实现显示：

- framing：4 字节 little-endian 长度 + UTF-8 JSON；
- 初始化：`request(method="initialize", params.clientType=...)`；
- router 为每个 client 分配 `clientId`，随后转发 broadcast/request/response；
- client discovery 可声明 `canHandle: false`，因此观察器不必接受任何控制请求；
- 当前广播版本表包含：
  - `thread-stream-state-changed` v11
  - `thread-stream-following-changed` v1
  - `thread-read-state-changed` v2
  - `thread-archived` v2
  - `thread-unarchived` v1
  - `thread-queued-followups-changed` v1
  - `client-status-changed`、`ipc-connection-reset`
- 同一协议还定义 follower 控制请求，包括 start/steer/interrupt turn、审批决定、提交 user input、提交 MCP elicitation response。当前原型**全部拒绝**，只做观察。

2026-07-19 实机运行 `node src/cli.mjs --ipc-only`：

```json
{"source":"codex-app-ipc","status":"attached","clientIdHash":"..."}
{"source":"codex-app-ipc","signal":"thread-stream-following-changed","version":1,"paramKeys":["conversationId","following","hostId"]}
```

这证明“已运行后附着”已跑通，不是只靠 strings 推断。

### 3. Session JSONL 是实时事件面

`~/.codex/sessions/**/rollout-*.jsonl` 在 App turn 进行中持续 append，不是仅在结束后落盘。当前机器样本中存在：

- `task_started`、`task_complete`、`turn_aborted`、`error`
- `agent_reasoning`、`token_count`
- `exec_command_end`、`mcp_tool_call_end`、`web_search_end`、`patch_apply_end`
- `guardian_assessment`
- `entered_review_mode`、`exited_review_mode`
- `sub_agent_activity`
- `response_item.function_call(name=request_user_input)`
- collab spawn/wait/interaction/close 事件

实机原型以 500ms 文件大小轮询兜底，已在当前 App 会话中连续捕获 reasoning、custom tool call/output、token_count。只输出事件类别、状态和 hash 后的 ID；prompt、消息、工具参数、stdout 和结果正文均被丢弃。

### 4. App 日志可以补当前可见 thread，但不是单独使用

- 实机日志包含 `thread_stream_view_activity_changed active=<bool> conversationId=<UUID>`，并同时给出 `rendererWindowId`、`rendererWebContentsId`、窗口 focused/visible、resumeState 和 streamRole。
- 切换 task 时可观察到旧 conversation `active=false`、新 conversation `active=true`；因此它能补 IPC/JSONL 缺失的 GUI view activity。
- `active` 不是全局唯一 selection。同一 window/webContents 可能同时激活真实 thread 和内部 conversation。原型已修正为：仅接受 `state_5.sqlite.threads` 中存在的 ID，并以 App instance + window + webContents + conversation 复合键对账。
- 2026-07-19 实机 `--logs-only` 启动快照成功返回当前 thread，未知内部 conversation 被过滤；ID 仅输出 hash。该信号仍属于私有日志契约，版本漂移时应降级为“selection unknown”。

### 5. DevTools 实验新增事实

- 公开插件的 macOS 启动链要求先关闭已运行实例，再用 wrapper 重启并附加 `--remote-debugging-address=127.0.0.1 --remote-debugging-port=...`；它不是对现有生产实例的无重启附着。
- 隔离第二实例实验受到 App 的 single-instance 锁；即使使用临时 profile，也不能据此宣称第二个 UI 实例已创建 renderer target。
- 对临时复制并重签的 App 副本做过 CDP 探针，但副本未进入可用 CDP/renderer target；该路线已放弃，不作为支持证据。
- 本机目前仅确认启动时 loopback browser-level CDP endpoint 可响应；`/json/list` 未拿到 renderer target。因此不能宣称已读取主 UI DOM、active route、selected task 或 React/store。

## 功能兼容矩阵

| 功能 | App IPC | JSONL | 结论 |
|---|---:|---:|---|
| 已运行 App 自动附着 | 已实测 | 已实测 | **兼容**，不重启、不配 Hook |
| 多 thread/host 标识 | 有 conversationId/hostId | 文件/thread/turn ID | **兼容**，ID 仅内存使用或 hash |
| 正在工作/turn start | stream 状态具备协议面 | `task_started`；中途 reasoning/tool 活动可恢复 working | **兼容** |
| 工具调用/完成 | 不负责细节 | tool call + 多类 `*_end` | **兼容** |
| turn 完成/中断/失败 | stream 状态可辅助 | `task_complete`/`turn_aborted`/`error` | **兼容** |
| 权限审批等待与响应 | owner snapshot + follower command/file/permission response | JSONL 仅补状态 | **已实现**，显式点击后定向返回 owner |
| 普通 request_user_input | owner snapshot + follower submit | JSONL 仅补状态 | **已实现**，opaque question ID 在 Rust 映射回原 ID |
| Review/plan | stream 辅助 | entered/exited review | **兼容** |
| Subagent 活动 | 无细节 | `sub_agent_activity` + collab events | **兼容** |
| 未读/read state | `thread-read-state-changed` | SQLite 可启动对账 | **兼容变化；启动快照待实现** |
| Archive/queued follow-up | 有明确广播 | SQLite 可对账 | **兼容** |
| 当前选中的 GUI task | 无明确广播 | 文件无 GUI selection | **实用兼容**：App activity log + SQLite thread index 已实机恢复当前 view；私有格式漂移时降级 unknown |
| token/文本流展示 | 可从私有 stream 进一步研究 | 有正文但默认禁止采集 | **刻意不兼容文本**，宠物不需要 |
| 代替用户审批/输入/停止 | 私有 follower request | 不可写 | **已实现显式控制**；无自动批准/回答/停止 |
| 官方 Pets 的 PiP/悬浮容器 | 与状态无关 | 无 | **自行实现桌面窗口** |
| 云端跨设备/未在本机落盘会话 | 无 | 无 | **不兼容** |

## 与官方 Pets 的差距

官方 Pets 在宿主 App 内部，可以直接读取 React/store/app-server 状态并跟随当前 UI。sidecar 无法获得同等级的稳定 ABI，所以差距主要是：

- App 更新可能改变 IPC method、版本或 JSONL schema；
- “当前选中的 task”来自私有 activity log + thread index，而非稳定公开 ABI；
- 某些 attention 状态必须先收集真实审批/request fixture 才能确认优先级；
- 无法复用官方 PiP 容器、菜单和内部动画状态机。

这不是“官方 Pets 也做不到”。官方能依赖内部 store；我们通过同一 App 的进程间同步层和落盘事件流逼近它，但需要维护适配器。

## 发布架构

```text
Codex App (already running)
  |-- ~/.codex/ipc/ipc.sock ----> IpcObserver (broadcast only)
  |-- ~/.codex/sessions/*.jsonl -> SessionTailer (append only)
  |-- ~/Library/Logs/... -------> ViewActivityTailer
  `-- state_5.sqlite -----------> Reconciler / thread index (read-only)
                                         |
                                  EventNormalizer
                                         |
                           Pet state machine / renderer
```

安全边界：

- IPC client 只自动发送初始化、following 订阅和 `canHandle:false`；审批/输入/停止/follow-up 仅由用户显式操作触发；
- JSONL 读取后立刻字段白名单化，不持久化正文；
- SQLite 用 read-only/query-only + busy timeout，不能用忽略 WAL 的 immutable 模式；
- 每个 Codex App 版本保存协议 fingerprint 和 fixture，未知版本降级到 JSONL；
- 任一来源断开时显示 `unknown/disconnected`，不能伪报 completed。

## 下一阶段

1. 扩展 SQLite 启动对账，恢复 archived/unread 快照；thread index 已用于 view activity 过滤。
2. 制作真实 fixture：approval、request_user_input、review、subagent、abort、failed 各跑一次。
3. 建版本 fingerprint：App bundle version、IPC method/version 表、JSONL schema hash。
4. 把事件归一化为稳定的 Pet 协议：`idle / working / tool / needs-input / needs-approval / reviewing / completed / failed / disconnected`。
5. 再接高分辨率 renderer 与 Pet Creator 导入；renderer 不直接依赖任何 Codex 私有 schema。
6. CDP 调研已收口：无重启 Prod 热附着不可行，且不是 MVP 功能前提；未来只在独立实验分支验证 renderer target。

## 2026-07-21 selection re-verification

Scope: projectless foreground selection only. Environment: Codex App `26.715.61943` build `5628`,
embedded `codex-cli 0.145.0-alpha.27`, macOS `26.5.2` (`25F84`), arm64.

A sanitized structural reconciliation of currently retained App activity logs against the current
`state_5.sqlite.threads` index found three conversation identifiers absent from the index. Two had at
least one event with the full conjunction `active=true`, focused, visible, `streamRole=owner`, and a
canonical UUID. Only hashes and aggregate counts were inspected; no title, prompt, answer, summary,
path, or raw identifier was recorded. The logs do not expose a trustworthy project-membership fact,
so the user's projectless classification remains external evidence rather than an inferred log field.

This disproves the older assumption that every real foreground conversation must already exist in
the SQLite index. The desktop selector now keeps the index requirement for ordinary activity and
historical owner routes, but accepts an unindexed canonical UUID only when all explicit foreground
owner fields above are present. Missing fields, follower streams, hidden/unfocused views, placeholder
IDs, and arbitrary unknown activity still fail closed. The standalone Node diagnostic remains
index-only and does not gain selection authority.

The same retained log supplied a sanitized switch sequence of `old active=false`,
`ownerRoutePath=/`, then a strict foreground owner activity and `/local/<new>` owner sync. The root
route is therefore an observed invalidation signal, not an unparseable route. DeskPal clears cached
route and activity authority on that exact owner-sync shape; otherwise an older indexed route can
mask a later projectless foreground activity whose `/local/<new>` route correctly remains ineligible
without an index row.

Regression coverage lives in `AppLogSelectionAdapter` tests under
[`selection.rs`](../../src-tauri/src/observer/selection.rs). A full user-visible projectless switch
still belongs to the macOS integration gate; this re-verification records schema/evidence and the
bounded fallback, not an end-to-end compatibility claim.

## 参考

- [OpenAI Codex app-server README](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md)
- [Apple AXUIElement](https://developer.apple.com/documentation/applicationservices/axuielement)
- [Electron webContents](https://www.electronjs.org/docs/latest/api/web-contents)
- [Hook 能力调查](./codex-hook-capability.md)
- [非 Hook API 调查](./codex-non-hook-api-capability.md)
- [WebView / DevTools / CDP Hook 调查](./codex-devtools-hook.md)
