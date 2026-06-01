param(
    [string]$InstallDir = (Join-Path (Join-Path $env:LOCALAPPDATA "Programs") "Local Dictate"),

    [switch]$CreateDesktopShortcut,

    [switch]$Launch
)

$ErrorActionPreference = "Stop"
$appDescription = "Local, no-network voice dictation"

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

function Write-UInt16 {
    param(
        [Parameter(Mandatory = $true)][System.IO.BinaryWriter]$Writer,
        [Parameter(Mandatory = $true)][uint16]$Value
    )

    $Writer.Write($Value)
}

function Write-UInt32 {
    param(
        [Parameter(Mandatory = $true)][System.IO.BinaryWriter]$Writer,
        [Parameter(Mandatory = $true)][uint32]$Value
    )

    $Writer.Write($Value)
}

function Write-Int32 {
    param(
        [Parameter(Mandatory = $true)][System.IO.BinaryWriter]$Writer,
        [Parameter(Mandatory = $true)][int32]$Value
    )

    $Writer.Write($Value)
}

function Write-LocalDictateIcon {
    param([Parameter(Mandatory = $true)][string]$Path)

    $size = 32
    $rgba = New-Object byte[] ($size * $size * 4)

    for ($y = 0; $y -lt $size; $y += 1) {
        for ($x = 0; $x -lt $size; $x += 1) {
            $index = (($y * $size) + $x) * 4
            $dx = $x - 16
            $dy = $y - 16
            $inDisc = (($dx * $dx) + ($dy * $dy)) -le (15 * 15)

            if ($inDisc) {
                $rgba[$index] = 32
                $rgba[$index + 1] = 88
                $rgba[$index + 2] = 120
                $rgba[$index + 3] = 255
            }

            $micBody = (12 -le $x -and $x -le 19) -and (7 -le $y -and $y -le 19)
            $micStem = (15 -le $x -and $x -le 16) -and (21 -le $y -and $y -le 25)
            $micBase = (11 -le $x -and $x -le 20) -and (25 -le $y -and $y -le 26)
            $micCurve = (9 -le $x -and $x -le 22) -and
                (17 -le $y -and $y -le 23) -and
                -not (12 -le $x -and $x -le 19)

            if ($micBody -or $micStem -or $micBase -or $micCurve) {
                $rgba[$index] = 248
                $rgba[$index + 1] = 252
                $rgba[$index + 2] = 255
                $rgba[$index + 3] = 255
            }
        }
    }

    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null

    $pixelBytes = $size * $size * 4
    $maskBytes = [int]([Math]::Ceiling($size / 32.0) * 4 * $size)
    $imageBytes = 40 + $pixelBytes + $maskBytes
    $stream = [System.IO.File]::Create($Path)
    $writer = New-Object System.IO.BinaryWriter($stream)

    try {
        Write-UInt16 -Writer $writer -Value 0
        Write-UInt16 -Writer $writer -Value 1
        Write-UInt16 -Writer $writer -Value 1

        $writer.Write([byte]$size)
        $writer.Write([byte]$size)
        $writer.Write([byte]0)
        $writer.Write([byte]0)
        Write-UInt16 -Writer $writer -Value 1
        Write-UInt16 -Writer $writer -Value 32
        Write-UInt32 -Writer $writer -Value $imageBytes
        Write-UInt32 -Writer $writer -Value 22

        Write-UInt32 -Writer $writer -Value 40
        Write-Int32 -Writer $writer -Value $size
        Write-Int32 -Writer $writer -Value ($size * 2)
        Write-UInt16 -Writer $writer -Value 1
        Write-UInt16 -Writer $writer -Value 32
        Write-UInt32 -Writer $writer -Value 0
        Write-UInt32 -Writer $writer -Value $pixelBytes
        Write-Int32 -Writer $writer -Value 0
        Write-Int32 -Writer $writer -Value 0
        Write-UInt32 -Writer $writer -Value 0
        Write-UInt32 -Writer $writer -Value 0

        for ($y = $size - 1; $y -ge 0; $y -= 1) {
            for ($x = 0; $x -lt $size; $x += 1) {
                $index = (($y * $size) + $x) * 4
                $writer.Write($rgba[$index + 2])
                $writer.Write($rgba[$index + 1])
                $writer.Write($rgba[$index])
                $writer.Write($rgba[$index + 3])
            }
        }

        for ($index = 0; $index -lt $maskBytes; $index += 1) {
            $writer.Write([byte]0)
        }
    } finally {
        $writer.Dispose()
        $stream.Dispose()
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
        [Parameter(Mandatory = $true)][string]$WorkingDirectory,
        [Parameter(Mandatory = $true)][string]$IconLocation
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
                -Description $appDescription `
                -IconLocation $IconLocation
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

$iconPath = Join-Path $resolvedInstallDir "LocalDictate.ico"
Write-LocalDictateIcon -Path $iconPath
$iconLocation = "$iconPath,0"

$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Local Dictate"
$startMenuShortcut = Join-Path $startMenuDir "Local Dictate.lnk"
$uninstallScript = Join-Path $resolvedInstallDir "Uninstall-LocalDictate.ps1"
$uninstallShortcut = Join-Path $startMenuDir "Uninstall Local Dictate.lnk"

New-Shortcut `
    -Path $startMenuShortcut `
    -TargetPath $installedUiPath `
    -WorkingDirectory $resolvedInstallDir `
    -Description $appDescription `
    -IconLocation $iconLocation

if (Test-Path -LiteralPath $uninstallScript -PathType Leaf) {
    $powerShellPath = Get-PowerShellExecutable
    New-Shortcut `
        -Path $uninstallShortcut `
        -TargetPath $powerShellPath `
        -Arguments "-NoProfile -ExecutionPolicy Bypass -File `"$uninstallScript`"" `
        -WorkingDirectory $env:USERPROFILE `
        -Description "Remove Local Dictate from this user account" `
        -IconLocation $iconLocation
}

if ($CreateDesktopShortcut) {
    $desktopShortcut = Join-Path ([Environment]::GetFolderPath("Desktop")) "Local Dictate.lnk"
    New-Shortcut `
        -Path $desktopShortcut `
        -TargetPath $installedUiPath `
        -WorkingDirectory $resolvedInstallDir `
        -Description $appDescription `
        -IconLocation $iconLocation
}

$updatedTaskbarPins = Update-ExistingTaskbarPins `
    -ExecutablePath $installedUiPath `
    -WorkingDirectory $resolvedInstallDir `
    -IconLocation $iconLocation

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
