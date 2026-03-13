$pythonCommand = Get-Command python -ErrorAction Stop
$pythonRoot = Split-Path -Parent $pythonCommand.Source
$env:PATH = "$pythonRoot;$pythonRoot\Library\bin;$pythonRoot\DLLs;" + $env:PATH
& "$PSScriptRoot\..\.venv\Scripts\Activate.ps1"
Write-Host 'Project environment activated.'
Write-Host "Python: $((Get-Command python).Source)"
