param(
    [string]$ArtifactDir = "artifacts/quick-presenter-windows-x64",
    [string]$Binary = "target/release/qp.exe",
    [string]$OutputMsix = "",
    [string]$Publisher = "CN=Quick Presenter",
    [string]$MakeAppxCommand = "",
    [switch]$KeepWorkDir
)

$ErrorActionPreference = "Stop"

function Resolve-RepoPath {
    param([string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }

    return [System.IO.Path]::GetFullPath((Join-Path $RepoRoot $Path))
}

function Read-CargoVersion {
    $cargoToml = Join-Path $RepoRoot "Cargo.toml"
    foreach ($line in Get-Content $cargoToml) {
        if ($line -match '^\s*version\s*=\s*"([^"]+)"') {
            return $Matches[1]
        }
    }

    throw "Could not read package version from Cargo.toml"
}

function Convert-ToMsixVersion {
    param([string]$Version)

    $core = ($Version -split "[-+]")[0]
    $parts = @($core -split "\.")
    if ($parts.Count -gt 3) {
        $parts = $parts[0..2]
    }
    while ($parts.Count -lt 3) {
        $parts += "0"
    }
    foreach ($part in $parts) {
        if ($part -notmatch '^\d+$') {
            throw "MSIX package version must contain numeric components: $Version"
        }
    }

    return (($parts + "0") -join ".")
}

function Resolve-MakeAppx {
    param([string]$Command)

    if (-not [string]::IsNullOrWhiteSpace($Command)) {
        return (Get-Command $Command -ErrorAction Stop).Source
    }

    $pathCommand = Get-Command "makeappx.exe" -ErrorAction SilentlyContinue
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
            Get-ChildItem $root -Recurse -Filter "makeappx.exe" -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -match '\\x64\\makeappx\.exe$' }
        }
    }

    $candidate = $candidates | Sort-Object FullName -Descending | Select-Object -First 1
    if ($null -eq $candidate) {
        throw "Could not find makeappx.exe. Install the Windows SDK or pass -MakeAppxCommand."
    }

    return $candidate.FullName
}

function Copy-ResizedPng {
    param(
        [string]$Source,
        [string]$Output,
        [int]$Size
    )

    Add-Type -AssemblyName System.Drawing

    $sourceImage = [System.Drawing.Image]::FromFile($Source)
    try {
        $bitmap = [System.Drawing.Bitmap]::new($Size, $Size)
        try {
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
                $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $graphics.Clear([System.Drawing.Color]::Transparent)
                $graphics.DrawImage($sourceImage, 0, 0, $Size, $Size)
            } finally {
                $graphics.Dispose()
            }

            $bitmap.Save($Output, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $bitmap.Dispose()
        }
    } finally {
        $sourceImage.Dispose()
    }
}

$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$artifactDirPath = Resolve-RepoPath $ArtifactDir
$binaryPath = Resolve-RepoPath $Binary
$pdfiumPath = Resolve-RepoPath "pdfium"
$licensePath = Resolve-RepoPath "LICENSE"
$sourceOfferPath = Resolve-RepoPath "packaging/SOURCE-OFFER.txt"
$pdfiumLicensePath = Resolve-RepoPath "pdfium/LICENSE"
$sourceIconPath = Resolve-RepoPath "assets/icons/source/quick-presenter-icon-1024.png"
$manifestTemplatePath = Resolve-RepoPath "packaging/windows/AppxManifest.xml.in"

if ([string]::IsNullOrWhiteSpace($OutputMsix)) {
    $OutputMsix = Join-Path $artifactDirPath "Quick Presenter.msix"
} else {
    $OutputMsix = Resolve-RepoPath $OutputMsix
}

if (-not (Test-Path $binaryPath -PathType Leaf)) {
    throw "Missing executable binary: $binaryPath"
}
if (-not (Test-Path $pdfiumPath -PathType Container)) {
    throw "Missing bundled PDFium directory: $pdfiumPath"
}
if (-not (Test-Path $licensePath -PathType Leaf)) {
    throw "Missing Quick Presenter license file: $licensePath"
}
if (-not (Test-Path $sourceOfferPath -PathType Leaf)) {
    throw "Missing Quick Presenter source offer file: $sourceOfferPath"
}
if (-not (Test-Path $pdfiumLicensePath -PathType Leaf)) {
    throw "Missing PDFium license file: $pdfiumLicensePath"
}
if (-not (Test-Path $sourceIconPath -PathType Leaf)) {
    throw "Missing source icon file: $sourceIconPath"
}
if (-not (Test-Path $manifestTemplatePath -PathType Leaf)) {
    throw "Missing MSIX manifest template: $manifestTemplatePath"
}
if ([string]::IsNullOrWhiteSpace($Publisher)) {
    throw "MSIX publisher must not be empty"
}

$makeAppx = Resolve-MakeAppx $MakeAppxCommand
$version = Convert-ToMsixVersion (Read-CargoVersion)
$stageDir = Join-Path $artifactDirPath "windows-msix-payload"
$assetDir = Join-Path $stageDir "Assets"
$licenseDir = Join-Path $stageDir "licenses"
$manifestPath = Join-Path $stageDir "AppxManifest.xml"

Remove-Item -Recurse -Force $stageDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $assetDir, $licenseDir | Out-Null

Copy-Item $binaryPath (Join-Path $stageDir "qp.exe")
Copy-Item -Recurse $pdfiumPath (Join-Path $stageDir "pdfium")
Copy-Item $licensePath (Join-Path $licenseDir "QuickPresenter-LICENSE.txt")
Copy-Item $sourceOfferPath (Join-Path $licenseDir "QuickPresenter-SOURCE-OFFER.txt")
Copy-Item $pdfiumLicensePath (Join-Path $licenseDir "PDFium-LICENSE.txt")

Copy-ResizedPng -Source $sourceIconPath -Output (Join-Path $assetDir "Square44x44Logo.png") -Size 44
Copy-ResizedPng -Source $sourceIconPath -Output (Join-Path $assetDir "Square150x150Logo.png") -Size 150

$manifest = Get-Content $manifestTemplatePath -Raw
$manifest = $manifest.Replace("{{PACKAGE_VERSION}}", $version)
$manifest = $manifest.Replace("{{PUBLISHER}}", [System.Security.SecurityElement]::Escape($Publisher))
Set-Content -Path $manifestPath -Value $manifest -Encoding UTF8

Remove-Item -Force $OutputMsix -ErrorAction SilentlyContinue

& $makeAppx pack /v /h SHA256 /d $stageDir /p $OutputMsix /o
if ($LASTEXITCODE -ne 0) {
    throw "MakeAppx pack failed with exit code $LASTEXITCODE"
}

if (-not (Test-Path $OutputMsix -PathType Leaf)) {
    throw "MSIX was not created: $OutputMsix"
}

if (-not $KeepWorkDir) {
    Remove-Item -Recurse -Force $stageDir -ErrorAction SilentlyContinue
}

Write-Host "Created $OutputMsix"
