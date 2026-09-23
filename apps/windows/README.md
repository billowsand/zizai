# 字在 Windows 输入法

Windows 端是一个产品，运行时由轻量 DLL、Server 与可选启用的语音 Worker 组成：

| 目录 | package | 产物 | 职责 |
| --- | --- | --- | --- |
| `server/` | `qingjian-windows-server` | `qingjian-server.exe` | 持有唯一的输入内核 `qingjian-core::Engine`，跑在所有应用进程之外 |
| `tsf/` | `qingjian-windows-tsf` | `qingjian_tsf.dll` | TSF 文本服务，被加载进每个应用进程，只做按键转发与文档写入（候选窗口由 Server 自绘） |
| `voice-worker/` | `qingjian-windows-voice-worker` | `qingjian-voice-worker.exe` | 功能开启时加载本地 SenseVoice、按需采音与识别；故障不拖垮 Server 或宿主应用 |

```
应用进程 A ── qingjian_tsf.dll ─┐
应用进程 B ── qingjian_tsf.dll ─┼─ 命名管道 \\.\pipe\qingjian.<会话号> ─▶ qingjian-server（唯一的 Engine）
应用进程 C ── qingjian_tsf.dll ─┘
                                      └─ 私有 stdio ─▶ qingjian-voice-worker
```

## 为什么核心逻辑要在进程外

Windows 的文本服务（TSF，Text Services Framework）是一个 COM DLL（`ITfTextInputProcessor`），
系统会把它加载进**每一个**接受文本输入的应用进程。因此输入内核不能待在 DLL 里（会被复制进几十个进程、
状态无法共享、崩溃会连累宿主应用）。字在照 Weasel（WeaselServer + WeaselTSF）、水杉（Server 进程）的做法：
Engine 只此一份，跑在独立的 Server 进程；每个应用进程里的 TSF DLL 只做两件事——把系统按键翻成协议消息发来、
把 Server 回的候选画到候选窗口。

## 为什么拆成多个 package

各产物的依赖集合刻意不同：DLL 只依赖 `qingjian-core`、`qingjian-platform` 与官方 `windows` crate（COM
`implement` 宏），Server 才依赖词库 / 学习整棵树，语音 Worker 独占音频与 sherpa-onnx。合成一个 package 后，DLL 的
编译单元会拉进 Server 的依赖；用 feature 区分也不行，workspace 一起构建时 feature 会统一。crate 边界就是
「DLL 不含 Engine」这条约束的强制手段。判断标准：换掉平台适配层，不应该需要改 Core 的任何一行。

## 四个部分

- **Server 进程**（`server/`）：装配并持有 Engine（词库 / 语言模型 / 学习），按 `SessionId` 为每个
  应用会话维护各自的组句状态，处理按键、产出候选与上屏文本，把本地整句模型的异步结果主动推给
  对应会话。
- **IPC 协议**（`qingjian-platform::protocol`，两端共用）：`ClientMessage`（DLL → Server：开 / 关会话（带宿主 exe 名，
  `[apps]` 按应用设置据此查）、按键、上屏、回上下文）与 `ServerMessage`（Server → DLL：按键结果、上屏结果、异步重绘、请求上下文），一次要绘制的状态是
  `Frame`（preedit 分段 + 候选页）。长度前缀 JSON 帧的编解码与缺省管道名也在这里，DLL 不必依赖整个 Server 库。
- **TSF DLL**（`tsf/`）：分「引擎层」`client`（平台无关的管道客户端 `EngineClient`，泛型在任意 `Read + Write`
  上，本机就能接真 Server 端到端测）与「COM 层」`com`（`cfg(windows)`：`DllGetClassObject` → `IClassFactory`
  → `#[implement(ITfTextInputProcessor, ITfKeyEventSink)]`，编辑会话上屏，把组句位置报给 Server 摆候选窗口，
  `DllRegisterServer` 注册文本服务）。
- **语音 Worker**（`voice-worker/` + `crates/qingjian-voice`）：Server 用长度前缀 JSON 的 stdio 私有协议控制；
  TSF 只拦语音键、量当前插入点并直接写入最终文字，不加载模型、不访问麦克风，也不经过剪贴板。Server 把实时电平与临时转写画进原候选窗口，语音输入不另开 OSD。

设计细节见 `docs/design/architecture.md`「Windows：TSF」。

## 构建

本机（非 Windows）只做交叉 `check`，产出不了可用二进制，但协议层的端到端测试能跑：

```bash
cargo check --target x86_64-pc-windows-gnu -p qingjian-windows-server -p qingjian-windows-tsf -p qingjian-windows-voice-worker
cargo test -p qingjian-windows-server -p qingjian-windows-tsf -p qingjian-voice
```

真正编译与试用都在 Windows 机器上（MSVC 工具链）：

```bat
:: 1) 编译出 DLL、Server 与语音 Worker
cargo build -p qingjian-windows-tsf -p qingjian-windows-server -p qingjian-windows-voice-worker

:: 2) 注册文本服务（改 HKEY_CLASSES_ROOT，图标写到 %ProgramData%\Qingjian\qingjian.ico，要管理员）
regsvr32 target\debug\qingjian_tsf.dll

:: 3) 起 Server（引擎在这里；没起时 DLL 整键放行——所敲字母直接进应用，起来后下一键 / 下次聚焦自动重连）
::    release 的 DLL 会自己拉起同目录的 qingjian-server.exe；debug 的不拉（会占住 target\debug 里的 exe，
::    下一次 cargo build 链接不上），所以开发时这一步得自己跑
cargo run -p qingjian-windows-server

:: 4) 在系统「语言 / 输入法」里应能看到「字在」，切到它，在任意输入框敲字
::    日志都在 %LOCALAPPDATA%\Qingjian\logs\：server.<日期>.log / tsf.<日期>.log / settings.<日期>.log（按天，留 7 天）

:: 反注册
regsvr32 /u target\debug\qingjian_tsf.dll
```

## 设置程序与 Windows App Runtime

打包缺省装的是 `settings-egui/`（egui 自绘，按 `qingjian-settings.exe` 这个名字装进去，不要运行时；
2026-09-18 定的，见 [egui-settings-spike.md](../../docs/notes/egui-settings-spike.md)）。下面这套只有 `build.ps1 -WinUiSettings` 才打。

`settings/`（`qingjian-settings.exe`）用 Windows Reactor（WinUI 3）画界面，是几个产物里唯一依赖 Windows App Runtime 的。
它的部署方式是**自包含**：`build.rs` 让 `windows-reactor-setup` 把 `Microsoft.WindowsAppSDK.Runtime` 的 MSIX 解到
`target\release\` 并按自包含标记嵌清单，安装包把这些文件装到 exe 同级——不依赖机器上装没装框架包。
Windows 10 上框架依赖的引导走不通（它要先调 Windows 11 才有的 `TryCreatePackageDependency`），
同一个 `build.rs` 还把这两个 API 改成延迟加载：否则它们会进 exe 的导入表，Windows 10 在加载期就起不来。
定位过程、上游 issue / PR 与取舍见 `docs/notes/windows-win10.md`；打包侧见 `apps/windows/installer/README.md`。

## 版本与发布

各平台壳版本号独立（见 `docs/notes/release.md`）：Windows app package 的 `version` 各自写在自己的 `Cargo.toml`，
不跟 workspace 走；发布标签用 `windows-v<版本>`。
