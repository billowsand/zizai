<#
.SYNOPSIS
    开发期自签：造 / 复用一张自签代码签名证书，装进本机受信任根，用它给指定文件签名。
.DESCRIPTION
    uiAccess=true 的 exe 只有在「代码签名 + 装在安全位置（Program Files）」时，系统才授予它
    升进 UIAccess 高 z-band 的权限（候选窗盖过微软商店 / 任务栏搜索）。已导入该证书的机器可通过本地信任校验，
    但自签证书没有公开 CA 信任或 SmartScreen 信誉；正式发布走 SignPath Foundation 签名。

    幂等：证书已在就复用，已装进根 / 受信任发布者就不重复装。装进 LocalMachine 存储需管理员。
    可选导出公钥 .cer 给内测目标机导入；不会导出私钥。
.PARAMETER Path
    要签名的文件（.exe / .dll），可多个。
.PARAMETER PublicCertificatePath
    可选：签名成功后导出的公钥证书路径（.cer），不包含私钥。
.PARAMETER CertSubject
    自签证书主题，缺省 "CN=Qingjian Dev CodeSign"。
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string[]]$Path,
    [string]$PublicCertificatePath,
    [string]$CertSubject = 'CN=Qingjian Dev CodeSign'
)

$ErrorActionPreference = 'Stop'

# 1) 找已有的自签代码签名证书（带私钥），没有就造一张（5 年，够开发用）。
$cert = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $CertSubject -and $_.HasPrivateKey } |
    Sort-Object NotAfter -Descending | Select-Object -First 1
if (-not $cert) {
    Write-Host "造自签代码签名证书 $CertSubject…" -ForegroundColor Cyan
    $cert = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $CertSubject `
        -KeyUsage DigitalSignature `
        -KeySpec Signature `
        -CertStoreLocation Cert:\CurrentUser\My `
        -NotAfter (Get-Date).AddYears(5)
}
$thumbprint = $cert.Thumbprint
Write-Host "证书指纹 $thumbprint" -ForegroundColor Cyan

# 2) 把公钥装进「本机受信任的根」+「受信任的发布者」，uiAccess 的签名校验才认这张自签证书。
#    已在就跳过（幂等）。
foreach ($store in @('Root', 'TrustedPublisher')) {
    $storePath = "Cert:\LocalMachine\$store"
    $exists = Get-ChildItem $storePath -ErrorAction SilentlyContinue |
        Where-Object { $_.Thumbprint -eq $thumbprint }
    if (-not $exists) {
        Write-Host "装证书进 LocalMachine\$store…" -ForegroundColor Cyan
        # 只导公钥（.cer），不带私钥。
        $tmp = Join-Path $env:TEMP 'qingjian-dev-codesign.cer'
        Export-Certificate -Cert $cert -FilePath $tmp -Type CERT | Out-Null
        Import-Certificate -FilePath $tmp -CertStoreLocation $storePath | Out-Null
        Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    }
}

# 3) 找 signtool（Windows SDK 里，取版本最新的一个）。
$signtool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe' -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { throw '在 Windows SDK 里找不到 signtool.exe' }

# 4) 签名（SHA256；开发自签不加时间戳，免网络依赖——证书 5 年内有效，够开发）。
foreach ($f in $Path) {
    if (-not (Test-Path -LiteralPath $f)) { throw "要签的文件不存在：$f" }
}
& $signtool.FullName sign /sha1 $thumbprint /fd sha256 /v $Path
if ($LASTEXITCODE -ne 0) { throw "signtool 签名失败（退出码 $LASTEXITCODE）" }
Write-Host "已签名 $($Path.Count) 个文件。" -ForegroundColor Green

if ($PublicCertificatePath) {
    $certificatePath = [System.IO.Path]::GetFullPath($PublicCertificatePath)
    $certificateDirectory = Split-Path -Parent $certificatePath
    if (-not (Test-Path -LiteralPath $certificateDirectory)) {
        New-Item -ItemType Directory -Path $certificateDirectory -Force | Out-Null
    }
    if (Test-Path -LiteralPath $certificatePath) {
        Remove-Item -LiteralPath $certificatePath -Force
    }
    Export-Certificate -Cert $cert -FilePath $certificatePath -Type CERT | Out-Null
    Write-Host "已导出公钥证书（不含私钥）：$certificatePath" -ForegroundColor Green
}
