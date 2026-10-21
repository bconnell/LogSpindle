$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Push-Location (Join-Path $PSScriptRoot '..')
try {
    $python = Get-Command python -ErrorAction SilentlyContinue
    if ($null -ne $python) {
        & python scripts/check_text.py
    } else {
        $launcher = Get-Command py -ErrorAction SilentlyContinue
        if ($null -eq $launcher) { throw 'Python 3 is required for the text gate.' }
        & py -3 scripts/check_text.py
    }
    if ($LASTEXITCODE -ne 0) { throw 'Text gate failed.' }
    if (-not (Test-Path 'Cargo.lock')) {
        throw 'Cargo.lock is missing. Generate, review, and commit it before verification.'
    }

    $env:CARGO_BUILD_JOBS = '1'
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting gate failed.' }
    & cargo clippy --locked --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy gate failed.' }
    & cargo test --locked --all-targets --all-features
    if ($LASTEXITCODE -ne 0) { throw 'Test gate failed.' }
    & cargo build --locked --release
    if ($LASTEXITCODE -ne 0) { throw 'Release build gate failed.' }

    $exe = Join-Path 'target/release' 'logspindle.exe'
    if (-not (Test-Path $exe)) { $exe = Join-Path 'target/release' 'logspindle' }
    if (-not (Test-Path $exe)) { throw 'Release executable is missing.' }
    $exe = (Resolve-Path $exe).Path
    & $exe --help | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Release smoke test failed.' }
    & $exe summary tests/fixtures/sample.jsonl --json | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Sample summary failed.' }

    & git diff --check
    if ($LASTEXITCODE -ne 0) { throw 'Git whitespace gate failed.' }
    $status = & git status --porcelain
    if ($LASTEXITCODE -ne 0 -or $status) { throw 'Working tree is not clean.' }
    Write-Output 'Local verification complete.'
}
finally {
    Pop-Location
}
