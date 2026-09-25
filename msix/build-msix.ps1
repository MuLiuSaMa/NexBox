# ============================================================
# NexBox MSIX 打包脚本
# 用途：把 payload.zip（完整应用文件集）打包成可侧载测试的 MSIX
# 流程：解包 payload -> 生成 AppxManifest + Assets + resources.pri -> makeappx -> 自签
# 重新发布前：先跑 installer\pack-payload.ps1 重建 payload.zip，再跑本脚本
# 商店（Store）提交必须用商店版构建：先跑 npm run tauri:build:store（隐藏全部第三方软件获取入口），
# 再跑 installer\pack-payload.ps1 + 本脚本；普通安装版用 npm run tauri:build。
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
    # 通用剔除：从 staging 移除的相对路径，逗号分隔。默认剔除 icc-tools（触发微软商店审核/预处理不过）
    [string]$RemoveFromStaging = "resources/binaries/icc-tools"
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
# Assets 完全由本脚本重建（payload.zip 中不含 Assets）。每次打包重建 staging，
# 因此 unplated 变体必须在此处随包重新生成，不能只手工放进 staging。
$Assets = "$Staging\Assets"
New-Item -ItemType Directory -Path $Assets -Force | Out-Null
foreach ($icon in @("Square44x44Logo.png", "Square150x150Logo.png", "StoreLogo.png")) {
    Copy-Item "$IconsDir\$icon" "$Assets\$icon" -Force
}
# 生成 altform-unplated / altform-lightunplated 变体（修复开始菜单/任务栏图标
# 因缺少 unplated 资产被系统回退渲染为主题色底板（红底）的问题）。
# 源图用 icon.png（512x512，带 Alpha 通道）；基名与 Square44x44Logo 引用一致。
# PaddingPercent=100：本应用图标为实心圆角方块（full-bleed）设计，铺满画布，
# 任务栏原样显示（同 Chrome/Spotify），不留微软 glyph 图标约定的 20% 透明边距。
& "$WorkDir\gen-assets.ps1" -SourcePng "$IconsDir\icon.png" -OutDir $Assets -BaseName "Square44x44Logo" -PaddingPercent 100

# PRI 说明：必须用 makepri 生成 resources.pri，否则 Shell（任务栏）无法通过 MRT
# 限定符解析 targetsize-N / _altform-unplated 等变体（无 PRI 时平铺的限定符文件
# 不会被 Shell 枚举），任务栏会回退到不透明的 Square44x44Logo.png 并绘制主题色底板。
# PRI 只存文件路径索引，体积小；全量索引 staging 是 MSIX Packaging Tool 的标准做法。
$MakePri = "$($sdkVer.FullName)\x64\makepri.exe"
& $MakePri createconfig /cf "$WorkDir\priconfig.xml" /dq zh-cn /pv 10.0.0 /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makepri createconfig 失败 (exit=$LASTEXITCODE)" }
# 去掉 packaging 节（autoResourcePackage 拆分 Scale/Language/DXFL）：单包侧载不打包 bundle，
# 拆分出的 resources.scale-N.pri 卫星文件可能不被加载，全部并入单个 resources.pri 最稳妥
$cfg = Get-Content "$WorkDir\priconfig.xml" -Raw
$cfg = $cfg -replace '(?s)\s*<packaging>.*?</packaging>', ''
[System.IO.File]::WriteAllText("$WorkDir\priconfig.xml", $cfg, (New-Object System.Text.UTF8Encoding($false)))
& $MakePri new /pr $Staging /cf "$WorkDir\priconfig.xml" /of "$Staging\resources.pri" /o
if ($LASTEXITCODE -ne 0) { throw "makepri new 失败 (exit=$LASTEXITCODE)" }
Write-Host "  [PRI] resources.pri 已生成：$((Get-Item "$Staging\resources.pri").Length) 字节"

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
