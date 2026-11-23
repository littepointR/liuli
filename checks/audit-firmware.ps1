param(
    [Parameter(Mandatory = $true)][string]$Elf,
    [ValidateSet('Baseline', 'Demo', 'StartupProbe')][string]$Profile = 'Demo'
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $Elf)) {
    throw "ELF not found: $Elf"
}

$bytes = [System.IO.File]::ReadAllBytes((Resolve-Path $Elf).Path)
if ($bytes.Length -lt 52 -or $bytes[0] -ne 0x7F -or $bytes[1] -ne 0x45 -or $bytes[2] -ne 0x4C -or $bytes[3] -ne 0x46) {
    throw "Not a valid ELF binary: $Elf"
}
if ($bytes[4] -ne 1 -or $bytes[5] -ne 1) {
    throw "Expected 32-bit little-endian ELF: $Elf"
}

$e_entry     = [BitConverter]::ToUInt32($bytes, 24)
$e_phoff     = [BitConverter]::ToUInt32($bytes, 28)
$e_shoff     = [BitConverter]::ToUInt32($bytes, 32)
$e_phentsize = [BitConverter]::ToUInt16($bytes, 42)
$e_phnum     = [BitConverter]::ToUInt16($bytes, 44)
$e_shentsize = [BitConverter]::ToUInt16($bytes, 46)
$e_shnum     = [BitConverter]::ToUInt16($bytes, 48)
$e_shstrndx  = [BitConverter]::ToUInt16($bytes, 50)

function Read-CStr([byte[]]$buf, [int]$offset) {
    $end = $offset
    while ($end -lt $buf.Length -and $buf[$end] -ne 0) { $end++ }
    return [System.Text.Encoding]::ASCII.GetString($buf, $offset, $end - $offset)
}

$shstrHdr = $e_shoff + $e_shstrndx * $e_shentsize
$shstrOff = [BitConverter]::ToUInt32($bytes, $shstrHdr + 16)
$shstrSz  = [BitConverter]::ToUInt32($bytes, $shstrHdr + 20)
$shstrBuf = New-Object byte[] $shstrSz
[Array]::Copy($bytes, [int]$shstrOff, $shstrBuf, 0, [int]$shstrSz)

$sections = @()
$secByName = @{}
for ($i = 0; $i -lt $e_shnum; $i++) {
    $off = $e_shoff + $i * $e_shentsize
    $shNameOff = [BitConverter]::ToUInt32($bytes, $off + 0)
    $sec = [pscustomobject]@{
        Index   = $i
        Name    = Read-CStr $shstrBuf ([int]$shNameOff)
        Type    = [BitConverter]::ToUInt32($bytes, $off + 4)
        Flags   = [BitConverter]::ToUInt32($bytes, $off + 8)
        Addr    = [BitConverter]::ToUInt32($bytes, $off + 12)
        Offset  = [BitConverter]::ToUInt32($bytes, $off + 16)
        Size    = [BitConverter]::ToUInt32($bytes, $off + 20)
        Link    = [BitConverter]::ToUInt32($bytes, $off + 24)
        Align   = [BitConverter]::ToUInt32($bytes, $off + 32)
        EntSize = [BitConverter]::ToUInt32($bytes, $off + 36)
    }
    $sections += $sec
    if ($sec.Name -ne '') {
        $secByName[$sec.Name] = $sec
    }
}

$flashOrigin = [uint32]0x08000000
$ramOrigin   = [uint32]0x20000000
$maxFlashEnd = $flashOrigin

Write-Host "=== ELF Audit ($Profile): $Elf (entry=0x$($e_entry.ToString('x8'))) ==="
for ($i = 0; $i -lt $e_phnum; $i++) {
    $off = $e_phoff + $i * $e_phentsize
    $pType   = [BitConverter]::ToUInt32($bytes, $off + 0)
    $pVaddr  = [BitConverter]::ToUInt32($bytes, $off + 8)
    $pPaddr  = [BitConverter]::ToUInt32($bytes, $off + 12)
    $pFilesz = [BitConverter]::ToUInt32($bytes, $off + 16)
    $pMemsz  = [BitConverter]::ToUInt32($bytes, $off + 20)
    if ($pType -eq 1 -and $pFilesz -gt 0) {
        # PT_LOAD
        if ($pPaddr -ge $flashOrigin -and $pPaddr -lt 0x10000000) {
            $segEnd = $pPaddr + $pFilesz
            if ($segEnd -gt $maxFlashEnd) { $maxFlashEnd = $segEnd }
        }
        Write-Host ("  PT_LOAD vaddr=0x{0:x8} paddr=0x{1:x8} filesz={2,7} memsz={3,7}" -f $pVaddr, $pPaddr, $pFilesz, $pMemsz)
    }
}

$allocSecs = $sections | Where-Object { ($_.Flags -band 2) -ne 0 }
foreach ($s in $allocSecs) {
    Write-Host ("  [{0,2}] {1,-16} type={2} addr=0x{3:x8} size={4,7}" -f $s.Index, $s.Name, $s.Type, $s.Addr, $s.Size)
}

$symtab = $sections | Where-Object { $_.Type -eq 2 } | Select-Object -First 1
if ($null -eq $symtab) {
    throw "Missing SHT_SYMTAB in $Elf"
}
$strtabSec = $sections[$symtab.Link]
$strBuf = New-Object byte[] $strtabSec.Size
[Array]::Copy($bytes, [int]$strtabSec.Offset, $strBuf, 0, [int]$strtabSec.Size)

$symbols = @{}
$symCount = [int]($symtab.Size / 16)
for ($i = 0; $i -lt $symCount; $i++) {
    $off = [int]$symtab.Offset + $i * 16
    $stName  = [BitConverter]::ToUInt32($bytes, $off + 0)
    $stValue = [BitConverter]::ToUInt32($bytes, $off + 4)
    $stSize  = [BitConverter]::ToUInt32($bytes, $off + 8)
    $stInfo  = $bytes[$off + 12]
    $stShndx = [BitConverter]::ToUInt16($bytes, $off + 14)
    $name = Read-CStr $strBuf ([int]$stName)
    if ($name -ne '') {
        $symbols[$name] = [pscustomobject]@{
            Name  = $name
            Value = $stValue
            Size  = $stSize
            Type  = ($stInfo -band 0xF)
            Shndx = $stShndx
        }
    }
}

$flashSpan = [int64]($maxFlashEnd - $flashOrigin)
$dataSec = $secByName['.data']
$bssSec  = $secByName['.bss']
$dataSize = if ($dataSec) { [int64]$dataSec.Size } else { 0 }
$bssSize  = if ($bssSec)  { [int64]$bssSec.Size } else { 0 }

$ramUsedEndSym = $symbols['_ram_used_end']
$stackStartSym = $symbols['_stack_start']
if ($Profile -ne 'Baseline') {
    if ($null -eq $ramUsedEndSym) { throw "Missing _ram_used_end symbol" }
    if ($null -eq $stackStartSym) { throw "Missing _stack_start symbol" }
    $ramEndAddr = $ramUsedEndSym.Value
    $stackStart = $stackStartSym.Value
} else {
    $ebssSym = $symbols['_ebss']
    $ramEndAddr = if ($ebssSym) { $ebssSym.Value } else { $ramOrigin + $dataSize + $bssSize }
    $stackStart = if ($stackStartSym) { $stackStartSym.Value } else { $ramOrigin + 136 * 1024 }
}

$staticRam = [int64]($ramEndAddr - $ramOrigin)
$stackReserve = [int64]($stackStart - $ramEndAddr)

Write-Host ("  Summary: FlashSpan={0} B, .data={1} B, .bss={2} B, StaticRAM={3} B, StackReserve={4} B" -f $flashSpan, $dataSize, $bssSize, $staticRam, $stackReserve)

if ($flashSpan -gt 262144) {
    throw "Flash PT_LOAD span ($flashSpan B) exceeds 256 KiB hard limit (262144 B)"
}
if ($staticRam -gt 131072) {
    throw "Static RAM ($staticRam B) exceeds 128 KiB hard limit (131072 B)"
}
if ($stackReserve -lt 8192) {
    throw "Stack reserve ($stackReserve B) below 8 KiB minimum (8192 B)"
}
if (($stackStart % 8) -ne 0) {
    throw "Stack top 0x$($stackStart.ToString('x8')) is not 8-byte aligned"
}

if ($Profile -eq 'Baseline') {
    exit 0
}

if ($flashSpan -gt 196608) {
    throw "Demo Flash PT_LOAD span ($flashSpan B) exceeds 192 KiB target (196608 B)"
}
if ($dataSize -gt 1024) {
    throw ".data size ($dataSize B) exceeds 1024 B limit"
}
if ($staticRam -gt 81920) {
    throw "Static RAM ($staticRam B) exceeds 80 KiB regression limit (81920 B)"
}

function Require-BssObj([string]$name, [int]$maxSize, [int]$exactSize = -1) {
    if (-not $symbols.ContainsKey($name)) {
        throw "Missing required symbol: $name"
    }
    $sym = $symbols[$name]
    if ($null -eq $bssSec -or $sym.Shndx -ne $bssSec.Index -or $bssSec.Type -ne 8) {
        $secName = if ($sym.Shndx -lt $sections.Count) { $sections[$sym.Shndx].Name } else { $sym.Shndx.ToString() }
        throw "Symbol $name must live in .bss (SHT_NOBITS=8), found in section '$secName'"
    }
    if ($sym.Size -le 0) {
        throw "Symbol $name has zero size"
    }
    if ($exactSize -ge 0 -and $sym.Size -ne $exactSize) {
        throw "Symbol $name size $($sym.Size) B != expected $exactSize B"
    }
    if ($sym.Size -gt $maxSize) {
        throw "Symbol $name size $($sym.Size) B exceeds cap $maxSize B"
    }
    Write-Host ("  Symbol {0,-18} addr=0x{1:x8} size={2,6} B (.bss)" -f $name, $sym.Value, $sym.Size)
    return $sym
}

$sStorage = Require-BssObj 'LIULI_STORAGE' 67456
$sBandA   = Require-BssObj 'LIULI_DMA_BAND_A' 5120 5120
$sBandB   = Require-BssObj 'LIULI_DMA_BAND_B' 5120 5120

$ranges = @(
    [pscustomobject]@{ Name = 'LIULI_STORAGE';    Start = [int64]$sStorage.Value; End = [int64]$sStorage.Value + [int64]$sStorage.Size },
    [pscustomobject]@{ Name = 'LIULI_DMA_BAND_A'; Start = [int64]$sBandA.Value;   End = [int64]$sBandA.Value + [int64]$sBandA.Size },
    [pscustomobject]@{ Name = 'LIULI_DMA_BAND_B'; Start = [int64]$sBandB.Value;   End = [int64]$sBandB.Value + [int64]$sBandB.Size }
) | Sort-Object Start

for ($i = 1; $i -lt $ranges.Count; $i++) {
    if ($ranges[$i].Start -lt $ranges[$i - 1].End) {
        throw "Overlapping BSS objects: $($ranges[$i-1].Name) and $($ranges[$i].Name)"
    }
}

if ($Profile -eq 'StartupProbe') {
    if ($dataSize -lt 4) {
        throw "StartupProbe expects .data >= 4 B, got $dataSize B"
    }
    if (-not $symbols.ContainsKey('LIULI_PROBE_DATA') -or $null -eq $dataSec -or $symbols['LIULI_PROBE_DATA'].Shndx -ne $dataSec.Index) {
        throw "StartupProbe requires LIULI_PROBE_DATA in .data"
    }
    $null = Require-BssObj 'LIULI_PROBE_BSS' 16 4
    Write-Host ("  Symbol {0,-18} addr=0x{1:x8} size={2,6} B (.data)" -f 'LIULI_PROBE_DATA', $symbols['LIULI_PROBE_DATA'].Value, $symbols['LIULI_PROBE_DATA'].Size)
} else {
    if ($dataSize -ne 0) {
        Write-Warning "Demo .data is $dataSize B (target is 0 B, limit <= 1024 B)"
    }
}

Write-Host "Firmware ELF audit PASSED ($Profile)."
