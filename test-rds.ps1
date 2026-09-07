$ErrorActionPreference = "Stop"

# Set environment variables for the test
$env:AC_DATABASE_URL = "postgres://postgres:dummy@database-1.cluster-ce5gog82gxo1.us-east-1.rds.amazonaws.com:5432/postgres?sslmode=require"
$env:AC_RDS_IAM_AUTH = "false"
$env:RUST_LOG = "trace"
$env:AC_PORT = "3050"

Write-Host "Starting agent-commons server with RDS IAM auth..."
$proc = Start-Process -FilePath ".\agent-commons-windows-amd64.exe" -NoNewWindow -PassThru

Write-Host "Waiting 45 seconds for the server to fetch IAM token, connect to RDS, and bind to port..."
Start-Sleep -Seconds 45

try {
    Write-Host "`n--- Testing Endpoints ---"
    
    # 1. Test basic liveness (no DB check)
    try {
        $health = Invoke-RestMethod -Uri "http://localhost:3050/v1/health"
        Write-Host "[OK] /v1/health returned: $health"
    } catch {
        Write-Host "[FAIL] /v1/health: $_"
    }
    
    # 2. Test database readiness (requires active DB connection)
    try {
        $ready = Invoke-RestMethod -Uri "http://localhost:3050/v1/ready"
        Write-Host "[OK] /v1/ready returned: $ready"
    } catch {
        Write-Host "[FAIL] /v1/ready: $_"
    }

} finally {
    Write-Host "`nStopping server..."
    if ($proc -and !$proc.HasExited) {
        Stop-Process -Id $proc.Id -Force
    }
}
