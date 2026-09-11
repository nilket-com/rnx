# Native acceptance for record 0025. Cargo's enclosing job on the measured
# Windows machine prevents Gate 10's outside breakaway control. Build through
# Cargo, then run every compiled test directly so that control can be drawn.
# No test is skipped and any failing executable makes this script fail.
[CmdletBinding()]
param([switch]$TestSupport)

$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $cargoArgs = @('test', '--locked', '--no-run', '--message-format=json')
    if ($TestSupport) { $cargoArgs += @('--features', 'test-support') }
    $messages = & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Windows acceptance build failed' }
    $executables = @($messages | ForEach-Object {
        $artifact = $_ | ConvertFrom-Json
        if ($artifact.reason -eq 'compiler-artifact' -and
            $artifact.profile.test -and $artifact.executable) {
            $artifact.executable
        }
    } | Sort-Object -Unique)
    if ($executables.Count -eq 0) { throw 'Cargo reported no test executables' }
    $failed = @()
    foreach ($executable in $executables) {
        Write-Output "Running $executable"
        & $executable
        if ($LASTEXITCODE -ne 0) { $failed += $executable }
    }
    if ($failed.Count) { throw "Failed test executables: $($failed -join ', ')" }
    Write-Output "Windows acceptance passed: $($executables.Count) executables; test-support=$TestSupport"
} finally {
    Pop-Location
}
