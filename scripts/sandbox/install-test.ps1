<#
.SYNOPSIS
  Exécuté DANS Windows Sandbox par scripts/test-installer.ps1 : installe GRAM et GRAM Harness
  sur une Windows vierge (sans VC++ redistribuable ni outils de développement), les lance et
  écrit le résultat dans C:\gram-results (dossier partagé avec la machine hôte).
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

# Installeur NSIS le plus récent d'une application (« GRAM_* » ne prend pas « GRAM Harness_* »).
function Find-Setup([string]$Pattern) {
    $setup = Get-ChildItem (Join-Path 'C:\gram-bundle\nsis' $Pattern) |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $setup) { throw "installeur $Pattern introuvable dans C:\gram-bundle\nsis" }
    $setup
}

# /S : installation silencieuse (utilisateur courant, %LOCALAPPDATA%\<application>).
function Install-App([string]$Name, [string]$Pattern, [string]$Exe) {
    $setup = Find-Setup $Pattern
    Write-Log "$Name, installeur : $($setup.Name)"
    $install = Start-Process $setup.FullName -ArgumentList '/S' -Wait -PassThru
    Write-Log "$Name, installation terminée : code $($install.ExitCode)"
    $path = Join-Path $env:LOCALAPPDATA $Exe
    if (-not (Test-Path $path)) { throw "$path absent après l'installation" }
    $shortcut = Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter "$Name.lnk"
    Write-Log "$Name, raccourci du menu Démarrer : $(if ($shortcut) { 'présent' } else { 'absent' })"
    $path
}

try {
    $webview = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    $before = (Get-ItemProperty $webview -ErrorAction SilentlyContinue).pv
    Write-Log "WebView2 avant l'installation : $(if ($before) { $before } else { 'absent' })"

    $gram = Install-App 'GRAM' 'GRAM_*-setup.exe' 'GRAM\gram.exe'
    $after = (Get-ItemProperty $webview -ErrorAction SilentlyContinue).pv
    Write-Log "WebView2 après l'installation : $(if ($after) { $after } else { 'absent' })"
    $harness = Install-App 'GRAM Harness' 'GRAM Harness_*-setup.exe' 'GRAM Harness\gram-harness.exe'

    # Schéma du lien profond « Ouvrir dans GRAM Harness », enregistré par l'installeur.
    $command = (Get-ItemProperty 'HKCU:\Software\Classes\gram-harness\shell\open\command' -ErrorAction SilentlyContinue).'(default)'
    if (-not $command) { throw 'schéma gram-harness non enregistré (HKCU\Software\Classes\gram-harness)' }
    if ($command -notlike "*$harness*") { throw "schéma gram-harness : commande inattendue ($command)" }
    Write-Log "Schéma gram-harness : $command"

    $app = Start-Process $gram -PassThru
    Start-Sleep -Seconds 25
    if ($app.HasExited) { throw "GRAM s'est arrêté (code $($app.ExitCode))" }
    Write-Log "GRAM en cours d'exécution après 25 s (PID $($app.Id))"

    $data = Join-Path $env:LOCALAPPDATA 'fr.gram.desktop'
    $files = @(Get-ChildItem $data -Recurse -File -ErrorAction SilentlyContinue)
    $cache = @($files | Where-Object { $_.FullName -like '*\cache\*' })
    Write-Log "Données : $($files.Count) fichiers dans $data, dont $($cache.Count) dans le cache"

    # GRAM Harness lancé par un lien, comme depuis « Ouvrir dans GRAM Harness » ; un second lien
    # va à la même instance. Sans Ollama, le modèle est seulement retenu.
    $link = 'gram-harness://open?model=gram%2Fqwen3-8b%3Aq4_k_m'
    Start-Process $link
    Start-Sleep -Seconds 15
    Start-Process $link
    Start-Sleep -Seconds 5
    $running = @(Get-Process gram-harness -ErrorAction SilentlyContinue)
    if ($running.Count -ne 1) { throw "GRAM Harness : $($running.Count) processus après deux liens (1 attendu)" }
    Write-Log "GRAM Harness lancé par le lien, une seule instance (PID $($running[0].Id))"

    # Présence pour GRAM, écrite au démarrage du harness.
    $presence = Join-Path $env:LOCALAPPDATA 'fr.gram.shared\harness.json'
    if (-not (Test-Path $presence)) { throw "$presence absent : GRAM ne verra pas GRAM Harness" }
    $exePath = (Get-Content $presence -Raw -Encoding utf8 | ConvertFrom-Json).exePath
    if ($exePath -ne $harness) { throw "harness.json : exePath inattendu ($exePath)" }
    Write-Log "Présence écrite : $presence (exePath = $exePath)"

    Save-Screenshot (Join-Path $results 'capture.png')
    Write-Log 'Capture d''écran : capture.png'
    Write-Log 'SUCCÈS'
}
catch {
    Write-Log "ÉCHEC : $($_.Exception.Message)"
    try { Save-Screenshot (Join-Path $results 'capture.png') } catch { }
}
