<#
.SYNOPSIS
    List the crates RakshaMemo can update. Changes nothing.

.DESCRIPTION
    Compares each direct dependency's version in Cargo.lock with the newest
    release on crates.io and prints only the ones with updates:

      Update now      a compatible release; `cargo update` applies it.
      Major versions  a breaking release; edit Cargo.toml (code may need changes).

    Needs cargo and internet access. Works with Windows PowerShell 5.1 and
    PowerShell 7 (pwsh) on any OS.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\check-updates.ps1
#>
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3
# Windows PowerShell 5.1 defaults to older TLS versions that crates.io refuses.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$manifest = Join-Path (Split-Path -Parent $PSScriptRoot) 'Cargo.toml'

# [major, minor, patch] for a plain release; $null for pre-releases.
function Get-Version([string]$text) {
    if ($text -match '^\D*(\d+)(?:\.(\d+))?(?:\.(\d+))?$') {
        return @([long]$Matches[1], [long]('0' + $Matches[2]), [long]('0' + $Matches[3]))
    }
    $null
}

function Compare-Version($a, $b) {
    for ($i = 0; $i -lt 3; $i++) {
        if ($a[$i] -ne $b[$i]) { return [math]::Sign($a[$i] - $b[$i]) }
    }
    0
}

# Cargo's default requirement: the leftmost non-zero part must not change.
function Test-Compatible($version, $locked) {
    if ($locked[0] -gt 0) { return $version[0] -eq $locked[0] }
    if ($locked[1] -gt 0) { return $version[0] -eq 0 -and $version[1] -eq $locked[1] }
    return $version[0] -eq 0 -and $version[1] -eq 0 -and $version[2] -eq $locked[2]
}

# The newest compatible and newest overall non-yanked releases on crates.io.
function Get-Newest([string]$name, $locked) {
    $name = $name.ToLowerInvariant()
    $path = switch ($name.Length) {
        1 { "1/$name" }
        2 { "2/$name" }
        3 { "3/$($name[0])/$name" }
        default { "$($name.Substring(0, 2))/$($name.Substring(2, 2))/$name" }
    }
    $response = Invoke-WebRequest "https://index.crates.io/$path" -UseBasicParsing
    $content = $response.Content
    if ($content -is [byte[]]) { $content = [Text.Encoding]::UTF8.GetString($content) }
    $compatible = $locked; $latest = $locked
    foreach ($line in $content -split "`n") {
        if (-not $line.Trim()) { continue }
        $release = $line | ConvertFrom-Json
        $version = Get-Version $release.vers
        if ($release.yanked -or -not $version) { continue }
        if ((Compare-Version $version $latest) -gt 0) { $latest = $version }
        if ((Test-Compatible $version $locked) -and (Compare-Version $version $compatible) -gt 0) {
            $compatible = $version
        }
    }
    $compatible, $latest
}

Write-Host 'Checking crates.io...'
# --locked: read Cargo.lock as committed and never rewrite it. Cargo reports
# progress such as "Downloading crates" on stderr, which Windows PowerShell 5.1
# would treat as a fatal error under 'Stop'; the exit code is checked instead.
$ErrorActionPreference = 'Continue'
$json = & cargo metadata --format-version 1 --locked --manifest-path $manifest 2>$null
$ErrorActionPreference = 'Stop'
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed; run it directly to see why.' }
$metadata = ($json -join "`n") | ConvertFrom-Json
$root = $metadata.resolve.root
$byId = @{}
foreach ($package in $metadata.packages) { $byId[$package.id] = $package }

$now = @(); $major = @()
foreach ($dependency in ($metadata.resolve.nodes | Where-Object { $_.id -eq $root }).deps) {
    $package = $byId[$dependency.pkg]
    $locked = Get-Version $package.version
    $compatible, $latest = Get-Newest $package.name $locked
    $show = { param($v) $v -join '.' }
    if ((Compare-Version $compatible $locked) -gt 0) {
        $now += '  {0,-14} {1,-9} -> {2}' -f $package.name, $package.version, (& $show $compatible)
    }
    if ((Compare-Version $latest $compatible) -gt 0) {
        $major += '  {0,-14} {1,-9} -> {2}' -f $package.name, $package.version, (& $show $latest)
    }
}

Write-Host ''
if ($now.Count -eq 0 -and $major.Count -eq 0) {
    Write-Host 'All dependencies are up to date.'
}
if ($now.Count -gt 0) {
    Write-Host 'Update now with "cargo update":'
    $now | Sort-Object | ForEach-Object { Write-Host $_ }
    Write-Host ''
}
if ($major.Count -gt 0) {
    Write-Host 'Major versions (edit Cargo.toml; code may need changes):'
    $major | Sort-Object | ForEach-Object { Write-Host $_ }
}
