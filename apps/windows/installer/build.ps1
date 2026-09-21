<#
.SYNOPSIS
    在 Windows 上打字在安装包：release 构建 TSF、Server、语音 Worker 与设置程序，再用 Inno Setup 编 qingjian.iss。
    设置程序缺省用 egui 那份（见 -WinUiSettings）。
.DESCRIPTION
    在编译机（MSVC 工具链 + Inno Setup）上跑。步骤：
      1) cargo build --release 出 DLL / Server / 语音 Worker / 设置程序，再单独编一份 32 位 DLL；
      2) 从 apps\windows\server\Cargo.toml 读版本号（-dev 版接 git 短哈希）；
      3) 找 ISCC.exe（PATH 或常见安装位置）；
      4) iscc /DAppVersion=<版本> 编脚本，成品在 target\installer\Zizai-<版本>-Setup.exe。
    随包数据（.qj / .tsv）直接由 .iss 从仓库 data\generated 与 assets 里取，不另建暂存目录；
    确保打包前 data\generated 里的 .qj 是最新的（bundle 流程见仓库 CLAUDE.md）。
    **缺省不签名、不开 uiAccess**，打出来的包任何机器都能起；uiAccess 跟着 -Sign 走，不用手设 QINGJIAN_UIACCESS。
.PARAMETER SkipBuild
    跳过 cargo build（数据或 .iss 改了、二进制没变时重编安装包用）。与 -PackageOnly 二选一即可。
.PARAMETER NoPackage
    只跑 cargo build，不打包。CI 签名流程用：构建 → 送 SignPath 签回 → -PackageOnly -PreSigned 打包。
.PARAMETER PackageOnly
    跳过 cargo build，直接用 target\ 里已有的产物打包（须先跑过一次构建）。与 -SkipBuild 的区别：
    是「分段打包」语义，CI 用它；本地重编安装包继续用 -SkipBuild。
.PARAMETER PreSigned
    产物已被 SignPath 在 CI 里签回（docs/design/code-signing.md）：不剥签名、打包前逐个校验签名有效，
    别让坏签名静默进安装包。与 -Sign（本地自签）互斥。
.PARAMETER Sign
    自签产物（sign-local.ps1）并开 uiAccess（候选窗才能盖过商店 / 任务栏搜索）。uiAccess=true 的 exe 要本机受信任的签名
    才准启动，自签证书只有编译机信任——所以 -Sign 只用于本机真机测，对外分发的包不加此开关：不签名、关 uiAccess，
    候选窗在那几个系统界面里会被盖住，但任何机器都能起。
    不加时还会剥掉上一次 -Sign 残留在 target\ 里的签名，两种包在同一台机器上交替打不会串。
.PARAMETER WinUiSettings
    设置程序换回 WinUI 3 那一份（qingjian-settings.exe + 118 项 / 56 MB 自包含 Windows App Runtime）。
    **缺省是 egui 那份**（qingjian-settings-egui.exe，按正式名字装，不带运行时）；这个开关只在要对比时用，
    成品另起名 Zizai-<版本>-winui-Setup.exe，不覆盖正式包。
.PARAMETER VoiceModelDir
    可选的 SenseVoice 模型目录，必须含 model.int8.onnx 与 tokens.txt。仅用于本地测试包；
    正式发布仍不默认携带模型，避免把模型许可与源码许可混为一谈。
.PARAMETER PunctuationModelDir
    可选的本地标点恢复模型目录，必须含 model.int8.onnx（sherpa-onnx 的
    punct-ct-transformer-zh-en …-int8，见 https://github.com/k2-fsa/sherpa-onnx/releases/tag/punctuation-models）。
    与 -VoiceModelDir 一样仅用于本地测试包；两者常一起用：带标点模型不带 SenseVoice 识别无法工作，意义不大。
.PARAMETER HrDir
    可选的同音词替换资源目录，必须含 lexicon.txt 与 replace.fst（sherpa-onnx 同音词替换）。
    装到 {app}\data\voice\hr；hr_lexicon / hr_rule_fsts 在配置里填相对这个安装根的路径。
.PARAMETER BuildLabel
    可选的阶段测试标识（仅允许字母、数字、点和短横线），追加到版本号与安装包文件名。
    同一阶段重新打包时递增 r1、r2，避免不同二进制共用文件名。
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$Sign,
    [switch]$WinUiSettings,
    [switch]$NoPackage,
    [switch]$PackageOnly,
    [switch]$PreSigned,
    [string]$VoiceModelDir,
    [string]$PunctuationModelDir,
    [string]$HrDir,
    [string]$BuildLabel
)

$ErrorActionPreference = 'Stop'

if ($NoPackage -and $PackageOnly) { throw '-NoPackage 与 -PackageOnly 互斥（一个只构建、一个只打包）' }
if ($PreSigned -and $Sign) { throw '-PreSigned 与 -Sign 互斥（签回产物与本地自签二选一）' }

# 剥掉产物上残留的签名（见调用处）。没签名的文件一个都不动，所以不签名的构建可以每次无脑调。
function Remove-Signatures {
    param([Parameter(Mandatory)][string[]]$Path)

    $signed = @($Path | Where-Object { (Get-AuthenticodeSignature -LiteralPath $_).Status -ne 'NotSigned' })
    if ($signed.Count -eq 0) { return }

    # 只在真有残留时才要 signtool：没装 Windows SDK 也能打不签名的包，除非上次签过。
    $signtool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe' -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $signtool) {
        throw "产物上有上次 -Sign 留下的签名（$($signed.Count) 个）但找不到 signtool.exe 剥不掉：装 Windows SDK，或删掉 target\release 重新构建"
    }
    foreach ($f in $signed) {
        & $signtool.FullName remove /s $f | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "剥离签名失败：$f（退出码 $LASTEXITCODE）" }
    }
    Write-Host "剥掉 $($signed.Count) 个产物上残留的自签名" -ForegroundColor Yellow
}

# 仓库根：本脚本在 apps\windows\installer 下，往上三层是 ime\。
$Repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$Iss  = Join-Path $PSScriptRoot 'qingjian.iss'

# uiAccess 跟着 -Sign 走（server\build.rs 读这个变量，改了会自动重编 Server）；理由见 -Sign 的说明。
$env:QINGJIAN_UIACCESS = if ($Sign) { '1' } else { '0' }
if ($Sign) {
    Write-Host 'uiAccess=1（-Sign：仅本机真机测，别用于对外分发）' -ForegroundColor Yellow
} else {
    Write-Host 'uiAccess=0（对外分发：Server 任何机器都能起；候选窗在商店 / 任务栏搜索里可能被盖）' -ForegroundColor Cyan
}

# 1) 构建 Windows 产物。-PackageOnly 假定产物已在 target\（CI 签名分段流程），跳过。
if (-not $SkipBuild -and -not $PackageOnly) {
    Write-Host '构建 release 产物…' -ForegroundColor Cyan
    Push-Location $Repo
    try {
        $settingsPackage = if ($WinUiSettings) { 'qingjian-windows-settings' } else { 'qingjian-windows-settings-egui' }
        cargo build --release --locked -p qingjian-windows-server -p qingjian-windows-voice-worker -p qingjian-windows-tsf -p $settingsPackage
        if ($LASTEXITCODE -ne 0) { throw "cargo build 失败（退出码 $LASTEXITCODE）" }
        cargo build --release --locked -p qingjian-windows-tsf --target i686-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { throw "32 位 DLL cargo build 失败（退出码 $LASTEXITCODE）" }
    } finally { Pop-Location }
}

# 缺一个产物就早报错。
$targets = @(
    'release\qingjian_tsf.dll',
    'i686-pc-windows-msvc\release\qingjian_tsf.dll',
    'release\qingjian-server.exe',
    'release\qingjian-voice-worker.exe',
    $(if ($WinUiSettings) { 'release\qingjian-settings.exe' } else { 'release\qingjian-settings-egui.exe' })
)
foreach ($t in $targets) {
    $p = Join-Path $Repo "target\$t"
    if (-not (Test-Path $p)) { throw "缺产物 $p，先跑一次不带 -SkipBuild 的构建" }
}

# -NoPackage：只构建，后面是打包段（CI 签名流程构建与打包之间要插 SignPath 签回）。
if ($NoPackage) {
    Write-Host '构建完成（-NoPackage：不打包）' -ForegroundColor Green
    return
}

# 1.2) 自包含 Windows App Runtime（只有 -WinUiSettings 要）：WinUI 那份设置程序不依赖机器上装的
#      框架包（Windows 10 上框架依赖的引导用不了，见 apps\windows\settings\build.rs）。cargo 构建时
#      windows-reactor-setup 已按清单把运行时铺到 target\release\，这里挑进暂存目录；
#      target\release 里还有 deps\ 之类的中间产物，不能整个目录装。
$runtimeStage = Join-Path $Repo 'target\installer\settings-runtime'
$runtimeList  = Join-Path $PSScriptRoot 'settings-runtime.txt'
# 清单是 UTF-8 且带中文注释：不指定编码时 PowerShell 5.1 按 GBK 读，注释末尾的字节会吞掉换行，紧跟其后的一项被当成注释漏掉。
$wanted = Get-Content $runtimeList -Encoding UTF8 | Where-Object { $_ -and -not $_.StartsWith('#') } | ForEach-Object { $_.Trim() }

# 1.3) 从上一版 WinUI 包升级到 egui 包时，{app} 里会留着那 118 项运行时（56 MB）白占地方——Inno 只管自己装过的
#      文件，删不掉「这一版不再装」的。按同一份清单生成一段 [InstallDelete] 给 qingjian.iss #include。
#      装 WinUI 版时生成空片段：那些文件正是要装的。
$retireIss = Join-Path $Repo 'target\installer\retire-runtime.iss'
New-Item -ItemType Directory -Path (Split-Path $retireIss) -Force | Out-Null
# 片段里只写 ASCII：下面按 ASCII 写出（文件名本来就都是 ASCII），中文注释会被压成问号。
$retireLines = @('; Generated by build.ps1 from settings-runtime.txt -- do not edit.')
if (-not $WinUiSettings) {
    $retireLines += $wanted | ForEach-Object { "Type: filesandordirs; Name: `"{app}\$_`"" }
}
Set-Content -LiteralPath $retireIss -Value $retireLines -Encoding ASCII

if (-not $WinUiSettings) {
    Write-Host "egui 设置程序：不装 Windows App Runtime（少 $($wanted.Count) 项 / 56 MB），升级时顺手删掉上一版留下的" -ForegroundColor Cyan
} else {
if (Test-Path $runtimeStage) { Remove-Item $runtimeStage -Recurse -Force }
New-Item -ItemType Directory -Path $runtimeStage -Force | Out-Null
$missing = @()
foreach ($name in $wanted) {
    $src = Join-Path $Repo "target\release\$name"
    if (Test-Path $src) {
        Copy-Item $src -Destination (Join-Path $runtimeStage $name) -Recurse -Force
    } else {
        $missing += $name
    }
}
# 缺文件说明自包含运行时没铺成功（build.rs 下载 NuGet 或解 MSIX 失败），早报错，别打出个跑不起来的包。
if ($missing.Count -gt 0) { throw "自包含 Windows App Runtime 缺 $($missing.Count) 项：$($missing -join ', ')" }
Write-Host "自包含运行时 $($wanted.Count) 项 → target\installer\settings-runtime" -ForegroundColor Cyan
}

# 1.5) 签名（必须在 iscc 打包前：Inno 把已签的文件原样拷进安装包）。
$binaries = $targets | ForEach-Object { Join-Path $Repo "target\$_" }
if ($Sign) {
    Write-Host '自签产物（uiAccess 要求 Server 代码签名）…' -ForegroundColor Cyan
    & (Join-Path $PSScriptRoot 'sign-local.ps1') -Path $binaries
} elseif ($PreSigned) {
    # CI：产物刚从 SignPath 签回。不剥签名，但逐个校验有效，别静默把签坏的文件打进包。
    foreach ($f in $binaries) {
        $status = (Get-AuthenticodeSignature -LiteralPath $f).Status
        if ($status -ne 'Valid') { throw "产物签名无效（$status）：$f（检查 SignPath 签回环节）" }
    }
    Write-Host "产物已由 SignPath 签回（-PreSigned，$($binaries.Count) 个签名有效）" -ForegroundColor Cyan
} else {
    # 上一次 -Sign 留下的自签名还粘在 target\ 里（代码没变就不重新链接，签名跟着留着），
    # 会被原样打进包——到别人机器上是「证书链不受信任」，观感差也更容易触杀软。剥掉。
    Remove-Signatures -Path $binaries
}

# 2) 从 server 的 Cargo.toml 读版本（apps\* 各自写死版本，不跟 workspace）。
$cargoToml = Get-Content (Join-Path $Repo 'apps\windows\server\Cargo.toml')
$verLine = $cargoToml | Where-Object { $_ -match '^\s*version\s*=\s*"(.+)"' } | Select-Object -First 1
if (-not ($verLine -match '"(.+)"')) { throw '在 server\Cargo.toml 里没找到 version' }
$Version = $Matches[1]
# 开发版接 git 短哈希（0.1.3-dev-1a2b3c4，脏加 +），有 bug 能定位到哪次改动；发版提交去掉 -dev 就不接。
if ($Version.EndsWith('-dev')) {
    Push-Location $Repo
    try {
        $rev = (git rev-parse --short HEAD 2>$null)
        if ($LASTEXITCODE -eq 0 -and $rev) {
            # 某些开发机的全局 core.excludesFile 指向已不可读路径；版本脏标记不应因此让整个打包失败。
            $emptyExclude = Join-Path $Repo 'target\installer\empty-git-excludes'
            if (-not (Test-Path $emptyExclude)) { Set-Content -LiteralPath $emptyExclude -Value '' -NoNewline }
            if (git -c "core.excludesFile=$emptyExclude" status --porcelain 2>$null) { $rev = "$rev+" }
            $Version = "$Version-$rev"
        }
    } finally { Pop-Location }
}
if ($BuildLabel) {
    if ($BuildLabel -notmatch '^[A-Za-z0-9][A-Za-z0-9.-]*$') {
        throw '-BuildLabel 只允许字母、数字、点和短横线，且必须以字母或数字开头'
    }
    $Version = "$Version-$BuildLabel"
}
# Inno 的 VersionInfoVersion 只认数字：去掉 -alpha.1 这类预发布后缀。
$VersionNumeric = $Version -replace '-.*$', ''
Write-Host "版本 $Version" -ForegroundColor Cyan

# 2.5) 随包数据：iscc 只会报第一张缺的表，这里先把要的都列出来一次报清。
#      本机的 data\generated 是生成出来的（见 assets\lexicon\QINGJIAN.md），CI 从 data Release 下载（见本目录 README）。
$productData = @(
    'data\generated\dict.qj',
    'data\generated\lm.qj',
    'data\generated\english.tsv'
)
$missingData = @($productData | Where-Object { -not (Test-Path (Join-Path $Repo $_)) })
$domainDicts = @(Get-ChildItem (Join-Path $Repo 'data\generated\dicts\*.qj') -ErrorAction SilentlyContinue)
if ($domainDicts.Count -eq 0) { $missingData += 'data\generated\dicts\*.qj（领域词库）' }
if ($missingData.Count -gt 0) {
    throw "缺随包数据：$($missingData -join '、')。本机生成或从 data Release 取，见 apps\windows\installer\README.md"
}
Write-Host "随包数据齐全（含 $($domainDicts.Count) 本领域词库）" -ForegroundColor Cyan

if ($VoiceModelDir) {
    $VoiceModelDir = (Resolve-Path -LiteralPath $VoiceModelDir).Path
    $missingVoice = @('model.int8.onnx', 'tokens.txt') | Where-Object {
        -not (Test-Path -LiteralPath (Join-Path $VoiceModelDir $_))
    }
    if ($missingVoice.Count -gt 0) {
        throw "语音模型目录缺文件：$($missingVoice -join '、')（$VoiceModelDir）"
    }
    Write-Host "本地测试包携带 SenseVoice 模型：$VoiceModelDir" -ForegroundColor Yellow
}

if ($PunctuationModelDir) {
    $PunctuationModelDir = (Resolve-Path -LiteralPath $PunctuationModelDir).Path
    if (-not (Test-Path -LiteralPath (Join-Path $PunctuationModelDir 'model.int8.onnx'))) {
        throw "标点模型目录缺 model.int8.onnx（$PunctuationModelDir）"
    }
    Write-Host "本地测试包携带标点恢复模型：$PunctuationModelDir" -ForegroundColor Yellow
}

if ($HrDir) {
    $HrDir = (Resolve-Path -LiteralPath $HrDir).Path
    $missingHr = @('lexicon.txt', 'replace.fst') | Where-Object {
        -not (Test-Path -LiteralPath (Join-Path $HrDir $_))
    }
    if ($missingHr.Count -gt 0) {
        throw "同音词替换目录缺文件：$($missingHr -join '、')（$HrDir）"
    }
    Write-Host "本地测试包携带同音词替换资源：$HrDir" -ForegroundColor Yellow
}

# 3) 找 ISCC.exe：先 Program Files 与每用户安装的 7（与开发机同版本；CI 镜像 PATH 上自带 Chocolatey 的 6，不带简中翻译，不能让它抢先），
#    再 PATH，最后 6。QINGJIAN_ISCC 环境变量可直接指定。
$iscc = $env:QINGJIAN_ISCC
if (-not $iscc) {
    # 7 装成每用户时（非管理员安装）落在 %LOCALAPPDATA%\Programs，目录是平铺的，没有 app\ 一层。
    $candidates = @(
        "${env:ProgramFiles}\Inno Setup 7\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 7\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe"
    )
    $iscc = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $iscc) { $iscc = (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source }
if (-not $iscc) {
    # 6 的每用户安装多一层 app\（与 7 不同）。
    $candidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\app\ISCC.exe"
    )
    $iscc = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $iscc) { throw '找不到 ISCC.exe：装 Inno Setup 7 或用 QINGJIAN_ISCC 指定' }
Write-Host "用 $iscc" -ForegroundColor Cyan

# 4) 编安装包。
$isccArgs = @("/DAppVersion=$Version", "/DAppVersionNumeric=$VersionNumeric")
if ($WinUiSettings) { $isccArgs += '/DWinUiSettings=1' }
if ($VoiceModelDir) { $isccArgs += "/DVoiceModelDir=$VoiceModelDir" }
if ($PunctuationModelDir) { $isccArgs += "/DPunctModelDir=$PunctuationModelDir" }
if ($HrDir) { $isccArgs += "/DHrModelDir=$HrDir" }
& $iscc @isccArgs $Iss
if ($LASTEXITCODE -ne 0) { throw "iscc 失败（退出码 $LASTEXITCODE）" }

$suffix = if ($WinUiSettings) { '-winui' } else { '' }
$out = Join-Path $Repo "target\installer\Zizai-$Version$suffix-Setup.exe"
Write-Host "完成：$out" -ForegroundColor Green
