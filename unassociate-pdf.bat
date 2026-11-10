@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "NAIVE_NOTE_PDF_SCRIPT=%~f0"
set "NAIVE_NOTE_PDF_HELP=%~1"
powershell.exe -NoLogo -NoProfile -Command "$ErrorActionPreference = 'Stop'; try { & ([ScriptBlock]::Create(([IO.File]::ReadAllText($env:NAIVE_NOTE_PDF_SCRIPT) -split '(?m)^# POWERSHELL\r?$', 2)[1])) } catch { Write-Host ('ERROR: ' + $_.Exception.Message); exit 1 }"
set "NAIVE_NOTE_PDF_RESULT=%ERRORLEVEL%"
if /i not "%~1"=="/?" pause
exit /b %NAIVE_NOTE_PDF_RESULT%

# POWERSHELL
if ($env:NAIVE_NOTE_PDF_HELP -eq '/?') {
    Write-Host 'Usage: unassociate-pdf.bat'
    Write-Host 'Removes Naive Note PDF Open with registration for the current user.'
    Write-Host 'The executable is not required. Default PDF settings are left untouched.'
    exit 0
}

$registry = [Microsoft.Win32.Registry]::CurrentUser
$history = 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.pdf'
# Remove only our values; other PDF readers and the shared .pdf keys stay intact.
foreach ($path in @('Software\Classes\.pdf\OpenWithProgids', "$history\OpenWithProgids")) {
    $key = $registry.OpenSubKey($path, $true)
    if ($null -ne $key) {
        try {
            $key.DeleteValue('NaiveNote.PDF', $false)
            $key.DeleteValue('Applications\naive-note.exe', $false)
        } finally {
            $key.Dispose()
        }
    }
}

# Explorer remembers applications selected through Open with. Clean our entries
# and their MRU letters so a previous selection does not leave a stale candidate.
$list = $registry.OpenSubKey("$history\OpenWithList", $true)
if ($null -ne $list) {
    try {
        $removed = @($list.GetValueNames() | Where-Object {
            $_ -cmatch '^[a-z]$' -and $list.GetValue($_) -ieq 'naive-note.exe'
        })
        $mru = [string]$list.GetValue('MRUList', '')
        foreach ($name in $removed) {
            $list.DeleteValue($name, $false)
            $mru = $mru.Replace($name, '')
        }
        if ($removed.Count -gt 0) {
            $list.SetValue('MRUList', $mru, [Microsoft.Win32.RegistryValueKind]::String)
        }
    } finally {
        $list.Dispose()
    }
}

$registry.DeleteSubKeyTree('Software\Classes\Applications\naive-note.exe', $false)
$registry.DeleteSubKeyTree('Software\Classes\NaiveNote.PDF', $false)
# Never delete or change FileExts\.pdf\UserChoice or the .pdf default value.
try {
    Add-Type -Namespace NaiveNote -Name Shell -MemberDefinition '[System.Runtime.InteropServices.DllImport("shell32.dll")] public static extern void SHChangeNotify(uint eventId, uint flags, System.IntPtr item1, System.IntPtr item2);'
    [NaiveNote.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
} catch {
    Write-Warning 'Registration removed, but Explorer could not be notified. Sign out and back in to refresh.'
}
Write-Host 'Naive Note PDF Open with registration removed for the current user.'
Write-Host 'Default PDF settings have not been changed.'
exit 0
