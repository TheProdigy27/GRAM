import { DialogRef } from '@angular/cdk/dialog';
import { Component, computed, inject, signal } from '@angular/core';
import type { ThemePref } from '@gram/theme';
import type { KeepAlive } from '@gram/types/KeepAlive';
import { Button } from '@gram/ui/button/button';
import { GDialog } from '@gram/ui/dialog/dialog';
import { FIELD } from '@gram/ui/field/field';
import { Icon } from '@gram/ui/icon/icon';
import { Spinner } from '@gram/ui/spinner/spinner';
import { Tag } from '@gram/ui/tag/tag';
import { type ToggleOption, ToggleGroup } from '@gram/ui/toggle-group/toggle-group';
import { HarnessApi } from '../../core/harness-api.service';
import { ThemeService } from '../../core/theme.service';
import { INSTRUCTIONS_MAX } from '../../shared/conversations';
import { PrefsStore } from '../../state/prefs.store';
import { Updates } from '../../state/updates';

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
 * Paramètres de GRAM Harness : thème, instructions des nouvelles conversations, durée pendant
 * laquelle Ollama garde le modèle en mémoire après une réponse, version et mises à jour. Tout
 * s'applique aussitôt.
 */
@Component({
  selector: 'app-settings-dialog',
  imports: [Button, GDialog, FIELD, Icon, Spinner, Tag, ToggleGroup],
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
      <section class="row version" aria-labelledby="settings-version">
        <h3 id="settings-version" class="label">Version</h3>
        <p class="line">
          <span class="mono" data-testid="app-version">GRAM Harness {{ version() ?? '…' }}</span>
          <span class="update" role="status" data-testid="update-state">
            @switch (updates.status()) {
              @case ('checking') {
                <g-spinner [size]="14" />
                Recherche…
              }
              @case ('upToDate') {
                <g-icon name="check" class="ok" />
                À jour.
              }
              @case ('available') {
                <g-tag tone="accent" icon="update"
                  >GRAM Harness {{ updates.available()?.version }} disponible</g-tag
                >
              }
              @case ('installing') {
                Installation en cours…
              }
              @case ('error') {
                <g-icon name="error" class="err" />
                {{ updates.error() }}
              }
            }
          </span>
        </p>
        @if (updates.status() === 'available') {
          <button gButton data-testid="update-open" (click)="openUpdate()">
            <g-icon name="download" />
            Voir et installer
          </button>
        } @else {
          <button
            gButton
            data-testid="update-check"
            [busy]="updates.status() === 'checking'"
            [disabled]="updates.status() === 'installing'"
            (click)="checkUpdate()"
          >
            <g-icon name="refresh" />
            Rechercher une mise à jour
          </button>
        }
      </section>
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
    .version {
      margin: 18px 0 0;
    }
    .line {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 6px 12px;
      margin: 0;
      font-size: 13px;
    }
    .update {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      color: var(--g-fg-2);
    }
    .ok {
      color: var(--g-ok);
    }
    .err {
      color: var(--g-error);
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
  private readonly updater = inject(Updates);
  protected readonly updates = this.updater.store;
  private readonly appVersion = signal<string | null>(null);
  /** Celle de la dernière vérification, sinon celle de l'application. */
  protected readonly version = computed(() => this.updates.currentVersion() ?? this.appVersion());

  constructor() {
    inject(HarnessApi)
      .appVersion()
      .then((v) => this.appVersion.set(v))
      .catch(() => undefined);
  }

  protected checkUpdate(): void {
    void this.updates.check();
  }

  /** Dialogue « Mise à jour » par-dessus les paramètres (notes, installation). */
  protected openUpdate(): void {
    this.updater.open();
  }

  protected setTheme(value: ThemePref | null): void {
    if (value) this.theme.set(value);
  }

  protected setKeepAlive(value: KeepAlive | null): void {
    if (value) this.prefs.setKeepAlive(value);
  }
}
