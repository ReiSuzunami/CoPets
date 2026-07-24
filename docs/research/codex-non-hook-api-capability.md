# Codex：Hooks 之外的 API 补偿能力

> Status: Research snapshot
> Owns: 2026-07-19 non-Hook interface evidence; not current product contract
> Update when: Preserve this snapshot; add a dated re-verification section or a new snapshot
> Last verified: 2026-07-19

快照日期：2026-07-19。

## 结论

真正能补 Hooks 缺口的接口只有 **Codex App Server**。但存在决定性边界：

- **被动旁路模式**：观察用户已在其他 CLI/ChatGPT Desktop 中运行的会话。App Server 不能透明附着另一个进程，仍以 Hooks 为主。
- **托管模式**：桌宠启动并持有自己的 `codex app-server` 连接。此时可获得完整 turn/item 流、用户输入请求、审批、明确成败、Review、plan/diff、token usage 等，接近官方 Pets 状态能力。

`exec --json`、Codex SDK、Codex MCP server、Responses API 都遵守同一所有权边界：能完整观察自己启动/恢复的运行，不能成为 ChatGPT Desktop 的全局监听器。Deep link 只负责导航。

## 本机验证

- PATH CLI：`codex-cli 0.144.5`。
- ChatGPT 内嵌 CLI：`codex-cli 0.145.0-alpha.18`。
- `app-server` transport：`stdio://`、`unix://`、`ws://IP:PORT`、`off`；WebSocket 标为 experimental/unsupported。
- 隔离 `CODEX_HOME` 下，stdio `initialize` 握手成功，并收到 `remoteControl/status/changed: disabled`。
- 当前 `$CODEX_HOME/app-server-control/app-server-control.sock` 不存在；`app-server daemon version` 无法连接；未发现 ChatGPT Desktop 暴露的可附着 control socket。
- `app-server generate-json-schema` 成功生成当前 binary 对应 v1/v2 request、response、notification schema。

传输与初始化契约见 [App Server README](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md#transport)。

## 接口比较

| 接口 | 能提供什么 | 能否观察别的进程正在运行的会话 | 适合度 |
|---|---|---|---|
| `codex app-server` | thread/turn/item 生命周期、delta、审批、用户输入、错误、token、plan、diff、review、subagent、控制 | **不能透明附着**。能读取同一 `CODEX_HOME` 的持久历史；实时事件限本连接启动/恢复并订阅的线程，或已知且开放的同一 server 端点 | 最高；推荐托管模式 |
| `codex exec --json` | `thread.started`、`turn.*`、`item.*`、`error` JSONL | 不能；只覆盖该子进程任务 | 适合桌宠启动的一次性任务 |
| Codex TypeScript SDK | 包装 CLI；`runStreamed()` 返回结构化事件；可按 thread id resume | 不能被动监听；只覆盖 SDK 启动/恢复的线程 | 适合应用内托管任务 |
| `codex mcp-server` | stdio RPC；可 start/resume/read/list thread、start/steer/interrupt turn，并流 `codex/event/*` | 无全局订阅；只覆盖该 server 管理或恢复的线程 | 实验性备选，不作为核心架构 |
| Responses API | 自有 response 的 queued/in_progress、stream、tool call、completed/failed/cancelled、webhook | 完全不能读取 Codex/ChatGPT 产品会话 | 只适合独立云端 agent |
| `codex://` deep link | 打开本地 thread、新建 thread、预填 prompt/path | 无读取、事件或回调 | 只做宠物点击跳转 |

官方来源：[`exec --json` 事件定义](https://github.com/openai/codex/blob/main/codex-rs/exec/src/exec_events.rs)、[Codex SDK README](https://github.com/openai/codex/blob/main/sdk/typescript/README.md)、[Codex MCP interface](https://github.com/openai/codex/blob/main/codex-rs/docs/codex_mcp_interface.md)、[Responses background mode](https://developers.openai.com/api/docs/guides/background)、[Responses streaming](https://platform.openai.com/docs/api-reference/responses-streaming)、[Codex deep links](https://developers.openai.com/codex/app/commands#deep-links)。

## App Server 可补能力

App Server 使用 JSON-RPC。连接需先 `initialize`，随后 `thread/start` 或 `thread/resume`；这两个方法会订阅该连接所管理线程的 turn/item 事件。[生命周期](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md#lifecycle-overview)

| Hooks 缺口 | App Server 信号 | 补偿结果 |
|---|---|---|
| 权威 Running | `thread/status/changed: active`、`turn/started` | 完整 |
| 权威 turn 成败 | `turn/completed`：`completed` / `interrupted` / `failed`，失败含 error | 完整 |
| 普通等待用户 | `item/tool/requestUserInput` + `serverRequest/resolved` | 完整 |
| MCP structured elicitation | `mcpServer/elicitation/request` + accept/decline/cancel | 完整 |
| 命令/文件审批 | `item/commandExecution/requestApproval`、`item/fileChange/requestApproval` | 完整 |
| 权限配置申请 | `item/permissions/requestApproval` | 完整 |
| 文本流式进度 | `item/agentMessage/delta` | 完整 |
| 工具生命周期 | `item/started` → delta → `item/completed` | 完整 |
| 工具权威结果 | command/file item status：completed/failed/declined | 完整 |
| Token 使用 | `thread/tokenUsage/updated` | 完整 |
| Plan 进度 | `turn/plan/updated`，step 为 pending/inProgress/completed | 完整 |
| Diff 进度 | `turn/diff/updated` | 完整 |
| Review 状态 | `enteredReviewMode` / `exitedReviewMode` | 完整 |
| 睡眠/定时等待 | `sleep` item | 完整 |
| 子代理 | `collabToolCall`、thread parent/ancestor、subagent item | 高 |
| 模型安全缓冲/改路由 | `model/safetyBuffering/updated`、`model/rerouted` | 部分；不是通用排队事件 |
| 进程硬崩溃 | socket EOF/heartbeat | 只能标 disconnected，无法获得权威原因 |

相关事件契约见 [App Server turn/item events](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md#turn-events)、[approval requests](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md#approval-requests)、[request_user_input](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md#request_user_input) 和 [MCP elicitations](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md#mcp-server-elicitations)。

## 两种产品模式兼容矩阵

| 桌宠能力 | 被动模式：Hooks + deep link | 托管模式：App Server + Hooks |
|---|---|---|
| Running | 高 | 完整 |
| Needs input：审批 | 高 | 完整 |
| Needs input：普通问题 | 无 | 完整 |
| Ready / turn 完成 | 近似 Stop | 完整 turn status；“未读”由桌宠自己维护 |
| Blocked / failed | 局部推断 | 完整 turn/tool error |
| Review | 启发式 | 完整 entered/exited Review |
| 文本流 | 无 | 完整 |
| Plan / diff / token | 无 | 完整 |
| 多线程状态 | 仅收到 Hook 的 session | 完整覆盖同一 app-server 内已订阅/加载线程 |
| 已有本地历史 | 可从文件侧推断，不推荐 | `thread/list/read` 正式读取 |
| 其他进程的实时会话 | 无 | 无，除非连接到该进程公开的同一端点 |
| ChatGPT Web/云会话 | 无 | 无 |
| 官方未读、活动托盘、当前选中聊天 | 无 | 无；可在自己的客户端内重建 |
| Computer Use PiP 附着 | 无 | 无公开第三方接口 |

## 为什么不能直接监听 ChatGPT Desktop

`thread/list/read` 读取的是 app-server 所在 `CODEX_HOME` 的持久线程。`thread/resume` 会把线程加载进当前 server，并用于后续 turn；它不是跨进程的只读 live subscribe。

官方协议允许客户端连接一个已知的 stdio/WebSocket/Unix-socket app-server，但没有“发现 ChatGPT Desktop 私有 server”“附着其现有 transport”或“订阅所有本机 Codex 进程”的 API。本机当前也没有默认 control socket。因此：

- 可以读取同一数据域的历史快照；
- 可以托管并实时观察自己恢复后的后续运行；
- 不能透明窃听另一进程正在进行的 turn。

这是协议边界推断，不排除未来 Desktop 主动开放 control socket。

## 推荐架构

### 1. Passive 模式

默认兼容现有 CLI/ChatGPT Desktop：Hooks → 本地 Unix socket → 桌宠状态机；deep link 用于点击打开 thread。功能有限，但不改变用户工作流。

### 2. Managed 模式

桌宠守护进程启动 `codex app-server --listen stdio://`，持有 stdin/stdout；完成 initialize；用 thread/start/resume 管理会话；将 turn/item/server requests 归一化成宠物状态。此模式提供完整体验。

### 3. 不建议

- 不轮询 SQLite/JSONL 模拟实时 API：schema、锁、隐私与跨版本风险高。
- 不使用 WebSocket 作为默认 transport：官方仍标 experimental/unsupported。
- 不用 Responses API 复制 Codex 会话：ID、身份、账单和存储域完全不同。
- 不把 MCP 当事件总线：它适合托管调用，不提供跨进程全局广播。

最终产品应明确标注模式：`Passive compatibility` 与 `Managed full telemetry`。不要把托管能力宣传成对任意 ChatGPT Desktop 会话的透明兼容。
