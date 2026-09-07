#!/usr/bin/env pwsh
<#
.SYNOPSIS
  Launch Agent Commons with RDS IAM authentication.

.DESCRIPTION
  Generates a short-lived RDS IAM auth token via the AWS CLI,
  constructs the AC_DATABASE_URL with the token URL-encoded as
  the password, and starts the agent-commons binary.

  The token is valid for ~15 minutes. For longer sessions the
  script can optionally loop, restarting the binary with a fresh
  token each cycle (--loop flag).

.PARAMETER RdsHost
  RDS endpoint hostname. Defaults to $env:RDSHOST if set.

.PARAMETER RdsPort
  RDS port. Default: 5432.

.PARAMETER RdsUser
  Database user configured for IAM auth. Default: postgres.

.PARAMETER RdsDb
  Database name. Default: aishelter.

.PARAMETER Region
  AWS region. Default: us-east-1.

.PARAMETER Binary
  Path to the agent-commons binary. Default: searches target/release
  then target/debug.

.PARAMETER Loop
  If set, restarts the binary every TokenLifetimeMins with a fresh
  token. Useful for long test sessions.

.PARAMETER TokenLifetimeMins
  Minutes between token refreshes when --Loop is used. Default: 12
  (conservative; tokens last 15 min).

.EXAMPLE
  # Minimal — uses defaults + $env:RDSHOST
  .\rds-iam-launch.ps1

.EXAMPLE
  # Explicit host, loop mode
  .\rds-iam-launch.ps1 -RdsHost "db.cluster-xxx.us-east-1.rds.amazonaws.com" -Loop
#>
[CmdletBinding()]
param(
    [string]$RdsHost   = $env:RDSHOST,
    [int]   $RdsPort   = 5432,
    [string]$RdsUser   = "postgres",
    [string]$RdsDb     = "aishelter",
    [string]$Region    = "us-east-1",
    [string]$Binary    = "",
    [switch]$Loop,
    [int]   $TokenLifetimeMins = 12
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ── Validate inputs ────────────────────────────────────────────────
if (-not $RdsHost) {
    Write-Error "RdsHost is required. Pass -RdsHost or set `$env:RDSHOST."
    exit 1
}

# ── Locate binary ──────────────────────────────────────────────────
if (-not $Binary) {
    $candidates = @(
        "$PSScriptRoot\..\target\release\agent-commons.exe",
        "$PSScriptRoot\..\target\debug\agent-commons.exe"
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { $Binary = (Resolve-Path $c).Path; break }
    }
    if (-not $Binary) {
        Write-Error "Cannot find agent-commons binary. Build with ``cargo build -p ac-server`` or pass -Binary."
        exit 1
    }
}
Write-Host "Binary: $Binary" -ForegroundColor Cyan

# ── Helpers ────────────────────────────────────────────────────────
function New-RdsAuthToken {
    Write-Host "Generating RDS IAM auth token..." -ForegroundColor Yellow
    $token = aws rds generate-db-auth-token `
        --hostname $RdsHost `
        --port $RdsPort `
        --username $RdsUser `
        --region $Region 2>&1

    if ($LASTEXITCODE -ne 0) {
        Write-Error "aws rds generate-db-auth-token failed: $token"
        exit 1
    }
    return $token.Trim()
}

function ConvertTo-UrlEncoded([string]$raw) {
    # The IAM token contains /, =, + which break postgresql:// URIs.
    [System.Uri]::EscapeDataString($raw)
}

function Build-DatabaseUrl([string]$encodedToken) {
    "postgresql://${RdsUser}:${encodedToken}@${RdsHost}:${RdsPort}/${RdsDb}?sslmode=require"
}

# ── Main loop ──────────────────────────────────────────────────────
do {
    $token       = New-RdsAuthToken
    $encoded     = ConvertTo-UrlEncoded $token
    $databaseUrl = Build-DatabaseUrl $encoded

    # Mask the token in logs (show first 20 chars only)
    $masked = $databaseUrl -replace '(?<=:)[^@]{20}[^@]*(?=@)', '***'
    Write-Host "AC_DATABASE_URL = $masked" -ForegroundColor Green

    $env:AC_DATABASE_URL = $databaseUrl

    Write-Host "`nStarting agent-commons..." -ForegroundColor Cyan
    & $Binary

    if ($Loop) {
        Write-Host "`nBinary exited. Refreshing token in $TokenLifetimeMins minutes..." -ForegroundColor Yellow
        Start-Sleep -Seconds ($TokenLifetimeMins * 60)
    }
} while ($Loop)
