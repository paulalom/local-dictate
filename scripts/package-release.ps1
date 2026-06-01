param(
    [Parameter(Mandatory = $true)]
    [string]$Version,

    [Parameter(Mandatory = $true)]
    [ValidateSet("windows-x64", "macos-universal", "linux-x64")]
    [string]$Platform,

    [Parameter(Mandatory = $true)]
    [string]$BinaryDir,

    [string]$OutputDir = "dist",

    [switch]$RequireEngine,

    [ValidateSet("tiny.en", "base.en", "small.en", "base", "small")]
    [string]$DefaultModel = "base.en",

    [switch]$RequireModel
)

$ErrorActionPreference = "Stop"
$isWindowsHost = if (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue) {
    $IsWindows
} else {
    $env:OS -eq "Windows_NT"
}

function Resolve-RepoPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }

    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Assert-WithinPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Parent
    )

    $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    $resolvedParent = [System.IO.Path]::GetFullPath($Parent)

    if (-not $resolvedParent.EndsWith([System.IO.Path]::DirectorySeparatorChar)) {
        $resolvedParent = "$resolvedParent$([System.IO.Path]::DirectorySeparatorChar)"
    }

    if (-not $resolvedPath.StartsWith($resolvedParent, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to operate on path outside expected parent: $resolvedPath"
    }
}

function Copy-Executable {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "Missing release binary: $Source"
    }

    Copy-Item -LiteralPath $Source -Destination $Destination -Force

    if (-not $isWindowsHost) {
        & chmod +x $Destination
    }
}

function Copy-EngineAssets {
    $engineRoot = Join-Path $repoRoot "engines"
    $destinationRoot = Join-Path $packageDir "engines"
    $engineFileName = if ($Platform -eq "windows-x64") {
        "whisper-cli.exe"
    } else {
        "whisper-cli"
    }
    $enginePath = Join-Path $engineRoot $engineFileName

    if (-not (Test-Path -LiteralPath $enginePath -PathType Leaf)) {
        if ($RequireEngine) {
            throw "Missing bundled whisper.cpp engine: $enginePath"
        }

        Write-Warning "Skipping bundled whisper.cpp engine; missing $enginePath"
        return
    }

    New-Item -ItemType Directory -Force -Path $destinationRoot | Out-Null
    Copy-Item -LiteralPath $enginePath -Destination (Join-Path $destinationRoot $engineFileName) -Force

    if ($Platform -eq "windows-x64") {
        Get-ChildItem -LiteralPath $engineRoot -Filter "*.dll" -File |
            Copy-Item -Destination $destinationRoot -Force
    } else {
        & chmod +x (Join-Path $destinationRoot $engineFileName)
    }
}

function Copy-ModelAssets {
    $modelRoot = Join-Path $repoRoot "models"
    $destinationRoot = Join-Path $packageDir "models"
    $modelFileName = "ggml-$DefaultModel.bin"
    $modelPath = Join-Path $modelRoot $modelFileName

    if (-not (Test-Path -LiteralPath $modelPath -PathType Leaf)) {
        if ($RequireModel) {
            throw "Missing bundled Whisper model: $modelPath"
        }

        Write-Warning "Skipping bundled Whisper model; missing $modelPath"
        return
    }

    New-Item -ItemType Directory -Force -Path $destinationRoot | Out-Null
    Copy-Item -LiteralPath $modelPath -Destination (Join-Path $destinationRoot $modelFileName) -Force
}

function Copy-SetupScripts {
    $scriptsDir = Join-Path $packageDir "scripts"
    New-Item -ItemType Directory -Force -Path $scriptsDir | Out-Null

    Copy-Item `
        -LiteralPath (Join-Path (Join-Path $repoRoot "scripts") "install-whisper-model.ps1") `
        -Destination (Join-Path $scriptsDir "install-whisper-model.ps1")

    if ($Platform -eq "windows-x64") {
        Copy-Item `
            -LiteralPath (Join-Path (Join-Path $repoRoot "scripts") "install-whisper-assets.ps1") `
            -Destination (Join-Path $scriptsDir "install-whisper-assets.ps1")
        Copy-Item `
            -LiteralPath (Join-Path (Join-Path $repoRoot "scripts") "install-windows.ps1") `
            -Destination (Join-Path $packageDir "Install-LocalDictate.ps1")
        Copy-Item `
            -LiteralPath (Join-Path (Join-Path $repoRoot "scripts") "uninstall-windows.ps1") `
            -Destination (Join-Path $packageDir "Uninstall-LocalDictate.ps1")
    } elseif ($Platform -eq "linux-x64") {
        Copy-Item `
            -LiteralPath (Join-Path (Join-Path $repoRoot "scripts") "install-linux.sh") `
            -Destination (Join-Path $packageDir "install-linux.sh")

        if (-not $isWindowsHost) {
            & chmod +x (Join-Path $packageDir "install-linux.sh")
        }
    }
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$binaryRoot = Resolve-RepoPath $BinaryDir
$outputRoot = Resolve-RepoPath $OutputDir
$stageRoot = Join-Path (Join-Path $repoRoot ".local") "package"
$packageName = "local-dictate-$Version-$Platform"
$packageDir = Join-Path $stageRoot $packageName

New-Item -ItemType Directory -Force -Path $stageRoot, $outputRoot | Out-Null
Assert-WithinPath -Path $packageDir -Parent $stageRoot

if (Test-Path -LiteralPath $packageDir) {
    Remove-Item -LiteralPath $packageDir -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $packageDir | Out-Null

Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination $packageDir
Copy-Item -LiteralPath (Join-Path $repoRoot "THIRD_PARTY_NOTICES.md") -Destination $packageDir
Copy-EngineAssets
Copy-ModelAssets
Copy-SetupScripts

switch ($Platform) {
    "windows-x64" {
        Copy-Executable `
            -Source (Join-Path $binaryRoot "local-dictate-ui.exe") `
            -Destination (Join-Path $packageDir "local-dictate-ui.exe")
        Copy-Executable `
            -Source (Join-Path $binaryRoot "local-dictate-cli.exe") `
            -Destination (Join-Path $packageDir "local-dictate-cli.exe")

        $archivePath = Join-Path $outputRoot "$packageName.zip"
        if (Test-Path -LiteralPath $archivePath) {
            Remove-Item -LiteralPath $archivePath -Force
        }

        Compress-Archive -Path $packageDir -DestinationPath $archivePath -CompressionLevel Optimal
    }

    "macos-universal" {
        $appRoot = Join-Path $packageDir "Local Dictate.app"
        $contentsDir = Join-Path $appRoot "Contents"
        $macosDir = Join-Path $contentsDir "MacOS"
        $resourcesDir = Join-Path $contentsDir "Resources"
        $binDir = Join-Path $packageDir "bin"

        New-Item -ItemType Directory -Force -Path $macosDir, $resourcesDir, $binDir | Out-Null

        Copy-Item `
            -LiteralPath (Join-Path (Join-Path (Join-Path $repoRoot "packaging") "macos") "Info.plist") `
            -Destination (Join-Path $contentsDir "Info.plist")
        Copy-Executable `
            -Source (Join-Path $binaryRoot "local-dictate-ui") `
            -Destination (Join-Path $macosDir "local-dictate-ui")
        Copy-Executable `
            -Source (Join-Path $binaryRoot "local-dictate-cli") `
            -Destination (Join-Path $binDir "local-dictate-cli")

        $archivePath = Join-Path $outputRoot "$packageName.zip"
        if (Test-Path -LiteralPath $archivePath) {
            Remove-Item -LiteralPath $archivePath -Force
        }

        Push-Location $stageRoot
        try {
            & ditto -c -k --sequesterRsrc --keepParent $packageName $archivePath
        } finally {
            Pop-Location
        }
    }

    "linux-x64" {
        $binDir = Join-Path $packageDir "bin"
        $shareDir = Join-Path (Join-Path $packageDir "share") "applications"

        New-Item -ItemType Directory -Force -Path $binDir, $shareDir | Out-Null

        Copy-Executable `
            -Source (Join-Path $binaryRoot "local-dictate-ui") `
            -Destination (Join-Path $binDir "local-dictate-ui")
        Copy-Executable `
            -Source (Join-Path $binaryRoot "local-dictate-cli") `
            -Destination (Join-Path $binDir "local-dictate-cli")
        Copy-Item `
            -LiteralPath (Join-Path (Join-Path (Join-Path $repoRoot "packaging") "linux") "local-dictate.desktop") `
            -Destination (Join-Path $shareDir "local-dictate.desktop")

        $archivePath = Join-Path $outputRoot "$packageName.tar.gz"
        if (Test-Path -LiteralPath $archivePath) {
            Remove-Item -LiteralPath $archivePath -Force
        }

        Push-Location $stageRoot
        try {
            & tar -czf $archivePath $packageName
        } finally {
            Pop-Location
        }
    }
}

Write-Host "Packaged $archivePath"
