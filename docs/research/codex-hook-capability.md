# Codex Hooks 能力上限与桌宠兼容矩阵

> Status: Research snapshot
> Owns: 2026-07-19 Hooks capability evidence; not current product contract
> Update when: Preserve this snapshot; add a dated re-verification section or a new snapshot
> Last verified: 2026-07-19

快照日期：2026-07-19。

## 结论

Codex Hooks 足够驱动本地旁路桌宠的会话、工具、授权、压缩和子代理动画；不足以精确复刻官方 Pets 的跨聊天聚合、通用“等待用户”、未读 Ready、全局 Blocked、流式进度及 ChatGPT UI 联动。

推荐目标：做“CLI/本地任务状态镜像”，不要承诺“官方 Pets 全状态等价”。

## 证据边界

- 官方 `openai/codex` `main`：commit [`b8b61bc692517adcd18622df260f2ddd80635122`](https://github.com/openai/codex/tree/b8b61bc692517adcd18622df260f2ddd80635122)，定义 11 个 Hook 事件。
- 本机 ChatGPT 内嵌 CLI：`codex-cli 0.145.0-alpha.18`；ChatGPT app `26.715.31925` build `5551`。
- 本机 PATH CLI：`codex-cli 0.144.5`。两者不是同一 binary。
- 两个本机 binary 都报告 `hooks stable true`，binary strings 均包含 11 个事件名。存在不等于每个前端都完成接线。
- 隔离 `CODEX_HOME` 实测确认 `codex exec` 在模型联网前触发 `SessionStart`。后续请求因当前香港出口返回 HTTP 403，其他事件未逐项实测。

隔离实测的 `SessionStart` stdin 字段：`session_id`、`transcript_path`、`cwd`、`hook_event_name`、`model`、`permission_mode`、`source`；其中 `source=startup`。

## 配置与执行上限

默认 Hook 文件为 `$CODEX_HOME/hooks.json`。事件键使用 PascalCase，并使用 matcher-group 结构：

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          { "type": "command", "command": "/absolute/path/to/pet-hook" }
        ]
      }
    ]
  }
}
```

隔离实测也确认 `[hooks] config_file = "./hooks.json"` 可指定其他文件。

源码协议虽然枚举 `command`、`prompt`、`agent` handler 和 sync/async execution mode，但当前 engine 只执行 command handler：prompt/agent 会被跳过；普通 async hook 也会被跳过。SessionEnd 即使配置 async，仍同步执行。证据：[discovery.rs:463-568](https://github.com/openai/codex/blob/b8b61bc692517adcd18622df260f2ddd80635122/codex-rs/hooks/src/engine/discovery.rs#L463-L568)。

Command hook 通过 stdin 接收 JSON；Codex 捕获 stdout/stderr、等待退出、执行 timeout，超时会终止子进程。证据：[command_runner.rs:49-136](https://github.com/openai/codex/blob/b8b61bc692517adcd18622df260f2ddd80635122/codex-rs/hooks/src/engine/command_runner.rs#L49-L136)。因此桌宠 hook 必须快速发本地 IPC 后退出，不能直接做渲染或网络请求。

## 11 个 Hook 事件

事件全集见 [hooks/src/lib.rs:18-48](https://github.com/openai/codex/blob/b8b61bc692517adcd18622df260f2ddd80635122/codex-rs/hooks/src/lib.rs#L18-L48)。

| 事件 | 可观察数据 | 可控制能力 | 桌宠用途 | 可靠性限制 |
|---|---|---|---|---|
| `SessionStart` | session、cwd、model、permission mode、startup/resume/clear/compact | 停止启动；注入 context | 唤醒、绑定会话 | 本机 `codex exec` 已实测；GUI 未实测 |
| `SessionEnd` | session、cwd、transcript；当前 reason 固定 `other` | 无阻断结果 | 清理会话、隐藏宠物 | 默认 timeout 1 秒，最大 3 秒；不能表达退出原因 |
| `UserPromptSubmit` | turn、prompt、model、subagent 信息 | 阻断提交；注入 context | 进入 running | 只覆盖 prompt 提交，不覆盖所有后续用户交互 |
| `PreToolUse` | tool、tool input、tool_use_id、turn、subagent | 阻断；注入 context；改写 tool input | 显示具体工作类型 | 改写需合法 allow decision；高敏 payload |
| `PermissionRequest` | 待授权 tool 与 input | allow、deny，或不表态继续正常 UI | 显示“等待授权” | 只覆盖权限审批，不覆盖普通提问/elicitation |
| `PostToolUse` | 原 input、tool response、tool_use_id | 阻断后续处理；注入 feedback/context；不能回滚 | 完成/错误动画、工具计时 | tool response 不等于整个 turn 成败 |
| `PreCompact` | trigger、turn、model、subagent | 停止当前流程 | 压缩预告 | 低频内部生命周期 |
| `PostCompact` | trigger、turn、model、subagent | 可停止后续流程；不能撤销压缩 | 压缩完成 | 不代表任务完成 |
| `SubagentStart` | agent_id、agent_type、turn | 注入 context；当前不能阻止启动 | 分身/并行指示 | matcher 依 agent type；不是完整子代理进度 |
| `SubagentStop` | agent_id/type、last message、turn | 可阻止停止并提供 continuation | 分身完成 | 若 hook 要求继续，则不是最终完成 |
| `Stop` | turn、last assistant message、stop_hook_active | 可阻止停止并提供 continuation | turn 完成信号 | 被阻止时会再次运行；必须去重，不能盲当最终 ready |

关键源码：

- `PreToolUse` 支持 block、additional context、`updated_input`：[pre_tool_use.rs:22-44](https://github.com/openai/codex/blob/b8b61bc692517adcd18622df260f2ddd80635122/codex-rs/hooks/src/events/pre_tool_use.rs#L22-L44)。
- `PermissionRequest` 在正常审批 UI 前运行，只返回 allow/deny/无决定，不能改写 input：[permission_request.rs:1-15](https://github.com/openai/codex/blob/b8b61bc692517adcd18622df260f2ddd80635122/codex-rs/hooks/src/events/permission_request.rs#L1-L15)。
- `Stop` 可 block 并生成 continuation：[stop.rs:62-79](https://github.com/openai/codex/blob/b8b61bc692517adcd18622df260f2ddd80635122/codex-rs/hooks/src/events/stop.rs#L62-L79)。
- `SessionEnd` timeout 硬限制：[session_end.rs:20-24](https://github.com/openai/codex/blob/b8b61bc692517adcd18622df260f2ddd80635122/codex-rs/hooks/src/events/session_end.rs#L20-L24)。

## Legacy `notify`

`notify` 不是事件总线。它只在 `AfterAgent` 生成一个 `agent-turn-complete` JSON，作为最后一个 argv 参数传给外部命令；stdin/stdout/stderr 均为空，spawn 后不等待。不能阻断、不能改写，也收不到 tool、permission、compact、subagent 等事件。证据：[legacy_notify.rs:12-67](https://github.com/openai/codex/blob/b8b61bc692517adcd18622df260f2ddd80635122/codex-rs/hooks/src/legacy_notify.rs#L12-L67)。

```json
{
  "type": "agent-turn-complete",
  "thread-id": "...",
  "turn-id": "...",
  "cwd": "...",
  "client": "...",
  "input-messages": ["..."],
  "last-assistant-message": "..."
}
```

只把 `notify` 当完成兜底。它不能支持实时桌宠。

## 官方 Pets 状态兼容

| 官方可见能力 | Hook 兼容度 | 可用信号 | 缺口 |
|---|---|---|---|
| Running | 高 | UserPromptSubmit → running；Pre/PostToolUse 刷新活动 | 无模型流式开始、重试、token 进度 |
| Needs input：权限 | 高 | PermissionRequest | GUI 是否接线仍需逐版本回归测试 |
| Needs input：普通问题 | 无 | 无对应 hook | `request_user_input`、elicitation、用户回答等待没有通用事件 |
| Ready | 部分 | passive Stop；legacy notify 完成兜底 | 无“未读”、前台/后台、用户是否看过结果 |
| Blocked | 部分 | deny、tool response、hook failure 可推断 | 无统一 turn result；模型/API/网络/进程崩溃未必发 hook |
| Review | 低 | 可按 tool 名或 Stop 后启发式推断 | 没有 review 状态事件 |
| Waiting 动画 | 低 | PermissionRequest 可映射一部分 | 无通用等待事件、无排队/模型等待信息 |
| 子代理活动 | 高 | SubagentStart/SubagentStop | 无子代理逐 token/逐阶段进度 |
| 压缩动画 | 高 | PreCompact/PostCompact | 仅本地生命周期，不代表业务进度 |
| 多会话分别显示 | 高（CLI） | session_id、turn_id | 无官方跨聊天优先级、选中聊天、未读状态 |
| 工具类别动画 | 高 | tool_name、tool_use_id | 工具 alias/版本变化需兼容 |
| 工具耗时 | 高 | 关联同一 tool_use_id 的 Pre/Post 时间 | Codex payload不直接给业务进度百分比 |

## 明确无法兼容

仅靠公开 Hook/notify，以下不能精确实现：

1. Token、reasoning 或 assistant 文本流式进度。
2. 模型请求排队、网络重试、限流、服务端等待状态。
3. 任意 `request_user_input`/MCP elicitation/普通追问的统一“等待用户”信号。
4. 整个 turn 的权威 success/failed/cancelled 结果。
5. 进程硬崩溃、强杀、断电后的最终 Hook；只能靠 heartbeat/watchdog 推断。
6. 官方 Pets 的未读、活动托盘、跨聊天优先级和当前选中聊天。
7. ChatGPT web/cloud 任务、其他设备任务或未经过本地 Codex core 的活动。
8. Computer Use 画中画附着、官方宠物位置/profile 同步、Wake/Tuck 状态。
9. 回滚已执行工具、撤销 compaction、任意改写模型已生成输出。
10. 稳定打开对应 ChatGPT GUI 线程；session id 到 deep link 的一致性仍需版本测试。

## 前端覆盖边界

- CLI/TUI：完整 hooks 的主目标表面；本机 binary 声明 stable。
- `codex exec`：隔离实测已确认 SessionStart；后续事件因区域 403 未验证。
- app-server：公开 v2 协议包含 HookStarted/HookCompleted notification，但用户 hooks 是否执行仍由 server 配置和客户端路径决定。协议证据：[app-server hook.rs:18-22, 139-155](https://github.com/openai/codex/blob/b8b61bc692517adcd18622df260f2ddd80635122/codex-rs/app-server-protocol/src/protocol/v2/hook.rs#L18-L22)。
- ChatGPT 桌面：内嵌 binary 包含相同事件和 stable feature，但本次没有证明 GUI 会话逐事件触发。必须把它列为“待兼容测试”，不能从 binary strings 推断完成接线。

## 推荐旁路适配器

1. Hook command 只读取最小字段，向 Unix domain socket 发包，立即退出。
2. 默认注册：SessionStart、SessionEnd、UserPromptSubmit、PreToolUse、PostToolUse、PermissionRequest、SubagentStart、SubagentStop、Stop。
3. Pre/PostCompact 可选；只用于动画，不参与主要状态判断。
4. 状态键：`(session_id, turn_id)`；工具键：`tool_use_id`；子代理键：`agent_id`。
5. passive Hook 永远 exit 0、stdout 为空。不要意外使用 block/context/rewrite 能力。
6. `Stop` 仅在本 hook 不阻止时标 ready；`notify` 作为丢事件兜底。
7. 维护进程 heartbeat；超时映射 `unknown/disconnected`，不要误报 failed。
8. 每次 Codex/ChatGPT 更新运行事件回归测试，分别验证 CLI、exec、GUI。

## 隐私与安全

Hook payload 可含 cwd、prompt、tool input/output、assistant message、transcript path。桌宠默认只应转发：session/turn/event/tool category/timestamp/result class。禁止转发 prompt、源码、命令全文和模型文本。

Hook command 在 Codex sandbox 之外以用户权限运行，并受 hook trust 管理。使用绝对可执行路径；不拼 shell 字符串；本地 socket 设用户权限；限制消息大小；所有观察型 hook fail-open。桌宠故障不能拖慢或阻断 Codex。
