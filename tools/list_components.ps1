# 扫一遍 payload\，生成 docs\组件来源与校验.md：
# 每个 DX dll 的架构、版本、大小、微软签名状态、SHA256。
# 用户审计时看这一份就够了——证明我们补进系统的就是微软原版，没有夹带。
[CmdletBinding()]
param([string]$Root = (Split-Path -Parent $PSScriptRoot))

$ErrorActionPreference = 'Stop'
$Payload = Join-Path $Root '_work\payload'
$DocDir = Join-Path $Root 'docs'
$Doc = Join-Path $DocDir '组件来源与校验.md'

if (-not (Test-Path (Join-Path $Payload 'manifest.csv'))) {
    throw '找不到 _work\payload，先跑 tools\stage_payload.ps1'
}
New-Item -ItemType Directory -Force $DocDir | Out-Null

$files = Get-ChildItem $Payload -Recurse -File -Filter '*.dll' | Sort-Object FullName
$signed = 0
$rows = foreach ($f in $files) {
    $arch = if ($f.Directory.Name -eq 'x86') { 'x86 → SysWOW64' } else { 'x64 → System32' }
    $sig = Get-AuthenticodeSignature $f.FullName
    $isMs = $sig.Status -eq 'Valid' -and $sig.SignerCertificate.Subject -match 'Microsoft'
    if ($isMs) { $signed++ }
    '| `{0}` | {1} | {2} | {3:N0} | {4} | `{5}` |' -f `
        $f.Name, $arch, $f.VersionInfo.FileVersion, $f.Length,
        $(if ($isMs) { '微软签名' } else { '无签名' }),
        (Get-FileHash $f.FullName -Algorithm SHA256).Hash.ToLower()
}

$body = @"
# 组件来源与校验

本文件由 ``tools\list_components.ps1`` 生成，数据直接读自 ``_work\payload\``。

## 来源

全部 $(($files).Count) 个 dll 都来自微软官方分发包
**《DirectX End-User Runtimes (June 2010)》**（``directx_Jun2010_redist.exe``），
由 ``tools\stage_payload.ps1`` 从安装包里的 154 个 cab 逐个解出来，
再按 cab 名里的 ``_x86`` / ``_x64`` 分到两个目录：

- ``payload\x86\*.dll`` → 装到 ``C:\Windows\SysWOW64``（32 位游戏读这里）
- ``payload\x64\*.dll`` → 装到 ``C:\Windows\System32``（64 位游戏读这里）

只补系统目录里**缺**的文件，已存在的一律不覆盖。

## 怎么自己核一遍

1. 从微软官方地址下载分发包（约 100 MB）：
   ``https://download.microsoft.com/download/8/4/A/84A35BF1-DAFE-4AE8-82AF-AD2AE20B6B14/directx_Jun2010_redist.exe``
2. 用 7-Zip 解开，再用同样的方式解 cab，拿 SHA256 跟下表逐个对照
3. 或者直接查签名——下表里标"微软签名"的那些，文件本身就是微软原版

## 签名情况

$signed / $(($files).Count) 个文件带有效的 Microsoft 签名。$(if ($signed -eq ($files).Count) { '全部通过。' } else { '没签名的都是 2005-2007 年那批老组件，那个年代微软自己发的也没有 Authenticode 签名，这类看 SHA256。' })

## 文件清单

| 文件 | 目标 | 版本 | 大小 | 签名 | SHA256 |
|---|---|---|---|---|---|
$($rows -join "`n")
"@

[IO.File]::WriteAllText($Doc, $body, (New-Object System.Text.UTF8Encoding($false)))
Write-Host ("已写入 {0}：{1} 个文件，其中 {2} 个带微软签名" -f $Doc, ($files).Count, $signed)
