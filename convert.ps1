$content = [System.IO.File]::ReadAllText('bootstrap')
$content = $content -replace "`r`n", "`n"
[System.IO.File]::WriteAllText('bootstrap', $content)
