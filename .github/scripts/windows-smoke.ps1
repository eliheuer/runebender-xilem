# Exercise the shipped executable on Windows without touching the source font.
$ErrorActionPreference = 'Stop'
$started = Get-Date
$exe = (Resolve-Path 'target/debug/runebender.exe').Path
$font = (Resolve-Path 'test-fonts/sources/VirtuaGrotesk-Regular.ufo').Path
$out = New-Item -ItemType Directory -Force 'windows-smoke'
& $exe --version
if ($LASTEXITCODE -ne 0) { throw 'Version command failed' }
$info = & $exe info $font --json
if ($LASTEXITCODE -ne 0) { throw 'Font inspection failed' }
$info | Set-Content "$out/info.json"
$data = $info | ConvertFrom-Json
if (!$data.ok -or $data.glyphs -le 0) { throw 'Font inspection returned no glyphs' }
& $exe proof $font --glyphs H,n,o --out "$out/proof.svg" --json
if ($LASTEXITCODE -ne 0) { throw 'SVG proof failed' }
$env:RUNEBENDER_SCREENSHOT = "$out/editor.png"
$env:RUNEBENDER_SIZE = '1100x720'
try {
    & $exe $font
    if ($LASTEXITCODE -ne 0) { throw 'Editor rendering failed' }
    if (!(Test-Path "$out/editor.png")) { throw 'Editor screenshot is missing' }
} finally {
    Remove-Item Env:RUNEBENDER_SCREENSHOT -ErrorAction SilentlyContinue
    Remove-Item Env:RUNEBENDER_SIZE -ErrorAction SilentlyContinue
}
# A real window is a separate check from the CPU rendering path above.
# Avoid mistaking the console window for the editor's native window.
$start = [System.Diagnostics.ProcessStartInfo]::new($exe)
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.ArgumentList.Add($font)
$app = [System.Diagnostics.Process]::Start($start)
$stdout = $app.StandardOutput.ReadToEndAsync()
$stderr = $app.StandardError.ReadToEndAsync()
try {
    $deadline = (Get-Date).AddSeconds(45)
    do {
        Start-Sleep -Milliseconds 500
        $app.Refresh()
        if ($app.HasExited) { throw "Native application exited early: $($app.ExitCode)" }
    } while (($app.MainWindowHandle -eq 0 -or $app.MainWindowTitle -ne 'Runebender') -and (Get-Date) -lt $deadline)
    if ($app.MainWindowHandle -eq 0 -or $app.MainWindowTitle -ne 'Runebender') { throw 'No native Runebender window appeared' }
    "Native window: $($app.MainWindowHandle), title: $($app.MainWindowTitle)" | Set-Content "$out/native-window.txt"
    if (!$app.CloseMainWindow()) { throw 'Window refused the close request' }
    if (!$app.WaitForExit(15000)) { throw 'Window did not close cleanly' }
    if ($app.ExitCode -ne 0) { throw "Native application failed: $($app.ExitCode)" }
} finally {
    if (!$app.HasExited) { Stop-Process -Id $app.Id -Force }
    $app.WaitForExit()
    $stdout.GetAwaiter().GetResult() | Set-Content "$out/window.stdout.txt"
    $stderr.GetAwaiter().GetResult() | Set-Content "$out/window.stderr.txt"
    $app.Dispose()
    # Export only fixed categories and a validated exception code, never raw event data.
    $faults = Get-WinEvent -FilterHashtable @{LogName='Application'; StartTime=$started; Id=1000} -ErrorAction SilentlyContinue
    $report = foreach ($fault in $faults) {
        $fields = @{}
        ([xml]$fault.ToXml()).Event.EventData.Data | ForEach-Object { $fields[$_.Name] = $_.'#text' }
        if ($fields.AppName -eq 'runebender.exe') {
            $module = switch ([System.IO.Path]::GetFileName($fields.ModuleName).ToLowerInvariant()) {
                'd3d12.dll' { 'D3D12' }
                'd3d12core.dll' { 'D3D12' }
                'dxgi.dll' { 'DXGI' }
                'runebender.exe' { 'Runebender' }
                'ntdll.dll' { 'Windows runtime' }
                'ucrtbase.dll' { 'C runtime' }
                default { 'Other' }
            }
            $code = if ($fields.ExceptionCode -match '^[0-9a-fA-F]{8}$') { $fields.ExceptionCode } else { 'Unknown' }
            [pscustomobject]@{ ModuleCategory=$module; ExceptionCode=$code }
        }
    }
    $report | ConvertTo-Json | Set-Content "$out/fault-category.json"
}
