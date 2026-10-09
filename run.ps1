[CmdletBinding(DefaultParameterSetName = 'Check')]
param(
    [Parameter(ParameterSetName = 'Check')][switch]$Check,
    [Parameter(Mandatory, ParameterSetName = 'Apply')][switch]$Apply,
    [Parameter(Mandatory, ParameterSetName = 'Verify')][switch]$Verify,
    [switch]$Offline,
    [switch]$Build
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# Re-enter once in PowerShell 7 without changing policy or opening a terminal.
if ($PSVersionTable.PSVersion.Major -lt 7) {
    $pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
    if (-not $pwsh) {
        [Console]::Error.WriteLine('PowerShell 7 is required. Install it explicitly, then rerun this launcher.')
        exit 3
    }
    $forward = @('-NoLogo', '-NoProfile', '-NonInteractive', '-File', $PSCommandPath)
    foreach ($key in $PSBoundParameters.Keys) {
        if ($PSBoundParameters[$key]) { $forward += "-$key" }
    }
    & $pwsh.Source @forward
    exit $LASTEXITCODE
}

$root = $PSScriptRoot
$utf8 = [Text.UTF8Encoding]::new($false, $true)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$log = $null
$exitCode = 3
$started = [DateTime]::UtcNow

function Protect-Text([string]$Text) {
    foreach ($name in @('GH_TOKEN', 'GITHUB_TOKEN', 'NUMERA_TOKEN', 'PASSWORD', 'API_KEY')) {
        $value = [Environment]::GetEnvironmentVariable($name)
        if ($value -and $value.Length -ge 4) { $Text = $Text.Replace($value, '[REDACTED]') }
    }
    $Text = $Text -replace '(?i)(token|password|secret|api[_-]?key)(["\s]*[:=]["\s]*)([^"\s,}]+)', '$1$2[REDACTED]'
    $Text = $Text -replace '(?i)https?://[^/\s"@]+@', 'https://[REDACTED]@'
    $Text = $Text -replace '(gh[pousr]_[A-Za-z0-9_]+|github_pat_[A-Za-z0-9_]+|sk-[A-Za-z0-9_-]+)', '[REDACTED]'
    return $Text
}

function Write-RunLog([string]$Level, [string]$Component, [string]$Message) {
    $safe = (Protect-Text $Message) -replace '[\r\n]', ' '
    if ($log) {
        try { $log.WriteLine("[$([DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')) UTC] [$Level] [$Component] $safe"); $log.Flush() }
        catch { [Console]::Error.WriteLine('WARNING logging: cannot write diagnostic file') }
    }
}

function Invoke-OwnedProcess([string]$Executable, [string[]]$Arguments, [int]$Seconds) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Executable
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
        if (-not $process.Start()) { throw 'Cannot start requested process.' }
        $running = $true
        $process.StandardInput.Close()
        Write-RunLog INFO process "Started pid=$($process.Id) ceiling_seconds=$Seconds"
        $out = [Text.StringBuilder]::new()
        $err = [Text.StringBuilder]::new()
        $stdout = $process.StandardOutput.ReadLineAsync()
        $stderr = $process.StandardError.ReadLineAsync()
        $clock = [Diagnostics.Stopwatch]::StartNew()
        $lastOutput = 0.0
        while ($stdout -or $stderr -or -not $process.HasExited) {
            foreach ($channel in @('stdout', 'stderr')) {
                $pending = Get-Variable -Name $channel -ValueOnly
                if ($pending -and $pending.IsCompleted) {
                    $line = $pending.GetAwaiter().GetResult()
                    if ($null -eq $line) { Set-Variable -Name $channel -Value $null }
                    else {
                        $buffer = if ($channel -eq 'stdout') { $out } else { $err }
                        $buffer.AppendLine($line) | Out-Null
                        $lastOutput = $clock.Elapsed.TotalSeconds
                        $reader = if ($channel -eq 'stdout') { $process.StandardOutput } else { $process.StandardError }
                        Set-Variable -Name $channel -Value $reader.ReadLineAsync()
                    }
                }
            }
            if ($clock.Elapsed.TotalSeconds -gt $Seconds -or ($clock.Elapsed.TotalSeconds - $lastOutput) -gt 180) {
                if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
                throw 'Process exceeded wall/idle ceiling; owned process tree terminated.'
            }
            if (-not $process.HasExited) { $process.WaitForExit(100) | Out-Null }
        }
        $out = $out.ToString()
        $err = $err.ToString()
        Write-RunLog INFO process "Finished exit_code=$($process.ExitCode)"
        if ($err) { Write-RunLog WARNING diagnostic $err }
        return @{ Code = $process.ExitCode; Output = $out; Error = $err }
    }
    finally {
        if ($running -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
        $process.Dispose()
    }
}

try {
    try {
        $directory = Join-Path $root '.forge-state/logs'
        [IO.Directory]::CreateDirectory($directory) | Out-Null
        $stamp = [DateTime]::UtcNow.ToString('yyyy-MM-dd_HH-mm-ss_UTC')
        for ($index = 0; $index -lt 1000; $index++) {
            $suffix = if ($index -eq 0) { '' } else { "_$index" }
            $path = Join-Path $directory "run_${stamp}${suffix}.log"
            try {
                $stream = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
                $log = [IO.StreamWriter]::new($stream, $utf8)
                break
            }
            catch [IO.IOException] { if (-not [IO.File]::Exists($path)) { throw } }
        }
        if (-not $log) { throw 'Cannot allocate unique run log.' }
    }
    catch { [Console]::Error.WriteLine('WARNING logging: file logging unavailable; using console diagnostics') }
    $mode = if ($Apply) { 'apply' } elseif ($Verify) { 'verify' } else { 'check' }
    Write-RunLog INFO startup "version=0.1.0 powershell=$($PSVersionTable.PSVersion) mode=$mode offline=$Offline root=$root"
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) { throw 'Git is required; install it explicitly and rerun.' }
    if ($Build) {
        $cargo = Get-Command cargo -ErrorAction SilentlyContinue
        if (-not $cargo) { throw 'Cargo is required for -Build; install the documented Rust toolchain explicitly.' }
        Write-RunLog INFO build 'Explicit locked release build requested; two Cargo workers'
        $oldWorkers = $env:CARGO_BUILD_JOBS
        try {
            $env:CARGO_BUILD_JOBS = '2'
            $result = Invoke-OwnedProcess $cargo.Source @('build', '--locked', '-p', 'forge-cli', '--release') 900
        }
        finally { $env:CARGO_BUILD_JOBS = $oldWorkers }
        if ($result.Code -ne 0) { throw 'CLI build failed; inspect the launcher log. No sync was attempted.' }
    }
    $binaryName = if ($IsWindows) { 'numera-forge.exe' } else { 'numera-forge' }
    $binary = Join-Path $root "target/release/$binaryName"
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { $binary = Join-Path $root "target/debug/$binaryName" }
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'CLI binary not found. Rerun with -Build to explicitly build it, or obtain a release binary.' }
    $arguments = @('--repository', $root, '--json', 'collection')
    if ($Verify) { $arguments += 'verify' }
    else {
        $arguments += 'sync'
        if ($Apply) { $arguments += '--apply' }
        if ($Offline) { $arguments += '--offline' }
    }
    Write-RunLog INFO operation "Invoking collection $mode binary=$binary"
    $result = Invoke-OwnedProcess $binary $arguments 600
    $exitCode = $result.Code
    if ($result.Output) { [Console]::Out.Write((Protect-Text $result.Output)) }
    if ($result.Error) { [Console]::Error.Write((Protect-Text $result.Error)) }
    if ($exitCode -eq 2) {
        [Console]::Error.WriteLine('Review or partial result: healthy accepted outputs may be updated; unresolved entries remain unchanged.')
        Write-RunLog WARNING operation 'Partial/review result; exit_code=2'
    }
    elseif ($exitCode -ne 0) { Write-RunLog ERROR operation "Collection operation failed exit_code=$exitCode" }
}
catch {
    $safe = Protect-Text $_.Exception.Message
    [Console]::Error.WriteLine("ERROR launcher: $safe")
    Write-RunLog ERROR launcher "$safe stack=$($_.ScriptStackTrace)"
    $exitCode = 3
}
finally {
    Write-RunLog INFO shutdown "exit_code=$exitCode duration_ms=$([int]([DateTime]::UtcNow - $started).TotalMilliseconds) cleanup=closed"
    if ($log) { $log.Dispose() }
}
exit $exitCode
