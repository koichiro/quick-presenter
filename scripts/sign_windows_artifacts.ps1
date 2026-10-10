param(
    [string]$ArtifactDir = "artifacts/quick-presenter-windows-x64",
    [string[]]$Path = @(),
    [string]$PfxPath = "",
    [string]$PfxPassword = "",
    [string]$TimestampUrl = "",
    [string]$ExpectedPublisher = "",
    [string]$ExpectedThumbprint = "",
    [string]$SignToolCommand = "",
    [switch]$AllowUntrustedSelfSigned,
    [switch]$VerifyOnly
)

$ErrorActionPreference = "Stop"

function Resolve-RepoPath {
    param([string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }

    return [System.IO.Path]::GetFullPath((Join-Path $RepoRoot $Path))
}

function Resolve-SignTool {
    param([string]$Command)

    if (-not [string]::IsNullOrWhiteSpace($Command)) {
        return (Get-Command $Command -ErrorAction Stop).Source
    }

    $pathCommand = Get-Command "signtool.exe" -ErrorAction SilentlyContinue
    if ($null -ne $pathCommand) {
        return $pathCommand.Source
    }

    $sdkRoots = @()
    if (-not [string]::IsNullOrWhiteSpace(${env:ProgramFiles(x86)})) {
        $sdkRoots += (Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin")
    }
    if (-not [string]::IsNullOrWhiteSpace($env:ProgramFiles)) {
        $sdkRoots += (Join-Path $env:ProgramFiles "Windows Kits\10\bin")
    }

    $candidates = foreach ($root in $sdkRoots) {
        if (Test-Path $root -PathType Container) {
            Get-ChildItem $root -Recurse -Filter "signtool.exe" -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' }
        }
    }

    $candidate = $candidates | Sort-Object FullName -Descending | Select-Object -First 1
    if ($null -eq $candidate) {
        throw "Could not find signtool.exe. Install the Windows SDK or pass -SignToolCommand."
    }

    return $candidate.FullName
}

function Invoke-Checked {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$FailureMessage
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$FailureMessage with exit code $LASTEXITCODE"
    }
}

function Sign-Artifact {
    param([string]$Path)

    Write-Host "Signing $Path"
    $args = @(
        "sign",
        "/f", $PfxPath,
        "/p", $PfxPassword,
        "/fd", "SHA256",
        "/tr", $TimestampUrl,
        "/td", "SHA256",
        "/d", "Quick Presenter",
        $Path
    )
    Invoke-Checked -FilePath $SignTool -Arguments $args -FailureMessage "SignTool sign failed for $Path"
}

function Verify-Artifact {
    param([string]$Path)

    Write-Host "Verifying $Path"
    $args = @("verify", "/pa", "/v", $Path)
    & $SignTool @args
    if ($LASTEXITCODE -eq 0) {
        return
    }

    $verifyExitCode = $LASTEXITCODE
    if (-not $AllowUntrustedSelfSigned) {
        throw "SignTool verify failed for $Path with exit code $verifyExitCode"
    }

    $signature = Get-AuthenticodeSignature -FilePath $Path
    if ($null -eq $signature.SignerCertificate) {
        throw "SignTool verify failed for $Path with exit code $verifyExitCode and no signer certificate was found"
    }
    $signatureStatus = $signature.Status.ToString()
    if ($signatureStatus -eq "NotSigned") {
        throw "SignTool verify failed for $Path with exit code $verifyExitCode and the artifact is not signed"
    }
    if ($signatureStatus -eq "HashMismatch") {
        throw "SignTool verify failed for $Path with exit code $verifyExitCode and the signature hash does not match"
    }
    if (-not [string]::IsNullOrWhiteSpace($ExpectedThumbprint) -and
        $signature.SignerCertificate.Thumbprint -ne $ExpectedThumbprint) {
        throw "Signed artifact thumbprint '$($signature.SignerCertificate.Thumbprint)' does not match expected thumbprint '$ExpectedThumbprint'"
    }

    $global:LASTEXITCODE = 0
    Write-Host "Accepted untrusted self-signed test signature for $Path"
}

function Resolve-SingleArtifact {
    param(
        [string]$Directory,
        [string]$Filter
    )

    $matches = @(Get-ChildItem -Path $Directory -File -Filter $Filter | Sort-Object Name)
    if ($matches.Count -eq 0) {
        throw "Missing Windows signing artifact matching '$Filter' in $Directory"
    }
    if ($matches.Count -gt 1) {
        $names = ($matches | ForEach-Object { $_.Name }) -join ", "
        throw "Expected exactly one Windows signing artifact matching '$Filter' in $Directory, found: $names"
    }

    return $matches[0].FullName
}

$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$artifactDirPath = Resolve-RepoPath $ArtifactDir

if ([string]::IsNullOrWhiteSpace($PfxPath)) {
    throw "Missing required -PfxPath"
}
if ([string]::IsNullOrWhiteSpace($PfxPassword)) {
    throw "Missing required -PfxPassword"
}
if ([string]::IsNullOrWhiteSpace($TimestampUrl)) {
    throw "Missing required -TimestampUrl"
}

$PfxPath = Resolve-RepoPath $PfxPath
if (-not (Test-Path $PfxPath -PathType Leaf)) {
    throw "Missing signing certificate PFX: $PfxPath"
}
if ($Path.Count -gt 0) {
    $artifactPaths = $Path | ForEach-Object { Resolve-RepoPath $_ }
} else {
    if (-not (Test-Path $artifactDirPath -PathType Container)) {
        throw "Missing Windows artifact directory: $artifactDirPath"
    }

    $artifactPaths = @(
        (Join-Path $artifactDirPath "quick-presenter.exe"),
        (Join-Path $artifactDirPath "qp.exe"),
        (Resolve-SingleArtifact -Directory $artifactDirPath -Filter "QuickPresenter-*.msi"),
        (Resolve-SingleArtifact -Directory $artifactDirPath -Filter "QuickPresenter-*.msix")
    )
}

foreach ($artifactPath in $artifactPaths) {
    if (-not (Test-Path $artifactPath -PathType Leaf)) {
        throw "Missing Windows signing artifact: $artifactPath"
    }
}

$SignTool = Resolve-SignTool $SignToolCommand

if (-not [string]::IsNullOrWhiteSpace($ExpectedPublisher)) {
    Write-Host "Expected Windows signing publisher: $ExpectedPublisher"
    $certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($PfxPath, $PfxPassword)
    if ($certificate.Subject -ne $ExpectedPublisher) {
        throw "Signing certificate subject '$($certificate.Subject)' does not match expected publisher '$ExpectedPublisher'"
    }
}
if (-not [string]::IsNullOrWhiteSpace($ExpectedThumbprint)) {
    $certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($PfxPath, $PfxPassword)
    if ($certificate.Thumbprint -ne $ExpectedThumbprint) {
        throw "Signing certificate thumbprint '$($certificate.Thumbprint)' does not match expected thumbprint '$ExpectedThumbprint'"
    }
}

foreach ($artifactPath in $artifactPaths) {
    if (-not $VerifyOnly) {
        Sign-Artifact $artifactPath
    }
    Verify-Artifact $artifactPath
}

if ($VerifyOnly) {
    Write-Host "Verified Windows artifact signatures"
} else {
    Write-Host "Signed and verified Windows artifacts"
}
$global:LASTEXITCODE = 0
