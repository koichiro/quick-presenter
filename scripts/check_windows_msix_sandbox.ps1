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
$packageName = 'KoichiroOhba.QuickPresenter'
if (Get-AppxPackage -Name $packageName) {
    throw 'Refusing to replace an existing Store package'
}
$certificate = $null
$trusted = $null
$package = $null
try {
    $msix = Join-Path $work 'sandbox-test.msix'
    & (Join-Path $PSScriptRoot 'build_windows_msix.ps1') -Binary $Binary `
        -ArtifactDir (Join-Path $work 'artifacts') -OutputMsix $msix -StoreIdentity
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
    & $signTool sign /fd SHA256 /s My /sha1 $certificate.Thumbprint $msix
    if ($LASTEXITCODE -ne 0) { throw 'Test MSIX signing failed' }
    Add-AppxPackage -Path $msix
    $package = Get-AppxPackage -Name $packageName
    if (-not $package) { throw 'Test MSIX was not installed' }
    $exe = Join-Path $package.InstallLocation 'quick-presenter.exe'
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
    Invoke-CommandInDesktopPackage -PackageFamilyName $package.PackageFamilyName `
        -AppId $appId -Command $python -Args $arguments -PreventBreakaway
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
    if ($package) { Remove-AppxPackage -Package $package.PackageFullName }
    if ($trusted) { Remove-Item -LiteralPath $trusted.PSPath }
    if ($certificate) { Remove-Item -LiteralPath $certificate.PSPath }
}
