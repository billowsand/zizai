; 青简 Windows 输入法安装脚本（Inno Setup）。
;
; 装到 Program Files\Qingjian（64 位），把 TSF DLL（64 位与 32 位各一份，见 README「安装布局」）、Server、语音 Worker、设置程序与随包数据装在一起，
; 然后：① 给安装目录加 ALL APPLICATION PACKAGES 读+执行权限（UWP/AppContainer 应用——任务栏搜索、
; 设置——才能加载 DLL）；② regsvr32 注册文本服务，64 位与 32 位各注册一次（图标落到 %ProgramData%\Qingjian）；
; ③ 在「启动」文件夹放 Server 快捷方式（登录时由 Explorer 走 ShellExecute 拉起，uiAccess 才生效——
;    计划任务直接拉起拿不到 uiAccess）；④ 装完立即以原用户 ShellExecute 起一次 Server，免得先注销
;    （静默安装没有完成页，在 ssPostInstall 起；有向导时在点 Finish 之后起）。
; 卸载反向：删旧任务（若有）、杀 Server、反注册 DLL，再删文件（用户数据 %APPDATA%\Qingjian 保留；启动快捷方式 Inno 自动删）。
;
; 升级：DLL 被加载进每个应用进程，文件锁着覆盖不了，所以 DLL 按版本起名（qingjian_tsf-<版本>.dll）并排装，
; 注册新的，旧的装完后删（删不掉的登记成重启后删）；已开着的应用继续用旧 DLL 直到重启，Server 两个版本都服务。
; Inno 的 CloseApplications 会用 Restart Manager 找出所有加载了 *.dll 的进程要求关闭——对输入法 DLL 就是关一切，故关掉；
; Server / 语音 Worker / 设置程序要覆盖，安装前自己 taskkill。
;
; 版本号由打包脚本用 /DAppVersion=... 传入，缺省 0.1.0。用法见本目录 README.md。

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
; VersionInfoVersion 只认 a.b.c.d 数字；版本带预发布后缀（0.1.0-alpha.1）时由打包脚本传去掉后缀的数字版本。
#ifndef AppVersionNumeric
  #define AppVersionNumeric AppVersion
#endif
#define AppName "字在"
#define Publisher "字在"
#define WebsiteUrl "https://qingjian.app"
; 脚本相对仓库根（ime/）：installer → windows → apps → ime
#define Repo "..\..\.."
; 按版本起名的 TSF DLL（见文件头「升级」）。
#define TsfDll "qingjian_tsf-" + AppVersion + ".dll"
#define TsfDll32 "qingjian_tsf-" + AppVersion + "-x86.dll"
; 设置程序缺省是 egui 那份；要 WinUI 那份得显式 /DWinUiSettings=1，包另起名字，和正式包放在一起不会互相覆盖。
#ifdef WinUiSettings
  #define SetupSuffix "-winui"
#else
  #define SetupSuffix ""
#endif

[Setup]
AppId={{A7E3C1F2-5B94-4D6A-9C0E-2F8B1D3A6E70}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#Publisher}
AppSupportURL={#WebsiteUrl}
VersionInfoVersion={#AppVersionNumeric}
; SignPath 元数据校验要求安装包与各 PE 的 ProductName 一致（ProductName 用 ASCII "Qingjian"：
; winresource 生成的 .rc 对非 ASCII 不可靠，见 docs/design/code-signing.md）。
; ProductVersion 不在这里写：Inno 缺省取 AppVersion（完整版本串，含预发布后缀），与 PE 嵌的
; QINGJIAN_PRODUCT_VERSION 一致；显式写 VersionInfoProductVersion 会被 Inno 按数字版本校验拒掉。
VersionInfoProductName=Qingjian
DefaultDirName={autopf}\Qingjian
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 1809 起（Windows App Runtime 的下限，见 docs\notes\windows-win10.md）。
MinVersion=10.0.17763
PrivilegesRequired=admin
; 别让 Restart Manager 去关所有加载了 DLL 的应用（那是每一个有文本框的应用）。
CloseApplications=no
OutputDir={#Repo}\target\installer
OutputBaseFilename=Zizai-{#AppVersion}{#SetupSuffix}-Setup
SetupIconFile={#Repo}\apps\windows\tsf\resources\qingjian.ico
UninstallDisplayIcon={app}\qingjian.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern dynamic
WizardImageFile={#Repo}\assets\icon\installer-wizard-light.png
WizardImageFileDynamicDark={#Repo}\assets\icon\installer-wizard-dark.png
WizardSmallImageFile={#Repo}\assets\icon\logo.png
WizardSmallImageFileDynamicDark={#Repo}\assets\icon\logo.png

[Languages]
Name: "chs"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Files]
; —— 二进制 ——
; DLL 按版本起名并排装；卸载时若仍被占用，登记成重启后删。
Source: "{#Repo}\target\release\qingjian_tsf.dll";      DestDir: "{app}"; DestName: "{#TsfDll}"; Flags: ignoreversion uninsrestartdelete
Source: "{#Repo}\target\i686-pc-windows-msvc\release\qingjian_tsf.dll"; DestDir: "{app}"; DestName: "{#TsfDll32}"; Flags: ignoreversion uninsrestartdelete
Source: "{#Repo}\target\release\qingjian-server.exe";   DestDir: "{app}"; Flags: ignoreversion
Source: "{#Repo}\target\release\qingjian-voice-worker.exe"; DestDir: "{app}"; Flags: ignoreversion
#ifdef WinUiSettings
; WinUI 3 那份设置程序（build.ps1 -WinUiSettings，只用于对比）：自带一份 Windows App Runtime
; （自包含部署：Windows 10 上机器装的框架包用不了，见 docs\notes\windows-win10.md）；
; 文件由 build.ps1 按 settings-runtime.txt 从 target\release 挑进 target\installer\settings-runtime，必须与 exe 同级。
Source: "{#Repo}\target\release\qingjian-settings.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Repo}\target\installer\settings-runtime\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
#else
; 缺省：egui 那份设置程序，按正式名字装，Server 的齿轮、开始菜单快捷方式与安装前 taskkill 都不用改。
; 自绘 UI 不要 Windows App Runtime，那 118 项 / 56 MB 整段不装（同一提交实测 76.9 → 66.2 MiB）。
Source: "{#Repo}\target\release\qingjian-settings-egui.exe"; DestDir: "{app}"; DestName: "qingjian-settings.exe"; Flags: ignoreversion
#endif
Source: "{#Repo}\apps\windows\tsf\resources\qingjian.ico"; DestDir: "{app}"; Flags: ignoreversion
; —— 随包生成数据（只装运行时要的 .qj / .tsv，不装 dev 中间产物）——
Source: "{#Repo}\data\generated\dict.qj";        DestDir: "{app}\data\generated";       Flags: ignoreversion
Source: "{#Repo}\data\generated\lm.qj";          DestDir: "{app}\data\generated";       Flags: ignoreversion
Source: "{#Repo}\data\generated\english.tsv";    DestDir: "{app}\data\generated";       Flags: ignoreversion
Source: "{#Repo}\data\generated\dicts\*.qj";     DestDir: "{app}\data\generated\dicts";  Flags: ignoreversion
; —— 本地整句模型（tools/release/pack-model.sh 打成的单文件 data\model\model.qjm；没有就不装，Server 不重排）——
Source: "{#Repo}\data\model\model.qjm"; DestDir: "{app}\data\model"; Flags: ignoreversion skipifsourcedoesntexist
; —— 本地测试包可显式携带 SenseVoice；正式包不传 VoiceModelDir，避免默认分发第三方模型 ——
#ifdef VoiceModelDir
Source: "{#VoiceModelDir}\model.int8.onnx"; DestDir: "{app}\data\voice\sense-voice"; Flags: ignoreversion
Source: "{#VoiceModelDir}\tokens.txt"; DestDir: "{app}\data\voice\sense-voice"; Flags: ignoreversion
#endif
; —— 随 git 的资源 ——
Source: "{#Repo}\assets\emoji\emoji-zh.tsv";     DestDir: "{app}\assets\emoji";  Flags: ignoreversion
Source: "{#Repo}\assets\emoji\emoji-en.tsv";     DestDir: "{app}\assets\emoji";  Flags: ignoreversion
Source: "{#Repo}\assets\fuma\xiaohe.txt";        DestDir: "{app}\assets\fuma";  Flags: ignoreversion
Source: "{#Repo}\assets\sample\dict.tsv";        DestDir: "{app}\assets\sample"; Flags: ignoreversion
Source: "{#Repo}\THIRD_PARTY_NOTICES.md";         DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\字在设置"; Filename: "{app}\qingjian-settings.exe"; IconFilename: "{app}\qingjian.ico"
Name: "{group}\卸载字在"; Filename: "{uninstallexe}"
; 登录自启：登录时 Explorer 走 ShellExecute 拉起本快捷方式 → AppInfo 授予 uiAccess，候选窗才能盖过商店 / 任务栏搜索。
; 用 {commonstartup}（所有用户「启动」文件夹）而非 {userstartup}：本安装器是 admin 机器级安装，
; admin 模式下写每用户区会落到「谁提权就写谁」的 profile（Inno 会告警且可能不是目标用户）；
; 机器级「启动」项对每个登录用户都在其会话里由该用户的 Explorer 拉起，仍是 per-user 运行、仍授予 uiAccess。
; （计划任务直接拉起拿不到 uiAccess，故不用 schtasks。）
Name: "{commonstartup}\字在 Server"; Filename: "{app}\qingjian-server.exe"; WorkingDir: "{app}"; IconFilename: "{app}\qingjian.ico"

[Run]
; ① UWP/AppContainer 应用要能读安装目录才能加载 DLL（*S-1-15-2-1 = ALL APPLICATION PACKAGES，按 SID 与语言无关）。
Filename: "{sys}\icacls.exe"; Parameters: """{app}"" /grant *S-1-15-2-1:(OI)(CI)RX /T /C /Q"; \
  Flags: runhidden waituntilterminated; StatusMsg: "配置应用容器权限…"
; ② 注册文本服务（写 HKCR + 图标到 %ProgramData%\Qingjian\qingjian.ico）。注册的是本版本的 DLL，
;    InprocServer32 指向新文件；旧版本的 DLL **不能** regsvr32 /u（那会把整个 CLSID 注销掉）。
Filename: "{sys}\regsvr32.exe"; Parameters: "/s ""{app}\{#TsfDll}"""; \
  Flags: runhidden waituntilterminated; StatusMsg: "注册输入法…"
; 32 位那份用 SysWOW64 里的 32 位 regsvr32 注册，InprocServer32 才落到 WOW6432Node 下给 32 位进程用。
Filename: "{syswow64}\regsvr32.exe"; Parameters: "/s ""{app}\{#TsfDll32}"""; \
  Flags: runhidden waituntilterminated; StatusMsg: "注册输入法（32 位）…"
; ④ 装完立即起一次 Server 见 [Code] 的 NextButtonClick：uiAccess=true 的 exe 不能用
;    CreateProcess / runasoriginaluser 拉起（报 740），必须走 ShellExecute（等同双击）。

[UninstallRun]
; 反向：先删登录任务、杀 Server / 语音 Worker / 设置程序、反注册 DLL，Inno 再删文件（DLL 若仍被占用，重启后删）。
Filename: "{sys}\schtasks.exe"; Parameters: "/delete /tn ""Qingjian Server"" /f"; \
  Flags: runhidden; RunOnceId: "DelLogonTask"
Filename: "{sys}\taskkill.exe"; Parameters: "/im qingjian-server.exe /f"; \
  Flags: runhidden; RunOnceId: "KillServer"
Filename: "{sys}\taskkill.exe"; Parameters: "/im qingjian-voice-worker.exe /f"; \
  Flags: runhidden; RunOnceId: "KillVoiceWorker"
Filename: "{sys}\taskkill.exe"; Parameters: "/im qingjian-settings.exe /f"; \
  Flags: runhidden; RunOnceId: "KillSettings"
Filename: "{sys}\regsvr32.exe"; Parameters: "/u /s ""{app}\{#TsfDll}"""; \
  Flags: runhidden; RunOnceId: "UnregDll"
Filename: "{syswow64}\regsvr32.exe"; Parameters: "/u /s ""{app}\{#TsfDll32}"""; \
  Flags: runhidden; RunOnceId: "UnregDll32"

[InstallDelete]
; 品牌切换前的开始菜单与机器级启动快捷方式。安装目录、AppId 与二进制名不变，升级仍是原地升级。
Type: files; Name: "{commonprograms}\青简\青简设置.lnk"
Type: files; Name: "{commonprograms}\青简\卸载青简.lnk"
Type: dirifempty; Name: "{commonprograms}\青简"
Type: files; Name: "{commonstartup}\青简 Server.lnk"
; 更早版本装在当前用户「启动」文件夹里的自启快捷方式：与机器级那份并存会起两个 Server（两条状态条）。
Type: files; Name: "{userstartup}\Qingjian Server.lnk"
; 更早版本装的模型三件套（现在只带 model.qjm）：留着白占 56 MB。
Type: files; Name: "{app}\data\model\model.safetensors"
Type: files; Name: "{app}\data\model\config.json"
Type: files; Name: "{app}\data\model\vocab.json"
; 更早版本装的释义表与词汇等级表（现在不带译文功能）：留着白占 53 MB。
Type: files; Name: "{app}\data\generated\glossary-*.qj"
Type: filesandordirs; Name: "{app}\assets\levels"
; 上一版 WinUI 设置程序带的那份 Windows App Runtime（118 项 / 56 MB）：egui 版不要它，升级时清掉。
; 片段由 build.ps1 按 settings-runtime.txt 生成（打 -WinUiSettings 时是空的，那些文件正是要装的）。
#define RetireRuntime Repo + "\target\installer\retire-runtime.iss"
#if FileExists(RetireRuntime)
  #include RetireRuntime
#endif

[UninstallDelete]
; 历次升级留下的旧版本 DLL（正常在升级时就删了；仍被占用的会留到这里）。
Type: files; Name: "{app}\qingjian_tsf-*.dll"

[Code]
procedure KillProcess(const Image: String);
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/im ' + Image + ' /f', '',
    SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

{ 同版本重装（开发期反复装）：目标文件名与已加载的 DLL 撞名，覆盖不了但 Windows 允许改名，
  先把它改成 qingjian_tsf-<版本>.old-<随机>.dll 腾出名字，装完由 DeleteStaleDlls 删掉 / 登记重启后删。 }
procedure RetireLoadedDll(const Name: String);
var
  Path, Retired: String;
begin
  Path := ExpandConstant('{app}\') + Name;
  if FileExists(Path) then
  begin
    Retired := ExpandConstant('{app}\qingjian_tsf-{#AppVersion}.old-') + IntToStr(Random(1000000)) + '.dll';
    if not RenameFile(Path, Retired) then
      Log('改名旧 DLL 失败: ' + Path);
  end;
end;

{ 覆盖前先结束 Server、语音 Worker 与设置程序（DLL 按版本并排装，不用关应用）。
  没在跑时 taskkill 返回非 0，忽略。 }
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  KillProcess('qingjian-server.exe');
  KillProcess('qingjian-voice-worker.exe');
  KillProcess('qingjian-settings.exe');
  RetireLoadedDll('{#TsfDll}');
  RetireLoadedDll('{#TsfDll32}');
  Result := '';
end;

{ 清掉旧版本建的「登录自启」计划任务（现在改用「启动」文件夹快捷方式，见 [Icons]）。
  计划任务直接拉起 Server 拿不到 uiAccess，升级安装时删掉它，免得它在登录时抢先以非 uiAccess 方式
  起 Server 并占住命名管道，让快捷方式那份起不来。没有旧任务时 schtasks 返回非 0，忽略即可。 }
procedure DeleteLegacyLogonTask;
var
  ResultCode: Integer;
begin
  Exec('schtasks.exe', '/delete /tn "Qingjian Server" /f', '',
    SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

{ 删掉旧版本的 DLL（含没带版本号的最早那份）。仍被某个应用加载着的删不掉，登记成重启后删：
  那些应用重启前继续用旧 DLL，Server 两个版本都服务。 }
procedure DeleteStaleDlls;
var
  Dir, Path: String;
  Found: TFindRec;
begin
  Dir := ExpandConstant('{app}');
  if FindFirst(Dir + '\qingjian_tsf*.dll', Found) then
  begin
    try
      repeat
        if (CompareText(Found.Name, '{#TsfDll}') <> 0) and (CompareText(Found.Name, '{#TsfDll32}') <> 0) then
        begin
          Path := Dir + '\' + Found.Name;
          if not DeleteFile(Path) then
            RestartReplace(Path, '');
        end;
      until not FindNext(Found);
    finally
      FindClose(Found);
    end;
  end;
end;

{ 起一次 Server。uiAccess=true 的 exe 不能用 CreateProcess / runasoriginaluser 拉起（报 740），
  必须以原（非提升）用户身份 ShellExecute（等同双击），AppInfo 才会授予 uiAccess 高 z-band 权限。 }
procedure StartServer;
var
  ErrorCode: Integer;
begin
  ShellExecAsOriginalUser(
    '', ExpandConstant('{app}\qingjian-server.exe'), '', ExpandConstant('{app}'),
    SW_SHOWNORMAL, ewNoWait, ErrorCode);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    DeleteLegacyLogonTask;
    DeleteStaleDlls;
    { 静默安装（升级脚本、企业推送）没有完成页，在这里起；不然 PrepareToInstall 杀掉的 Server
      要等到下次登录才回来，这中间输入法在每个应用里都连不上。 }
    if WizardSilent then
      StartServer;
  end;
end;

{ 有向导时在完成页点 Finish 之后起，免得装到一半就冒出状态条。 }
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpFinished) and (not WizardSilent) then
    StartServer;
end;
