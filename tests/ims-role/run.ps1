$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '../..')
try {
    cargo build -p zihuan_service --bin zihuan_next
    if ($LASTEXITCODE -ne 0) { throw 'Service build failed' }
    python tests/ims-role/run.py @args
    if ($LASTEXITCODE -ne 0) { throw 'IMS role behaviour test failed' }
} finally {
    Pop-Location
}
