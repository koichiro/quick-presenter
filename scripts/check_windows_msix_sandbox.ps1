param(
    [string]$Binary = "target/release/quick-presenter.exe"
)

$ErrorActionPreference = 'Stop'
# This script installs a disposable test certificate and package. Never run it
# on a user machine or publish the self-signed package as a release artifact.
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'Installed MSIX validation requires a disposable Windows Actions runner'
}
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$work = Join-Path $env:RUNNER_TEMP ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $work | Out-Null
$logs = Join-Path $env:RUNNER_TEMP 'quick-presenter-msix-verification'
$runner = Join-Path $PSScriptRoot 'run_bounded_command.py'
function Invoke-BoundedCommand {
    param([string]$Label, [string[]]$Command, [int]$Seconds = 30)
    New-Item -ItemType Directory -Path $logs -Force | Out-Null
    $diagnostics = Join-Path $logs ([guid]::NewGuid().ToString() + '.diagnostics')
    Write-Host "Starting $Label (timeout $Seconds seconds)"
    $output = & python $runner --timeout $Seconds --label $Label --log-dir $logs --diagnostics $diagnostics -- @Command
    $exitCode = $LASTEXITCODE
    if (Test-Path -LiteralPath $diagnostics) { Get-Content -LiteralPath $diagnostics -Encoding UTF8 | Write-Host }
    if ($exitCode -ne 0) { throw "$Label failed with exit code $exitCode" }
    return $output
}
function Invoke-BoundedPowerShell {
    param([string]$Label, [string]$Code, [int]$Seconds = 60)
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes(
        "`$ErrorActionPreference = 'Stop'; " + $Code))
    Invoke-BoundedCommand -Label $Label -Seconds $Seconds -Command @(
        'powershell.exe', '-NoProfile', '-NonInteractive', '-EncodedCommand', $encoded)
}
$packageName = 'KoichiroOhba.QuickPresenter'
if (Get-AppxPackage -Name $packageName) {
    throw 'Refusing to replace an existing Store package'
}
$certificate = $null
$trusted = $null
$package = $null
$report = $null
try {
    $msix = Join-Path $work 'sandbox-test.msix'
    $buildScript = Join-Path $PSScriptRoot 'build_windows_msix.ps1'
    Invoke-BoundedCommand -Label 'Build test MSIX' -Seconds 90 -Command @(
        'powershell.exe', '-NoProfile', '-NonInteractive', '-File', $buildScript,
        '-Binary', $Binary, '-ArtifactDir', (Join-Path $work 'artifacts'),
        '-OutputMsix', $msix, '-StoreIdentity') | Write-Host
    if (-not (Test-Path $msix -PathType Leaf)) { throw 'Missing test MSIX' }
    $certificate = New-SelfSignedCertificate -Type Custom `
        -Subject 'CN=A3F64AFD-6298-42F8-BAC9-DB0AB1831F51' `
        -KeyUsage DigitalSignature -FriendlyName 'Quick Presenter disposable CI test' `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
    $cer = Join-Path $work 'test.cer'
    Export-Certificate -Cert $certificate -FilePath $cer | Out-Null
    $trusted = Import-Certificate -FilePath $cer -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople'
    $sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $signTool = Get-ChildItem $sdk -Recurse -Filter signtool.exe |
        Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
        Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
    if (-not $signTool) { throw 'Missing signtool' }
    Invoke-BoundedCommand -Label 'Sign test MSIX' -Command @(
        $signTool, 'sign', '/fd', 'SHA256', '/s', 'My', '/sha1', $certificate.Thumbprint, $msix) | Write-Host
    Invoke-BoundedPowerShell -Label 'Install test MSIX' -Code "Add-AppxPackage -Path '$msix'" | Write-Host
    $package = Get-AppxPackage -Name $packageName
    if (-not $package) { throw 'Test MSIX was not installed' }
    $exe = Join-Path $package.InstallLocation 'quick-presenter.exe'
    Invoke-BoundedCommand -Label 'Installed MSIX CLI contract' -Seconds 60 -Command @(
        'python', (Join-Path $PSScriptRoot 'check_qp.py'), (Join-Path $package.InstallLocation 'qp.exe')) | Write-Host
    # Exercise Windows' registered console alias, not just the unpacked file.
    $alias = Join-Path $env:LOCALAPPDATA 'Microsoft/WindowsApps/qp.exe'
    Write-Host 'Waiting for the registered MSIX execution alias'
    $aliasDeadline = (Get-Date).AddSeconds(30)
    while (-not (Test-Path -LiteralPath $alias) -and (Get-Date) -lt $aliasDeadline) {
        Start-Sleep -Milliseconds 200
    }
    if (-not (Test-Path -LiteralPath $alias)) { throw 'MSIX execution alias was not registered within 30 seconds' }
    $version = Invoke-BoundedCommand -Label 'Alias version' -Command @($alias, '--version', '--json')
    $metadata = $version | ConvertFrom-Json
    $cliMetadata = Invoke-BoundedCommand -Label 'Installed CLI version' -Command @(
        (Join-Path $package.InstallLocation 'qp.exe'), '--version', '--json') | ConvertFrom-Json
    if ($metadata.protocol_version -ne 1 -or
        $metadata.application_version -ne $cliMetadata.application_version) {
        throw 'MSIX CLI execution alias returned unexpected version metadata'
    }
    # Verify CLI-triggered startup in the installed package context. This runner
    # is disposable, and only this package's exact GUI path is cleaned up.
    $pdf = Join-Path $repo 'tests/fixtures/marp-speaker-notes.pdf'
    $existingGui = @(Get-Process -Name quick-presenter -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $exe })
    if ($existingGui.Count -ne 0) { throw 'Expected no running Store GUI before CLI startup test' }
    try {
        $opened = Invoke-BoundedCommand -Label 'Alias open and GUI startup' -Command @($alias, 'open', $pdf, '--json') | ConvertFrom-Json
        if ($opened.kind -ne 'mutation') {
            throw 'MSIX CLI could not start the GUI and open the fixture'
        }
        $deadline = (Get-Date).AddSeconds(30)
        do {
            $snapshot = Invoke-BoundedCommand -Label 'Alias presentation status' -Command @($alias, 'status', '--json') | ConvertFrom-Json
            if (-not $snapshot.opening -and $snapshot.render_state -eq 'ready') { break }
            Start-Sleep -Milliseconds 100
        } while ((Get-Date) -lt $deadline)
        if ($snapshot.render_state -ne 'ready' -or $snapshot.page -ne 1 -or $snapshot.pages -lt 1) {
            throw 'MSIX CLI startup did not produce a ready presentation'
        }
        $gui = @(Get-Process -Name quick-presenter -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $exe })
        if ($gui.Count -ne 1) { throw 'Expected one GUI after CLI startup' }
        $reopened = Invoke-BoundedCommand -Label 'Alias reopen existing GUI' -Command @($alias, 'open', $pdf, '--json') | ConvertFrom-Json
        if ($reopened.state.session_id -ne $snapshot.session_id) {
            throw 'MSIX CLI did not reuse the running GUI'
        }
        $closed = Invoke-BoundedCommand -Label 'Alias close document' -Command @($alias, 'close', '--json') | ConvertFrom-Json
        if ($null -ne $closed.state.document) { throw 'MSIX CLI close failed' }
    } finally {
        Get-Process -Name quick-presenter -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $exe } | Stop-Process -Force
    }
    $manifest = Get-AppxPackageManifest -Package $package.PackageFullName
    $appId = [string]$manifest.Package.Applications.Application.Id
    $python = (Get-Command python.exe).Source
    # Use the workspace, not virtualized LocalAppData, for the completion report.
    $report = Join-Path $repo ('msix-sandbox-' + [guid]::NewGuid().ToString() + '.json')
    $probe = Join-Path $PSScriptRoot 'check_renderer_sandbox.py'
    $pdf = Join-Path $repo 'tests/fixtures/marp-speaker-notes.pdf'
    $arguments = '"{0}" "{1}" "{2}" --require-package-identity --result-file "{3}"' -f $probe, $exe, $pdf, $report
    Remove-Item Env:PDFIUM_DYNAMIC_LIB_PATH -ErrorAction SilentlyContinue
    Remove-Item Env:QUICK_PRESENTER_ALLOW_PDFIUM_OVERRIDE -ErrorAction SilentlyContinue
    # Force the probe's child broker into the installed package context. The
    # broker independently asserts package identity before launching its helper.
    $family = $package.PackageFamilyName
    Invoke-BoundedPowerShell -Label 'Launch package-identity renderer probe' -Code (
        "Invoke-CommandInDesktopPackage -PackageFamilyName '$family' -AppId '$appId' " +
        "-Command '$python' -Args '$arguments' -PreventBreakaway") | Write-Host
    $deadline = (Get-Date).AddSeconds(90)
    $result = $null
    while ((Get-Date) -lt $deadline) {
        if (Test-Path $report) {
            try { $result = Get-Content $report -Raw | ConvertFrom-Json } catch { }
            if ($null -ne $result) { break }
        }
        Start-Sleep -Milliseconds 200
    }
    if ($null -eq $result) { throw 'Installed MSIX sandbox probe timed out' }
    if (-not $result.success) { throw "Installed MSIX sandbox probe failed: $($result.error)" }
    Write-Host 'Installed Store-identity MSIX: package identity, PDF render/notes and sandbox denial gate passed'
} finally {
    # Installation may succeed even if its supervising command times out.
    if (-not $package) { $package = Get-AppxPackage -Name $packageName }
    try {
        if ($report) {
            # Select only the Python probe carrying this invocation's unique report.
            Get-CimInstance Win32_Process -Filter "Name = 'python.exe'" |
                Where-Object { $_.CommandLine -and $_.CommandLine.Contains($report) } |
                ForEach-Object {
                    Invoke-BoundedCommand -Label 'Stop package renderer probe' -Seconds 15 -Command @(
                        'taskkill.exe', '/PID', [string]$_.ProcessId, '/T', '/F') | Write-Host
                }
        }
        # A timeout can occur before the inner GUI cleanup scope is entered.
        if ($package) {
            $installedExe = Join-Path $package.InstallLocation 'quick-presenter.exe'
            Get-Process -Name quick-presenter -ErrorAction SilentlyContinue |
                Where-Object { $_.Path -eq $installedExe } | Stop-Process -Force
        }
    } finally {
        try {
            if ($package) {
                $fullName = $package.PackageFullName
                Invoke-BoundedPowerShell -Label 'Uninstall test MSIX' -Code "Remove-AppxPackage -Package '$fullName'" | Write-Host
            }
        } finally {
            if ($trusted) { Remove-Item -LiteralPath $trusted.PSPath }
            if ($certificate) { Remove-Item -LiteralPath $certificate.PSPath }
        }
    }
}
