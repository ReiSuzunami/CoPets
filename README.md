<p align="center">
  <img src="docs/assets/brand/copets-cloud.png" width="128" alt="CoPets 黄色云朵">
</p>

<h1 align="center">CoPets</h1>

<p align="center">
  <strong>让正在运行的 Codex 任务变成桌面宠物。</strong>
</p>

<p align="center">
  简体中文 · <a href="README.en.md">English</a>
</p>

CoPets 是独立、开源的 macOS Codex App 桌面伴侣：它把当前所选任务的本机状态显示为宠物动画和简短气泡。
它不代理模型流量、不修改 Codex 界面，也不是 OpenAI 官方产品。

## 安装

需要 macOS 11+ 和 Codex App。

1. 从 [`v0.2.1` Releases](https://github.com/ReiSuzunami/CoPets/releases/tag/v0.2.1) 下载 DMG 与 `.sha256` 文件。
2. 将两者放在同一目录并校验：

   ```bash
   shasum -a 256 -c CoPets-v0.2.1-macos-universal.dmg.sha256
   ```

3. 打开 DMG，双击 **Install CoPets**，按提示安装或升级。

> `v0.2.1` 是正式 GitHub Release，但仍使用开发签名且未经 Apple 公证。Gatekeeper 可能阻止首次打开；仅在校验值匹配后，从 **系统设置 → 隐私与安全性** 选择“仍要打开”。

## 使用

1. 打开 Codex App 并选中一个本地任务。
2. 打开 CoPets；没有有效宠物时会自动打开设置。
3. 导入 Pet Creator 兼容的包文件夹、`pet.json` 或 ZIP。应用不会自动安装宠物；源码仓库提供 [Sunflower 示例](examples/pets/sunflower)。
4. 关闭设置，宠物会跟随当前所选任务。

实验桥接只可在设置中显式启用，依赖版本敏感的本机私有接口。升级 Codex App 后，请先阅读[用户指南](docs/user-guide.zh-CN.md)并重新验证兼容性。

## 从源码构建

```bash
npm ci
npm run codesign:setup
npm run build:macos:signed -- --bundles app
```

构建结果：`src-tauri/target/release/bundle/macos/CoPets.app`。

更多安装、宠物管理与排障见[用户指南](docs/user-guide.zh-CN.md)。素材授权见[ASSET_LICENSES.md](ASSET_LICENSES.md)。

[MIT](LICENSE) © 2026 CoPets contributors.
