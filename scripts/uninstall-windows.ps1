param(
    [string]$InstallDir = (Join-Path (Join-Path $env:LOCALAPPDATA "Programs") "Local Dictate")
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
        throw "InstallDir must be inside LOCALAPPDATA for this per-user uninstaller: $resolvedPath"
    }

    if ($resolvedPath.Equals($pathRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to uninstall from a drive root: $resolvedPath"
    }
}

function Test-PathWithin {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Parent
    )

    $resolvedPath = Normalize-ComparablePath $Path
    $resolvedParent = Normalize-ComparablePath $Parent
    $requiredPrefix = "$resolvedParent$([System.IO.Path]::DirectorySeparatorChar)"

    return $resolvedPath.Equals($resolvedParent, [System.StringComparison]::OrdinalIgnoreCase) -or
        $resolvedPath.StartsWith($requiredPrefix, [System.StringComparison]::OrdinalIgnoreCase)
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

function Remove-ShortcutIfTarget {
    param(
        [Parameter(Mandatory = $true)][string]$ShortcutPath,
        [Parameter(Mandatory = $true)][string]$TargetPath
    )

    if (-not (Test-Path -LiteralPath $ShortcutPath -PathType Leaf)) {
        return
    }

    $actualTarget = Get-ShortcutTargetPath -Path $ShortcutPath
    if ($actualTarget -and
        (Normalize-ComparablePath $actualTarget).Equals(
            (Normalize-ComparablePath $TargetPath),
            [System.StringComparison]::OrdinalIgnoreCase
        )) {
        Remove-Item -LiteralPath $ShortcutPath -Force
    }
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

$resolvedInstallDir = Resolve-FullPath $InstallDir
Assert-InstallDir -Path $resolvedInstallDir

$installedUiPath = Join-Path $resolvedInstallDir "local-dictate-ui.exe"
$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Local Dictate"
$desktopShortcut = Join-Path ([Environment]::GetFolderPath("Desktop")) "Local Dictate.lnk"
$taskbarDir = Join-Path $env:APPDATA "Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"

$stoppedCount = Stop-InstalledApp -ExecutablePath $installedUiPath

Remove-ShortcutIfTarget -ShortcutPath (Join-Path $startMenuDir "Local Dictate.lnk") -TargetPath $installedUiPath
Remove-Item -LiteralPath (Join-Path $startMenuDir "Uninstall Local Dictate.lnk") -Force -ErrorAction SilentlyContinue
Remove-ShortcutIfTarget -ShortcutPath $desktopShortcut -TargetPath $installedUiPath

if (Test-Path -LiteralPath $taskbarDir -PathType Container) {
    foreach ($shortcut in Get-ChildItem -LiteralPath $taskbarDir -Filter "*.lnk" -File) {
        Remove-ShortcutIfTarget -ShortcutPath $shortcut.FullName -TargetPath $installedUiPath
    }
}

if ((Test-Path -LiteralPath $startMenuDir -PathType Container) -and
    -not (Get-ChildItem -LiteralPath $startMenuDir -Force -ErrorAction SilentlyContinue)) {
    Remove-Item -LiteralPath $startMenuDir -Force
}

if (Test-PathWithin -Path (Get-Location).Path -Parent $resolvedInstallDir) {
    Set-Location $env:USERPROFILE
}

if (Test-Path -LiteralPath $resolvedInstallDir -PathType Container) {
    Remove-Item -LiteralPath $resolvedInstallDir -Recurse -Force
}

Write-Host "Uninstalled Local Dictate from $resolvedInstallDir"

if ($stoppedCount -gt 0) {
    Write-Host "Stopped $stoppedCount running Local Dictate process(es)."
}
