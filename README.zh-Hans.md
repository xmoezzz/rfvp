# rfvp: FVP 引擎与 IDE 的非官方 Rust 跨平台实现

<p align="center">
  <a href="README.md">English</a> |
  <a href="README.ja.md">日本語</a> |
  <a href="README.zh-Hant.md">繁體中文</a> |
  <a href="README.zh-Hans.md">简体中文</a>
</p>

<img src="images/flake.png" width="20%">

### 状态
* 可游玩？
* 详情请参阅 [setsumei](setsumei/README.md)。
![IN-GAME](./images/in-game.png)
![legacy](./images/legacy.png)

### rfvp 调试 HUD
* 可通过 F2 热键启用（macOS 上为 Fn + F2）。

### 预构建二进制文件
支持平台的预构建二进制文件可从 [Pre-built Binaries](https://github.com/xmoezzz/rfvp/releases/tag/pre-release) 获取。

* 请注意，pre-release 版本是包含最新功能和修复的最新版本。
* 我通常不会频繁提升版本号。因此，如果你想获取最新功能和修复，请查看 pre-release 版本。

### 构建方式
* macOS Bundle: [setsumei/HOW-TO-BUILD.macos-bundle.md](setsumei/HOW-TO-BUILD.macos-bundle.md)
* iOS IPA: [setsumei/HOW-TO-BUILD.ios.md](setsumei/HOW-TO-BUILD.ios.md)
* Android APK: [setsumei/HOW-TO-BUILD.android-apk.md](setsumei/HOW-TO-BUILD.android-apk.md)
* Windows EXE: [setsumei/HOW-TO-BUILD.windows-msvc.md](setsumei/HOW-TO-BUILD.windows-msvc.md)
* Linux ELF: [setsumei/HOW-TO-BUILD.host.md](setsumei/HOW-TO-BUILD.host.md)

### 超越重新实现
* 由于本项目同时具备反编译器和编译器，我们也可以基于该引擎编写应用程序，例如一个简单的 Windows 95 风格绘图程序。
![win95-painter](./images/win95-painter.png)

### 安装

RFVP 在 [`setsumei/installation`](setsumei/installation) 下提供各平台的安装指南：

- [Windows](setsumei/installation/Windows.md)
- [Linux](setsumei/installation/Linux.md)
- [macOS](setsumei/installation/macOS.md)
- [iOS](setsumei/installation/iOS.md)
- [Android](setsumei/installation/Android.md)

### 文档

本项目的 Rust API 文档可在此处查看：

- [RFVP Rust API Docs](https://xmoezzz.github.io/rfvp/)

### 支持平台与打包类型
| 平台 | 支持的打包类型                                           | 启动器 | 独立可执行文件 | 架构                                  |
| ---- | ---------------------------------------------------------- | -----: | -------------: | ------------------------------------- |
| macOS | App Bundle (`.app`) 和 DMG (`.dmg`)                       |     是 |             否 | Universal                             |
| iOS | 未签名 IPA (`.ipa`, AltStore)                              |     是 |             否 | arm64                                 |
| Android | APK (`.apk`)                                            |     是 |             否 | arm64-v8a, x86_64                     |
| Windows | 独立 EXE                                               |     否 |             是 | x86_64, arm64                         |
| Linux | 独立程序                                                |     否 |             是 | x86_64, aarch64                       |
| WASM | Bundle                                                    |     是 |             否 | **任意架构**（通过 WASI）             |

* 由于本项目是 Rust 项目，因此理论上也可以构建到许多其他平台。

### 兼容性
本项目旨在兼容原始 FVP 引擎的所有版本。  
要确保 100% 兼容，需要对所有相关游戏进行测试。如果你觉得本项目有用，并希望帮助加速更多游戏的兼容性测试流程，请考虑赞助本项目。

* 另请参阅 [setsumei/COMPATIBILITY.md](setsumei/COMPATIBILITY.md) 获取详情。部分功能和行为可能与原始引擎不同。

### 免责声明
- 本项目是针对原始游戏引擎逻辑所做的独立逆向工程重新实现。所有源代码均基于对目标软件行为的研究和观察，从零开始编写。本仓库不包含原始开发者的任何源代码。
- 使用本引擎必须拥有原始游戏的合法副本。严禁分发、分享或提供任何原始游戏数据、素材，或随附游戏可执行文件的下载链接。
- 原始游戏公司和权利持有人保留其知识产权的所有权。出于善意，原始公司可以将本仓库中的代码用于任何目的，包括商业用途，且无需事先取得许可。

### 许可证
本项目采用 MPL-2.0 License 授权。详情请参阅 [LICENSE](LICENSE) 文件。
