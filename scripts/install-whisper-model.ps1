param(
    [ValidateSet("tiny.en", "base.en", "small.en", "base", "small")]
    [string]$Model = "base.en",

    [string]$Destination = ""
)

$ErrorActionPreference = "Stop"

function Invoke-DownloadWithRetry {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$OutFile,
        [int]$MaxAttempts = 5
    )

    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt += 1) {
        try {
            Invoke-WebRequest -Uri $Uri -OutFile $OutFile
            return
        } catch {
            if ($attempt -ge $MaxAttempts) {
                throw
            }

            $statusCode = $null
            if ($_.Exception.Response -and $_.Exception.Response.StatusCode) {
                $statusCode = [int]$_.Exception.Response.StatusCode
            }

            $delaySeconds = if ($statusCode -eq 429) {
                [Math]::Min(300, [int](15 * [Math]::Pow(2, $attempt - 1)))
            } else {
                [Math]::Min(60, 5 * $attempt)
            }

            $statusText = if ($statusCode) {
                " HTTP $statusCode"
            } else {
                ""
            }

            Write-Warning "Download failed on attempt $attempt of $MaxAttempts.$statusText Retrying in $delaySeconds seconds."
            Start-Sleep -Seconds $delaySeconds
        }
    }
}

$repoRoot = if ($Destination.Trim()) {
    [System.IO.Path]::GetFullPath($Destination)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
}

$modelsDir = Join-Path $repoRoot "models"
$downloadDir = Join-Path $repoRoot ".local/downloads"

New-Item -ItemType Directory -Force -Path $modelsDir, $downloadDir | Out-Null

$modelHashes = @{
    "tiny.en"  = "c78c86eb1a8faa21b369bcd33207cc90d64ae9df"
    "base.en"  = "137c40403d78fd54d454da0f9bd998f78703390c"
    "small.en" = "db8a495a91d927739e50b3fc1cc4c6b8f6c2d022"
    "base"     = "465707469ff3a37a2b9b8d8f89f2f99de7299dac"
    "small"    = "55356645c2b361a969dfd0ef2c5a50d530afd8d5"
}

$modelFileName = "ggml-$Model.bin"
$modelPath = Join-Path $modelsDir $modelFileName
$modelUrl = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$modelFileName"
$expectedHash = $modelHashes[$Model]

if (-not (Test-Path -LiteralPath $modelPath -PathType Leaf)) {
    $downloadPath = Join-Path $downloadDir $modelFileName

    if (Test-Path -LiteralPath $downloadPath) {
        Remove-Item -LiteralPath $downloadPath -Force
    }

    Write-Host "Downloading ggml model $Model from $modelUrl"
    Invoke-DownloadWithRetry -Uri $modelUrl -OutFile $downloadPath

    $downloadHash = (Get-FileHash -Path $downloadPath -Algorithm SHA1).Hash.ToLowerInvariant()
    if ($downloadHash -ne $expectedHash) {
        throw "Model hash mismatch for $downloadPath. Expected $expectedHash, got $downloadHash."
    }

    Move-Item -LiteralPath $downloadPath -Destination $modelPath -Force
}

$actualHash = (Get-FileHash -Path $modelPath -Algorithm SHA1).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash) {
    throw "Model hash mismatch for $modelPath. Expected $expectedHash, got $actualHash."
}

Write-Host "Installed ggml model to $modelPath"
