param(
    [string]$Executable = "src-tauri/target/release/marklite-cli.exe",
    [string]$Application = "src-tauri/target/release/marklite.exe"
)

$ErrorActionPreference = "Stop"
$workspace = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$launcher = (Resolve-Path (Join-Path $workspace $Executable)).Path
$applicationExe = (Resolve-Path (Join-Path $workspace $Application)).Path
$existingInstances = @(Get-Process -Name "marklite", "marklite-cli" -ErrorAction SilentlyContinue)
if ($existingInstances.Count -gt 0) {
    throw "Close MarkLite after saving your work before running CLI tests; app-data directories do not isolate the single-instance mutex. Existing PIDs: $($existingInstances.Id -join ', ')"
}
$caseRoot = Join-Path $workspace "tmp/cli-blackbox-0062"
if (Test-Path -LiteralPath $caseRoot) {
    Remove-Item -LiteralPath $caseRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $caseRoot | Out-Null
$profileRoot = Join-Path $caseRoot "profile-must-stay-absent"
$assertions = 0
$initialProcessIds = @(Get-Process -Name "marklite", "marklite-cli" -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) {
        throw $Message
    }
    $script:assertions++
}

function Get-Sha256([string]$Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $sha = [System.Security.Cryptography.SHA256]::Create()
        try {
            return ([System.BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '')
        } finally {
            $sha.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

function ConvertTo-NativeArgument([string]$Value) {
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') {
        return $Value
    }
    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $slashes++
            continue
        }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * ($slashes * 2 + 1)))
            [void]$builder.Append('"')
        } else {
            [void]$builder.Append(('\' * $slashes))
            [void]$builder.Append($character)
        }
        $slashes = 0
    }
    [void]$builder.Append(('\' * ($slashes * 2)))
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Invoke-Cli([string[]]$Arguments, [string]$WorkingDirectory = $caseRoot) {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $launcher
    $start.WorkingDirectory = $WorkingDirectory
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [System.Text.UTF8Encoding]::new($false)
    $start.StandardErrorEncoding = [System.Text.UTF8Encoding]::new($false)
    $start.Environment["MARKLITE_BENCHMARK_MODE"] = "1"
    $start.Environment["MARKLITE_BENCHMARK_DATA_DIR"] = $profileRoot
    $start.Arguments = (($Arguments | ForEach-Object { ConvertTo-NativeArgument $_ }) -join ' ')
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (-not $process.Start()) {
        throw "failed to start CLI"
    }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    return [pscustomobject]@{
        ExitCode = $process.ExitCode
        Stdout = $stdout
        Stderr = $stderr
    }
}

function Parse-SingleJson([string]$Value) {
    $lines = @($Value -split "`r?`n" | Where-Object { $_.Length -gt 0 })
    Assert-True ($lines.Count -eq 1) "stdout must contain exactly one non-empty JSON line (length=$($Value.Length), lines=$($lines.Count)): $Value"
    return $lines[0] | ConvertFrom-Json
}

try {
    $launcherSize = (Get-Item -LiteralPath $launcher).Length
    Assert-True ($launcherSize -le 524288) "release launcher exceeds 0.5 MiB: $launcherSize"

    $input = Join-Path $caseRoot "输入 (draft)&.md"
    $html = Join-Path $caseRoot "输出 (final)&.html"
    [System.IO.File]::WriteAllText($input, "# 标题`n`n正文 & body", [System.Text.UTF8Encoding]::new($false))
    $success = Invoke-Cli @("export", "--input", $input, "--format", "html", "--output", $html, "--json")
    Assert-True ($success.ExitCode -eq 0) "HTML AI-process export failed: $($success.Stderr)"
    Assert-True ([string]::IsNullOrEmpty($success.Stderr)) "successful JSON export contaminated stderr"
    $successJson = Parse-SingleJson $success.Stdout
    Assert-True ($successJson.schemaVersion -eq 1 -and $successJson.ok) "success JSON schema mismatch"
    Assert-True ($successJson.bytes -eq (Get-Item -LiteralPath $html).Length) "success returned before complete artifact"
    Assert-True ([System.IO.File]::ReadAllText($html).EndsWith("</html>")) "HTML artifact is incomplete"

    $conflict = Invoke-Cli @("export", $input, "--format", "html", "--output", $html, "--json")
    Assert-True ($conflict.ExitCode -eq 4) "existing output did not return exit 4"
    $conflictJson = Parse-SingleJson $conflict.Stdout
    Assert-True ($conflictJson.error.code -eq "EXPORT_TARGET_EXISTS") "conflict code mismatch"
    $originalHash = Get-Sha256 $html
    $overwrite = Invoke-Cli @("export", $input, "--format", "html", "--output", $html, "--overwrite", "--no-title", "--json")
    Assert-True ($overwrite.ExitCode -eq 0) "explicit overwrite failed"
    Assert-True ((Get-Sha256 $html) -ne $originalHash) "explicit overwrite did not replace output"

    $docx = Join-Path $caseRoot "Word 输出 (final)&.docx"
    $docxResult = Invoke-Cli @("export", $input, "--format", "docx", "--output", $docx, "--json")
    Assert-True ($docxResult.ExitCode -eq 0) "DOCX export failed"
    $docxJson = Parse-SingleJson $docxResult.Stdout
    $docxBytes = [System.IO.File]::ReadAllBytes($docx)
    Assert-True ($docxJson.bytes -eq $docxBytes.Length -and $docxBytes[0] -eq 0x50 -and $docxBytes[1] -eq 0x4b) "DOCX artifact is incomplete"

    $pixel = Join-Path $caseRoot "pixel (local)&.png"
    [System.IO.File]::WriteAllBytes($pixel, [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl2nJ8AAAAASUVORK5CYII="))
    $pdfInput = Join-Path $caseRoot "PDF input (draft)&.md"
    $zhText = ([char]0x4E2D).ToString() + [char]0x6587 + [char]0x5185 + [char]0x5BB9
    $longBody = ((1..450 | ForEach-Object { "Line $_ $zhText [link](https://example.com/$_)." }) -join "`n`n")
    $pdfMarkdown = "# PDF $zhText`n`n| A | B |`n| --- | --- |`n| one | two |`n`n![local image](pixel (local)&.png)`n`n$longBody"
    [System.IO.File]::WriteAllText($pdfInput, $pdfMarkdown, [System.Text.UTF8Encoding]::new($false))
    $pdf = Join-Path $caseRoot "PDF output (final)&.pdf"
    $pdfResult = Invoke-Cli @("export", "--input", $pdfInput, "--format", "pdf", "--output", $pdf, "--include-local-images", "--json")
    Assert-True ($pdfResult.ExitCode -eq 0) "native PDF export failed: $($pdfResult.Stderr)"
    Assert-True ([string]::IsNullOrEmpty($pdfResult.Stderr)) "successful PDF JSON export contaminated stderr"
    $pdfJson = Parse-SingleJson $pdfResult.Stdout
    $pdfBytes = [System.IO.File]::ReadAllBytes($pdf)
    $pdfTail = [System.Text.Encoding]::ASCII.GetString($pdfBytes, [Math]::Max(0, $pdfBytes.Length - 32), [Math]::Min(32, $pdfBytes.Length))
    Assert-True ($pdfJson.bytes -eq $pdfBytes.Length -and [System.Text.Encoding]::ASCII.GetString($pdfBytes, 0, 5) -eq "%PDF-") "PDF completion barrier returned an incomplete artifact"
    Assert-True ($pdfTail.Contains("%%EOF")) "PDF artifact has no terminal EOF marker"

    $emptyInput = Join-Path $caseRoot "empty.md"
    $emptyPdf = Join-Path $caseRoot "empty.pdf"
    [System.IO.File]::WriteAllText($emptyInput, "", [System.Text.UTF8Encoding]::new($false))
    $emptyResult = Invoke-Cli @("export", $emptyInput, "--format", "pdf", "--output", $emptyPdf, "--json")
    Assert-True ($emptyResult.ExitCode -eq 0 -and (Get-Item -LiteralPath $emptyPdf).Length -gt 128) "empty-document PDF failed"

    $pdfConflictHash = Get-Sha256 $pdf
    $pdfConflict = Invoke-Cli @("export", $pdfInput, "--format", "pdf", "--output", $pdf, "--json")
    Assert-True ($pdfConflict.ExitCode -eq 4) "existing PDF did not return exit 4"
    Assert-True ((Parse-SingleJson $pdfConflict.Stdout).error.code -eq "EXPORT_TARGET_EXISTS") "PDF conflict code mismatch"
    Assert-True ((Get-Sha256 $pdf) -eq $pdfConflictHash) "PDF conflict changed the existing target"

    $timeoutPdf = Join-Path $caseRoot "timeout.pdf"
    [System.IO.File]::WriteAllText($timeoutPdf, "keep-timeout", [System.Text.UTF8Encoding]::new($false))
    $timeoutResult = Invoke-Cli @("export", $pdfInput, "--format", "pdf", "--output", $timeoutPdf, "--overwrite", "--timeout-ms", "1", "--json")
    Assert-True ($timeoutResult.ExitCode -eq 124) "PDF timeout did not return exit 124"
    Assert-True ((Parse-SingleJson $timeoutResult.Stdout).error.code -eq "PDF_EXPORT_TIMEOUT") "PDF timeout code mismatch"
    Assert-True ([System.IO.File]::ReadAllText($timeoutPdf) -eq "keep-timeout") "PDF timeout changed the existing target"

    $missingRuntimePdf = Join-Path $caseRoot "missing-runtime.pdf"
    $previousBrowserFolder = $env:WEBVIEW2_BROWSER_EXECUTABLE_FOLDER
    try {
        $env:WEBVIEW2_BROWSER_EXECUTABLE_FOLDER = Join-Path $caseRoot "missing-webview2-runtime"
        $missingRuntime = Invoke-Cli @("export", $pdfInput, "--format", "pdf", "--output", $missingRuntimePdf, "--json")
    } finally {
        $env:WEBVIEW2_BROWSER_EXECUTABLE_FOLDER = $previousBrowserFolder
    }
    Assert-True ($missingRuntime.ExitCode -eq 6) "missing WebView2 runtime did not return exit 6"
    Assert-True ((Parse-SingleJson $missingRuntime.Stdout).error.code -eq "PDF_PLATFORM_UNAVAILABLE") "missing WebView2 runtime code mismatch"
    Assert-True (-not (Test-Path -LiteralPath $missingRuntimePdf)) "missing WebView2 runtime created a target"

    $cancelInput = Join-Path $caseRoot "cancel-source.md"
    $cancelPdf = Join-Path $caseRoot "cancel.pdf"
    $cancelStdout = Join-Path $caseRoot "cancel-stdout.txt"
    $cancelStderr = Join-Path $caseRoot "cancel-stderr.txt"
    [System.IO.File]::WriteAllText($cancelInput, "# cancel`n`n" + ("content line`n" * 500000), [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText($cancelPdf, "keep-cancel", [System.Text.UTF8Encoding]::new($false))
    $cancelHelper = Join-Path $caseRoot "windows-cli-cancel.exe"
    & rustc --edition=2021 (Join-Path $workspace "scripts/fixtures/windows-cli-cancel.rs") -o $cancelHelper
    Assert-True ($LASTEXITCODE -eq 0) "could not compile the Windows cancellation probe"
    & $cancelHelper $launcher $caseRoot $cancelStdout $cancelStderr export --input $cancelInput --format pdf --output $cancelPdf --overwrite --json
    Assert-True ($LASTEXITCODE -eq 130) "PDF cancellation did not return exit 130"
    $cancelJson = [System.IO.File]::ReadAllText($cancelStdout, [System.Text.UTF8Encoding]::new($false)) | ConvertFrom-Json
    Assert-True ($cancelJson.error.code -eq "PDF_EXPORT_CANCELLED") "PDF cancellation code mismatch"
    Assert-True ([string]::IsNullOrEmpty([System.IO.File]::ReadAllText($cancelStderr, [System.Text.UTF8Encoding]::new($false)))) "PDF cancellation contaminated stderr"
    Assert-True ([System.IO.File]::ReadAllText($cancelPdf) -eq "keep-cancel") "PDF cancellation changed the existing target"
    Assert-True (@(Get-ChildItem -LiteralPath $caseRoot -Force | Where-Object { $_.Name -like ".marklite-pdf-work-*" }).Count -eq 0) "PDF WebView workspace remains after a terminal result"

    $leadingInput = Join-Path $caseRoot "-前导.md"
    [System.IO.File]::WriteAllText($leadingInput, "leading", [System.Text.UTF8Encoding]::new($false))
    $leading = Invoke-Cli @("export", "--format", "html", "--json", "--", "-前导.md") $caseRoot
    Assert-True ($leading.ExitCode -eq 0) "leading-hyphen literal path failed"
    Assert-True (Test-Path -LiteralPath (Join-Path $caseRoot "-前导.html")) "default output was not created"

    $missing = Invoke-Cli @("export", "missing.md", "--format", "html", "--json")
    Assert-True ($missing.ExitCode -eq 3) "missing input did not return exit 3"
    Assert-True ((Parse-SingleJson $missing.Stdout).error.code -eq "FILE_NOT_FOUND") "missing input code mismatch"
    $invalid = Invoke-Cli @("export", $input, "--json")
    Assert-True ($invalid.ExitCode -eq 2) "invalid arguments did not return exit 2"

    $occupiedParent = Join-Path $caseRoot "occupied-parent"
    [System.IO.File]::WriteAllText($occupiedParent, "not a directory", [System.Text.UTF8Encoding]::new($false))
    $writeFailure = Invoke-Cli @("export", $input, "--format", "html", "--output", (Join-Path $occupiedParent "denied.html"), "--json")
    Assert-True ($writeFailure.ExitCode -eq 5) "target write failure did not return exit 5"
    Assert-True ((Parse-SingleJson $writeFailure.Stdout).error.code -eq "FILE_WRITE_FAILED") "target write failure code mismatch"

    $help = Invoke-Cli @("--help")
    $version = Invoke-Cli @("--version")
    Assert-True ($help.ExitCode -eq 0 -and $help.Stdout.Contains("USAGE:")) "help failed"
    Assert-True ($version.ExitCode -eq 0 -and $version.Stdout.StartsWith("marklite-cli ")) "version failed"
    Assert-True (-not (Test-Path -LiteralPath $profileRoot)) "CLI initialized the GUI profile"

    $nodeProbe = Join-Path $caseRoot "ai-process.mjs"
    $nodeOutput = Join-Path $caseRoot "ai process.html"
    $nodeText = 'import { spawnSync } from "node:child_process"; const child = spawnSync(process.argv[2], process.argv.slice(3), { encoding: "utf8", shell: false }); process.stdout.write(JSON.stringify({ status: child.status, stdout: child.stdout, stderr: child.stderr, error: child.error?.message ?? null }));'
    [System.IO.File]::WriteAllText($nodeProbe, $nodeText, [System.Text.UTF8Encoding]::new($false))
    $nodeEnvelope = (& node $nodeProbe $launcher export --input $input --format html --output $nodeOutput --json) | ConvertFrom-Json
    Assert-True ($LASTEXITCODE -eq 0 -and $nodeEnvelope.status -eq 0) "Node/AI child-process invocation failed"
    Assert-True ([string]::IsNullOrEmpty($nodeEnvelope.stderr)) "Node/AI child-process stderr was contaminated"
    Assert-True (($nodeEnvelope.stdout | ConvertFrom-Json).ok) "Node/AI child-process JSON invalid"

    $pipelineOutput = Join-Path $caseRoot "pipeline.json"
    $pipelineScript = Join-Path $caseRoot "pipeline.ps1"
    $pipelineText = '& $args[0] export --input $args[1] --format html --output $args[2] --json | Set-Content -LiteralPath $args[3] -Encoding utf8NoBOM; exit $LASTEXITCODE'
    [System.IO.File]::WriteAllText($pipelineScript, $pipelineText, [System.Text.UTF8Encoding]::new($false))
    & pwsh -NoProfile -File $pipelineScript $launcher $input (Join-Path $caseRoot "pipeline.html") $pipelineOutput
    Assert-True ($LASTEXITCODE -eq 0) "PowerShell pipeline invocation failed"
    Assert-True ((Get-Content -LiteralPath $pipelineOutput -Raw | ConvertFrom-Json).ok) "PowerShell pipeline JSON invalid"

    $cmdHtml = Join-Path $caseRoot "cmd output.html"
    $cmdJson = Join-Path $caseRoot "cmd result.json"
    $cmdLine = '"{0}" export --input "{1}" --format html --output "{2}" --json > "{3}"' -f $launcher, $input, $cmdHtml, $cmdJson
    & $env:ComSpec /d /s /c $cmdLine
    Assert-True ($LASTEXITCODE -eq 0) "cmd invocation failed"
    Assert-True ((Get-Content -LiteralPath $cmdJson -Raw | ConvertFrom-Json).ok) "cmd JSON invalid"

    $gui = $null
    try {
        $oldMode = $env:MARKLITE_BENCHMARK_MODE
        $oldData = $env:MARKLITE_BENCHMARK_DATA_DIR
        $env:MARKLITE_BENCHMARK_MODE = "1"
        $env:MARKLITE_BENCHMARK_DATA_DIR = (Join-Path $caseRoot "intentional-gui-profile")
        $gui = Start-Process -FilePath $applicationExe -PassThru -WindowStyle Hidden
        Start-Sleep -Milliseconds 1500
        Assert-True (-not $gui.HasExited) "intentional GUI instance did not stay running"
        $withGui = Invoke-Cli @("export", $input, "--format", "html", "--output", (Join-Path $caseRoot "while gui open.html"), "--json")
        Assert-True ($withGui.ExitCode -eq 0) "CLI failed while GUI instance was running"
        Assert-True (-not $gui.HasExited) "CLI disturbed the existing GUI instance"
    } finally {
        $env:MARKLITE_BENCHMARK_MODE = $oldMode
        $env:MARKLITE_BENCHMARK_DATA_DIR = $oldData
        if ($null -ne $gui -and -not $gui.HasExited) {
            Stop-Process -Id $gui.Id -Force
            $gui.WaitForExit()
        }
    }

    Assert-True (-not (Get-ChildItem -LiteralPath $caseRoot -Recurse -File | Where-Object { $_.Name -like "*.marklite-tmp-*" })) "temporary export files remain"
    $newProcesses = @(Get-Process -Name "marklite", "marklite-cli" -ErrorAction SilentlyContinue | Where-Object { $initialProcessIds -notcontains $_.Id })
    Assert-True ($newProcesses.Count -eq 0) "CLI left a MarkLite process running"
    Write-Output "CLI black-box assertions passed: $assertions"
    Write-Output "launcherBytes=$launcherSize"
    & node (Join-Path $workspace "scripts/test-cli-console.mjs") $launcher
    Assert-True ($LASTEXITCODE -eq 0) "real console / output transport regression failed"
} finally {
    if (Test-Path -LiteralPath $caseRoot) {
        Remove-Item -LiteralPath $caseRoot -Recurse -Force
    }
}
