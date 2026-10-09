# Validation-only by default. Staging and publication are separate explicit approvals.
[CmdletBinding(DefaultParameterSetName = 'Validate')]
param(
    [Parameter(ParameterSetName = 'Stage', Mandatory)][switch]$StageOnly,
    [Parameter(ParameterSetName = 'Publish', Mandatory)][switch]$Publish,
    [Parameter(ParameterSetName = 'Publish', Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$ExpectedHead,
    [Parameter(ParameterSetName = 'Publish', Mandatory)][string]$Branch,
    [Parameter(ParameterSetName = 'Publish', Mandatory)][string]$Repository
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($PSVersionTable.PSVersion.Major -lt 7) { throw 'PowerShell 7 is required.' }
$root = Split-Path -Parent $PSScriptRoot
$utf8 = [Text.UTF8Encoding]::new($false, $true)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$log = $null
$exitCode = 3
$started = [DateTime]::UtcNow
$oldEnvironment = @{}
$exactPaths = [Collections.Generic.Dictionary[string, string]]::new([StringComparer]::Ordinal)
$publicationPaths = @('packages/claude-code/', 'catalogs/collections.lock.json', '.claude-plugin/marketplace.json', 'catalogs/collections.json')

function Write-PublishLog([string]$Level, [string]$Message) {
    if ($log) {
        try {
            $log.WriteLine("[$([DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')) UTC] [$Level] [publication] $Message")
            $log.Flush()
        }
        catch { [Console]::Error.WriteLine('WARNING logging: diagnostic write failed') }
    }
}

function Invoke-Native([string]$Executable, [string[]]$Arguments) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = (Get-Command $Executable -ErrorAction Stop).Source
    $info.WorkingDirectory = $root
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.RedirectStandardInput = $true
    $info.StandardOutputEncoding = $utf8
    $info.StandardErrorEncoding = $utf8
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    $running = $false
    try {
        $running = $process.Start()
        if (-not $running) { throw 'Cannot start publication prerequisite.' }
        $process.StandardInput.Close()
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        # Each small Git/API operation has the same 90-second wall and idle ceiling.
        if (-not $process.WaitForExit(90000)) {
            $process.Kill($true)
            $process.WaitForExit()
            throw 'Publication subprocess timed out; owned process tree terminated.'
        }
        $output = $stdout.GetAwaiter().GetResult()
        $null = $stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw 'Publication subprocess failed; outputs and index preserved.' }
        return $output.TrimEnd([char[]]"`r`n")
    }
    finally {
        if ($running -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
        $process.Dispose()
    }
}

function Invoke-Git([string[]]$Arguments) {
    return Invoke-Native git (@('-C', $root) + $Arguments)
}

function Add-LockedPath([string]$Path, [string]$Mode, [string]$Digest) {
    if ($Path -notmatch '^packages/claude-code/[a-z0-9][a-z0-9-]*/' -or
        $Path -match '(^|/)(\.|\.\.)(/|$)|[\\:\x00-\x1f]' -or
        $Mode -notin @('100644', '100755') -or $Digest -notmatch '^[a-f0-9]{64}$') {
        throw 'Lock contains an invalid publication path, mode or digest.'
    }
    if ($exactPaths.ContainsKey($Path)) { throw 'Duplicate locked publication path.' }
    $file = Join-Path $root $Path
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw 'A locked publication file is missing.' }
    if ((Get-Item -LiteralPath $file -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Publication links are forbidden.' }
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Digest) { throw 'Locked publication bytes differ; refusing staging.' }
    $exactPaths.Add($Path, $Mode)
}

try {
    try {
        $directory = Join-Path $root '.forge-state/logs'
        [IO.Directory]::CreateDirectory($directory) | Out-Null
        $stamp = [DateTime]::UtcNow.ToString('yyyy-MM-dd_HH-mm-ss_UTC')
        for ($index = 0; $index -lt 1000; $index++) {
            $suffix = if ($index -eq 0) { '' } else { "_$index" }
            $path = Join-Path $directory "publish-collections_${stamp}${suffix}.log"
            try {
                $stream = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
                $log = [IO.StreamWriter]::new($stream, $utf8)
                break
            }
            catch [IO.IOException] { if (-not [IO.File]::Exists($path)) { throw } }
        }
        if (-not $log) { throw 'Cannot allocate unique log.' }
    }
    catch { [Console]::Error.WriteLine('WARNING logging: file logging unavailable; using console diagnostics') }
    Write-PublishLog INFO "Started staging=$StageOnly publish=$Publish"
    $tools = if ($Publish) { @('git', 'pwsh', 'gh') } else { @('git', 'pwsh') }
    foreach ($tool in $tools) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw 'Required publication prerequisite unavailable.' }
    }
    # The launcher preserves real CLI exit codes and performs byte/mode/catalog verification.
    & pwsh -NoLogo -NoProfile -NonInteractive -File (Join-Path $root 'run.ps1') -Verify
    if ($LASTEXITCODE -ne 0) { throw 'Collection verification failed; publication refused.' }
    $lock = Get-Content -LiteralPath (Join-Path $root 'catalogs/collections.lock.json') -Raw -Encoding utf8 | ConvertFrom-Json -AsHashtable
    if ($lock.schema_version -ne 1 -or $lock.entries -isnot [Collections.IDictionary]) { throw 'Unsupported collection lock schema.' }
    foreach ($entry in $lock.entries.Values) {
        foreach ($file in $entry.files) { Add-LockedPath $file.output_path $file.mode $file.digest }
        foreach ($path in $entry.generated.Keys) {
            if (-not $exactPaths.ContainsKey($path)) { Add-LockedPath $path '100644' $entry.generated[$path] }
        }
    }
    foreach ($path in $publicationPaths[1..3]) {
        if (-not (Test-Path -LiteralPath (Join-Path $root $path) -PathType Leaf)) { throw 'A generated catalog is missing.' }
        $exactPaths.Add($path, '100644')
    }
    # NUL-delimited status is literal even for spaces, quotes and non-ASCII filenames.
    $status = Invoke-Git @('status', '--porcelain=v1', '-z', '--untracked-files=all')
    foreach ($record in $status.Split([char]0, [StringSplitOptions]::RemoveEmptyEntries)) {
        # Added generated files are allowed; deletion/rename/conflict and unrelated work are not.
        if ($record.Length -lt 4 -or $record.Substring(0, 2) -notmatch '^[ A?M]{2}$' -or
            -not $exactPaths.ContainsKey($record.Substring(3))) { throw 'Unrelated or unsafe working tree changes; publication refused.' }
    }
    if (-not $StageOnly -and -not $Publish) {
        Write-PublishLog INFO 'Validation passed; no index or remote mutation requested'
        [Console]::Out.WriteLine('Collection publication validated; use -StageOnly or -Publish explicitly.')
        $exitCode = 0
    }
    else {
        if (Invoke-Git @('diff', '--cached', '--name-only', '-z')) { throw 'Existing staged work must be reviewed separately before collection staging.' }
        if ($Publish) {
            if ($env:GITHUB_ACTIONS -ne 'true' -or $Repository -cne $env:GITHUB_REPOSITORY -or
                $env:GITHUB_REF -cne "refs/heads/$Branch" -or -not $env:GH_TOKEN) { throw 'Publication requires the intended GitHub Actions default-branch context and token.' }
            if ($Repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw 'Invalid publication repository.' }
            Invoke-Git @('check-ref-format', '--branch', $Branch) | Out-Null
            $origin = Invoke-Git @('remote', 'get-url', 'origin')
            if ($origin -cne "https://github.com/$Repository" -and $origin -cne "https://github.com/$Repository.git") { throw 'Origin does not match the intended repository.' }
            if ((Invoke-Git @('rev-parse', 'HEAD')) -cne $ExpectedHead) { throw 'Local HEAD advanced; publication refused.' }
            foreach ($name in @('GIT_CONFIG_COUNT', 'GIT_CONFIG_KEY_0', 'GIT_CONFIG_VALUE_0', 'GIT_CONFIG_KEY_1', 'GIT_CONFIG_VALUE_1', 'GIT_TERMINAL_PROMPT')) {
                $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name)
            }
            # Authentication stays only in this process environment, never URLs, arguments or logs.
            $env:GIT_CONFIG_COUNT = '2'
            $env:GIT_CONFIG_KEY_0 = 'http.https://github.com/.extraheader'
            $env:GIT_CONFIG_VALUE_0 = 'AUTHORIZATION: basic ' + [Convert]::ToBase64String($utf8.GetBytes("x-access-token:$env:GH_TOKEN"))
            $env:GIT_CONFIG_KEY_1 = 'credential.helper'
            $env:GIT_CONFIG_VALUE_1 = ''
            $env:GIT_TERMINAL_PROMPT = '0'
            Invoke-Git @('fetch', '--no-tags', 'origin', "refs/heads/${Branch}:refs/remotes/origin/$Branch") | Out-Null
            if ((Invoke-Git @('rev-parse', "refs/remotes/origin/$Branch")) -cne $ExpectedHead) { throw 'Origin advanced; publication refused without force.' }
        }
        foreach ($path in $exactPaths.Keys) {
            # Raw blobs bypass EOL/filter conversion; cacheinfo preserves executable Git modes on Windows.
            $blob = Invoke-Git @('hash-object', '-w', '--no-filters', '--', $path)
            Invoke-Git @('update-index', '--add', '--cacheinfo', $exactPaths[$path], $blob, $path) | Out-Null
        }
        $changed = Invoke-Git @('diff', '--cached', '--name-only', '-z')
        foreach ($path in $changed.Split([char]0, [StringSplitOptions]::RemoveEmptyEntries)) {
            if (-not $exactPaths.ContainsKey($path)) { throw 'Unexpected staged path; publication refused.' }
        }
        if (-not $changed) {
            Write-PublishLog INFO 'No generated diff; no commit, push or dispatch'
            [Console]::Out.WriteLine('No generated collection changes.')
        }
        elseif ($StageOnly) {
            Write-PublishLog INFO 'Exact generated files staged with locked modes; no commit or push'
            [Console]::Out.WriteLine('Generated collection files staged with exact bytes and locked Git modes. Review the index before committing.')
        }
        else {
            Invoke-Git @('config', '--local', 'user.name', 'github-actions[bot]') | Out-Null
            Invoke-Git @('config', '--local', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com') | Out-Null
            foreach ($name in @('GIT_AUTHOR_NAME', 'GIT_COMMITTER_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_EMAIL')) {
                $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name)
                [Environment]::SetEnvironmentVariable($name, $(if ($name.EndsWith('EMAIL')) { '41898282+github-actions[bot]@users.noreply.github.com' } else { 'github-actions[bot]' }))
            }
            $remote = Invoke-Git @('ls-remote', '--exit-code', 'origin', "refs/heads/$Branch")
            if (($remote -split '\s+')[0] -cne $ExpectedHead) { throw 'Origin advanced during staging; publication refused.' }
            Invoke-Git @('commit', '-m', 'chore: update collections', '-m', 'Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>') | Out-Null
            $sha = Invoke-Git @('rev-parse', 'HEAD')
            Invoke-Git @('push', 'origin', "HEAD:refs/heads/$Branch") | Out-Null
            if ($env:GITHUB_STEP_SUMMARY) {
                [IO.File]::AppendAllText($env:GITHUB_STEP_SUMMARY, "### Published collection commit`nSHA: $sha`nCI must verify this exact headSha; dispatch is not a passing result.`n", $utf8)
            }
            $remote = Invoke-Git @('ls-remote', '--exit-code', 'origin', "refs/heads/$Branch")
            if (($remote -split '\s+')[0] -cne $sha) { throw 'Published head changed before CI dispatch; exact-SHA verification required.' }
            try { Invoke-Native gh @('workflow', 'run', 'ci.yml', '--repo', $Repository, '--ref', $Branch) | Out-Null }
            catch { throw "Collection commit $sha pushed, but CI dispatch failed. Dispatch CI manually and verify that exact SHA." }
            Write-PublishLog INFO "Published sha=$sha CI dispatch requested; result not yet verified"
            [Console]::Out.WriteLine("Published collection commit $sha; CI dispatched. Verify its exact headSha before accepting completion.")
        }
        $exitCode = 0
    }
}
catch {
    # Native output is deliberately not logged: Git errors can contain credential-bearing URLs.
    [Console]::Error.WriteLine("ERROR publication: $($_.Exception.Message)")
    Write-PublishLog ERROR 'Publication failed; generated outputs and index preserved for review'
}
finally {
    foreach ($name in $oldEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name]) }
    Write-PublishLog INFO "Finished exit_code=$exitCode duration_ms=$([int]([DateTime]::UtcNow - $started).TotalMilliseconds) cleanup=closed"
    if ($log) { $log.Dispose() }
}
exit $exitCode
