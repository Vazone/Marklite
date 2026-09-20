param(
    [Parameter(Mandatory = $true)]
    [string]$Path,

    [ValidateRange(1, 10000)]
    [int]$MinimumLaunches = 100,

    [ValidateRange(1000, 600000)]
    [int]$MaximumReadyMs = 10000
)

$ErrorActionPreference = "Stop"
$MaximumDiagnosticFiles = 128
$MaximumDiagnosticFileBytes = 128KB
$MaximumExportBytes = 20MB
$MaximumRecords = 100000
$schemaPath = Join-Path (Split-Path -Parent $PSScriptRoot) "src/shared/startup-diagnostics-schema.json"
$schema = Get-Content -LiteralPath $schemaPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
if ($schema.schemaVersion -ne 1) { throw "The bundled startup diagnostics schema is invalid" }

function Read-DiagnosticRecords {
    param([string]$InputPath)

    $resolved = Resolve-Path -LiteralPath $InputPath -ErrorAction Stop
    $item = Get-Item -LiteralPath $resolved.Path
    $records = [System.Collections.Generic.List[object]]::new()

    if ($item.PSIsContainer) {
        $files = @(Get-ChildItem -LiteralPath $item.FullName -File -Filter "startup-*.jsonl" -ErrorAction Stop)
        if ($files.Count -gt $MaximumDiagnosticFiles) {
            throw "Startup diagnostics contain more than $MaximumDiagnosticFiles files"
        }
        foreach ($file in $files) {
            if ($file.Length -gt $MaximumDiagnosticFileBytes) {
                throw "Startup diagnostic file exceeds the $MaximumDiagnosticFileBytes byte limit: $($file.Name)"
            }
            $stream = [IO.File]::Open($file.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
            $reader = [IO.StreamReader]::new($stream, [Text.UTF8Encoding]::new($false, $true), $true)
            try {
                while (-not $reader.EndOfStream) {
                    $line = $reader.ReadLine()
                    if ([string]::IsNullOrWhiteSpace($line)) { continue }
                    try {
                        $records.Add(($line | ConvertFrom-Json -ErrorAction Stop))
                    } catch {
                        throw "Startup diagnostics contain invalid JSONL: $($file.Name)"
                    }
                    if ($records.Count -gt $MaximumRecords) {
                        throw "Startup diagnostics contain more than $MaximumRecords records"
                    }
                }
            } finally {
                $reader.Dispose()
                $stream.Dispose()
            }
        }
    } else {
        if ($item.Extension -ne ".json") {
            throw "The startup diagnostics export must use the .json extension"
        }
        if ($item.Length -gt $MaximumExportBytes) {
            throw "The startup diagnostics export exceeds the $MaximumExportBytes byte limit"
        }
        $export = Get-Content -LiteralPath $item.FullName -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
        if ($null -eq $export.schemaVersion -or [int64]$export.schemaVersion -ne 1 -or $null -eq $export.records) {
            throw "The startup diagnostics export schema is invalid"
        }
        foreach ($record in @($export.records)) {
            $records.Add($record)
            if ($records.Count -gt $MaximumRecords) {
                throw "Startup diagnostics contain more than $MaximumRecords records"
            }
        }
    }

    return $records
}

$records = @(Read-DiagnosticRecords -InputPath $Path)
if ($records.Count -eq 0) {
    throw "No startup diagnostic records were found"
}

$requiredFields = @($schema.requiredFields)
$allowedFields = @($schema.requiredFields) + @($schema.optionalFields)
$forbiddenFields = @($schema.forbiddenFields)
$allowedStages = @($schema.stages)
$allowedStatuses = @($schema.statuses)
$allowedCodes = @($schema.codes)
foreach ($record in $records) {
    $names = @($record.PSObject.Properties.Name)
    $recordCode = if ($names -contains "code") { $record.code } else { $null }
    $recordWebviewVersion = if ($names -contains "webviewVersion") { $record.webviewVersion } else { $null }
    foreach ($required in $requiredFields) {
        if ($names -notcontains $required) {
            throw "Startup diagnostics are missing required field: $required"
        }
    }
    foreach ($forbidden in $forbiddenFields) {
        if ($names -contains $forbidden) {
            throw "Startup diagnostics contain forbidden field: $forbidden"
        }
    }
    foreach ($name in $names) {
        if ($allowedFields -notcontains $name) {
            throw "Startup diagnostics contain an unknown field: $name"
        }
    }
    if (($record.schemaVersion -isnot [int]) -and ($record.schemaVersion -isnot [int64]) -or $record.schemaVersion -ne 1) {
        throw "Startup diagnostics contain an unsupported schemaVersion"
    }
    if ($record.launchId -isnot [string] -or [string]::IsNullOrWhiteSpace($record.launchId)) {
        throw "Startup diagnostics contain an invalid launchId"
    }
    if ($record.stage -isnot [string] -or $allowedStages -notcontains $record.stage) {
        throw "Startup diagnostics contain an invalid stage"
    }
    if ($record.status -isnot [string] -or $allowedStatuses -notcontains $record.status) {
        throw "Startup diagnostics contain an invalid status"
    }
    if ((($record.elapsedMs -isnot [int]) -and ($record.elapsedMs -isnot [int64])) -or $record.elapsedMs -lt 0 -or $record.elapsedMs -gt 600000) {
        throw "Startup diagnostics contain an invalid elapsedMs"
    }
    if ($recordCode -ne $null -and ($recordCode -isnot [string] -or $allowedCodes -notcontains $recordCode)) {
        throw "Startup diagnostics contain an invalid code"
    }
    if ($recordWebviewVersion -ne $null -and $recordWebviewVersion -isnot [string]) {
        throw "Startup diagnostics contain an invalid webviewVersion"
    }
    $timestamp = [DateTimeOffset]::MinValue
    if ($record.timestamp -isnot [string] -or -not [DateTimeOffset]::TryParse($record.timestamp, [ref]$timestamp)) {
        throw "Startup diagnostics contain an invalid timestamp"
    }
    if ($record.appVersion -isnot [string] -or [string]::IsNullOrWhiteSpace($record.appVersion)) {
        throw "Startup diagnostics contain an invalid appVersion"
    }
}

$launches = @($records | Group-Object -Property launchId)
$failures = [System.Collections.Generic.List[string]]::new()
foreach ($launch in $launches) {
    $ready = @($launch.Group | Where-Object { $_.stage -eq "frontendReady" -and $_.status -eq "succeeded" })
    if ($ready.Count -ne 1) {
        $failures.Add("$($launch.Name): frontendReady count is $($ready.Count)")
        continue
    }
    if ([int64]$ready[0].elapsedMs -gt $MaximumReadyMs) {
        $failures.Add("$($launch.Name): ready took $($ready[0].elapsedMs)ms")
    }
    $timeouts = @($launch.Group | Where-Object { $_.stage -eq "startupWatchdog" -and $_.status -eq "timeout" })
    if ($timeouts.Count -gt 0) {
        $failures.Add("$($launch.Name): startupWatchdog timeout was recorded")
    }
    $processFailures = @($launch.Group | Where-Object { $_.stage -eq "webviewProcess" -and $_.status -eq "failed" })
    if ($processFailures.Count -gt 0) {
        $codes = @($processFailures | ForEach-Object { $_.code } | Sort-Object -Unique) -join ","
        $failures.Add("$($launch.Name): WebView2 process failure was recorded ($codes)")
    }
}

if ($launches.Count -lt $MinimumLaunches) {
    $failures.Add("Launch count $($launches.Count) is below required $MinimumLaunches")
}

if ($failures.Count -gt 0) {
    $preview = ($failures | Select-Object -First 10) -join [Environment]::NewLine
    throw "Startup diagnostics verification failed: $($failures.Count) issue(s)`n$preview"
}

Write-Output "Startup diagnostics verified: $($launches.Count) launches reached ready within ${MaximumReadyMs}ms without watchdog timeout or WebView2 process failure."
