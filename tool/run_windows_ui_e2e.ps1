$ErrorActionPreference = 'Stop'

$shared = '\\host.lan\Data'
$workRoot = Join-Path $env:USERPROFILE 'microslop-work'
$appRoot = Join-Path $workRoot 'app'
$archive = Join-Path $shared 'microslop-changes.zip'
$temp = Join-Path $env:TEMP 'microslop-ui-e2e-current'
$log = Join-Path $shared 'microslop-windows-ui-current.log'
$err = Join-Path $shared 'microslop-windows-ui-current.err.log'
$exitFile = Join-Path $shared 'microslop-windows-ui-current.exit'
$statusFile = Join-Path $shared 'microslop-windows-ui-current.status'
$selfChat = $env:MICROSLOP_E2E_SELF_CHAT
if ([string]::IsNullOrWhiteSpace($selfChat)) {
    throw 'Set MICROSLOP_E2E_SELF_CHAT to the isolated test account'
}

Remove-Item $log, $err, $exitFile, $statusFile -ErrorAction SilentlyContinue
Remove-Item $temp -Recurse -Force -ErrorAction SilentlyContinue
Expand-Archive $archive -DestinationPath $temp -Force
Get-ChildItem $temp -File -Recurse | ForEach-Object {
    $relative = $_.FullName.Substring($temp.Length + 1)
    $destination = Join-Path $workRoot $relative
    New-Item (Split-Path $destination) -ItemType Directory -Force | Out-Null
    Copy-Item $_.FullName $destination -Force
}
'synced' | Set-Content $statusFile

$arguments = @(
    'test',
    'integration_test\linux_ui_exploration_test.dart',
    '-d',
    'windows',
    "--dart-define=MICROSLOP_E2E_SELF_CHAT=$selfChat"
)
$process = Start-Process -FilePath 'C:\flutter\bin\flutter.bat' -ArgumentList $arguments -WorkingDirectory $appRoot -NoNewWindow -Wait -PassThru -RedirectStandardOutput $log -RedirectStandardError $err
$process.ExitCode | Set-Content $exitFile
('finished ' + $process.ExitCode) | Set-Content $statusFile
exit $process.ExitCode
