# 把微软官方 DirectX 分发包（Jun2010 redist）里的 DLL 抽出来，做成 payload。
#
# 为什么要自己抽：DirectX 修复工具那类软件带的是 2019/2021 年的老运行库（14.29），
# 顺手"修复"一遍会把最新版顶掉。这个工具只补 DirectX 那几个 dll，不碰 VC++ 运行库。
#
# 源包按顺序找：
#   1) -Source 指定的目录（含一堆 *.cab）
#   2) _work\dxredist\
#   3) E:\GTAV\Redistributables\DirectX   <- GTA V 自带的就是微软官方那份，154 个 cab
#   4) 都没有就从微软官方下载 directx_Jun2010_redist.exe 再拆
#
# 产物：_work\payload\{x86,x64}\*.dll + manifest.csv
#   x86 -> SysWOW64（32 位游戏读这里）    x64 -> System32（64 位游戏读这里）
#
# 用法：.\tools\stage_payload.ps1
[CmdletBinding()]
param(
    [string]$Root    = (Split-Path -Parent $PSScriptRoot),
    [string]$Source,
    [string]$OutDir,
    [switch]$SkipDownload
)

$ErrorActionPreference = 'Stop'

$Work = Join-Path $Root '_work'
$Dl   = Join-Path $Work 'download'
if (-not $OutDir) { $OutDir = Join-Path $Work 'payload' }

# 微软官方：《DirectX End-User Runtimes (June 2010)》完整分发包
$DxUrl = 'https://download.microsoft.com/download/8/4/A/84A35BF1-DAFE-4AE8-82AF-AD2AE20B6B14/directx_Jun2010_redist.exe'

$SevenZip = @(
    'C:\Program Files\7-Zip\7z.exe'
    'C:\Program Files (x86)\7-Zip\7z.exe'
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $SevenZip) { throw '需要 7-Zip：装一个，或把 7z.exe 放到 PATH 里' }

New-Item -ItemType Directory -Force $Work | Out-Null

# ---------- 1) 找到源包 ----------
$candidates = @()
if ($Source) { $candidates += $Source }
$candidates += @(
    (Join-Path $Work 'dxredist')
    'E:\GTAV\Redistributables\DirectX'
)

$srcDir = $null
foreach ($c in $candidates) {
    if ((Test-Path $c) -and (Get-ChildItem $c -Filter '*.cab' -File -ErrorAction SilentlyContinue)) {
        $srcDir = $c
        break
    }
}

if (-not $srcDir -and -not $SkipDownload) {
    $exe = Join-Path $Dl 'directx_Jun2010_redist.exe'
    if (-not (Test-Path $exe)) {
        New-Item -ItemType Directory -Force $Dl | Out-Null
        Write-Host '下载微软官方 DirectX 分发包（约 100 MB）...'
        Invoke-WebRequest -Uri $DxUrl -OutFile $exe -UseBasicParsing
    }
    $srcDir = Join-Path $Work 'dxredist'
    if (-not (Test-Path (Join-Path $srcDir 'DXSETUP.exe'))) {
        New-Item -ItemType Directory -Force $srcDir | Out-Null
        Write-Host '拆包...'
        & $SevenZip x $exe "-o$srcDir" -y | Out-Null
    }
}

if (-not $srcDir) {
    throw '找不到 DirectX 源包：把含 *.cab 的目录用 -Source 指过来，或让脚本自己从微软官方下载'
}

Write-Host "源包目录: $srcDir"
$cabs = Get-ChildItem $srcDir -Filter '*.cab' -File | Sort-Object Name
Write-Host ("cab 数量: {0}" -f $cabs.Count)

# ---------- 2) 逐个 cab 抽 dll ----------
$x86 = Join-Path $OutDir 'x86'
$x64 = Join-Path $OutDir 'x64'
foreach ($d in @($x86, $x64)) {
    if (Test-Path $d) { Remove-Item -LiteralPath $d -Recurse -Force }
    New-Item -ItemType Directory -Force $d | Out-Null
}
$tmp = Join-Path $Work 'extracted\dx'
if (Test-Path $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force }
New-Item -ItemType Directory -Force $tmp | Out-Null

$rows = New-Object System.Collections.Generic.List[object]
$skipped = New-Object System.Collections.Generic.List[string]

foreach ($cab in $cabs) {
    $arch = if ($cab.Name -match '(?i)_x86\.cab$') { 'x86' }
            elseif ($cab.Name -match '(?i)_x64\.cab$') { 'x64' }
            else { $null }
    if (-not $arch) { $skipped.Add($cab.Name); continue }   # dxupdate.cab 这类不是 DLL 载荷

    $one = Join-Path $tmp ([IO.Path]::GetFileNameWithoutExtension($cab.Name))
    New-Item -ItemType Directory -Force $one | Out-Null
    & $SevenZip x $cab.FullName "-o$one" -y | Out-Null

    $dst = if ($arch -eq 'x86') { $x86 } else { $x64 }
    foreach ($f in (Get-ChildItem $one -File -Filter '*.dll')) {
        # Apr2006 的 MDX 归档里带的是 Managed DirectX 托管程序集（Microsoft.DirectX.*.dll），
        # 那不是系统 DLL，扔进 System32 只会添乱；DirectX 修复工具也不收它们
        if ($f.Name -match '(?i)^microsoft\.directx') { continue }
        $target = Join-Path $dst $f.Name
        if (Test-Path $target) {
            # 同一个 DLL 出现在多个 cab 里时留版本高的那个
            $oldV = (Get-Item $target).VersionInfo.FileVersionRaw
            $newV = $f.VersionInfo.FileVersionRaw
            if ($oldV -and $newV -and $newV -le $oldV) { continue }
        }
        Copy-Item -LiteralPath $f.FullName -Destination $target -Force
    }
    Remove-Item -LiteralPath $one -Recurse -Force
}

# ---------- 3) 清单 ----------
foreach ($arch in 'x86', 'x64') {
    $dir = if ($arch -eq 'x86') { $x86 } else { $x64 }
    foreach ($f in (Get-ChildItem $dir -File -Filter '*.dll' | Sort-Object Name)) {
        $rows.Add([pscustomobject]@{
            文件 = $f.Name
            版本 = $f.VersionInfo.FileVersion
            架构 = $arch
            字节 = $f.Length
        })
    }
}
$csv = Join-Path $OutDir 'manifest.csv'
$rows | Export-Csv -LiteralPath $csv -NoTypeInformation -Encoding UTF8

$sizeX86 = (Get-ChildItem $x86 -File | Measure-Object Length -Sum).Sum
$sizeX64 = (Get-ChildItem $x64 -File | Measure-Object Length -Sum).Sum
$nX86 = (Get-ChildItem $x86 -File).Count
$nX64 = (Get-ChildItem $x64 -File).Count

Write-Host ''
Write-Host ("x86 {0} 个 / {1:N1} MB  -> SysWOW64" -f $nX86, ($sizeX86 / 1MB))
Write-Host ("x64 {0} 个 / {1:N1} MB  -> System32" -f $nX64, ($sizeX64 / 1MB))
Write-Host ("合计 {0:N1} MB，{1} 个文件，清单: {2}" -f (($sizeX86 + $sizeX64) / 1MB), ($nX86 + $nX64), $csv)
if ($skipped.Count) { Write-Host ("跳过的非 DLL 载荷: {0}" -f ($skipped -join ', ')) }

# 参考值：DirectX 修复工具 v4.1 的 Data\A / Data\B 各 93 个，减去 D3DCompiler_47.dll
# ——那个是 Win10 1709 之后的系统组件，官方 DX 分发包里本来就没有，我们也不塞。所以各 92 个。
if ($nX86 -ne 92 -or $nX64 -ne 92) {
    Write-Warning ("文件数和预期（各 92 个）不一致，源包可能不全：x86={0} x64={1}" -f $nX86, $nX64)
}
