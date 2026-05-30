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
        "-DWHISPER_BUILD_TESTS=OFF",
        "-DWHISPER_BUILD_SERVER=OFF"
    ) + $ExtraCmakeArgs

    & cmake @cmakeArgs
    & cmake --build $BuildDir --config Release --target whisper-cli --parallel
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

    & git clone `
        --depth 1 `
        --branch $WhisperCppVersion `
        https://github.com/ggml-org/whisper.cpp.git `
        $sourceRoot
}

switch ($Platform) {
    "linux-x64" {
        $buildDir = Join-Path (Join-Path $buildParent $WhisperCppVersion) "linux-x64"
        Invoke-WhisperCppBuild -BuildDir $buildDir

        $binary = Find-WhisperCli -BuildDir $buildDir
        $destination = Join-Path $destinationRoot "whisper-cli"

        Copy-Item -LiteralPath $binary -Destination $destination -Force
        & chmod +x $destination
    }

    "macos-universal" {
        $versionBuildRoot = Join-Path $buildParent $WhisperCppVersion
        $arm64BuildDir = Join-Path $versionBuildRoot "macos-arm64"
        $x64BuildDir = Join-Path $versionBuildRoot "macos-x64"

        Invoke-WhisperCppBuild `
            -BuildDir $arm64BuildDir `
            -ExtraCmakeArgs @("-DCMAKE_OSX_ARCHITECTURES=arm64")
        Invoke-WhisperCppBuild `
            -BuildDir $x64BuildDir `
            -ExtraCmakeArgs @("-DCMAKE_OSX_ARCHITECTURES=x86_64")

        $arm64Binary = Find-WhisperCli -BuildDir $arm64BuildDir
        $x64Binary = Find-WhisperCli -BuildDir $x64BuildDir
        $destination = Join-Path $destinationRoot "whisper-cli"

        & lipo -create -output $destination $arm64Binary $x64Binary
        & chmod +x $destination
    }
}

Write-Host "Installed whisper.cpp $WhisperCppVersion $Platform engine to $destinationRoot"
