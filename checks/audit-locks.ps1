param(
    [string]$RootLock = (Join-Path (Split-Path -Parent $PSScriptRoot) 'Cargo.lock'),
    [string]$FirmwareLock = (Join-Path (Split-Path -Parent $PSScriptRoot) 'checks\firmware\Cargo.lock')
)

$ErrorActionPreference = 'Stop'

$ExpectedRegistry = 'registry+https://github.com/rust-lang/crates.io-index'

$ExpectedExternal = @{
    'autocfg' = @{
        version  = '1.5.1'
        checksum = 'f2032f911046de80f0a198e0901378627c33f59ea0ac00e363d481118bd70a53'
        deps     = @()
    }
    'az' = @{
        version  = '1.2.1'
        checksum = '7b7e4c2464d97fe331d41de9d5db0def0a96f4d823b8b32a2efd503578988973'
        deps     = @()
    }
    'byteorder' = @{
        version  = '1.5.0'
        checksum = '1fd0f2584146f6f2ef48085050886acf353beff7305ebd1ae69500e27c67f64b'
        deps     = @()
    }
    'embedded-graphics' = @{
        version  = '0.8.2'
        checksum = '4e8da660bb0c829b34a56a965490597f82a55e767b91f9543be80ce8ccb416fe'
        deps     = @('az', 'byteorder', 'embedded-graphics-core', 'float-cmp', 'micromath')
    }
    'embedded-graphics-core' = @{
        version  = '0.4.1'
        checksum = '95743bef3ff70fcba3930246c4e6872882bbea0dcc6da2ca860112e0cd4bd09f'
        deps     = @('az', 'byteorder')
    }
    'float-cmp' = @{
        version  = '0.9.0'
        checksum = '98de4bbd547a563b716d8dfa9aad1cb19bfab00f4fa09a6a4ed21dbcf44ce9c4'
        deps     = @('num-traits')
    }
    'micromath' = @{
        version  = '2.1.0'
        checksum = 'c3c8dda44ff03a2f238717214da50f65d5a53b45cd213a7370424ffdb6fae815'
        deps     = @()
    }
    'num-traits' = @{
        version  = '0.2.19'
        checksum = '071dfc062690e90b734c0b2273ce72ad0ffa95f0c74596bc250dcfd960262841'
        deps     = @('autocfg')
    }
}

$AllowedLocalExternalEdges = @{
    'liuli-core'     = @('embedded-graphics-core')
    'liuli-text'     = @()
    'liuli-render'   = @('embedded-graphics')
    'liuli'          = @('embedded-graphics-core')
    'liuli-settings' = @()
    'liuli-fontpack' = @()
    'liuli-sim'      = @('embedded-graphics')
    'liuli-firmware' = @('embedded-graphics-core')
}

function Parse-CargoLock([string]$Path) {
    if (-not (Test-Path $Path)) {
        throw "Lockfile not found: $Path"
    }
    $lines = Get-Content -Path $Path -Encoding UTF8
    $packages = @()
    $cur = $null
    $inDeps = $false

    foreach ($raw in $lines) {
        $line = $raw.Trim()
        if ($line -eq '[[package]]') {
            if ($null -ne $cur) { $packages += $cur }
            $cur = [ordered]@{ name = ''; version = ''; source = $null; checksum = $null; deps = @() }
            $inDeps = $false
            continue
        }
        if ($null -eq $cur) { continue }
        if ($inDeps) {
            if ($line -eq ']') {
                $inDeps = $false
            } elseif ($line -match '^"([^"\s]+)') {
                $cur.deps += $Matches[1]
            }
            continue
        }
        if ($line -match '^name\s*=\s*"([^"]+)"$') { $cur.name = $Matches[1] }
        elseif ($line -match '^version\s*=\s*"([^"]+)"$') { $cur.version = $Matches[1] }
        elseif ($line -match '^source\s*=\s*"([^"]+)"$') { $cur.source = $Matches[1] }
        elseif ($line -match '^checksum\s*=\s*"([^"]+)"$') { $cur.checksum = $Matches[1] }
        elseif ($line -match '^dependencies\s*=\s*\[$') { $inDeps = $true }
    }
    if ($null -ne $cur) { $packages += $cur }
    return $packages
}

function Assert-Lockfile([string]$Path) {
    $raw = Get-Content -Raw -Encoding UTF8 $Path
    if ($raw -match 'rsproxy') {
        throw "Forbidden mirror URL ('rsproxy') found in $Path"
    }

    $pkgs = Parse-CargoLock $Path
    $extFound = @{}

    foreach ($p in $pkgs) {
        if ($null -ne $p.source) {
            if ($p.source -ne $ExpectedRegistry) {
                throw "${Path}: unexpected source '$($p.source)' on package '$($p.name)'"
            }
            if (-not $ExpectedExternal.ContainsKey($p.name)) {
                throw "${Path}: unexpected external dependency '$($p.name) $($p.version)'"
            }
            $exp = $ExpectedExternal[$p.name]
            if ($p.version -ne $exp.version) {
                throw "${Path}: version mismatch for '$($p.name)': got '$($p.version)', expected '$($exp.version)'"
            }
            if ($p.checksum -ne $exp.checksum) {
                throw "${Path}: checksum mismatch for '$($p.name)'"
            }
            $gotDeps = ($p.deps | Sort-Object) -join ','
            $expDeps = ($exp.deps | Sort-Object) -join ','
            if ($gotDeps -ne $expDeps) {
                throw "${Path}: dependency edge mismatch for '$($p.name)': got [$gotDeps], expected [$expDeps]"
            }
            $extFound[$p.name] = $true
        } else {
            if (-not $AllowedLocalExternalEdges.ContainsKey($p.name)) {
                throw "${Path}: unexpected local package '$($p.name)'"
            }
            $extDeps = @($p.deps | Where-Object { $ExpectedExternal.ContainsKey($_) } | Sort-Object) -join ','
            $expLocalExt = ($AllowedLocalExternalEdges[$p.name] | Sort-Object) -join ','
            if ($extDeps -ne $expLocalExt) {
                throw "${Path}: local package '$($p.name)' external edges changed: got [$extDeps], expected [$expLocalExt]"
            }
        }
    }

    foreach ($req in $ExpectedExternal.Keys) {
        if (-not $extFound.ContainsKey($req)) {
            throw "${Path}: missing expected external crate '$req'"
        }
    }
    Write-Host "Lock audit OK: $Path ($($extFound.Count) frozen external crates)"
}

Assert-Lockfile $RootLock
Assert-Lockfile $FirmwareLock
