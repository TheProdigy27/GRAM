<#
.SYNOPSIS
  Teste l'installeur NSIS sur une Windows vierge, avec Windows Sandbox (critère de la phase 6).

.DESCRIPTION
  Lance Windows Sandbox avec target/release/bundle en lecture seule. Dans la sandbox,
  scripts/sandbox/install-test.ps1 installe GRAM en silencieux, le lance, vérifie qu'il
  tourne encore après 25 s et prend une capture d'écran. Le résultat arrive dans
  target/sandbox-results (resultat.txt, capture.png). Fermer la sandbox efface tout.

  Prérequis : `npm run tauri build`, et Windows Sandbox activé (Windows Pro ou Entreprise),
  une seule fois, dans un PowerShell administrateur, puis redémarrer :
    Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/test-installer.ps1
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$bundle = Join-Path $root 'target\release\bundle'
$results = Join-Path $root 'target\sandbox-results'

if (-not (Get-Command WindowsSandbox.exe -ErrorAction SilentlyContinue)) {
    Write-Host 'Windows Sandbox n''est pas activé. Dans un PowerShell administrateur :' -ForegroundColor Yellow
    Write-Host '  Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM'
    Write-Host 'puis redémarrer, et relancer ce script.'
    exit 1
}
if (-not (Test-Path (Join-Path $bundle 'nsis\*-setup.exe'))) {
    Write-Host "Aucun installeur dans $bundle\nsis : lancer d'abord « npm run tauri build » (app/ui)." -ForegroundColor Yellow
    exit 1
}

Remove-Item $results -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $results | Out-Null
$wsb = Join-Path $results 'gram.wsb'
@"
<Configuration>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>$bundle</HostFolder>
      <SandboxFolder>C:\gram-bundle</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$(Join-Path $PSScriptRoot 'sandbox')</HostFolder>
      <SandboxFolder>C:\gram-scripts</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$results</HostFolder>
      <SandboxFolder>C:\gram-results</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>powershell.exe -ExecutionPolicy Bypass -File C:\gram-scripts\install-test.ps1</Command>
  </LogonCommand>
</Configuration>
"@ | Set-Content -Path $wsb -Encoding utf8

Write-Host 'Démarrage de Windows Sandbox (environ 1 à 3 minutes)…' -ForegroundColor Cyan
Start-Process $wsb
$log = Join-Path $results 'resultat.txt'
$deadline = (Get-Date).AddMinutes(10)
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 5
    if ((Test-Path $log) -and (Select-String -Path $log -Pattern 'SUCCÈS|ÉCHEC' -Quiet)) { break }
}
if (-not (Test-Path $log)) {
    Write-Host 'Pas de résultat après 10 minutes.' -ForegroundColor Red
    exit 1
}
Get-Content $log -Encoding utf8
Write-Host "`nCapture d'écran : $(Join-Path $results 'capture.png')"
Write-Host 'La sandbox peut être fermée (tout y est effacé).'
if (Select-String -Path $log -Pattern 'ÉCHEC' -Quiet) { exit 1 }
