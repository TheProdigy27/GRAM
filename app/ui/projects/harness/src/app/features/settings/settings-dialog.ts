import { DialogRef } from '@angular/cdk/dialog';
import { Component, inject } from '@angular/core';
import type { ThemePref } from '@gram/theme';
import type { KeepAlive } from '@gram/types/KeepAlive';
import { Button } from '@gram/ui/button/button';
import { GDialog } from '@gram/ui/dialog/dialog';
import { FIELD } from '@gram/ui/field/field';
import { type ToggleOption, ToggleGroup } from '@gram/ui/toggle-group/toggle-group';
import { ThemeService } from '../../core/theme.service';
import { INSTRUCTIONS_MAX } from '../../shared/conversations';
import { PrefsStore } from '../../state/prefs.store';

const THEMES: readonly ToggleOption<ThemePref>[] = [
  { value: 'light', label: 'Clair', icon: 'sun' },
  { value: 'dark', label: 'Sombre', icon: 'moon' },
  { value: 'system', label: 'Système', icon: 'monitor' },
];

const KEEP_ALIVES: readonly ToggleOption<KeepAlive>[] = [
  {
    value: 'ollama',
    label: '5 min',
    tooltip: 'Réglage d’Ollama (OLLAMA_KEEP_ALIVE peut le changer)',
  },
  { value: 'halfHour', label: '30 min' },
  { value: 'forever', label: 'Toujours', tooltip: 'Jusqu’à l’arrêt d’Ollama' },
];

/**
 * Paramètres de GRAM Harness : thème, instructions des nouvelles conversations, et durée
 * pendant laquelle Ollama garde le modèle en mémoire après une réponse. Tout s'applique
 * aussitôt.
 */
@Component({
  selector: 'app-settings-dialog',
  imports: [Button, GDialog, FIELD, ToggleGroup],
  template: `
    <g-dialog heading="Paramètres" width="560px">
      <section class="row" aria-labelledby="settings-theme">
        <h3 id="settings-theme" class="label">Thème</h3>
        <g-toggle-group
          appearance="segmented"
          label="Thème"
          data-testid="settings-theme"
          [options]="themes"
          [value]="theme.preference()"
          (valueChange)="setTheme($event)"
        />
      </section>
      <section class="row" aria-labelledby="settings-keep-alive">
        <h3 id="settings-keep-alive" class="label">Garder le modèle en mémoire</h3>
        <g-toggle-group
          appearance="segmented"
          label="Garder le modèle en mémoire"
          data-testid="settings-keep-alive"
          [options]="keepAlives"
          [value]="prefs.keepAlive()"
          (valueChange)="setKeepAlive($event)"
        />
        <p class="hint">
          Après chaque réponse. Plus longtemps : la réponse suivante part sans rechargement, mais la
          mémoire reste occupée.
        </p>
      </section>
      <g-field
        label="Instructions des nouvelles conversations"
        hint="Chaque conversation garde les siennes (bouton Instructions de la zone de saisie)."
        clearable
        clearLabel="Effacer les instructions"
      >
        <textarea
          gInput
          class="text"
          rows="5"
          data-testid="settings-instructions"
          placeholder="Par exemple : réponds en français, en quelques phrases."
          [attr.maxlength]="max"
          [value]="prefs.instructions()"
          (input)="prefs.setInstructions($any($event.target).value)"
        ></textarea>
      </g-field>
      <div gDialogActions>
        <button gButton variant="primary" data-testid="settings-close" (click)="ref.close()">
          Fermer
        </button>
      </div>
    </g-dialog>
  `,
  styles: `
    .row {
      display: flex;
      flex-direction: column;
      align-items: flex-start;
      gap: 8px;
      margin-bottom: 18px;
    }
    .label {
      margin: 0;
      font-size: 12.5px;
      font-weight: 500;
      color: var(--g-fg-2);
    }
    .hint {
      margin: 0;
      font-size: 12px;
      line-height: 1.45;
      color: var(--g-muted);
    }
    .text {
      min-height: 96px;
      resize: vertical;
    }
  `,
})
export class SettingsDialog {
  protected readonly ref = inject(DialogRef);
  protected readonly theme = inject(ThemeService);
  protected readonly prefs = inject(PrefsStore);
  protected readonly themes = THEMES;
  protected readonly keepAlives = KEEP_ALIVES;
  protected readonly max = INSTRUCTIONS_MAX;

  protected setTheme(value: ThemePref | null): void {
    if (value) this.theme.set(value);
  }

  protected setKeepAlive(value: KeepAlive | null): void {
    if (value) this.prefs.setKeepAlive(value);
  }
}
