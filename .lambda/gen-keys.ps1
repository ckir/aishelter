$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$b1 = [byte[]]::new(32)
$b2 = [byte[]]::new(32)
$rng.GetBytes($b1)
$rng.GetBytes($b2)
$p1 = [System.BitConverter]::ToString($b1).Replace('-','').ToLower()
$p2 = [System.BitConverter]::ToString($b2).Replace('-','').ToLower()
Write-Output "agent-test-1|$p1"
Write-Output "agent-test-2|$p2"
