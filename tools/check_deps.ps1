# 检查打出来的 exe 依赖什么——尤其是"它自己要不要 VC++ 运行库"。
#
# 一个"修运行库"的工具如果在缺运行库的机器上起不来，那就毫无意义。
# 所以每次发版前跑一下：这个脚本真的去解析 PE 导入表，不是全文搜字符串
#（exe 里内嵌着 payload，字符串里全是 d3dx9_43 之类的名字，搜不准）。
#
# 用法：.\tools\check_deps.ps1 [-Exe <路径>]
[CmdletBinding()]
param(
    [string]$Exe = (Join-Path (Split-Path -Parent $PSScriptRoot) 'ui\src-tauri\target\release\clackdx.exe')
)

$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Exe)) { throw "找不到 $Exe，先构建" }

function Get-PeImports {
    param([byte[]]$Data)

    $pe = [BitConverter]::ToInt32($Data, 0x3C)
    $nsec = [BitConverter]::ToUInt16($Data, $pe + 6)
    $optsz = [BitConverter]::ToUInt16($Data, $pe + 20)
    $magic = [BitConverter]::ToUInt16($Data, $pe + 24)
    $ddOff = $pe + 24 + $(if ($magic -eq 0x20B) { 112 } else { 96 })

    $secOff = $pe + 24 + $optsz
    $secs = @()
    for ($i = 0; $i -lt $nsec; $i++) {
        $o = $secOff + $i * 40
        $secs += [pscustomobject]@{
            VA    = [BitConverter]::ToUInt32($Data, $o + 12)
            VSize = [BitConverter]::ToUInt32($Data, $o + 8)
            Raw   = [BitConverter]::ToUInt32($Data, $o + 20)
            RSize = [BitConverter]::ToUInt32($Data, $o + 16)
        }
    }

    function Convert-RvaToOffset {
        param([int]$Rva)
        foreach ($s in $secs) {
            $span = [Math]::Max($s.VSize, $s.RSize)
            if ($Rva -ge $s.VA -and $Rva -lt ($s.VA + $span)) {
                return $s.Raw + ($Rva - $s.VA)
            }
        }
        return -1
    }

    function Read-CString {
        param([int]$Offset)
        $end = $Offset
        while ($Data[$end] -ne 0) { $end++ }
        return [Text.Encoding]::ASCII.GetString($Data, $Offset, $end - $Offset)
    }

    # 普通导入表 + 延迟导入表，两处都要看
    $results = @()
    foreach ($dirIndex in 1, 13) {
        $rva = [BitConverter]::ToUInt32($Data, $ddOff + $dirIndex * 8)
        if ($rva -eq 0) { continue }
        $off = Convert-RvaToOffset $rva
        if ($off -lt 0) { continue }

        # IMAGE_IMPORT_DESCRIPTOR 20 字节；延迟导入表结构不同但前 20 字节里偏移 12 也是名字 RVA
        while ($true) {
            $nameRva = [BitConverter]::ToUInt32($Data, $off + 12)
            if ($nameRva -eq 0) { break }
            $nameOff = Convert-RvaToOffset $nameRva
            if ($nameOff -ge 0) { $results += (Read-CString $nameOff) }
            $off += 20
        }
    }
    return ($results | Sort-Object -Unique)
}

# 这些是我们要装的运行库。exe 的导入表里出现任何一个，都说明它在没装运行库的机器上起不来。
$banned = @(
    'VCRUNTIME140.dll', 'VCRUNTIME140_1.dll', 'MSVCP140.dll',
    'MSVCP140_1.dll', 'MSVCP140_2.dll', 'CONCRT140.dll'
)

$bytes = [IO.File]::ReadAllBytes($Exe)
$imports = Get-PeImports $bytes

Write-Host ("检查 {0}  ({1:N1} MB)" -f (Split-Path -Leaf $Exe), ($bytes.Length / 1MB))
Write-Host "`n导入表里的依赖："
foreach ($name in $imports) {
    $tag = ''
    if ($banned -contains $name) { $tag = '   <== VC++ 运行库依赖！' }
    elseif ($name -like 'api-ms-win-crt-*') { $tag = '   (UCRT，Win10 起系统自带)' }
    Write-Host ("  {0}{1}" -f $name, $tag)
}

$bad = $imports | Where-Object { $banned -contains $_ }
Write-Host ''
if ($bad) {
    Write-Host ('❌ 依赖了 VC++ 运行库：{0}' -f ($bad -join ', ')) -ForegroundColor Red
    exit 1
}
Write-Host '✅ 没有 VC++ 运行库依赖，没装运行库的机器也能启动' -ForegroundColor Green
exit 0
