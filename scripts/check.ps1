<#
.SYNOPSIS
  CI locale de GRAM : à lancer avant chaque commit de fin de phase.

.DESCRIPTION
  1. cargo fmt (vérification)
  2. cargo clippy -D warnings
  3. cargo test (régénère aussi les types TypeScript via ts-rs)
  4. vérifie que les types TypeScript générés sont à jour dans git
  5. tests Angular (Vitest) et build de production

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/check.ps1
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Invoke-Step([string]$Name, [scriptblock]$Command) {
    Write-Host "`n=== $Name ===" -ForegroundColor Cyan
    & $Command
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ÉCHEC : $Name" -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

Invoke-Step 'cargo fmt' { cargo fmt --all -- --check }
Invoke-Step 'cargo clippy' { cargo clippy --workspace --all-targets -- -D warnings }
Invoke-Step 'cargo test' { cargo test --workspace }
Invoke-Step 'types TypeScript à jour' {
    $generated = 'app/ui/src/app/core/generated'
    # Fichiers régénérés différents de l'index git, ou nouveaux fichiers non suivis.
    $changes = @(git diff --name-only -- $generated) + @(git ls-files --others --exclude-standard -- $generated)
    if ($changes) {
        Write-Host "Types générés modifiés ou non suivis par git :`n$($changes -join "`n")" -ForegroundColor Yellow
        Write-Host "Vérifier puis ajouter ces fichiers au commit." -ForegroundColor Yellow
        $global:LASTEXITCODE = 1
    }
}

Push-Location app/ui
try {
    Invoke-Step 'tests Angular' { npm run test:ci }
    Invoke-Step 'build Angular' { npm run build }
}
finally {
    Pop-Location
}

Write-Host "`nTout est vert." -ForegroundColor Green
