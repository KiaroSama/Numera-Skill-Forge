# First-party isolated publication fixture; never pushes or executes imported payloads.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$project = Split-Path -Parent $PSScriptRoot
$fixture = Join-Path $project ('.ci-work/launcher-tests/' + [Guid]::NewGuid().ToString('N'))
$utf8 = [Text.UTF8Encoding]::new($false)
$old = Get-Location
function Write-Fixture([string]$Path, [string]$Text) {
    $full = Join-Path $fixture $Path
    [IO.Directory]::CreateDirectory((Split-Path -Parent $full)) | Out-Null
    [IO.File]::WriteAllText($full, $Text, $utf8)
}
function Git-Fixture([string[]]$Arguments) {
    $value = & git -C $fixture @Arguments
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Git operation failed' }
    return $value
}
try {
    [IO.Directory]::CreateDirectory((Join-Path $fixture 'scripts')) | Out-Null
    Copy-Item (Join-Path $project 'run.ps1') $fixture
    Copy-Item (Join-Path $project 'scripts/publish-collections.ps1') (Join-Path $fixture 'scripts')
    Write-Fixture '.gitignore' ".forge-state/`ntarget/`n.ci-work/`n"
    Git-Fixture @('init', '-b', 'main') | Out-Null
    Git-Fixture @('add', '.') | Out-Null
    Git-Fixture @('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-m','fixture') | Out-Null
    $binary = if ($IsWindows) { 'numera-forge.exe' } else { 'numera-forge' }
    [IO.Directory]::CreateDirectory((Join-Path $fixture 'target/release')) | Out-Null
    Copy-Item (Join-Path $project "target/release/$binary") (Join-Path $fixture 'target/release')
    $upstream = Join-Path $fixture '.ci-work/upstream'
    [IO.Directory]::CreateDirectory((Join-Path $upstream 'skills/demo/scripts')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $upstream 'skills/demo/SKILL.md'), "---`nname: demo`ndescription: Fixture`n---`nOriginal.`n", $utf8)
    [IO.File]::WriteAllText((Join-Path $upstream 'skills/demo/.gitignore'), "temporary/`n", $utf8)
    [IO.File]::WriteAllText((Join-Path $upstream 'skills/demo/LICENSE'), "MIT License`nPermission is hereby granted, free of charge`n", $utf8)
    [IO.File]::WriteAllText((Join-Path $upstream 'skills/demo/scripts/helper.sh'), "#!/bin/sh`necho never-run`n", $utf8)
    & git -C $upstream init -b main | Out-Null
    & git -C $upstream add .
    & git -C $upstream update-index --chmod=+x skills/demo/scripts/helper.sh
    & git -C $upstream -c user.name=Fixture -c user.email=fixture@example.invalid commit -m fixture | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Upstream fixture commit failed' }
    $revision = (& git -C $upstream rev-parse HEAD).Trim()
    $digest = (Get-FileHash (Join-Path $upstream 'skills/demo/LICENSE') -Algorithm SHA256).Hash.ToLowerInvariant()
    $registration = @{schema_version=1;collections=@(@{id='fixture';repository="local:$upstream";branch='main';reviewed_revision=$revision;skills=@(@{id='fixture-demo';root='skills/demo';license='MIT';license_path='skills/demo/LICENSE';license_digest=$digest})})}
    Write-Fixture 'registry/collections.json' ($registration | ConvertTo-Json -Depth 10)
    Git-Fixture @('add','registry/collections.json') | Out-Null
    Git-Fixture @('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-m','registration') | Out-Null
    Set-Location (Join-Path $fixture 'scripts')
    & pwsh -NoLogo -NoProfile -NonInteractive -File (Join-Path $fixture 'run.ps1') -Apply
    if ($LASTEXITCODE -ne 0) { throw 'Launcher Apply failed from other CWD' }
    & pwsh -NoLogo -NoProfile -NonInteractive -File (Join-Path $fixture 'run.ps1') -Verify
    if ($LASTEXITCODE -ne 0) { throw 'Launcher Verify failed' }
    Write-Fixture 'unrelated.txt' 'Unrelated user content'
    & pwsh -NoLogo -NoProfile -NonInteractive -File (Join-Path $fixture 'scripts/publish-collections.ps1') -StageOnly
    if ($LASTEXITCODE -eq 0) { throw 'Publisher accepted unrelated user work' }
    if (Git-Fixture @('diff','--cached','--name-only')) { throw 'Refused publication staged files' }
    Remove-Item (Join-Path $fixture 'unrelated.txt')
    & pwsh -NoLogo -NoProfile -NonInteractive -File (Join-Path $fixture 'scripts/publish-collections.ps1') -StageOnly
    if ($LASTEXITCODE -ne 0) { throw 'Exact publication staging failed' }
    $entry = (Get-Content (Join-Path $fixture 'catalogs/collections.lock.json') -Raw -Encoding utf8 | ConvertFrom-Json).entries.'fixture/fixture-demo'
    foreach ($catalog in @('.claude-plugin/marketplace.json', '.cursor-plugin/marketplace.json')) {
        if (-not (Git-Fixture @('ls-files','--',$catalog))) { throw 'Native catalog not staged' }
    }
    foreach ($file in $entry.files) {
        $row = Git-Fixture @('ls-files','--stage','--',$file.output_path)
        if (($row -split ' ')[0] -ne $file.mode) { throw 'Staged executable mode lost' }
        $blob = (Git-Fixture @('hash-object','--no-filters','--',$file.output_path)).Trim()
        if (-not $row.Contains($blob)) { throw 'Staged exact raw bytes changed' }
    }
    Git-Fixture @('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-m','accepted') | Out-Null
    & pwsh -NoLogo -NoProfile -NonInteractive -File (Join-Path $fixture 'scripts/publish-collections.ps1') -StageOnly
    if ($LASTEXITCODE -ne 0 -or (Git-Fixture @('diff','--cached','--name-only'))) { throw 'No-op staging created a diff' }
    $logs = @(Get-ChildItem (Join-Path $fixture '.forge-state/logs') -Filter '*.log')
    if ($logs.Count -lt 4) { throw 'Per-run diagnostic logs missing' }
    foreach ($log in $logs) {
        $text = [IO.File]::ReadAllText($log.FullName, [Text.UTF8Encoding]::new($false,$true))
        if ($text -notmatch '\[\d{4}-\d{2}-\d{2} .* UTC\]') { throw 'UTC log format missing' }
    }
    'Launcher and raw-mode publication fixture passed; no remote mutation'
}
finally {
    Set-Location $old
    if (Test-Path $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
