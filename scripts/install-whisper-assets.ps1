param(
    [string]$WhisperCppVersion = "v1.8.4",
    [ValidateSet("tiny.en", "base.en", "small.en", "base", "small")]
    [string]$Model = "base.en",
    [string]$Destination = "",
    [switch]$SkipModel
)

$ErrorActionPreference = "Stop"

$repoRoot = if ($Destination.Trim()) {
    [System.IO.Path]::GetFullPath($Destination)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
}

$engineDir = Join-Path $repoRoot "engines"
$modelsDir = Join-Path $repoRoot "models"
$downloadDir = Join-Path $repoRoot ".local\downloads"

New-Item -ItemType Directory -Force -Path $engineDir, $modelsDir, $downloadDir | Out-Null

$assetName = if ([Environment]::Is64BitOperatingSystem) {
    "whisper-bin-x64.zip"
} else {
    "whisper-bin-Win32.zip"
}

$engineZip = Join-Path $downloadDir "$WhisperCppVersion-$assetName"
$engineUrl = "https://github.com/ggml-org/whisper.cpp/releases/download/$WhisperCppVersion/$assetName"

if (-not (Test-Path -LiteralPath $engineZip -PathType Leaf)) {
    Write-Host "Downloading whisper.cpp $WhisperCppVersion from $engineUrl"
    Invoke-WebRequest -Uri $engineUrl -OutFile $engineZip
}

$extractDir = Join-Path $downloadDir "whisper-cpp-$WhisperCppVersion-$([guid]::NewGuid())"
New-Item -ItemType Directory -Force -Path $extractDir | Out-Null
Expand-Archive -Path $engineZip -DestinationPath $extractDir -Force

$whisperCli = Get-ChildItem -Path $extractDir -Filter "whisper-cli.exe" -Recurse -File |
    Select-Object -First 1

if (-not $whisperCli) {
    throw "Could not find whisper-cli.exe in $engineZip"
}

$runtimeFiles = @($whisperCli) + @(
    Get-ChildItem -LiteralPath $whisperCli.DirectoryName -Filter "*.dll" -File
)

$runtimeFiles |
    Copy-Item -Destination $engineDir -Force

Write-Host "Installed whisper.cpp CLI runtime files to $engineDir"

if ($SkipModel) {
    return
}

& (Join-Path $PSScriptRoot "install-whisper-model.ps1") `
    -Model $Model `
    -Destination $repoRoot
