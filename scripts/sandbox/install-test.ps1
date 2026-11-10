<#
.SYNOPSIS
  Exécuté DANS Windows Sandbox par scripts/test-installer.ps1 : installe GRAM sur une
  Windows vierge (sans VC++ redistribuable ni outils de développement), le lance et écrit
  le résultat dans C:\gram-results (dossier partagé avec la machine hôte).
#>
$ErrorActionPreference = 'Stop'
$results = 'C:\gram-results'
$log = Join-Path $results 'resultat.txt'

function Write-Log([string]$Message) {
    $line = "$(Get-Date -Format 'HH:mm:ss') $Message"
    Add-Content -Path $log -Value $line -Encoding utf8
}

function Save-Screenshot([string]$Path) {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
}

try {
    $setup = Get-ChildItem 'C:\gram-bundle\nsis\*-setup.exe' | Select-Object -First 1
    if (-not $setup) { throw 'installeur NSIS introuvable dans C:\gram-bundle\nsis' }
    Write-Log "Installeur : $($setup.Name)"

    $webview = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    $before = (Get-ItemProperty $webview -ErrorAction SilentlyContinue).pv
    Write-Log "WebView2 avant l'installation : $(if ($before) { $before } else { 'absent' })"

    # /S : installation silencieuse (utilisateur courant, %LOCALAPPDATA%\GRAM).
    $install = Start-Process $setup.FullName -ArgumentList '/S' -Wait -PassThru
    Write-Log "Installation terminée : code $($install.ExitCode)"
    $after = (Get-ItemProperty $webview -ErrorAction SilentlyContinue).pv
    Write-Log "WebView2 après l'installation : $(if ($after) { $after } else { 'absent' })"

    $exe = Join-Path $env:LOCALAPPDATA 'GRAM\gram.exe'
    if (-not (Test-Path $exe)) { throw "$exe absent après l'installation" }
    $shortcut = Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter 'GRAM.lnk'
    Write-Log "Raccourci du menu Démarrer : $(if ($shortcut) { 'présent' } else { 'absent' })"

    $app = Start-Process $exe -PassThru
    Start-Sleep -Seconds 25
    if ($app.HasExited) { throw "l'application s'est arrêtée (code $($app.ExitCode))" }
    Write-Log "Application en cours d'exécution après 25 s (PID $($app.Id))"

    $data = Join-Path $env:LOCALAPPDATA 'fr.gram.desktop'
    $files = @(Get-ChildItem $data -Recurse -File -ErrorAction SilentlyContinue)
    $cache = @($files | Where-Object { $_.FullName -like '*\cache\*' })
    Write-Log "Données : $($files.Count) fichiers dans $data, dont $($cache.Count) dans le cache"

    Save-Screenshot (Join-Path $results 'capture.png')
    Write-Log 'Capture d''écran : capture.png'
    Write-Log 'SUCCÈS'
}
catch {
    Write-Log "ÉCHEC : $($_.Exception.Message)"
    try { Save-Screenshot (Join-Path $results 'capture.png') } catch { }
}
