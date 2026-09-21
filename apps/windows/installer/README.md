# 字在 Windows 安装包

用 [Inno Setup](https://jrsoftware.org/isinfo.php) 打的安装包，把 TSF DLL（64 位与 32 位各一份）、Server、语音 Worker、设置程序与随包数据一起装进
`C:\Program Files\Qingjian`，注册文本服务，并设登录自启。

## 安装布局

```
C:\Program Files\Qingjian\
    qingjian_tsf-<版本>.dll       TSF 文本服务（64 位；被加载进每个应用进程；按版本起名，见「升级」）
    qingjian_tsf-<版本>-x86.dll   同上的 32 位版（企业微信 / WPS / 32 位 QQ 这类 32 位应用只能加载它）
    qingjian-server.exe       输入内核 Server（跑在应用进程外）
    qingjian-settings.exe     设置界面
    Microsoft.UI.Xaml.dll …   仅 `-WinUiSettings` 对比包包含的 Windows App Runtime
    qingjian.ico              开始菜单 / 启动项快捷方式的图标（exe 里也嵌了一份）
    data\generated\           dict.qj / lm.qj / english.tsv / dicts\*.qj
    data\voice\               仅 -VoiceModelDir / -PunctuationModelDir / -HrDir 本地测试包包含
    assets\                   emoji\ sample\
```

Server、语音 Worker 与设置程序按 **exe 相对**定位随包资源（`qingjian_platform::resources`）：装机时资源与 exe 同级，
开发时是仓库 `ime\`（exe 在 `target\{debug,release}\` 下往上三层）。相对写法两套布局一致，只有根不同。

用户数据仍在 `%APPDATA%\Qingjian`（config.toml、学习数据、统计），日志在 `%LOCALAPPDATA%\Qingjian\logs`（`server.` / `tsf.` / `settings.` 前缀，按天，留 7 天）；
卸载不动这些。图标由 `regsvr32` 写到 `%ProgramData%\Qingjian\qingjian.ico`（DLL 里 include_bytes 内嵌）。

## 安装程序做的几件事

1. **结束旧进程**：`PrepareToInstall` 里 `taskkill` Server、语音 Worker 与设置程序。
2. **应用容器权限**：`icacls` 给安装目录加 `ALL APPLICATION PACKAGES`（SID `*S-1-15-2-1`）读+执行。
   不加的话 UWP/AppContainer 应用（任务栏搜索、设置）读不到 DLL，切不到字在。
3. **注册文本服务**：64 位 DLL 用 `regsvr32`、32 位 DLL 用 `SysWOW64\regsvr32`，各注册一次（各自写进自己视图的 HKCR，`CTF\TIP` 两边共用；要管理员——安装程序本就提权）。
4. **清旧 DLL**：装完删历次版本留下的 `qingjian_tsf*.dll`，仍被应用占用的登记成重启后删（`RestartReplace`）。
5. **登录自启**：「启动」文件夹放 Server 快捷方式（Explorer 走 ShellExecute 拉起才拿到 uiAccess；计划任务拿不到）。
6. **立即启动**：完成页以当前非提升用户 ShellExecute 起一次 Server，装完就能用，不必先注销。

卸载反向：杀 Server / 语音 Worker / 设置程序 → 反注册当前版本 DLL → 删文件（占用中的 DLL 重启后删，`[UninstallDelete]` 兜住旧版本的）。

## 升级：DLL 被占用怎么办

`qingjian_tsf.dll` 被加载进每一个有文本框的应用进程，文件锁着覆盖不了；Inno 缺省的 `CloseApplications`
用 Restart Manager 找出所有占用者要求关闭——对输入法 DLL 就是「关掉一切」，所以关掉它（`CloseApplications=no`），改成：

- DLL **按版本起名并排装**（`DestName: qingjian_tsf-<版本>.dll`），新文件从不与旧文件撞名；
- 只 `regsvr32` 新文件（InprocServer32 指向它）。**不要**对旧 DLL `regsvr32 /u`：那会把整个 CLSID / profile 注销掉；
- 已开着的应用继续用进程里的旧 DLL 直到重启，Server 两个版本都服务（`OpenSession` 带协议版本，对不上只记警告）；
- 装完删旧 DLL，删不掉的登记成重启后删。

## WinUI 对比包的 Windows App Runtime

缺省设置界面使用 egui，不需要 Windows App Runtime。只有 `-WinUiSettings` 对比包使用 Windows Reactor（WinUI 3），而它的框架依赖引导只有 Windows 11 走得通：要 Windows 11 才有的
AppModel API 把框架包加进进程包图，Windows 10 上没有那两个函数（定位见 `docs\notes\windows-win10.md`）。
所以设置程序用**自包含部署**——`apps\windows\settings\build.rs` 让 `windows-reactor-setup` 把 Windows App Runtime
铺到 `target\release\`，打包时按 `settings-runtime.txt` 挑进 `target\installer\settings-runtime`，本目录的
`qingjian.iss` 再整个目录装到 `{app}` 下、与 `qingjian-settings.exe` 同级。

- 这些文件是运行时必需：少一件（或层级装错）设置窗口就起不来，`build.ps1` 发现缺文件会直接失败。
- 升级 `windows-reactor` / `windows-reactor-setup` 时，照新版 crate 的 `assets/runtime.txt` 核对 `settings-runtime.txt`。
- Server 与 TSF DLL 不依赖它；装机体积的大头仍是随包数据。
- `windows-reactor-setup` 在 `cargo build` 时用系统 `curl.exe` 从 NuGet 下运行时包（无校验，失败只打印），缓存在 `%LOCALAPPDATA%\windows-reactor-setup`；CI 的 runner 每次都会重下一遍。下载失败的后果由 `build.ps1` 的缺项检查兜住。

## 打包（在编译机上）

```powershell
# 需要 MSVC 工具链 + Inno Setup。数据取自仓库 data\generated 与 assets，打包前先确保 .qj 是最新的。
powershell -ExecutionPolicy Bypass -File apps\windows\installer\build.ps1
```

本地语音联调时可把已经准备好的 SenseVoice 一并放入测试包；目录须含 `model.int8.onnx` 与 `tokens.txt`：

```powershell
powershell -ExecutionPolicy Bypass -File apps\windows\installer\build.ps1 -VoiceModelDir E:\auto-voice\models\sense-voice
```

标点恢复与同音词替换资源同理；带这三个开关的资源打进包后**装完即用，无需在设置里填任何路径**——
Server 按固定相对路径（`data\voice\punctuation\model.int8.onnx`、`data\voice\hr\lexicon.txt`、`replace.fst`）
自动发现，设置里只选择加载与否（关着不加载；文件不在时自动跳过，不影响语音输入）。标点模型用 sherpa-onnx 的
`sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8`（72 MB，从
[punctuation-models Release](https://github.com/k2-fsa/sherpa-onnx/releases/tag/punctuation-models) 下载）。
通常与 `-VoiceModelDir` 一起传：

```powershell
powershell -ExecutionPolicy Bypass -File apps\windows\installer\build.ps1 -VoiceModelDir E:\auto-voice\models\sense-voice -PunctuationModelDir data\voice\punctuation -HrDir E:\auto-voice\models\hr
```

此参数只用于本地测试，正式发布包不默认分发第三方模型。

阶段测试包用 `-BuildLabel phase1-r1` 追加唯一标识；同一阶段重编时递增 `r2`、`r3`，避免不同二进制共用文件名：

```powershell
powershell -ExecutionPolicy Bypass -File apps\windows\installer\build.ps1 -BuildLabel phase1-r1
```

脚本 release 构建 Windows 产物、从 `apps\windows\server\Cargo.toml` 读版本、找 `ISCC.exe`、编 `qingjian.iss`，
成品在 `target\installer\Zizai-<版本>-Setup.exe`。改了数据 / 脚本但二进制没变时加 `-SkipBuild`；`-Sign` 用自签证书签产物
（uiAccess 要求 Server 签名 + 装 Program Files）。

**设置程序缺省用 egui 那份**（`qingjian-settings-egui.exe`，按正式名字装进去，Server 的齿轮、开始菜单、安装前 taskkill 都不用改），
不装那 118 项 Windows App Runtime，同一提交实测 76.9 → 66.2 MiB。从带 WinUI 的老版本升上来时，
`[InstallDelete]` 会把上一版留在 `{app}` 里的那 118 项删掉（片段由 `build.ps1` 按 `settings-runtime.txt` 生成成
`target\installer\retire-runtime.iss`，`qingjian.iss` `#include` 它）——Inno 只管自己装过的文件，删不掉「这一版不再装」的。

`-WinUiSettings` 换回 WinUI 3 那份（连同自包含运行时），成品另起名 `Zizai-<版本>-winui-Setup.exe`，不覆盖正式包；只在要对比时用。

安装向导使用 `assets\icon\installer-wizard-light.png` 与 `installer-wizard-dark.png`，由 `assets\icon\generate.py` 和主图标一起生成；
`WizardStyle=modern dynamic` 会在启动时按 Windows 明暗模式选对应画面。改了图标生成脚本后先重新生成资源再打包。

`ISCC.exe` 按「Program Files 里的 7 → 每用户安装的 7（`%LOCALAPPDATA%\Programs\Inno Setup 7`）→ PATH → 6」找，
找不到就用 `QINGJIAN_ISCC` 指定。`data\generated` 里 iscc 要用的表都得在（`dict.qj` / `lm.qj` / `english.tsv` / `dicts\*.qj`）：
少一张 iscc 会报 `Source file ... does not exist`，说明本机的产品数据没生成全（或 `data` Release 上的包旧了，跑 `tools/release/data-bundle.sh` 重传）。
`data\model\model.qjm` 可选，没有就不装本地整句模型。

也可手动：`iscc /DAppVersion=0.1.0 apps\windows\installer\qingjian.iss`。

## 注意

- **Inno 版本**：开发机与 CI 统一用 Inno Setup **7.1.0**（CI 从 jrsoftware/issrc 的 GitHub Release 钉死下载）。它自带简体中文翻译；
  6.x 的安装包不带 `Languages\ChineseSimplified.isl`，Chocolatey 也只有 6.x，别用。`ArchitecturesAllowed=x64compatible` 需 6.3+。
- **签名**：对外分发走 `release.yml` 的 SignPath Foundation 免费签名（流程与配置见 `docs\design\code-signing.md`，不在本机签）；`-PreSigned` 是给 CI 的：产物已被 SignPath 签回时跳过剥离并逐个校验签名。开发期用 `-Sign` 的自签证书。
- **不签名是缺省**：`build.ps1` 不加 `-Sign` / `-PreSigned` 就不签名、不嵌 uiAccess（`server\build.rs` 只认 `QINGJIAN_UIACCESS=1`），
  打出来的包任何机器都能起——没签名的 exe 带 uiAccess=true 会起不来（os error 740）。代价是候选窗在 UWP 宿主里可能被盖住、TSF DLL 进不了系统应用。
  `release.yml` 在 SignPath 配置缺省时仍走这条路，配齐后自动签名并开 uiAccess。
- **两种包别串**：`-Sign` 会把签名留在 `target\` 里，代码没变时下次构建不重新链接、签名跟着留着。
  所以不带 `-Sign` 时 `build.ps1` 会先剥掉残留签名（要 Windows SDK 的 `signtool`），免得把「证书链不受信任」的文件打进对外分发的包。
