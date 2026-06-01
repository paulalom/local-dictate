param(
    [string]$InstallDir = (Join-Path (Join-Path $env:LOCALAPPDATA "Programs") "Local Dictate"),

    [switch]$CreateDesktopShortcut,

    [switch]$Launch
)

$ErrorActionPreference = "Stop"

function Resolve-FullPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    return [System.IO.Path]::GetFullPath($Path)
}

function Normalize-ComparablePath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $trimChars = [char[]]@(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    )

    return (Resolve-FullPath $Path).TrimEnd($trimChars)
}

function Test-SamePath {
    param(
        [Parameter(Mandatory = $true)][string]$Left,
        [Parameter(Mandatory = $true)][string]$Right
    )

    return (Normalize-ComparablePath $Left).Equals(
        (Normalize-ComparablePath $Right),
        [System.StringComparison]::OrdinalIgnoreCase
    )
}

function Assert-InstallDir {
    param([Parameter(Mandatory = $true)][string]$Path)

    $resolvedPath = Normalize-ComparablePath $Path
    $localAppData = Normalize-ComparablePath $env:LOCALAPPDATA
    $requiredPrefix = "$localAppData$([System.IO.Path]::DirectorySeparatorChar)"
    $pathRoot = [System.IO.Path]::GetPathRoot($resolvedPath).TrimEnd([char[]]@(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    ))

    if ($resolvedPath.Equals($localAppData, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not $resolvedPath.StartsWith($requiredPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "InstallDir must be inside LOCALAPPDATA for this per-user installer: $resolvedPath"
    }

    if ($resolvedPath.Equals($pathRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to install into a drive root: $resolvedPath"
    }
}

function Get-PackageRoot {
    $candidates = @(
        $PSScriptRoot,
        (Split-Path -Parent $PSScriptRoot)
    )

    foreach ($candidate in $candidates) {
        if (-not [string]::IsNullOrWhiteSpace($candidate)) {
            $candidatePath = Resolve-FullPath $candidate
            if (Test-Path -LiteralPath (Join-Path $candidatePath "local-dictate-ui.exe") -PathType Leaf) {
                return $candidatePath
            }
        }
    }

    throw "Run this script from the extracted Local Dictate Windows release folder."
}

function New-Shortcut {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$TargetPath,
        [string]$Arguments = "",
        [string]$WorkingDirectory = "",
        [string]$Description = "Local Dictate",
        [string]$IconLocation = ""
    )

    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null

    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($Path)
    $shortcut.TargetPath = $TargetPath
    $shortcut.Arguments = $Arguments
    $shortcut.WorkingDirectory = $WorkingDirectory
    $shortcut.Description = $Description

    if (-not [string]::IsNullOrWhiteSpace($IconLocation)) {
        $shortcut.IconLocation = $IconLocation
    }

    $shortcut.Save()
}

function Get-ShortcutTargetPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    try {
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($Path)
        return $shortcut.TargetPath
    } catch {
        return $null
    }
}

function Get-PowerShellExecutable {
    $windowsPowerShell = Get-Command powershell.exe -ErrorAction SilentlyContinue
    if ($windowsPowerShell) {
        return $windowsPowerShell.Source
    }

    $powerShellCore = Get-Command pwsh.exe -ErrorAction SilentlyContinue
    if ($powerShellCore) {
        return $powerShellCore.Source
    }

    throw "Unable to find a PowerShell executable for the uninstall shortcut."
}

function Stop-InstalledApp {
    param([Parameter(Mandatory = $true)][string]$ExecutablePath)

    if (-not (Test-Path -LiteralPath $ExecutablePath -PathType Leaf)) {
        return 0
    }

    $resolvedExecutablePath = Normalize-ComparablePath $ExecutablePath
    $processes = @(
        Get-CimInstance Win32_Process -Filter "Name = 'local-dictate-ui.exe'" -ErrorAction SilentlyContinue |
            Where-Object {
                $_.ExecutablePath -and
                (Normalize-ComparablePath $_.ExecutablePath).Equals(
                    $resolvedExecutablePath,
                    [System.StringComparison]::OrdinalIgnoreCase
                )
            }
    )

    foreach ($process in $processes) {
        Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
        try {
            Wait-Process -Id $process.ProcessId -Timeout 10 -ErrorAction SilentlyContinue
        } catch {
            # The process may have already exited.
        }
    }

    return $processes.Count
}

function Copy-PackagePayload {
    param(
        [Parameter(Mandatory = $true)][string]$PackageRoot,
        [Parameter(Mandatory = $true)][string]$DestinationRoot
    )

    New-Item -ItemType Directory -Force -Path $DestinationRoot | Out-Null

    foreach ($item in Get-ChildItem -LiteralPath $PackageRoot -Force) {
        if (Test-SamePath -Left $item.FullName -Right $DestinationRoot) {
            continue
        }

        Copy-Item -LiteralPath $item.FullName -Destination $DestinationRoot -Recurse -Force
    }
}

function Update-ExistingTaskbarPins {
    param(
        [Parameter(Mandatory = $true)][string]$ExecutablePath,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    $taskbarDir = Join-Path $env:APPDATA "Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"
    if (-not (Test-Path -LiteralPath $taskbarDir -PathType Container)) {
        return 0
    }

    $updatedCount = 0
    foreach ($pin in Get-ChildItem -LiteralPath $taskbarDir -Filter "*.lnk" -File) {
        $targetPath = Get-ShortcutTargetPath -Path $pin.FullName
        $targetFileName = if ($targetPath) {
            [System.IO.Path]::GetFileName($targetPath)
        } else {
            ""
        }

        $matchesLocalDictate =
            $pin.BaseName -match "(?i)local[- ]dictate" -or
            $targetFileName.Equals("local-dictate-ui.exe", [System.StringComparison]::OrdinalIgnoreCase)

        if ($matchesLocalDictate) {
            New-Shortcut `
                -Path $pin.FullName `
                -TargetPath $ExecutablePath `
                -WorkingDirectory $WorkingDirectory `
                -Description "Local Dictate" `
                -IconLocation $ExecutablePath
            $updatedCount += 1
        }
    }

    return $updatedCount
}

$packageRoot = Get-PackageRoot
$resolvedInstallDir = Resolve-FullPath $InstallDir
Assert-InstallDir -Path $resolvedInstallDir

$installedUiPath = Join-Path $resolvedInstallDir "local-dictate-ui.exe"
$stoppedCount = Stop-InstalledApp -ExecutablePath $installedUiPath

if (-not (Test-SamePath -Left $packageRoot -Right $resolvedInstallDir)) {
    Copy-PackagePayload -PackageRoot $packageRoot -DestinationRoot $resolvedInstallDir
}

if (-not (Test-Path -LiteralPath $installedUiPath -PathType Leaf)) {
    throw "Installed UI executable is missing: $installedUiPath"
}

$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Local Dictate"
$startMenuShortcut = Join-Path $startMenuDir "Local Dictate.lnk"
$uninstallScript = Join-Path $resolvedInstallDir "Uninstall-LocalDictate.ps1"
$uninstallShortcut = Join-Path $startMenuDir "Uninstall Local Dictate.lnk"

New-Shortcut `
    -Path $startMenuShortcut `
    -TargetPath $installedUiPath `
    -WorkingDirectory $resolvedInstallDir `
    -Description "Local Dictate" `
    -IconLocation $installedUiPath

if (Test-Path -LiteralPath $uninstallScript -PathType Leaf) {
    $powerShellPath = Get-PowerShellExecutable
    New-Shortcut `
        -Path $uninstallShortcut `
        -TargetPath $powerShellPath `
        -Arguments "-NoProfile -ExecutionPolicy Bypass -File `"$uninstallScript`"" `
        -WorkingDirectory $env:USERPROFILE `
        -Description "Uninstall Local Dictate" `
        -IconLocation $installedUiPath
}

if ($CreateDesktopShortcut) {
    $desktopShortcut = Join-Path ([Environment]::GetFolderPath("Desktop")) "Local Dictate.lnk"
    New-Shortcut `
        -Path $desktopShortcut `
        -TargetPath $installedUiPath `
        -WorkingDirectory $resolvedInstallDir `
        -Description "Local Dictate" `
        -IconLocation $installedUiPath
}

$updatedTaskbarPins = Update-ExistingTaskbarPins `
    -ExecutablePath $installedUiPath `
    -WorkingDirectory $resolvedInstallDir

Write-Host "Installed Local Dictate to $resolvedInstallDir"
Write-Host "Start Menu shortcut: $startMenuShortcut"

if ($stoppedCount -gt 0) {
    Write-Host "Stopped $stoppedCount running Local Dictate process(es) before updating."
}

if ($updatedTaskbarPins -gt 0) {
    Write-Host "Updated $updatedTaskbarPins existing taskbar shortcut(s)."
} else {
    Write-Host "Pin Local Dictate from the Start Menu to keep the taskbar target stable across updates."
}

if ($Launch) {
    Start-Process -FilePath $installedUiPath -WorkingDirectory $resolvedInstallDir
}
