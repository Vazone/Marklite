$ErrorActionPreference = "Stop"
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$analyzer = Join-Path $scriptRoot "analyze-startup-diagnostics.ps1"
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("marklite-startup-analysis-" + [Guid]::NewGuid().ToString("N"))
$assertions = 0

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
    $script:assertions += 1
}

function New-Record {
    param([string]$LaunchId, [string]$Stage, [string]$Status, [int]$ElapsedMs)
    return [ordered]@{
        schemaVersion = 1
        timestamp = "2026-08-09T00:00:00Z"
        launchId = $LaunchId
        stage = $Stage
        status = $Status
        code = $null
        elapsedMs = $ElapsedMs
        appVersion = "0.1.2"
        webviewVersion = "151.0.4129.72"
    }
}

try {
    $goodDir = New-Item -ItemType Directory -Path (Join-Path $tempRoot "good") -Force
    foreach ($index in 1..2) {
        $records = @(
            (New-Record -LaunchId "launch-$index" -Stage "nativeProcess" -Status "started" -ElapsedMs 0),
            (New-Record -LaunchId "launch-$index" -Stage "singleInstanceOpen" -Status "failed" -ElapsedMs 50),
            (New-Record -LaunchId "launch-$index" -Stage "frontendReady" -Status "succeeded" -ElapsedMs (100 + $index))
        )
        $records[1]["code"] = "unsupportedPathEncoding"
        $lines = $records | ForEach-Object { $_ | ConvertTo-Json -Compress }
        [IO.File]::WriteAllLines((Join-Path $goodDir.FullName "startup-$index.jsonl"), $lines, [Text.UTF8Encoding]::new($false))
    }

    $result = & $analyzer -Path $goodDir.FullName -MinimumLaunches 2 -MaximumReadyMs 10000
    Assert-True ($result -match "verified: 2 launches") "Valid diagnostics did not pass analysis"

    $recoveryExport = Join-Path $tempRoot "recovery.json"
    $recoveryRecords = @(
        (New-Record -LaunchId "recovered-launch" -Stage "webviewRecovery" -Status "started" -ElapsedMs 100),
        (New-Record -LaunchId "recovered-launch" -Stage "frontendReady" -Status "succeeded" -ElapsedMs 500)
    )
    $recoveryRecords[0]["code"] = "browserProcessRestartRequested"
    @{ schemaVersion = 1; records = $recoveryRecords } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $recoveryExport -Encoding utf8
    $recoveryResult = & $analyzer -Path $recoveryExport -MinimumLaunches 1
    Assert-True ($recoveryResult -match "verified: 1 launches") "A valid native recovery record was rejected"

    $invalidStageExport = Join-Path $tempRoot "invalid-stage.json"
    $invalidStage = New-Record -LaunchId "invalid-stage" -Stage "inventedRecovery" -Status "observed" -ElapsedMs 10
    @{ schemaVersion = 1; records = @($invalidStage) } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $invalidStageExport -Encoding utf8
    $invalidStageRejected = $false
    try {
        & $analyzer -Path $invalidStageExport -MinimumLaunches 1
    } catch {
        $invalidStageRejected = $_.Exception.Message -match "invalid stage"
    }
    Assert-True $invalidStageRejected "An unknown recovery stage was accepted"

    $invalidWebviewExport = Join-Path $tempRoot "invalid-webview.json"
    $invalidWebview = New-Record -LaunchId "invalid-webview" -Stage "frontendReady" -Status "succeeded" -ElapsedMs 10
    $invalidWebview["webviewVersion"] = 152
    @{ schemaVersion = 1; records = @($invalidWebview) } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $invalidWebviewExport -Encoding utf8
    $invalidWebviewRejected = $false
    try {
        & $analyzer -Path $invalidWebviewExport -MinimumLaunches 1
    } catch {
        $invalidWebviewRejected = $_.Exception.Message -match "invalid webviewVersion"
    }
    Assert-True $invalidWebviewRejected "A non-string webviewVersion was accepted"

    $badDir = New-Item -ItemType Directory -Path (Join-Path $tempRoot "bad") -Force
    $badRecords = @(
        (New-Record -LaunchId "bad-launch" -Stage "frontendReady" -Status "succeeded" -ElapsedMs 100),
        (New-Record -LaunchId "bad-launch" -Stage "startupWatchdog" -Status "timeout" -ElapsedMs 10001)
    )
    [IO.File]::WriteAllText(
        (Join-Path $badDir.FullName "startup-bad.jsonl"),
        (($badRecords | ForEach-Object { $_ | ConvertTo-Json -Compress }) -join [Environment]::NewLine) + [Environment]::NewLine,
        [Text.UTF8Encoding]::new($false)
    )
    $failedAsExpected = $false
    try {
        & $analyzer -Path $badDir.FullName -MinimumLaunches 1 -MaximumReadyMs 10000
    } catch {
        $failedAsExpected = $_.Exception.Message -match "startupWatchdog timeout was recorded"
    }
    Assert-True $failedAsExpected "A ready launch with a watchdog timeout was not rejected"

    $unsafeExport = Join-Path $tempRoot "unsafe.json"
    $unsafe = New-Record -LaunchId "unsafe" -Stage "frontendReady" -Status "succeeded" -ElapsedMs 10
    $unsafe["path"] = "C:\Users\private\note.md"
    @{ schemaVersion = 1; records = @($unsafe) } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $unsafeExport -Encoding utf8
    $unsafeRejected = $false
    try {
        & $analyzer -Path $unsafeExport -MinimumLaunches 1
    } catch {
        $unsafeRejected = $_.Exception.Message -match "forbidden field: path"
    }
    Assert-True $unsafeRejected "A diagnostic path field was not rejected"

    $processDir = New-Item -ItemType Directory -Path (Join-Path $tempRoot "process") -Force
    $processRecords = @(
        (New-Record -LaunchId "process-launch" -Stage "frontendReady" -Status "succeeded" -ElapsedMs 100),
        (New-Record -LaunchId "process-launch" -Stage "webviewProcess" -Status "failed" -ElapsedMs 150)
    )
    $processRecords[1]["code"] = "gpuProcessExited"
    $processLines = $processRecords | ForEach-Object { $_ | ConvertTo-Json -Compress }
    [IO.File]::WriteAllLines((Join-Path $processDir.FullName "startup-process.jsonl"), $processLines, [Text.UTF8Encoding]::new($false))
    $processRejected = $false
    try {
        & $analyzer -Path $processDir.FullName -MinimumLaunches 1
    } catch {
        $processRejected = $_.Exception.Message -match "WebView2 process failure was recorded"
    }
    Assert-True $processRejected "A WebView2 process failure was not rejected"

    $invalidSchemaDir = New-Item -ItemType Directory -Path (Join-Path $tempRoot "invalid-schema") -Force
    $invalidRecord = New-Record -LaunchId "invalid-schema" -Stage "frontendReady" -Status "succeeded" -ElapsedMs 10
    $invalidRecord["elapsedMs"] = "10"
    [IO.File]::WriteAllText(
        (Join-Path $invalidSchemaDir.FullName "startup-invalid.jsonl"),
        (($invalidRecord | ConvertTo-Json -Compress) + [Environment]::NewLine),
        [Text.UTF8Encoding]::new($false)
    )
    $schemaRejected = $false
    try {
        & $analyzer -Path $invalidSchemaDir.FullName -MinimumLaunches 1
    } catch {
        $schemaRejected = $_.Exception.Message -match "invalid elapsedMs"
    }
    Assert-True $schemaRejected "A string elapsedMs was accepted as a numeric schema field"

    $oversizedDir = New-Item -ItemType Directory -Path (Join-Path $tempRoot "oversized") -Force
    [IO.File]::WriteAllBytes(
        (Join-Path $oversizedDir.FullName "startup-oversized.jsonl"),
        [Text.Encoding]::UTF8.GetBytes((" " * (128KB + 1)))
    )
    $oversizedRejected = $false
    try {
        & $analyzer -Path $oversizedDir.FullName -MinimumLaunches 1
    } catch {
        $oversizedRejected = $_.Exception.Message -match "exceeds"
    }
    Assert-True $oversizedRejected "An oversized diagnostic file was not rejected before parsing"

    Write-Output "Startup diagnostics analyzer tests passed: $assertions assertions."
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
