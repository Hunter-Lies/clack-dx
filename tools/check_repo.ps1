# 发布前的仓库自检：确认该上传的都上传了、不该上传的都没上传。
#
# 一次检查三件事：
#   1) 被跟踪的文件里没有本机痕迹（绝对路径、用户名、代理地址、token 样式字符串）
#   2) 被跟踪的文件里没有微软二进制（DX 那批 dll 要么内嵌进 exe、要么在 _work\ 下）
#   3) 仓库体积正常（源码应该只有几百 KB）
#
# 用法：.\tools\check_repo.ps1
[CmdletBinding()]
param([string]$Root = (Split-Path -Parent $PSScriptRoot))

$ErrorActionPreference = 'Stop'

# 不许出现在仓库里的字符串。前面几个是本机路径和用户名，后面是密钥样式。
$forbidden = @(
    'C:\Users\cheng'          # 开发机用户名
    'E:\HUNTER'               # 开发机项目路径
    '127.0.0.1:7897'          # 本机代理
    'gho_'                    # GitHub token 前缀
    'ghp_'
    'github_pat_'
)

# 不该被跟踪的文件类型（微软二进制 / 构建产物）
$badExt = @('.exe', '.msi', '.cab', '.msp', '.dll', '.ocx', '.msm', '.msu', '.zip', '.7z')

# 这个脚本自己就写着上面那些特征串，扫描时跳过它，否则永远报自己
$selfSkip = @('tools/check_repo.ps1')

Push-Location $Root
try {
    $files = git ls-files
    if (-not $files) { throw 'git ls-files 什么都没有，确认在仓库根目录跑' }

    $problems = @()

    # 1) 本机痕迹
    foreach ($rel in $files) {
        if (-not (Test-Path $rel -PathType Leaf)) { continue }
        if ($selfSkip -contains ($rel -replace '\\', '/')) { continue }
        foreach ($needle in $forbidden) {
            $hit = Select-String -LiteralPath $rel -Pattern ([regex]::Escape($needle)) -SimpleMatch -ErrorAction SilentlyContinue
            if ($hit) { $problems += "本机痕迹 [$needle] → $rel :$($hit[0].LineNumber)" }
        }
    }

    # 2) 二进制
    foreach ($rel in $files) {
        $ext = [IO.Path]::GetExtension($rel).ToLower()
        if ($badExt -contains $ext) { $problems += "不该跟踪的二进制 → $rel" }
    }

    # 3) 体积
    $bytes = 0
    foreach ($rel in $files) { if (Test-Path $rel -PathType Leaf) { $bytes += (Get-Item $rel).Length } }
    $mb = $bytes / 1MB

    Write-Host ("被跟踪文件：{0} 个，合计 {1:N2} MB`n" -f $files.Count, $mb)

    if ($problems) {
        Write-Host '发现问题：' -ForegroundColor Red
        $problems | ForEach-Object { Write-Host "  x $_" -ForegroundColor Red }
    }
    else {
        Write-Host '✓ 没有本机痕迹、没有二进制、体积正常' -ForegroundColor Green
    }

    if ($mb -gt 1) {
        Write-Host ("  x 体积偏大（{0:N2} MB），源码仓库应该只有几百 KB" -f $mb) -ForegroundColor Red
        exit 1
    }
    if ($problems) { exit 1 }
}
finally { Pop-Location }

exit 0
