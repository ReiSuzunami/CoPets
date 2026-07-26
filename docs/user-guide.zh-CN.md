# 用户指南

> Status: Normative
> Owns: Simplified Chinese rendering of the end-user guide; user-guide.md remains canonical
> Update when: The English user guide changes or a translated end-user workflow needs correction
> Last verified: 2026-07-27

简体中文 · [English](user-guide.md)

CoPets 是一个独立的 macOS 伴侣应用，用于配合已运行的 Codex App。安装 <code>.app</code> 后，选择和管理宠物不需要 Agent 或手动修改文件。

## 系统要求与安装

公开的 <code>v0.2.1</code> GitHub Release 要求：

- macOS 11 或更高版本。
- 用于观察任务状态的 Codex App。CoPets 可以在 Codex 之前或之后启动。

从 [<code>v0.2.1</code> release](https://github.com/ReiSuzunami/CoPets/releases/tag/v0.2.1) 下载通用 DMG 及其校验和文件。校验下载内容：

~~~bash
shasum -a 256 -c CoPets-v0.2.1-macos-universal.dmg.sha256
~~~

打开 DMG，双击 **Install CoPets**，再确认 **Install** 或 **Upgrade**。安装器会验证嵌入的应用载荷与暂存副本，退出正在运行且可识别的 CoPets 实例，并通过同目录备份替换已有版本；若最终放置失败，可以恢复该备份。它会拒绝符号链接、其他用户拥有但名为 <code>CoPets.app</code> 的项目、已变更的 bundle identity、意外的可执行文件布局和无效签名。

安装成功后，可以选择弹出并保留 DMG，或弹出并将已验证的 DMG 移到废纸篓。临时弹出辅助程序会自行删除。若 <code>/Applications</code> 不可写，安装器会使用当前用户的 <code>~/Applications</code> 目录。

此 GitHub Release 使用本地开发签名，尚未经过 Apple 公证。Gatekeeper 可能要求使用 Finder 的标准 **Open** 确认。它是正式 GitHub Release，但不是经过 Apple 公证的分发版本。

### 从源码构建

源码构建还需要 Node.js 20 或更高版本，以及 Rust 工具链：

~~~bash
npm install
npm run codesign:setup
npm run build:macos:signed -- --bundles app
~~~

本地签名设置会在当前用户的登录钥匙串中创建仅用于开发的签名身份。构建出的应用位于：

~~~text
src-tauri/target/release/bundle/macos/CoPets.app
~~~

从 Finder 打开该应用。宠物以菜单栏配件形式运行：不会出现在 Dock 中、始终位于普通窗口上方，并通过圆形菜单栏图标提供 **Open Settings**、**Show/Hide** 和 **Quit**。Developer ID 签名、公证和更广泛分发仍是[路线图](roadmap.md)中的 M5 工作。

## 首次运行

首次运行、或尚未安装有效宠物时，CoPets 会打开紧凑的设置面板。最快路径：

1. 打开 Codex App 并选中一个本地任务。
2. 从设置面板导入宠物。
3. 关闭设置。宠物现在会跟随 Codex 中选中的任务。

首次运行说明可以关闭。只要设置处于打开状态而 CoPets 无法观察 Codex，断开连接的引导仍会显示。Codex 更新可能改变 CoPets 所观察的私有本地接口；遇到这种情况，请更新 CoPets 或查看项目最新的兼容性证据，而不是授予辅助功能权限。

## 打开设置

将指针移到宠物窗口上。圆形状态控件会出现在宠物左脚旁，无需让 CoPets 成为前台应用。点击它，再点击小型状态菜单中的设置图标。

也可以点击 macOS 菜单栏中的圆形 CoPets 图标，选择 **Open Settings…**。屏幕中央会出现独立窗口。即使宠物被隐藏，它仍可使用，并且不会显示、移动或调整宠物窗口大小。

任何时候只会显示一个设置界面。打开菜单栏窗口会关闭宠物的内嵌面板；打开内嵌设置会关闭独立窗口。

如果保存的位置不再适合当前显示器，也可以在设置中重置窗口大小和位置。

## 导入和管理宠物

CoPets 支持以下与 Pet Creator 兼容的输入：

| 输入 | 选择方式 | 结果 |
| --- | --- | --- |
| 包文件夹 | 点击 **Folder**，选择包含 <code>pet.json</code> 的文件夹 | 验证并复制完整文件夹 |
| 清单文件 | 点击 **ZIP / pet.json**，选择 <code>pet.json</code> | 将其所在文件夹视为包 |
| ZIP 存档 | 点击 **ZIP / pet.json**，选择一个 <code>.zip</code> | 提取并验证根包或仅包一层文件夹的存档 |

不能只导入 spritesheet 图像，因为它缺少解释 atlas 所需的 manifest 身份与版本。支持的字段、几何、限制和存档安全规则见[宠物包规范](protocol/pet-package.md)。

选择来源后，CoPets 会验证它，并在宠物窗口中临时预览候选项。预览面板会显示名称、ID、sprite 版本、atlas 尺寸和原生缩放比例。选择：

- **Install**：添加一个新包。
- **Replace**：替换具有相同 ID 的已安装包；会先出现原生确认。
- **Cancel**：恢复当前选中的宠物，不改变文件。

来源文件绝不会被移动或编辑。安装使用已验证的暂存副本。若验证或激活失败，已安装包保持不变，设置会显示原因。

使用 Pet selector 切换包。**Rescan** 会检测在应用外添加的有效包。**Show in Finder** 会打开 <code>${CODEX_HOME:-~/.codex}/pets</code>。手动放置但无效的文件夹不会出现在选择列表中，而会列在 **Needs attention** 下。

**Remove** 会在原生确认后永久删除 pets 目录中选中的包。若仍有其他宠物，CoPets 会选择当前有效宠物或第一个剩余宠物；若没有剩余宠物，它会清空渲染器并恢复导入引导。移除不会删除最初导入的文件夹或 ZIP。

## 读取任务状态

状态控件和宠物动画反映 Codex 中当前选中的一个任务。后台任务保留各自独立的状态，不能接管可见宠物。

常见状态包括 ready、working、review、needs input 或 approval、complete、failed、interrupted 和 waiting for Codex。活动状态会循环播放工作动画。终止动作播放一次、短暂停留，随后可见展示回到 idle 并清除气泡。

窗口顶部最多显示两条紧凑消息：尽力取得的当前问题和最新的有界 Codex 进度。Markdown 会安全渲染。长内容保留可见前缀并以省略号结束；CoPets 不是对话记录查看器。

## 使用控件

Approval 和 stop 需要当前已验证的原生 owner，且只针对选中的任务。除非你明确启用实验性 CoPets bridge，follow-up 也使用相同的默认路径：

- Approval 和 question 卡片允许你显式允许、拒绝或回答准确的待处理请求。
- reply 控件只在 live turn 中出现。它引导该 turn，不会创建新任务或唤醒已完成任务。
- 已完成的任务会保留一个 **Continue** 箭头。标准模式下，只有 CoPets 拥有当前已验证的 owner 时，它才会开始下一轮。如果该 owner 正在重连，显式 retry 会先只刷新该任务的原生 follow 注册。若仍不可用，请在官方 Codex App 中打开并聚焦准确的任务，等待其 owner 恢复，然后重试。
- **Experimental bridge：** 在 Settings 中打开紧凑的 **Experimental bridge** disclosure。要开始新的 bridge session，选择自动端口或一个未使用的自定义本地端口，先退出 Codex，再点击 **Launch Codex**。要使用已通过 loopback CDP 端口启动的 Codex App，改为点击 **Connect existing**；无需重启 Codex。自动 Connect 只接受恰好一个同用户的官方 Codex 候选进程；如果运行多个，请选择 **Custom port**，输入启动 Codex 时使用的端口，然后点击 **Connect**。

  **Launch Codex** 会请 macOS 通过 Launch Services 打开官方 App，然后由 CoPets 独立发现并验证准确的同用户 App 进程和 loopback listener；系统启动助手绝不会被当作 Codex。任何 macOS 权限弹窗显示的名称由系统决定，CoPets 不承诺固定显示结果。

  如果普通 Codex App 已经打开，而你希望 CoPets 使它具备 bridge 能力，请使用 **Restart Codex with bridge**。它需要确认，因为会关闭该 App；活动工作可能中断，未保存的 App UI 状态可能丢失。CoPets 只接受一个同用户的普通官方 Codex App，会重新检查它、请求它正常关闭，并在短暂等待后启动 bridge 替代实例。它绝不会强制关闭 Codex。若打开了多个 App，它不会替你选择一个；请自行关闭多余实例。此操作只会在标准 IPC 活跃时显示；降级的已跟踪 bridge 会改为提供 Retry verification。它只出现在 Settings 中，绝不会从 Continue/Steer 错误处运行。

  当 bridge 显示 ready 时，即使 IPC owner 已过期，Continue 和活动 Steer 也可以使用选中任务现有的窗口内 session。这使用私有、版本敏感的本机调试接口，不是官方 OpenAI 接口。CoPets 只接受具有 IPv4 loopback listener、匹配 CDP 端口、Codex renderer 和已验证 Pets handler 的同用户官方 App 进程；不会接受 host、DevTools URL、普通浏览器或任意端口。只能在受信任的本机 macOS 用户会话中使用。

  若验证显示 unavailable，请保持该 Codex App 打开并选择 **Retry verification**。Retry 只重新检查相同的已跟踪进程，绝不会发送 follow-up 或启动另一个 App。若 listener 已关闭或进程发生变化，请再次 connect、restart 或 launch。Launch、Restart、Connect 和 Retry 都会在有界本地检查后以 Ready 或 Unavailable 结束；不会无限停留在 Launching。
- Stop 只针对所选任务当前 live owner。

若控件无法发送，请切回 Codex 中预期的任务，并确认 turn 仍处于 live 或 ready。CoPets 故意不会回退到后台任务，也不会激活 Codex App 来制造可用性。

## 移动、调整大小与恢复

- 拖动宠物身体以移动窗口。只有发生真实的指针移动后才开始跑动，并会跟随左右方向，在停顿期间持续直到释放。
- 将指针悬停在宠物右脚附近，拖动圆形斜向握柄以按比例调整大小。
- 位置和大小会在下次启动时恢复，并限制在已连接的显示器内。
- 使用设置中的 **Reset size & position** 恢复默认大小和居中位置。

CoPets 会跟随 macOS 浅色/深色外观。启用 Reduce Motion 时会保持稳定帧，而不是循环动画。

## 隐私模型

CoPets 观察本机同一用户的 Codex 信号。在 IPC 初始化前，它会验证 socket 路径 owner 和已连接 peer。session、app-log 和 thread-index 读取会拒绝符号链接、非常规文件以及其他用户拥有的文件。基于路径的 IPC 和 thread-index 访问也会拒绝可写或由其他用户拥有的祖先目录。它不代理模型流量、不注入 Codex WebView，也不使用辅助功能来传递消息。

它只读取活动 session logs 的最新有界部分，并将所选任务预览保留在内存中。它不会持久化对话预览，也不会显示完整 prompts、完整 answers、hidden reasoning、tool arguments 或 command output。原始 task/request IDs 和私有控制 payloads 保留在 native memory 中。导入来源路径只在当前 picker/preview 操作期间存在，不会保存。

完整信任边界见[运行时架构](architecture/runtime.md)。

## 故障排除

| 症状 | 检查方式 |
| --- | --- |
| **Waiting for Codex** | 打开 Codex App，选中一个本地任务，并让 CoPets 保持运行。不需要辅助功能权限。 |
| Codex 更新后状态持续断开 | 退出并重新打开两个应用，然后查看最新的带日期兼容性研究。私有本地接口可能变更。 |
| 找不到宠物 | 打开设置，导入包文件夹、其中的 <code>pet.json</code> 或 ZIP。单独的 PNG/WebP 不足以导入。 |
| 包显示在 **Needs attention** 下 | 打开 pets 文件夹，修复或移除具名的手动包。显示的诊断会指出第一个失败的检查。 |
| 导入失败 | 确认只有一个 package manifest、没有超过一层的嵌套包装文件夹，且官方或 CoPets atlas 几何有效。 |
| 宠物模糊 | 确认包中存在已验证 <code>sidecarSpritesheetPath</code>，指向原生 2x–4x atlas；仅修改 manifest 无法增加分辨率。 |
| Reply/approval/stop 被隐藏 | 选中任务没有 live compatible owner，或当前操作无效。在已验证的 experimental bridge 中，只有 Ready/Steer 能绕过新的 IPC owner 证明；approval 和 stop 不行。CoPets 不会把开始新 turn 当成回退。 |
| 窗口位置不对或太小 | 打开设置，选择 **Reset size & position**。 |
| 错误遮挡控件 | 错误会在五秒后自动清除，也可以立即关闭。在设置中，它们会留在面板内。 |

开发诊断请使用[更新与发布规则](maintenance/updating.md)中记录的独立 probes。绝不要将原始对话日志或私有 payload 附到公开 issue。

## 更新和移除

要更新正式版本，请下载较新的 DMG 并再次运行 **Install CoPets**。安装器会退出已识别的运行中副本、验证替换版本、在目标位置旁暂存它，若最终放置失败则恢复前一版本。任一应用更新后都要重新检查 Codex 兼容性。

要移除 CoPets，请重新打开其 release DMG，双击 **Install CoPets**，然后选择 **Uninstall Existing…**。确认后，已验证的应用会移到废纸篓。卸载程序不会删除 <code>${CODEX_HOME:-~/.codex}/pets</code>、Codex sessions、logs、databases、sockets 或导入来源文件夹，因为它们属于共享内容或外部所有者。只有在你确实想删除这些已安装副本时，才通过 CoPets 设置移除单个包。
