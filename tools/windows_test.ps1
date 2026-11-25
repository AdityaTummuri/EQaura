# Installs, uses, reinstalls and removes EQaura on a throwaway Windows machine.
# It changes the machine it runs on: only for GitHub Actions.
$ErrorActionPreference = "Stop"
if (-not $env:GITHUB_ACTIONS) { throw "Only for GitHub Actions" }
$setup = (Get-Item dist\EQaura-*-setup.exe).FullName
$dir = "$env:ProgramFiles\EQaura"

function Check($what, $ok) {
  if (-not $ok) { throw "FAILED: $what" }
  Write-Host "ok: $what"
}

# Equalizer APO opens its device list, which waits for a person.
function Run-Setup($file, $arguments) {
  $p = Start-Process $file $arguments -PassThru
  for ($i = 0; $i -lt 120 -and -not $p.HasExited; $i++) {
    Start-Sleep 1
    if ($i -gt 5) { Get-Process DeviceSelector, Configurator -ErrorAction SilentlyContinue | Stop-Process -Force }
  }
  if (-not $p.HasExited) {
    Get-Process | Where-Object MainWindowTitle | Format-Table Name, MainWindowTitle | Out-Host
    throw "$file did not finish"
  }
}

function Config { Get-Content (Join-Path $script:config "config.txt") -ErrorAction SilentlyContinue }
function Includes { @(Config | Where-Object { $_ -match "^Include: EQaura.txt" }).Count }

Run-Setup $setup "/S"
foreach ($f in "eqaura.exe", "eqaura-agent.exe", "LICENSE.txt", "THIRD-PARTY-LICENSES.txt", "uninstall.exe") {
  Check "installed $f" (Test-Path "$dir\$f")
}
$apo = Get-ItemProperty "HKLM:\SOFTWARE\EqualizerAPO" -ErrorAction SilentlyContinue
Check "Equalizer APO is installed" ($apo -and (Test-Path $apo.InstallPath))
$script:config = if ($apo.ConfigPath) { $apo.ConfigPath } else { Join-Path $apo.InstallPath "config" }
$ours = Join-Path $script:config "EQaura.txt"
Check "EQaura.txt exists" (Test-Path $ours)
Check "EQaura.txt is included once" ((Includes) -eq 1)
Check "normal users may change EQaura.txt" ((icacls $ours | Out-String) -match "BUILTIN\\Users:.*\(M\)")

$env:EQAURA_HOME = Join-Path $env:RUNNER_TEMP "eqaura-home"
$app = Start-Process "$dir\eqaura.exe" -PassThru
$agent = Start-Process "$dir\eqaura-agent.exe" -PassThru
Start-Sleep 12
Check "the app stays open" (-not $app.HasExited)
Check "the tray helper stays open" (-not $agent.HasExited)
$app.Refresh(); $agent.Refresh()
Write-Host ("App {0:N0} MB, tray helper {1:N0} MB (private memory)" -f ($app.PrivateMemorySize64 / 1MB), ($agent.PrivateMemorySize64 / 1MB))
$log = Join-Path $env:EQAURA_HOME "eqaura.log"
if (Test-Path $log) {
  Get-Content $log | Out-Host
  Check "no crash in the log" (-not (Select-String -Path $log -Pattern " crash " -Quiet))
}

Run-Setup $setup "/S"
Check "installing again closes the helper" ($agent.HasExited)
Check "installing again includes EQaura.txt once" ((Includes) -eq 1)
Stop-Process $app -Force -ErrorAction SilentlyContinue

Run-Setup "$dir\uninstall.exe" "/S _?=$dir"
Check "the app is removed" (-not (Test-Path "$dir\eqaura.exe"))
Check "our file is removed" (-not (Test-Path $ours))
Check "our line is removed" ((Includes) -eq 0)
Check "Equalizer APO is kept" (Test-Path $apo.InstallPath)
