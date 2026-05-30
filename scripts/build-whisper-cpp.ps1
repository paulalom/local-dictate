param(
    [string]$WhisperCppVersion = "v1.8.4",

    [Parameter(Mandatory = $true)]
    [ValidateSet("macos-universal", "linux-x64")]
    [string]$Platform,

    [string]$Destination = ""
)

$ErrorActionPreference = "Stop"

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

function Invoke-NativeCommand {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$ArgumentList = @()
    )

    & $FilePath @ArgumentList

    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE"
    }
}

function Invoke-WhisperCppBuild {
    param(
        [Parameter(Mandatory = $true)][string]$BuildDir,
        [string[]]$ExtraCmakeArgs = @()
    )

    New-Item -ItemType Directory -Force -Path $BuildDir | Out-Null

    $cmakeArgs = @(
        "-S", $sourceRoot,
        "-B", $BuildDir,
        "-DCMAKE_BUILD_TYPE=Release",
        "-DBUILD_SHARED_LIBS=OFF",
        "-DGGML_NATIVE=OFF",
        "-DWHISPER_BUILD_TESTS=OFF",
        "-DWHISPER_BUILD_SERVER=OFF"
    ) + $ExtraCmakeArgs

    Invoke-NativeCommand -FilePath "cmake" -ArgumentList $cmakeArgs
    Invoke-NativeCommand `
        -FilePath "cmake" `
        -ArgumentList @("--build", $BuildDir, "--config", "Release", "--target", "whisper-cli", "--parallel")
}

function Find-WhisperCli {
    param([Parameter(Mandatory = $true)][string]$BuildDir)

    $candidateNames = if ($Platform -eq "windows-x64") {
        @("whisper-cli.exe")
    } else {
        @("whisper-cli")
    }

    foreach ($candidateName in $candidateNames) {
        $candidate = Get-ChildItem -LiteralPath $BuildDir -Filter $candidateName -Recurse -File |
            Select-Object -First 1

        if ($candidate) {
            return $candidate.FullName
        }
    }

    throw "Could not find whisper-cli in $BuildDir"
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$destinationRoot = if ($Destination.Trim()) {
    Resolve-RepoPath $Destination
} else {
    Join-Path $repoRoot "engines"
}

$localRoot = Join-Path $repoRoot ".local"
$sourceParent = Join-Path $localRoot "whisper.cpp"
$buildParent = Join-Path $localRoot "whisper.cpp-build"
$sourceRoot = Join-Path $sourceParent $WhisperCppVersion

New-Item -ItemType Directory -Force -Path $sourceParent, $buildParent, $destinationRoot | Out-Null

if (-not (Test-Path -LiteralPath (Join-Path $sourceRoot ".git") -PathType Container)) {
    if (Test-Path -LiteralPath $sourceRoot) {
        Assert-WithinPath -Path $sourceRoot -Parent $sourceParent
        Remove-Item -LiteralPath $sourceRoot -Recurse -Force
    }

    Invoke-NativeCommand `
        -FilePath "git" `
        -ArgumentList @(
            "clone",
            "--depth", "1",
            "--branch", $WhisperCppVersion,
            "https://github.com/ggml-org/whisper.cpp.git",
            $sourceRoot
        )
}

switch ($Platform) {
    "linux-x64" {
        $buildDir = Join-Path (Join-Path $buildParent $WhisperCppVersion) "linux-x64"
        Invoke-WhisperCppBuild -BuildDir $buildDir

        $binary = Find-WhisperCli -BuildDir $buildDir
        $destination = Join-Path $destinationRoot "whisper-cli"

        Copy-Item -LiteralPath $binary -Destination $destination -Force
        Invoke-NativeCommand -FilePath "chmod" -ArgumentList @("+x", $destination)
    }

    "macos-universal" {
        $versionBuildRoot = Join-Path $buildParent $WhisperCppVersion
        $arm64BuildDir = Join-Path $versionBuildRoot "macos-arm64"
        $x64BuildDir = Join-Path $versionBuildRoot "macos-x64"

        Invoke-WhisperCppBuild `
            -BuildDir $arm64BuildDir `
            -ExtraCmakeArgs @(
                "-DCMAKE_OSX_ARCHITECTURES=arm64",
                "-DCMAKE_OSX_DEPLOYMENT_TARGET=11.0"
            )
        Invoke-WhisperCppBuild `
            -BuildDir $x64BuildDir `
            -ExtraCmakeArgs @(
                "-DCMAKE_OSX_ARCHITECTURES=x86_64",
                "-DCMAKE_OSX_DEPLOYMENT_TARGET=11.0"
            )

        $arm64Binary = Find-WhisperCli -BuildDir $arm64BuildDir
        $x64Binary = Find-WhisperCli -BuildDir $x64BuildDir
        $destination = Join-Path $destinationRoot "whisper-cli"

        Invoke-NativeCommand `
            -FilePath "lipo" `
            -ArgumentList @("-create", "-output", $destination, $arm64Binary, $x64Binary)
        Invoke-NativeCommand -FilePath "chmod" -ArgumentList @("+x", $destination)
    }
}

Write-Host "Installed whisper.cpp $WhisperCppVersion $Platform engine to $destinationRoot"
