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

$modelHashes = @{
    "tiny.en"  = "c78c86eb1a8faa21b369bcd33207cc90d64ae9df"
    "base.en"  = "137c40403d78fd54d454da0f9bd998f78703390c"
    "small.en" = "db8a495a91d927739e50b3fc1cc4c6b8f6c2d022"
    "base"     = "465707469ff3a37a2b9b8d8f89f2f99de7299dac"
    "small"    = "55356645c2b361a969dfd0ef2c5a50d530afd8d5"
}

$modelPath = Join-Path $modelsDir "ggml-$Model.bin"
$modelUrl = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-$Model.bin"

if (-not (Test-Path -LiteralPath $modelPath -PathType Leaf)) {
    Write-Host "Downloading ggml model $Model from $modelUrl"
    Invoke-WebRequest -Uri $modelUrl -OutFile $modelPath
}

$expectedHash = $modelHashes[$Model]
$actualHash = (Get-FileHash -Path $modelPath -Algorithm SHA1).Hash.ToLowerInvariant()

if ($actualHash -ne $expectedHash) {
    throw "Model hash mismatch for $modelPath. Expected $expectedHash, got $actualHash."
}

Write-Host "Installed ggml model to $modelPath"
