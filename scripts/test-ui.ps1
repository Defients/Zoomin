param([string]$Executable)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $Executable) { $Executable = Join-Path $projectRoot 'target\release\zoomin.exe' }
if (-not (Test-Path -LiteralPath $Executable)) { throw 'Build the Windows executable first: npm run tauri build' }
if (Get-Process zoomin -ErrorAction SilentlyContinue) { throw 'Close Zoomin before the UI smoke test so its normal instance and global shortcuts are preserved.' }
if (Get-NetTCPConnection -LocalPort 9222 -State Listen -ErrorAction SilentlyContinue) { throw 'Test port 9222 is already in use; refusing to attach to an unrelated browser.' }
$previousArgs = $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
$testProcess = $null
try {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
    $testProcess = Start-Process -FilePath $Executable -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru
    $ready = $false
    for ($attempt = 0; $attempt -lt 50; $attempt++) {
        if ($testProcess.HasExited) { throw "App exited with code $($testProcess.ExitCode)" }
        try { $null = Invoke-RestMethod -Uri 'http://127.0.0.1:9222/json/version'; $ready = $true; break } catch { Start-Sleep -Milliseconds 200 }
    }
    if (-not $ready) { throw 'WebView2 test transport did not start.' }
    Push-Location $projectRoot
    try { & npx.cmd playwright test; $testExit = $LASTEXITCODE } finally { Pop-Location }
    if ($testExit -ne 0) { throw "UI tests failed ($testExit)" }
} finally {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $previousArgs
    if ($testProcess -and -not $testProcess.HasExited) {
        Push-Location $projectRoot
        try { & node.exe scripts/close-test-app.mjs } finally { Pop-Location }
        if (-not $testProcess.WaitForExit(5000)) { throw 'The test app did not exit cleanly; use its tray Exit action.' }
    }
}
