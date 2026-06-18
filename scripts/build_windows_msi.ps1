param(
    [string]$ArtifactDir = "artifacts/quick-presenter-windows-x64",
    [string]$Binary = "target/release/qp.exe",
    [string]$OutputMsi = "",
    [string]$WixCommand = "wix",
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

function Convert-ToMsiVersion {
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
            throw "MSI product version must contain numeric components: $Version"
        }
    }

    return ($parts -join ".")
}

function ConvertTo-WixXmlText {
    param([string]$Value)
    return [System.Security.SecurityElement]::Escape($Value)
}

function ConvertTo-WixSourcePath {
    param([string]$RelativePath)
    return '$(var.SourceDir)\' + ($RelativePath -replace '/', '\')
}

function Add-WixDirectoryRefs {
    param(
        [System.Text.StringBuilder]$Builder,
        [object[]]$Directories,
        [hashtable]$DirectoryIds
    )

    foreach ($directory in $Directories) {
        $relative = $directory.Relative
        $name = Split-Path $relative -Leaf
        $parent = Split-Path $relative -Parent
        $parentId = if ([string]::IsNullOrEmpty($parent)) { "INSTALLFOLDER" } else { $DirectoryIds[$parent] }

        [void]$Builder.AppendLine("    <DirectoryRef Id=`"$parentId`">")
        [void]$Builder.AppendLine("      <Directory Id=`"$($directory.Id)`" Name=`"$(ConvertTo-WixXmlText $name)`" />")
        [void]$Builder.AppendLine("    </DirectoryRef>")
    }
}

function New-WixFilesFragment {
    param(
        [string]$StageDir,
        [string]$OutputPath
    )

    $directoryItems = Get-ChildItem $StageDir -Directory -Recurse |
        ForEach-Object {
            [pscustomobject]@{
                FullName = $_.FullName
                Relative = [System.IO.Path]::GetRelativePath($StageDir, $_.FullName)
            }
        } |
        Sort-Object Relative

    $directoryIds = @{}
    $index = 1
    foreach ($directory in $directoryItems) {
        $directory | Add-Member -NotePropertyName Id -NotePropertyValue ("dir_{0}" -f $index)
        $directoryIds[$directory.Relative] = $directory.Id
        $index += 1
    }

    $fileItems = Get-ChildItem $StageDir -File -Recurse |
        ForEach-Object {
            [pscustomobject]@{
                FullName = $_.FullName
                Relative = [System.IO.Path]::GetRelativePath($StageDir, $_.FullName)
            }
        } |
        Sort-Object Relative

    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.AppendLine('<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">')
    [void]$builder.AppendLine("  <Fragment>")
    Add-WixDirectoryRefs -Builder $builder -Directories $directoryItems -DirectoryIds $directoryIds
    [void]$builder.AppendLine("  </Fragment>")
    [void]$builder.AppendLine("  <Fragment>")
    [void]$builder.AppendLine('    <ComponentGroup Id="ApplicationFiles">')

    $index = 1
    foreach ($file in $fileItems) {
        $parent = Split-Path $file.Relative -Parent
        $directoryId = if ([string]::IsNullOrEmpty($parent)) { "INSTALLFOLDER" } else { $directoryIds[$parent] }
        $source = ConvertTo-WixSourcePath $file.Relative

        [void]$builder.AppendLine("      <Component Id=`"cmp_$index`" Directory=`"$directoryId`" Guid=`"*`">")
        [void]$builder.AppendLine("        <File Id=`"file_$index`" Source=`"$(ConvertTo-WixXmlText $source)`" KeyPath=`"yes`" />")
        [void]$builder.AppendLine("      </Component>")
        $index += 1
    }

    [void]$builder.AppendLine("    </ComponentGroup>")
    [void]$builder.AppendLine("  </Fragment>")
    [void]$builder.AppendLine("</Wix>")

    Set-Content -Path $OutputPath -Value $builder.ToString() -Encoding UTF8
}

$RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$artifactDirPath = Resolve-RepoPath $ArtifactDir
$binaryPath = Resolve-RepoPath $Binary
$pdfiumPath = Resolve-RepoPath "pdfium"
$licensePath = Resolve-RepoPath "LICENSE"
$sourceOfferPath = Resolve-RepoPath "packaging/SOURCE-OFFER.txt"
$pdfiumLicensePath = Resolve-RepoPath "pdfium/LICENSE"
$iconPath = Resolve-RepoPath "assets/icons/windows/quick-presenter.ico"
$productWxs = Resolve-RepoPath "packaging/windows/Product.wxs"

if ([string]::IsNullOrWhiteSpace($OutputMsi)) {
    $OutputMsi = Join-Path $artifactDirPath "Quick Presenter.msi"
} else {
    $OutputMsi = Resolve-RepoPath $OutputMsi
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
if (-not (Test-Path $iconPath -PathType Leaf)) {
    throw "Missing Windows icon file: $iconPath"
}

$wix = Get-Command $WixCommand -ErrorAction Stop
$version = Convert-ToMsiVersion (Read-CargoVersion)
$stageDir = Join-Path $artifactDirPath "windows-installer-payload"
$licenseDir = Join-Path $stageDir "licenses"
$wixWorkDir = Join-Path $artifactDirPath "wix"
$generatedWxs = Join-Path $wixWorkDir "Files.wxs"

Remove-Item -Recurse -Force $stageDir, $wixWorkDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stageDir, $licenseDir, $wixWorkDir | Out-Null

Copy-Item $binaryPath (Join-Path $stageDir "qp.exe")
Copy-Item -Recurse $pdfiumPath (Join-Path $stageDir "pdfium")
Copy-Item $licensePath (Join-Path $licenseDir "QuickPresenter-LICENSE.txt")
Copy-Item $sourceOfferPath (Join-Path $licenseDir "QuickPresenter-SOURCE-OFFER.txt")
Copy-Item $pdfiumLicensePath (Join-Path $licenseDir "PDFium-LICENSE.txt")

New-WixFilesFragment -StageDir $stageDir -OutputPath $generatedWxs

Remove-Item -Force $OutputMsi -ErrorAction SilentlyContinue

& $wix.Source build `
    $productWxs `
    $generatedWxs `
    -d "SourceDir=$stageDir" `
    -d "ProductVersion=$version" `
    -d "IconPath=$iconPath" `
    -o $OutputMsi

if ($LASTEXITCODE -ne 0) {
    throw "WiX build failed with exit code $LASTEXITCODE"
}

if (-not (Test-Path $OutputMsi -PathType Leaf)) {
    throw "MSI was not created: $OutputMsi"
}

if (-not $KeepWorkDir) {
    Remove-Item -Recurse -Force $stageDir, $wixWorkDir -ErrorAction SilentlyContinue
}

Write-Host "Created $OutputMsi"
