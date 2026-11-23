param(
    [Parameter(Mandatory = $true)][string]$Elf,
    [int]$MaxThreadBytes = 6144
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $Elf)) {
    throw "ELF not found: $Elf"
}

$objdump = $null
$cmd = Get-Command llvm-objdump -ErrorAction SilentlyContinue
if ($cmd) {
    $objdump = $cmd.Source
} elseif (Test-Path 'D:\Program Files\LLVM\bin\llvm-objdump.exe') {
    $objdump = 'D:\Program Files\LLVM\bin\llvm-objdump.exe'
} else {
    throw "llvm-objdump not found in PATH or D:\Program Files\LLVM\bin\llvm-objdump.exe"
}

$lines = & $objdump -d --demangle (Resolve-Path $Elf).Path
if ($LASTEXITCODE -ne 0) {
    throw "llvm-objdump failed on $Elf"
}

$funcs = @{}
$curFn = $null
$fmtWriteTargets = New-Object System.Collections.Generic.List[string]
$fmtDisplayTargets = New-Object System.Collections.Generic.List[string]

function Add-Callee([hashtable]$f, [string]$target) {
    if ($target -and $target -ne $f.Name -and -not $f.Calls.Contains($target)) {
        [void]$f.Calls.Add($target)
    }
}

foreach ($rawLine in $lines) {
    if ($rawLine -match '^[0-9a-fA-F]+\s+<(.+)>:\s*$') {
        $name = $Matches[1]
        if ($name -eq '$d' -or $name -eq '$t') { continue }
        $curFn = @{
            Name          = $name
            PushBytes     = 0
            SubSpBytes    = 0
            EpilogueAddSp = $false
            FrameBytes    = 0
            Calls         = New-Object System.Collections.Generic.List[string]
            IndirectCalls = New-Object System.Collections.Generic.List[string]
        }
        if (-not $funcs.ContainsKey($name)) {
            $funcs[$name] = $curFn
        } else {
            $curFn = $funcs[$name]
        }
        if ($name -match 'core::fmt::Write>::write_str') {
            [void]$fmtWriteTargets.Add($name)
        } elseif ($name -match 'core::fmt::Display>::fmt') {
            [void]$fmtDisplayTargets.Add($name)
        }
        continue
    }
    if ($null -eq $curFn) { continue }

    # Ignore constant pool markers
    if ($rawLine -match '^\s*[0-9a-fA-F]+:\s+[0-9a-f]+\s+(\.word|\.short|\.byte)\b') { continue }

    # push / push.w / stmdb sp!
    if ($rawLine -match '\b(?:push(?:\.w)?|stmdb\s+sp!)\s+\{([^}]+)\}') {
        $regs = ($Matches[1] -split ',') | Where-Object { $_.Trim() -ne '' }
        $curFn.PushBytes += ($regs.Count * 4)
    }
    # vpush {d8..d15} or {s16..s31}
    elseif ($rawLine -match '\bvpush(?:\.\w+)?\s+\{([^}]+)\}') {
        $list = $Matches[1]
        if ($list -match 'd(\d+)\s*-\s*d(\d+)') {
            $curFn.PushBytes += (([int]$Matches[2] - [int]$Matches[1] + 1) * 8)
        } elseif ($list -match 's(\d+)\s*-\s*s(\d+)') {
            $curFn.PushBytes += (([int]$Matches[2] - [int]$Matches[1] + 1) * 4)
        } else {
            $regs = ($list -split ',') | Where-Object { $_.Trim() -ne '' }
            $bytes = 0
            foreach ($r in $regs) {
                if ($r.Trim() -match '^d\d+$') { $bytes += 8 } else { $bytes += 4 }
            }
            $curFn.PushBytes += $bytes
        }
    }
    # sub sp, #imm  or sub.w/subw sp, sp, #imm  or str rN, [sp, #-imm]!
    elseif ($rawLine -match '\bsub(?:w|\.w)?\s+sp,\s*(?:sp,\s*)?#(?:0x([0-9a-fA-F]+)|(\d+))') {
        $val = if ($Matches[1]) { [Convert]::ToInt32($Matches[1], 16) } else { [int]$Matches[2] }
        $curFn.SubSpBytes += $val
    }
    elseif ($rawLine -match '\bstr(?:\.w)?\s+\w+,\s*\[sp,\s*#-(?:0x([0-9a-fA-F]+)|(\d+))\]!') {
        $val = if ($Matches[1]) { [Convert]::ToInt32($Matches[1], 16) } else { [int]$Matches[2] }
        $curFn.SubSpBytes += $val
    }
    elseif ($rawLine -match '\badd(?:w|\.w)?\s+sp,\s*(?:sp,\s*)?#(?:0x([0-9a-fA-F]+)|(\d+))') {
        $curFn.EpilogueAddSp = $true
    }

    # Direct calls: bl / bl.w <target>  or tail calls: b / b.w <other_func_without_+0x>
    if ($rawLine -match '\bbl(?:\.w)?\s+0x[0-9a-fA-F]+\s+<(.+)>') {
        $rawTarget = $Matches[1]
        # Strip inline "@ imm = ..." or "+0x..." offset if present
        $target = ($rawTarget -replace '\+0x[0-9a-fA-F]+$', '').Trim()
        Add-Callee $curFn $target
    }
    elseif ($rawLine -match '\bb(?:\.w)?\s+0x[0-9a-fA-F]+\s+<(.+)>') {
        $rawTarget = $Matches[1]
        # Inter-function tail branch only (intra-function branches contain +0xOffset)
        if ($rawTarget -notmatch '\+0x[0-9a-fA-F]+$') {
            Add-Callee $curFn $rawTarget.Trim()
        }
    }
    elseif ($rawLine -match '\bblx\s+(r\d+|ip|lr)\b') {
        $reg = $Matches[1]
        [void]$curFn.IndirectCalls.Add("$($curFn.Name): blx $reg")
    }
}

foreach ($k in @($funcs.Keys)) {
    $f = $funcs[$k]
    $f.FrameBytes = $f.PushBytes + $f.SubSpBytes
    # core::fmt::write invokes Write::write_str and Display::fmt function pointers;
    # Formatter::pad_integral* only invokes Write::write_str.
    if ($f.IndirectCalls.Count -gt 0 -and ($k -like 'core::fmt::*' -or $k -like '<core::fmt::*')) {
        foreach ($wt in $fmtWriteTargets) {
            Add-Callee $f $wt
        }
        if ($k -eq 'core::fmt::write') {
            foreach ($dt in $fmtDisplayTargets) {
                Add-Callee $f $dt
            }
        }
        $f.IndirectCalls.Clear()
    }
    # Diverging panic=abort paths terminate execution cleanly without returning;
    # avoid false cycles between core::fmt::write bounds checks and panic_fmt.
    if ($k -match '^(core::panicking::|core::slice::index::slice_.*fail|core::str::slice_error_fail|core::result::unwrap_failed|core::option::unwrap_failed|rust_begin_unwind)') {
        $f.Calls.Clear()
        $f.IndirectCalls.Clear()
    }
}

$entries = @()
if ($funcs.ContainsKey('Reset')) { $entries += 'Reset' }
if ($funcs.ContainsKey('firmware_main')) { $entries += 'firmware_main' }
if ($entries.Count -eq 0) {
    throw "Neither Reset nor firmware_main symbol found in $Elf"
}

$unresolved = New-Object System.Collections.Generic.List[string]
$memo = @{}
$visiting = @{}

function Measure-Chain([string]$fnName) {
    if ($memo.ContainsKey($fnName)) { return $memo[$fnName] }
    if (-not $funcs.ContainsKey($fnName)) {
        return @{ Depth = 0; Chain = @($fnName) }
    }
    if ($visiting.ContainsKey($fnName)) {
        throw "Cycle/recursion detected in call graph at '$fnName'"
    }
    $visiting[$fnName] = $true
    $f = $funcs[$fnName]
    foreach ($ic in $f.IndirectCalls) {
        [void]$unresolved.Add($ic)
    }

    $bestChild = @{ Depth = 0; Chain = @() }
    foreach ($c in $f.Calls) {
        $res = Measure-Chain $c
        if ($res.Depth -gt $bestChild.Depth) {
            $bestChild = $res
        }
    }
    $visiting.Remove($fnName)

    $total = $f.FrameBytes + $bestChild.Depth
    $chain = @("$fnName ($($f.FrameBytes) B)") + $bestChild.Chain
    $out = @{ Depth = $total; Chain = $chain }
    $memo[$fnName] = $out
    return $out
}

$worst = @{ Entry = ''; Depth = 0; Chain = @() }
foreach ($e in $entries) {
    $m = Measure-Chain $e
    if ($m.Depth -gt $worst.Depth) {
        $worst = @{ Entry = $e; Depth = $m.Depth; Chain = $m.Chain }
    }
}

Write-Host "=== Stack Audit: $Elf (limit <= $MaxThreadBytes B) ==="
Write-Host "Top 10 largest individual frames:"
$funcs.Values | Sort-Object { $_.FrameBytes } -Descending | Select-Object -First 10 | ForEach-Object {
    Write-Host ("  {0,5} B  {1}" -f $_.FrameBytes, $_.Name)
}
Write-Host "Deepest call chain from '$($worst.Entry)' = $($worst.Depth) B:"
foreach ($node in $worst.Chain) {
    Write-Host "  -> $node"
}

if ($unresolved.Count -gt 0) {
    throw ("Unresolved indirect calls found:`n" + ($unresolved -join "`n"))
}
if ($worst.Depth -gt $MaxThreadBytes) {
    throw "Deepest stack chain ($($worst.Depth) B) exceeds MaxThreadBytes ($MaxThreadBytes B)"
}
Write-Host "Stack audit PASSED ($($worst.Depth) B <= $MaxThreadBytes B)."
