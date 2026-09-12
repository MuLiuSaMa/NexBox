# ============================================================
# NexBox MSIX 侧载安装脚本（自动 UAC 提权版）
# 直接双击运行：自动请求管理员权限 -> 导入证书 -> 安装 MSIX
# ============================================================
$ErrorActionPreference = "Stop"

# --- 非管理员则自动提权重启自身 ---
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Start-Process powershell.exe -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`""
    exit
}

$WorkDir = "D:\NexBox\msix"
$PfxPath = "$WorkDir\NexBoxTest.pfx"
$CertPassword = "NexBoxTest"
$PublisherCN = "CN=30F199CB-855A-42F4-96A9-F43088B508BF"

try {
    # 1. 导入测试证书：TrustedPeople + 受信任的根（双库，签名链验证最稳）
    $securePwd = ConvertTo-SecureString -String $CertPassword -Force -AsPlainText
    foreach ($store in @("Cert:\LocalMachine\TrustedPeople", "Cert:\LocalMachine\Root")) {
        $existing = Get-ChildItem $store -ErrorAction SilentlyContinue | Where-Object { $_.Subject -eq $PublisherCN }
        if (-not $existing) {
            Import-PfxCertificate -FilePath $PfxPath -CertStoreLocation $store -Password $securePwd | Out-Null
            Write-Host "[OK] 证书已导入 $store"
        } else {
            Write-Host "[SKIP] 证书已存在于 $store"
        }
    }

    # 2. 先移除旧包（同版本重装会报 0x80073CFB），再安装最新 MSIX
    $msix = Get-ChildItem "$WorkDir\*.msix" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $msix) { throw "未找到 MSIX 文件，请先运行 build-msix.ps1" }
    Get-AppxPackage -Name "*NexBox*" -ErrorAction SilentlyContinue | Remove-AppxPackage -ErrorAction SilentlyContinue
    Add-AppxPackage -Path $msix.FullName
    Write-Host ""
    Write-Host "=== 安装成功：$($msix.Name) ===" -ForegroundColor Green
    Write-Host "开始菜单搜索『新境盒』启动；卸载：设置 > 应用"
} catch {
    Write-Host ""
    Write-Host "=== 安装失败 ===" -ForegroundColor Red
    Write-Host $_.Exception.Message
}
Write-Host ""
Read-Host "按回车键退出"
