@echo off
setlocal EnableExtensions DisableDelayedExpansion
rem Pass paths through the environment to preserve Unicode and shell characters.
set "NAIVE_NOTE_PDF_SCRIPT=%~f0"
set "NAIVE_NOTE_PDF_EXE=%~1"
powershell.exe -NoLogo -NoProfile -Command "$ErrorActionPreference = 'Stop'; try { & ([ScriptBlock]::Create(([IO.File]::ReadAllText($env:NAIVE_NOTE_PDF_SCRIPT) -split '(?m)^# POWERSHELL\r?$', 2)[1])) } catch { Write-Host ('ERROR: ' + $_.Exception.Message); exit 1 }"
set "NAIVE_NOTE_PDF_RESULT=%ERRORLEVEL%"
if /i not "%~1"=="/?" pause
exit /b %NAIVE_NOTE_PDF_RESULT%

# POWERSHELL
if ($env:NAIVE_NOTE_PDF_EXE -eq '/?') {
    Write-Host 'Usage: associate-pdf.bat ["C:\path\to\naive-note.exe"]'
    Write-Host 'Search order: beside this BAT, target\release, target\debug.'
    Write-Host 'Adds PDF Open with for the current user; does not set the default.'
    exit 0
}

$directory = [IO.Path]::GetDirectoryName($env:NAIVE_NOTE_PDF_SCRIPT)
$candidates = if ($env:NAIVE_NOTE_PDF_EXE) {
    @($env:NAIVE_NOTE_PDF_EXE)
} else {
    @(
        (Join-Path $directory 'naive-note.exe')
        (Join-Path $directory 'target\release\naive-note.exe')
        (Join-Path $directory 'target\debug\naive-note.exe')
    )
}

$exe = $null
foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
        $exe = (Get-Item -LiteralPath $candidate).FullName
        break
    }
}
if (-not $exe) {
    throw 'Executable not found. Build first, place this BAT beside naive-note.exe, or pass its path.'
}
if ([IO.Path]::GetFileName($exe) -ine 'naive-note.exe') {
    throw 'Please select the application executable named naive-note.exe.'
}

function Set-RegistryString([string]$SubKey, [string]$Name, [string]$Value) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey("Software\Classes\$SubKey")
    try {
        $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String)
    } finally {
        $key.Dispose()
    }
}

$command = '"' + $exe + '" "%1"'
$icon = '"' + $exe + '",0'
Set-RegistryString 'NaiveNote.PDF' '' 'Naive Note PDF'
Set-RegistryString 'NaiveNote.PDF\DefaultIcon' '' $icon
Set-RegistryString 'NaiveNote.PDF\shell\open\command' '' $command
Set-RegistryString 'Applications\naive-note.exe' 'FriendlyAppName' 'Naive Note'
Set-RegistryString 'Applications\naive-note.exe\SupportedTypes' '.pdf' ''
Set-RegistryString 'Applications\naive-note.exe\shell\open\command' '' $command
# Add only an Open with candidate. Never write .pdf's default or UserChoice.
Set-RegistryString '.pdf\OpenWithProgids' 'NaiveNote.PDF' ''

try {
    Add-Type -Namespace NaiveNote -Name Shell -MemberDefinition '[System.Runtime.InteropServices.DllImport("shell32.dll")] public static extern void SHChangeNotify(uint eventId, uint flags, System.IntPtr item1, System.IntPtr item2);'
    [NaiveNote.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
} catch {
    Write-Warning 'Registration saved, but Explorer could not be notified. Sign out and back in to refresh.'
}
Write-Host "PDF Open with registered: $exe"
Write-Host 'The default PDF application has not been changed.'
exit 0
