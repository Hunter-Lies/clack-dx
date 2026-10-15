# 出一个单文件 exe：把 payload 打成 zip 内嵌进程序，再 build（不打安装包）。
#
# 用法：
#   .\make_release.ps1                    产物放 out\
#   .\make_release.ps1 -OutDir D:\某个目录
[CmdletBinding()]
param(
    # 这个脚本就在仓库根目录，所以 Root 是它自己所在目录（tools\ 下的脚本才要往上一层）
    [string]$Root = $PSScriptRoot,
    [string]$OutDir
)

$ErrorActionPreference = 'Stop'
if (-not $OutDir) { $OutDir = Join-Path $Root 'out' }

$SevenZip = @(
    'C:\Program Files\7-Zip\7z.exe'
    'C:\Program Files (x86)\7-Zip\7z.exe'
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $SevenZip) { throw '需要 7-Zip：装一个，或把 7z.exe 放到 PATH 里' }

$Ui = Join-Path $Root 'ui'
$Payload = Join-Path $Root '_work\payload'
$Zip = Join-Path $Ui 'src-tauri\payload.zip'

if (-not (Test-Path (Join-Path $Payload 'manifest.csv'))) {
    throw "payload 还没准备好，先跑 tools\stage_payload.ps1"
}

# 1) payload -> zip（条目以组件目录开头，运行时解出来就是 payload 根）
if (Test-Path $Zip) { Remove-Item -LiteralPath $Zip -Force }
Push-Location $Payload
try { & $SevenZip a -tzip -mx=5 $Zip '*' | Out-Null }
finally { Pop-Location }
Write-Host ("payload.zip  {0:N1} MB" -f ((Get-Item $Zip).Length / 1MB))

# 2) build（--no-bundle：只要 exe，不要 NSIS 安装包）
Push-Location $Ui
try { & pnpm tauri build --no-bundle }
finally { Pop-Location }

# 3) 产物挪到 out\
New-Item -ItemType Directory -Force $OutDir | Out-Null
$exe = Join-Path $Ui 'src-tauri\target\release\clackdx.exe'
$dst = Join-Path $OutDir '咔哒DX-DirectX修复.exe'
Copy-Item -LiteralPath $exe -Destination $dst -Force

# Release 附件用纯 ASCII 名字，版本号从 tauri.conf.json 里读，免得两处对不上
$version = (Get-Content (Join-Path $Ui 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$ascii = Join-Path $OutDir ("ClackDX-{0}-x64.exe" -f $version)
Copy-Item -LiteralPath $exe -Destination $ascii -Force

Write-Host ("`n单文件 exe: {0}  ({1:N1} MB)" -f $dst, ((Get-Item $dst).Length / 1MB))
Write-Host ("发布用:     {0}" -f $ascii)

# 4) 自检：这个 exe 自己不能依赖 VC++ 运行库，否则在缺库的机器上起不来（那就成了先有鸡还是先有蛋）
& (Join-Path $Root 'tools\check_deps.ps1') -Exe $dst
if ($LASTEXITCODE -ne 0) { throw 'exe 依赖了 VC++ 运行库，别发出去' }
