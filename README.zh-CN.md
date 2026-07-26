<p align="center">
  <img src="docs/assets/brand/copets-cloud.png" width="128" alt="CoPets 黄色云朵">
</p>

<h1 align="center">CoPets</h1>

<p align="center">
  <strong>让正在运行的 Codex 任务变成桌面宠物。</strong>
</p>

<p align="center">
  <a href="README.md">English</a> · 简体中文
</p>

CoPets 是一个独立、开源的 macOS Codex App 桌面伴侣。它读取 Codex 当前所选任务在本机的生命周期信号，
并将其变成宠物动画、简短的上下文气泡和与任务状态对应的操作。

CoPets 完全在本机运行，不代理模型流量、不修改 Codex 界面，也不需要辅助功能权限。它不是 OpenAI
官方产品。Codex 集成依赖未公开且没有版本保证的本机接口；Codex App 更新后，CoPets 可能也需要更新。

实验桥接仅能在设置中由用户显式启用，并且只使用已验证、由官方 Codex App 持有的本机回环端点。启用前请阅读
[用户指南](docs/user-guide.md)。

## 安装

系统要求：

- macOS 11 或更高版本
- Codex App

从 [GitHub Releases](https://github.com/ReiSuzunami/CoPets/releases) 下载通用版 DMG
及其校验文件。把两个文件放在同一目录，然后校验当前版本：

```bash
shasum -a 256 -c CoPets-v0.2.0-macos-universal.dmg.sha256
```

打开 DMG，双击 **Install CoPets**。同一个安装器也能安全升级 CoPets，或将已有安装移到废纸篓。
卸载应用不会删除 `${CODEX_HOME:-~/.codex}/pets` 下的宠物包。

从源码构建：

```bash
npm ci
npm run codesign:setup
npm run build:macos:signed -- --bundles app
```

构建结果位于 `src-tauri/target/release/bundle/macos/CoPets.app`。

## macOS 未公证提示

CoPets 发布版本有意不进行 Apple 公证。macOS 可能会在首次启动时阻止 **Install CoPets** 或 CoPets。

遇到阻止时：

1. 先尝试打开一次被阻止的应用。
2. 打开 **系统设置 → 隐私与安全性**。
3. 点击 **仍要打开**，完成身份验证，再确认打开。
4. 如果安装后 macOS 再次询问，请对 CoPets 本体重复一次。

仅在文件来自本仓库 Releases 页面且校验值匹配时绕过 Gatekeeper。项目源码和 CI 流程公开，
你也可以选择在本机从源码构建。

## 使用

1. 打开 Codex App，并选择一个本地任务。
2. 打开 CoPets。没有有效宠物时，设置窗口会自动出现。
3. 导入兼容 Pet Creator 的文件夹、`pet.json` 或 ZIP。项目不会内置或自动安装宠物；源码检出包含可导入的
   [示例宠物](examples/pets/)。
4. 关闭设置；宠物会开始跟随 Codex 当前所选任务。

将鼠标移到宠物上可打开状态菜单；也可以使用菜单栏中的圆形 CoPets 图标打开设置、显示或隐藏宠物，
以及退出应用。只有当 CoPets 能确认操作目标是当前所选的活动任务时，审批、回复、停止等控件才会出现。

宠物管理和故障排查请参阅[用户指南](docs/user-guide.md)。

[MIT](LICENSE) © 2026 CoPets contributors.

素材溯源和贡献要求见：[ASSET_LICENSES.md](ASSET_LICENSES.md)。
