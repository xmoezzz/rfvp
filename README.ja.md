# rfvp: FVP エンジンおよび IDE の非公式 Rust 製クロスプラットフォーム実装

<p align="center">
  <a href="README.md">English</a> |
  <a href="README.ja.md">日本語</a> |
  <a href="README.zh-Hant.md">繁體中文</a> |
  <a href="README.zh-Hans.md">简体中文</a>
</p>

<img src="images/flake.png" width="20%">

### ステータス
* プレイ可能？
* 詳細は [説明](setsumei/README.md) を参照してください。
![IN-GAME](./images/in-game.png)
![legacy](./images/legacy.png)

### rfvp デバッグ HUD
* F2 ホットキーで有効にできます（macOS では Fn + F2）。

### ビルド済みバイナリ
対応プラットフォーム向けのビルド済みバイナリは、[Pre-built Binaries](https://github.com/xmoezzz/rfvp/releases/tag/pre-release) から入手できます。

* プレリリース版は、最新の機能と修正を含む最新版です。
* 通常、バージョン番号は頻繁には更新しません。そのため、最新の機能や修正を利用したい場合は、プレリリース版を確認してください。

### ビルド方法
* macOS Bundle: [setsumei/HOW-TO-BUILD.macos-bundle.md](setsumei/HOW-TO-BUILD.macos-bundle.md)
* iOS IPA: [setsumei/HOW-TO-BUILD.ios.md](setsumei/HOW-TO-BUILD.ios.md)
* Android APK: [setsumei/HOW-TO-BUILD.android-apk.md](setsumei/HOW-TO-BUILD.android-apk.md)
* Windows EXE: [setsumei/HOW-TO-BUILD.windows-msvc.md](setsumei/HOW-TO-BUILD.windows-msvc.md)
* Linux ELF: [setsumei/HOW-TO-BUILD.host.md](setsumei/HOW-TO-BUILD.host.md)

### 再実装を超えて
* 本プロジェクトにはデコンパイラとコンパイラの両方があるため、このエンジンを基盤としたアプリケーションを作成することもできます。たとえば、簡単な Windows 95 風のペイントアプリケーションなどです。
![win95-painter](./images/win95-painter.png)

### インストール

RFVP では、プラットフォーム別のインストールガイドを [`setsumei/installation`](setsumei/installation) 以下に用意しています。

- [Windows](setsumei/installation/Windows.md)
- [Linux](setsumei/installation/Linux.md)
- [macOS](setsumei/installation/macOS.md)
- [iOS](setsumei/installation/iOS.md)
- [Android](setsumei/installation/Android.md)

### ドキュメント

本プロジェクトの Rust API ドキュメントは以下で公開されています。

- [RFVP Rust API Docs](https://xmoezzz.github.io/rfvp/)

### 対応プラットフォームとパッケージ形式
| プラットフォーム | 対応パッケージ形式                                      | ランチャー | スタンドアロン実行ファイル | アーキテクチャ                         |
| ---------------- | -------------------------------------------------------- | ---------: | --------------------------: | -------------------------------------- |
| macOS            | App Bundle (`.app`) および DMG (`.dmg`)                  |        Yes |                          No | Universal                              |
| iOS              | 署名なし IPA (`.ipa`, AltStore)                          |        Yes |                          No | arm64                                  |
| Android          | APK (`.apk`)                                             |        Yes |                          No | arm64-v8a, x86_64                      |
| Windows          | スタンドアロン EXE                                      |         No |                         Yes | x86_64, arm64                          |
| Linux            | スタンドアロン                                          |         No |                         Yes | x86_64, aarch64                        |
| WASM             | Bundle                                                   |        Yes |                          No | **任意のアーキテクチャ**（WASI 経由） |

* 本プロジェクトは Rust プロジェクトであるため、他の多くのプラットフォーム向けにもビルドできる可能性があります。

### 互換性
本プロジェクトは、オリジナルの FVP エンジンのすべてのバージョンとの互換性を目指しています。  
100% の互換性を保証するには、関連するすべてのゲームでの検証が必要です。このプロジェクトが有用だと感じ、より多くのゲームに対する互換性検証を加速することに協力したい場合は、プロジェクトへのスポンサー支援をご検討ください。

* 詳細は [setsumei/COMPATIBILITY.md](setsumei/COMPATIBILITY.md) も参照してください。一部の機能や挙動は、オリジナルのエンジンと異なる場合があります。

### 免責事項
- 本プロジェクトは、オリジナルのゲームエンジンのロジックを対象とした、独立したリバースエンジニアリングによる再実装です。すべてのソースコードは、対象ソフトウェアの挙動に関する調査と観察に基づき、ゼロから作成されています。本リポジトリには、オリジナル開発元のソースコードは含まれていません。
- このエンジンを使用するには、オリジナルゲームの正規コピーを所有している必要があります。オリジナルのゲームデータ、アセット、または同梱のゲーム実行ファイルを配布、共有、またはダウンロードリンクとして提供することは固く禁止されています。
- オリジナルのゲーム会社および権利者は、自らの知的財産に対するすべての所有権を保持します。敬意を表し、オリジナルの会社は、本リポジトリ内のコードを、商用利用を含むあらゆる目的で、事前の許可なく自由に利用できます。

### ライセンス
本プロジェクトは MPL-2.0 License の下でライセンスされています。詳細は [LICENSE](LICENSE) ファイルを参照してください。
