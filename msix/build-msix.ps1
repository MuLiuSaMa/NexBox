# ============================================================
# NexBox MSIX 打包脚本
# 用途：把 payload.zip（完整应用文件集）打包成可侧载测试的 MSIX
# 流程：解包 payload -> 生成 AppxManifest + Assets -> makeappx -> 自签
# 重新发布前：先跑 installer\pack-payload.ps1 重建 payload.zip，再跑本脚本
# ============================================================
param(
    # 安装目录名由 Identity Name 决定：NexBox -> WindowsApps\NexBox_版本_x64_hash
    # Identity Name / Publisher 必须与 Partner Center 保留名和账号 Publisher ID 完全一致
    [string]$IdentityName = "MuLiuSaMa.NexBox",
    [string]$Publisher = "CN=30F199CB-855A-42F4-96A9-F43088B508BF",
    [string]$PublisherDisplayName = "MuLiu_SaMa",
    [string]$CertPassword = "NexBoxTest",
    # 最小能力模式：去掉 unvirtualizedResources（用于排查商店预处理 5001）
    [switch]$MinimalCaps,
    # 最小诊断模式：只保留主程序 exe（零中文文件名/零托管 dll/零资源，用于二分定位 5001）
    [switch]$Diagnostic,
    # 诊断模式下额外保留的条目，逗号分隔字符串（如 "monitor,resources"），用于增量二分
    [string]$OnlyAdd = "",
    # 诊断模式下从 staging 剔除的相对路径，逗号分隔（如 "resources/binaries/fxvad"）
    [string]$RemoveFromStaging = ""
)

$ErrorActionPreference = "Stop"
$Root = "D:\NexBox"
$WorkDir = "$Root\msix"
$Staging = "$WorkDir\staging"
$PayloadZip = "$Root\installer\src-tauri\payload.zip"
$IconsDir = "$Root\src-tauri\icons"

# --- SDK 工具定位 ---
$kitsBin = "C:\Program Files (x86)\Windows Kits\10\bin"
if (-not (Test-Path $kitsBin)) { throw "未找到 Windows SDK：$kitsBin" }
$sdkVer = Get-ChildItem $kitsBin -Directory | Sort-Object Name -Descending | Where-Object { Test-Path "$($_.FullName)\x64\makeappx.exe" } | Select-Object -First 1
if (-not $sdkVer) { throw "未找到 makeappx.exe" }
$MakeAppx = "$($sdkVer.FullName)\x64\makeappx.exe"
$Signtool = "$($sdkVer.FullName)\x64\signtool.exe"
Write-Host "[1/6] SDK 工具：$($sdkVer.Name)"

# --- 版本号：读 src-tauri/tauri.conf.json ---
$conf = Get-Content "$Root\src-tauri\tauri.conf.json" -Raw
if ($conf -match '"version"\s*:\s*"([\d\.]+)"') { $Version = $Matches[1] } else { throw "无法从 tauri.conf.json 读取版本号" }
# MSIX 版本号必须为四段式（如 9.5.3 -> 9.5.3.0）
$parts = $Version.Split('.')
while ($parts.Count -lt 4) { $parts += '0' }
$Version = $parts -join '.'
Write-Host "[2/6] 应用版本：$Version"

# --- 解包 payload 到 staging ---
if (Test-Path $Staging) { Remove-Item $Staging -Recurse -Force }
New-Item -ItemType Directory -Path $Staging -Force | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory($PayloadZip, $Staging)
# MSIX 由系统接管卸载，卸载器不进包
Remove-Item "$Staging\Uninstnexbox.exe" -Force -ErrorAction SilentlyContinue
# 诊断模式：剥离一切除主程序外的内容（定位服务端 5001 是否由包内文件触发）
if ($Diagnostic) {
    $tmpAll = "$WorkDir\staging_full"
    if (Test-Path $tmpAll) { Remove-Item $tmpAll -Recurse -Force }
    Copy-Item $Staging $tmpAll -Recurse -Force
    Get-ChildItem $Staging -Exclude "nexbox.exe" | Remove-Item -Recurse -Force
    foreach ($keep in ($OnlyAdd -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
        if (Test-Path "$tmpAll\$keep") { Copy-Item "$tmpAll\$keep" "$Staging\$keep" -Recurse -Force }
        else { Write-Host "  [WARN] 附加项不存在: $keep" }
    }
    Remove-Item $tmpAll -Recurse -Force
    Write-Host "  [DIAG] 基线=nexbox.exe, 额外保留: $($OnlyAdd -join ', ')"
}
# 通用剔除：从 staging 移除指定相对路径（正常/诊断模式均生效），如 icc-tools 等触发商店预处理 5001 的文件
foreach ($rm in ($RemoveFromStaging -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
    $rp = Join-Path $Staging $rm
    if (Test-Path $rp) { Remove-Item $rp -Recurse -Force; Write-Host "  [EXCLUDE] 已剔除: $rm" }
    else { Write-Host "  [WARN] 剔除目标不存在: $rm" }
}
Write-Host "[3/6] payload 解包完成：$((Get-ChildItem $Staging -Recurse -File | Measure-Object).Count) 个文件"

# --- Assets 图标 ---
$Assets = "$Staging\Assets"
New-Item -ItemType Directory -Path $Assets -Force | Out-Null
foreach ($icon in @("Square44x44Logo.png", "Square150x150Logo.png", "StoreLogo.png")) {
    Copy-Item "$IconsDir\$icon" "$Assets\$icon" -Force
}

# --- AppxManifest.xml ---
if ($MinimalCaps) {
    $capsBlock = @"
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
"@
} else {
    $capsBlock = @"
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
    <rescap:Capability Name="unvirtualizedResources" />
  </Capabilities>
"@
}

$manifest = @"
<?xml version="1.0" encoding="utf-8"?>
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities">
  <Identity Name="$IdentityName" Publisher="$Publisher" Version="$Version" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>新境盒</DisplayName>
    <PublisherDisplayName>$PublisherDisplayName</PublisherDisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Resources>
    <Resource Language="zh-cn" />
    <Resource Language="en-us" />
  </Resources>
  <Applications>
    <Application Id="NexBox" Executable="nexbox.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="新境盒" Description="NexBox 全能系统工具箱" BackgroundColor="transparent" Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png" />
    </Application>
  </Applications>
$capsBlock
</Package>
"@
[System.IO.File]::WriteAllText("$Staging\AppxManifest.xml", $manifest, (New-Object System.Text.UTF8Encoding($false)))
Write-Host "[4/6] AppxManifest.xml 已生成"

# --- makeappx 打包 ---
$suffix = ""; if ($MinimalCaps) { $suffix = "_mincaps" }; if ($Diagnostic) { $suffix = "_diag" + ($(if ($OnlyAdd.Count) { "-" + ($OnlyAdd -join "-") } else { "" })) }
$MsixOut = "$WorkDir\NexBox_${Version}_x64$suffix.msix"
if (Test-Path $MsixOut) { Remove-Item $MsixOut -Force }
& $MakeAppx pack /d $Staging /p $MsixOut /o 2>&1 | Out-Null
if (-not (Test-Path $MsixOut)) { throw "makeappx 打包失败" }
Write-Host ("[5/6] MSIX 已生成：{0}  ({1:N1} MB)" -f $MsixOut, ((Get-Item $MsixOut).Length / 1MB))

# --- 自签测试证书 + 签名（仅供本地侧载；商店提交不需要签名包） ---
$PfxPath = "$WorkDir\NexBoxTest.pfx"
$cert = Get-ChildItem "Cert:\CurrentUser\My" | Where-Object { $_.Subject -eq $Publisher -and $_.HasPrivateKey } | Sort-Object NotAfter -Descending | Select-Object -First 1
if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher -KeyUsage DigitalSignature `
        -FriendlyName "NexBox MSIX Test" -CertStoreLocation "Cert:\CurrentUser\My" `
        -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
    Write-Host "  已创建测试证书：$($cert.Thumbprint)"
}
$securePwd = ConvertTo-SecureString -String $CertPassword -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath $PfxPath -Password $securePwd | Out-Null
& $Signtool sign /fd SHA256 /f $PfxPath /p $CertPassword $MsixOut 2>&1 | Out-Null
$verify = & $Signtool verify /pa $MsixOut 2>&1
Write-Host "[6/6] 签名完成（signtool verify /pa: $($verify | Select-Object -Last 1)）"

Write-Host "`n=== 完成 ===" -ForegroundColor Green
Write-Host "MSIX : $MsixOut"
Write-Host "证书 : $PfxPath (密码 $CertPassword)"
Write-Host "侧载 : 以管理员运行 install-msix.ps1"
