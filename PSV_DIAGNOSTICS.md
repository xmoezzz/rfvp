# RFVP PSV 诊断与性能优化

本 PR 分支 `psv-diagnostics-main` 基于 [xmoezzz/rfvp](https://github.com/xmoezzz/rfvp) 当前 `main`（`99435f22e94eaec27c0c1b9f22cc2a6b1910d7d1`），收录 2026-10-19 对《星空的记忆》PS Vita 移植版的实机诊断、渲染/内存优化、文字度量和可选的 GBK 汉化入口。

原始开发分支 `psv-diagnostics-20261019` 保留在 fork 中，基于固定提交 `709b6ba2f9ae4ba9f34c56df82a044eb958ff329`，包含一版“播放时清除临时静音”的音频实现；本 PR 分支已 rebase 到当前 `main`，沿用上游已有的 `AudioSilentOn` no-op 语义，只保留音频诊断事件。

本仓库不包含游戏脚本、图片/音频资源包、字体、存档或第三方汉化补丁。

## 摘要

- 相对当前 `upstream/main`：**18 个源码文件**（15 个修改、3 个新增），源码差分 **+520 / −123** 行；连同本说明共 19 个文件（+662 / −123）。
- 新增有界诊断报告：35 个阶段计时、最慢 8 帧、最近 12 个超过 50ms 的帧、资源/音频事件、音频/硬件错误计数。
- 主要优化：
  - 存档准备约 338ms → 102–104ms：缩略图直接采样帧缓冲；拆分截图/快照/整理计时。
  - 对白文字：每个字 4 次 `font.rasterize` → 1 次 metrics-only 路径；实机 `text_raster` 约 31–73ms。
  - 纹理：轻度缩小交给 GPU，RGBA 输入借用；失败回退旧尺寸。
  - 可选汉化：存在 `MemoriaGBK.gbk` 时按 GBK 解码并枚举 `.gbk`，否则原 `.hcb` + ShiftJIS 不变。
- 验证：固定基线分支 61 项测试、1068 组字体度量、日文 14/14 张画面一致；本 PR 分支另跑 2 项真实游戏状态测试、GBK 4200 帧启动和 PSV 交叉编译，均通过。
- PSV 实机：用户已确认第六版 + `MemoriaGBK.gbk` + Droid Sans Fallback 中文文本完整、运行正常（该验证来自基于固定基线的 v6 构建；本 PR 分支沿用上游音频语义，建议合入前再做一次实机 smoke test）。

## 基线与范围

- 本 PR 分支直接基于 `upstream/main` `99435f22e94eaec27c0c1b9f22cc2a6b1910d7d1`，没有未合并的基线提交。
- 原始固定基线为 `709b6ba2f9ae4ba9f34c56df82a044eb958ff329`；历史验收和 VPK 证据记录在 fork 的 `psv-diagnostics-20261019` 分支。
- 审查只统计引擎源码；构建脚本、日志、VPK、WSL 磁盘和游戏数据不计入。

## 改动摘要

| 版本 | 主要改动 | 结果 / 范围 |
| --- | --- | --- |
| 01 | 接通现有 perf 计时；加入读包、解码、差分、纹理上传、音频诊断；保留慢帧历史 | 定位 CPU 缩图、CPU 图片副本恢复、零音量等问题 |
| 02 | 轻度缩小的纹理交给 GPU；RGBA 输入借用；失败时回退旧尺寸 | 慢帧中的 CPU 纹理缩图耗时消失；部分 GPU 内存占用增加 |
| 03 | 历史实现：下一次播放清除临时静音 | 原始分支已实机确认配音恢复；当前 `upstream/main` 已把 `AudioSilentOn` 改为 no-op，本 PR **不覆盖**该行为，仅保留零音量/类型音量诊断 |
| 04 | 存档缩略图直接采样帧缓冲；拆分截图、快照、存档整理计时 | `save` 约 338ms → 102–104ms |
| 05 | 纯度量改用 `Font::metrics`；单字绘制复用 `item_adv` | 文字光栅化明显下降；1068 组度量一致、日文 14/14 截图一致 |
| 06 | 存在 `MemoriaGBK.gbk` 时选择 GBK 和 `.gbk` 扩展名 | 本机中文 4200 帧通过；PSV 实机（Droid Sans Fallback）已确认 |

## 诊断系统

- `crates/rfvp/src/perf.rs`：阶段从 23 项扩到 **35** 项；新增 `enabled`、`Scope`、`scope`、`event`。新增阶段一律追加在末尾，并用 `VFS_READ`、`GRAPH_DECODE` 等常量给本分支的钩子使用；**数字索引是报告 ABI，禁止插入或重排**。
- `platform/vita/host/src/diagnostics.rs`：有界慢帧历史（最慢 8 帧、最近 12 个 ≥50ms）、阶段汇总、音频累计；报告写入在测量帧之外。
- `platform/vita/host/src/main.rs`：启用诊断时钟、周期写出 `debug_report.txt`/`debug_boot.txt`；`diagnostics_off.txt` 可关闭记录；检测 `MemoriaGBK.gbk`。
- `platform/vita/host/src/audio.rs` 与 `c/rfvp_psv_vitasdk.c`：记录混音/加载/播放错误、主线程等锁、硬件线程/端口/音量/输出计数。
- 共享资源的 `perf::scope`/`event` 钩子：VFS 慢读、图片解码、CPU 副本恢复、差分合成、纹理上传、存档快照/整理；诊断时钟未安装时是 no-op。

## 关键改动

### 存档缩略图（04）

`capture_thumbnail` 不再复制整幅画面后做原面积平均滤波，而是直接读取帧缓冲，用四邻点双线性采样生成缩略图；保存路径新增 `psv_thumbnail`、`save_snapshot`、`save_finalize` 分段计时。缩略图尺寸和存档结构保持原约定。

### 文字度量（05）

`text_manager::measure_char_advance` 和 `char_draw_bounds` 原先调用 `font.rasterize` 后丢弃位图。现在改用上游已有的 `Font::metrics`（相同的 `glyph_metrics_internal`，不生成覆盖位图）；单字绘制仍走一次 `draw_char`，并复用已计算的 `item_adv`、扣回字间距。1068 组度量一致、日文 14/14 截图一致。

### 音频诊断（相对上游）

`upstream/main` 已把 `Channels::silent_on` 改为 no-op，并注明 Sakura 等脚本在播放中调用它时声音应被听见。本 PR 沿用该语义，不重新引入静音覆盖；仅在 `Channels::play` 记录 `ZERO audio gain`（slot/type 音量来源）和 `set_type_volume` 事件，便于继续排查零音量。原始分支的 transient-mute 实现保留在 `psv-diagnostics-20261019` 供对比。

### 汉化脚本入口（06）

原流程按目录顺序取第一份 `.hcb` 并按 ShiftJIS 解码。第六版在 PSV 入口检查 `ux0:data/rfvp/game/MemoriaGBK.gbk`：存在时 `boot.hcb_extension = "gbk"`、`boot.nls = Nls::GBK`；否则保持默认 `.hcb` + ShiftJIS。GBK 解码器、按扩展名枚举和字体回退沿用上游实现。

这是当前游戏的约定文件名开关，不是通用汉化自动识别；核心按扩展名枚举后取第一份，应只保留一份 `.gbk`。

### 纹理与内存策略（02）

`texture_policy.rs` 对符合尺寸限制的轻度缩小使用 GPU 过滤；RGBA 输入尽量减少复制；失败或内存不足时回退原尺寸。原尺寸纹理可能占用更多 GPU 内存，缩略图细节也可能与旧面积滤波不同。

## 源码差分

相对 `upstream/main` 的全部源码差分；路径为仓库相对路径。

| 文件 | 状态 | 行数 | 内容 |
| --- | --- | ---: | --- |
| [crates/rfvp/src/audio_player/channels.rs](crates/rfvp/src/audio_player/channels.rs) | 修改 | +13 / −1 | 零音量/类型音量诊断事件；不改变上游 `silent_on` no-op |
| [crates/rfvp/src/no_std_core.rs](crates/rfvp/src/no_std_core.rs) | 修改 | +29 / −22 | 存档快照/整理计时；音频命令错误带编号 |
| [crates/rfvp/src/perf.rs](crates/rfvp/src/perf.rs) | 修改 | +44 / −1 | 35 阶段、作用域计时、带时间戳事件、命名常量 |
| [crates/rfvp/src/rfvp_audio/commands.rs](crates/rfvp/src/rfvp_audio/commands.rs) | 修改 | +1 / −0 | 音频源路径和流编号 |
| [crates/rfvp/src/subsystem/components/syscalls/parts.rs](crates/rfvp/src/subsystem/components/syscalls/parts.rs) | 修改 | +4 / −2 | 差分加载/选择错误写入诊断 |
| [crates/rfvp/src/subsystem/resources/graph_buff.rs](crates/rfvp/src/subsystem/resources/graph_buff.rs) | 修改 | +4 / −0 | 图片解码/CPU 副本恢复计时和路径 |
| [crates/rfvp/src/subsystem/resources/motion_manager/mod.rs](crates/rfvp/src/subsystem/resources/motion_manager/mod.rs) | 修改 | +2 / −0 | 差分合成计时和编号事件 |
| [crates/rfvp/src/subsystem/resources/parts_manager.rs](crates/rfvp/src/subsystem/resources/parts_manager.rs) | 修改 | +2 / −0 | 多帧差分解码计时和路径 |
| [crates/rfvp/src/subsystem/resources/text_manager.rs](crates/rfvp/src/subsystem/resources/text_manager.rs) | 修改 | +9 / −11 | 纯度量改用 metrics；单字复用 item_adv |
| [crates/rfvp/src/subsystem/resources/vfs.rs](crates/rfvp/src/subsystem/resources/vfs.rs) | 修改 | +20 / −4 | 慢读、读取失败、配音路径 |
| [platform/vita/host/c/rfvp_psv_vitasdk.c](platform/vita/host/c/rfvp_psv_vitasdk.c) | 修改 | +18 / −3 | 音频线程/端口/音量/硬件输出计数 |
| [platform/vita/host/src/audio.rs](platform/vita/host/src/audio.rs) | 修改 | +47 / −5 | 混音/加载/播放错误、锁等待、音频事件 |
| [platform/vita/host/src/diagnostics.rs](platform/vita/host/src/diagnostics.rs) | 新增 | +115 / −0 | 有界慢帧历史、阶段汇总、音频统计 |
| [platform/vita/host/src/lib.rs](platform/vita/host/src/lib.rs) | 修改 | +2 / −0 | 导出纹理尺寸策略和缩略图模块 |
| [platform/vita/host/src/main.rs](platform/vita/host/src/main.rs) | 修改 | +58 / −46 | 诊断时钟/周期报告；`.gbk` 汉化入口 |
| [platform/vita/host/src/render.rs](platform/vita/host/src/render.rs) | 修改 | +44 / −28 | 尺寸策略、RGBA 借用/回退、缩略图直接采样 |
| [platform/vita/host/src/texture_policy.rs](platform/vita/host/src/texture_policy.rs) | 新增 | +17 / −0 | 轻度缩小交给 GPU；保留大幅缩小/内存不足回退 |
| [platform/vita/host/src/thumbnail.rs](platform/vita/host/src/thumbnail.rs) | 新增 | +91 / −0 | 裁切双线性缩略图采样及 5 项单元测试 |

## 验证

固定基线分支（`psv-diagnostics-20261019`）的独立验收：

- 10 项诊断/缩略图/纹理尺寸策略测试通过。
- 1 项字体 `metrics`/`rasterize` 度量一致性，覆盖 1068 组“字体 × 字符 × 字号”。
- 49 项引擎 no_std 核心测试通过，包含日文 4200 帧、存读档、音频状态。
- 1 项中文 GBK + Droid Sans Fallback 4200 帧运行通过。
- 日文历史画面 14/14 张逐字节一致。
- 第五、六版 VPK 的 ZIP/SELF/版本/SFO/SHA-256/包内补丁独立核对通过。

本 PR 分支（rebase 后）：

- `no_std_harness` 真实游戏状态/存读档 2 项通过。
- 中文 GBK + Droid Sans Fallback 4200 帧启动通过。
- PSV ARM release 交叉编译、`vita-elf-create`、`vita-make-fself`、`vita-pack-vpk` 通过。
- 本地采样截图（未随仓库发布）中有一帧出现两个缺字方框，定位为 Droid Sans Fallback 的 `U+2026`（省略号）缺字，不是代码回归。PSV 实机若出现省略号方框，请换含 `U+2026` 的中文字体或做字体合并。

PSV 实机：

- 用户确认第六版 + `MemoriaGBK.gbk` + Droid Sans Fallback 中文文本完整、运行正常（实机测试场景）。
- 该 v6 构建来自固定基线分支；本 PR 分支沿用上游 `AudioSilentOn` no-op 并已通过本地回归/交叉编译，建议合入前做一次 PSV smoke test。

安装包标识（构建产物不随仓库发布）：

| 版本 | 字节数 | SHA-256 |
| --- | ---: | --- |
| 05 | 917311 | `2a15e8804e4f960b7b57a5b746a4294db7ceeb5923ff7f340fbaa3f50b993ac7` |
| 06 | 917744 | `e7f401eda5edda3a515d5e72a31f991c6c422ca02f870a638faeae6b2f1424b1` |

## 构建与使用

- 构建沿用上游 PSV 的 VitaSDK/cargo-vita 流程。
- 日文：保留 `.hcb` + ShiftJIS，默认路径不变。
- 中文：在 `ux0:data/rfvp/game/` 放置唯一一份 `MemoriaGBK.gbk`，并用覆盖简体中文字形的单文件 TTF 作为 `default.ttf`。推荐 Droid Sans Fallback（4,033,420 字节，Apache-2.0）；注意它缺少 `U+2026`，若游戏使用省略号需换字体或合并该字形。安装后检查 `debug_boot.txt`：
  - `build=rfvp-psv-diag-20261019-06`
  - `hcb=MemoriaGBK.gbk`
- 切换前备份存档；中文脚本通常使用独立存档目录，跨脚本兼容未验证。
- 仓库不包含 VPK、游戏脚本、资源包、字体或存档。

## 已知限制

1. 本 PR 分支的 PSV 实机 smoke test 建议在合入前补做；中文实机确认来自固定基线的 v6 构建。
2. 存档约 102ms、整图上传约 59ms、`cpu_restore` 约 700ms、音频同步加载长帧仍有日志证据。
3. 诊断报告有开销；帧时间排除了报告整理/写入，不能当作无诊断版本的端到端帧率。
4. 共享文字/音频改动影响其他平台；本轮只覆盖普通 no_std 路径，未测试所有平台/`old_school`。
5. 上游 `main` 已 rebase；后续上游更新仍需重新 rebase 和回归。
6. `.gbk` 入口是游戏专用约定，不是通用汉化识别；扩展名枚举取第一份，只保留一份 `.gbk`。
7. 旧存档跨脚本兼容未验证；建议新开游戏。
8. Droid Sans Fallback 缺少 `U+2026`；字体覆盖以外的稀有字/全流程仍需实机确认。

## 许可与第三方说明

- 本分支基于 MPL-2.0 的 rfvp 源码，修改部分沿用同一许可证。
- 仓库不包含《星空的记忆》的游戏脚本、图片、音频、字体、存档，也不包含第三方汉化补丁文件；这些内容由用户自行准备。
- 游戏名称、商标和第三方汉化文本归各自权利人所有；本仓库仅包含引擎侧的加载/诊断/性能改动。
