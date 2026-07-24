# Codex App WebView / DevTools / CDP Hook 研究

> Status: Research snapshot
> Owns: 2026-07-19 DevTools/CDP evidence; not current product contract
> Update when: Preserve this snapshot; add a dated re-verification section or a new snapshot
> Last verified: 2026-07-19

快照：2026-07-19；目标：评估已运行 ChatGPT/Codex App 的 Electron/Chromium WebView 是否可被外部 sidecar 读取、监听或改写。

## 结论

- **无重启附着：当前生产实例不可行。** 外部 CDP 客户端需要目标主动暴露 DevTools endpoint，或宿主进程内调用 Electron `webContents` API。当前 PID 96117 无 remote-debugging 参数和 TCP listener；其启动日志明确为 `buildFlavor=prod allowDevtools=false allowInspectElement=false`。
- **启动时 hook：已实测 CDP server 可打开。** 隔离临时 profile、临时 `CODEX_HOME` 下，以 `BUILD_FLAVOR=dev --remote-debugging-port=<loopback-port>` 启动同一 App binary，得到 Chrome 150、CDP 1.3 和 `DevTools listening`。这证明方案真实可用，不只是官方文档推断。
- **当前实验尚未拿到 renderer target。** 首次 `/json/list` 返回 0 target；原因可能是查询过早或第二实例锁。现在只能确认 browser-level CDP endpoint，不能宣称已 hook 主 UI DOM/store。
- **公开主题项目的真实启动方式已核对。** `Fei-Away/Codex-Dream-Skin` 的 macOS `start-dream-skin-macos.sh` 默认端口 `9341`，调用 `launch_codex_with_cdp`，并再次执行 `open -na "$CODEX_BUNDLE" --args --remote-debugging-address=127.0.0.1 --remote-debugging-port="$PORT"`；随后等待 `/json/version`/CDP endpoint，再由 Node injector 验证 renderer、注入主题并提交 active state。项目 README 明确这是非官方、外部 loopback CDP 注入，不改 `.app`/`app.asar`。这证明公开实现采用“wrapper 启动 + CDP”，不证明当前生产实例已有 renderer target。
- **Electron 内部 hook：若能执行 App 自己代码，可读/监听/改写能力很强；外部 sidecar 无此权限。** `webContents` 可打开 DevTools、获取 CDP target id；CDP 可读 DOM/JS/runtime/network/performance，并可执行脚本、改 DOM/页面状态。它不等于 Codex 业务 API，React/store、IPC、模型状态字段均属私有实现，版本更新会破坏。
- **安全边界高风险。** CDP endpoint 一旦暴露，等同授予调试客户端页面脚本执行与网络/DOM 观察能力；不能监听或改写已通过 CDP 不可见的 native/main-process 状态。默认应只读、绑定 loopback、随机 token/端口，版本不匹配即降级。
- **与 IPC+JSONL：DevTools 适合 UI 观测/临时调试；IPC+JSONL 更适合稳定状态镜像。** 本机已有 `~/.codex/ipc/ipc.sock` 广播和 session JSONL 增量事件；两者无需重启且不暴露页面脚本执行面。

## 本机已验证

1. 运行进程：`/Applications/ChatGPT.app/Contents/MacOS/ChatGPT`（PID 96117）；命令行未见 `--remote-debugging-port` 或 `--remote-debugging-pipe`。
2. `lsof -nP -a -p 96117 -iTCP -sTCP:LISTEN` 无输出；当前没有可直接连接的 CDP TCP listener。
3. 当前 App 日志：`Launching app ... allowDevtools=false allowInspectElement=false buildFlavor=prod packaged=true`。
4. Bundle gate：`allowDevtools(e){ return isInternal(e) }`；internal flavor 仅 Dev、Agent、Nightly、InternalAlpha。Prod/PublicBeta 不开放。
5. 主窗口 `webPreferences.devTools` 直接取 `allowDevtools`；View 菜单中的 Electron `toggleDevTools` 同样受该 gate 控制。
6. App 另有 `CmdOrCtrl+Alt+Y` 的 Query Devtools 和 sandbox guest `openDevTools` 路径；它们不是外部 CDP endpoint，也不能让 sidecar 附着主 renderer。
7. `/tmp/codex-browser-use/*.sock` 是 Browser/Computer Use 的 native-pipe JSON-RPC/CDP 转发层，不是 Codex 主 UI 的通用 DevTools socket。
8. 隔离启动探针结果：`Chrome/150.0.7871.124`、`Protocol-Version=1.3`、loopback CDP server 成功响应；当次 target 数为 0。
9. 现有研究已实测 App 私有 IPC router 与 session JSONL：见 `codex-app-parasitic-attachment.md`；该路径支持运行中附着。

## 公开主题插件交叉核对（非官方实现）

以公开仓库 [Fei-Away/Codex-Dream-Skin](https://github.com/Fei-Away/Codex-Dream-Skin) 为样本，核对到的启动链：

1. 安装脚本默认端口 `9341`，生成 Desktop launcher；启动前检查签名，并要求已运行实例关闭，除非显式 `--restart-existing`。
2. 启动脚本选择端口后执行 `launch_codex_with_cdp "$PORT"`，再执行：
   ```sh
   /usr/bin/open -na "$CODEX_BUNDLE" --args \
     --remote-debugging-address=127.0.0.1 \
     --remote-debugging-port="$PORT"
   ```
3. 脚本等待并验证 loopback CDP endpoint；Node injector 再以 `--verify` 检查 renderer、精确主题和 payload revision，失败则尝试一次 `--once` 注入，仍失败即停止 injector、标记 stale。该流程目标是“启动时开放 endpoint”，不是附着已按 Prod 启动且未开放 endpoint 的实例。
4. 公开站点/README 的“真实注入截图”和“renderer targets”属于项目自身声明；本报告未独立复现实例中的 target URL、页面选择器或注入成功率，不能把它们当作本机验证证据。项目亦明确非 OpenAI 官方产品，不能作为授权或安全背书。

## 下一步可复现实验（只读、隔离、可回收）

目标：先证明 renderer target 出现，再验证最小只读 DOM；不推断 React/store 为权威状态。

1. 退出当前 Codex/ChatGPT App；复制当前用户数据到临时目录，设置临时 `CODEX_HOME`，选随机高位 loopback 端口（记录端口、bundle 路径、版本）。不要复用生产 profile，避免第二实例锁。
2. 以 wrapper 方式启动：`/usr/bin/open -na <bundle> --args --remote-debugging-address=127.0.0.1 --remote-debugging-port=<port>`；循环请求 `http://127.0.0.1:<port>/json/version` 和 `/json/list`，每 250 ms、最长 60 s，保存原始 JSONL 响应。
3. 判定分层：`/json/version` 成功 = browser endpoint；`/json/list` 出现 `type="page"` 且含 `webSocketDebuggerUrl` = renderer target；仅在第二层成立时，CDP attach 后调用 `Runtime.evaluate('document.readyState')` 与 `DOM.getDocument`，只记录 title/url/节点计数，不注入脚本。
4. 若持续 0 target，记录完整启动日志、PID/命令行、端口监听和 profile 路径，标记“endpoint-only / target 未证实”；排查启动过早、第二实例锁、页面尚未创建、bundle gate，不得以 browser endpoint 推断 UI 可读。
5. 实验结束关闭临时 App、确认端口释放并删除临时 profile；生产 App 保持原启动方式。只有拿到 renderer target 后，另行设计白名单 DOM/Runtime fixture，禁止默认 Network/Fetch 监听。

## 能力矩阵（条件性）

| 能力 | CDP/WebView 条件 | 只读/改写 | 结论 |
|---|---|---|---|
| 列页面、frame、DOM、可见文本 | CDP endpoint + target | 读 | 可行；页面渲染层可见内容，不保证业务全量 |
| 监听 console、network、runtime、performance | CDP domain attach | 读 | 可行；可能含 prompt、token、cookie、Authorization 等敏感数据 |
| 执行 JS、改 DOM/CSS、注入监听器 | Runtime/DOM/Page domain | 改写 | 可行但易破坏 UI；页面刷新/React 重绘会丢失 |
| 拦截/改写 fetch/XHR/WebSocket | Fetch/Network domain | 改写 | 技术上可行；可篡改请求/响应，可能导致会话损坏或越权 |
| 读取 React/store、IPC、main process | 需页面暴露对象或 preload bridge | 读 | 不可据 CDP 保证；私有实现、隔离上下文、bundle 更新均会阻断 |
| 代替用户点击/提交/审批 | DOM 可见控件或 Electron 内部 API | 改写 | 可模拟 UI；不等于获得 app-server 权限，默认禁用 |
| 读取模型/工具完整事件 | 页面确实渲染且事件未被聚合 | 读 | 不可靠；JSONL/app IPC 是更完整来源 |
| 当前已运行 Prod App 后附着 | 已预先开放 endpoint，或 App 内部提供 openDevTools IPC | 读/改 | 当前两者均不存在，**不可行** |
| 由 wrapper 启动后附着 | `BUILD_FLAVOR=dev` + loopback remote-debugging port | 读/改 | browser endpoint **已实测**；renderer target 待验证 |

## 官方一手资料

- Electron `webContents.openDevTools()`、`getOrCreateDevToolsTargetId()`：可在**宿主进程内**打开 DevTools、取得 CDP target；API 不提供外部进程“注入已运行 App”的入口。[Electron webContents](https://www.electronjs.org/docs/latest/api/web-contents)
- Electron command-line switch 文档：`remote-debugging-port` 启用 TCP CDP；V8 inspector 通过该端口连接。[Electron command-line switches](https://www.electronjs.org/docs/latest/api/command-line-switches)
- Chrome 官方远程调试文档：以 `--remote-debugging-port=PORT` 启动另一 Chrome 实例。[Chrome DevTools remote debugging](https://developer.chrome.com/docs/devtools/remote-debugging/local-server)
- Apple AXUIElement：可读取/设置可访问属性、执行动作、注册 AXObserver 通知，但需 Accessibility trust；AX 只能看到暴露的 UI 语义，不能替代 CDP 或读取 React/store。[AXUIElement.h](https://developer.apple.com/documentation/applicationservices/axuielement_h)

## 风险与边界

- **参数重启风险：** 需杀掉原 App，可能中断 turn；`--remote-debugging-port` 改变攻击面；端口冲突、签名/启动器参数丢失、自动更新后参数失效均需处理。
- **CDP 内容风险：** Network/Runtime 可触及 prompt、源码、cookie、鉴权头、工具输入输出；绝不能默认持久化。sidecar 仅白名单事件并 hash ID。
- **协议漂移：** CDP 基础协议相对稳定，但 target URL、Electron 版本、页面 bundle、CSP、contextIsolation、内部 IPC 名称不稳定；每次 App 更新做 fingerprint + fixture。
- **AX 补充：** Apple API 可观察前台窗口、焦点、标题等 UI 语义；权限关闭或 App 未暴露属性时返回 `kAXErrorAPIDisabled`/`kAXErrorCannotComplete`。不应把 AX 当业务事件总线。

## 推荐判定（收口）

生产旁路：继续使用 App IPC broadcast + JSONL 增量尾读；AX 仅补前台/焦点。

CDP 不进入 MVP 主链。它只能作为显式“重启后实验模式”：wrapper 启动 Codex 时开放随机 loopback port，拿到 renderer target 后只订阅白名单 Runtime/DOM 事件。不能用于已经按 Prod 启动的实例；不能默认暴露固定端口；不能把 DOM/store 推断成权威业务状态。

当前功能缺口已由 App activity log + SQLite thread index 补齐：无需 CDP 即可恢复当前 GUI view 对应的真实 thread。因此本阶段不再继续尝试重签、复制 App、绕过 single-instance lock 或注入主 renderer。
