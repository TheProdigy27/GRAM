import { Component, DestroyRef, computed, inject } from '@angular/core';
import { ShortcutsService } from '@gram/shortcuts';
import type { ThemePref } from '@gram/theme';
import { Button } from '@gram/ui/button/button';
import { Icon } from '@gram/ui/icon/icon';
import type { IconName } from '@gram/ui/icon/icons';
import { MENU } from '@gram/ui/menu/menu';
import { Tooltip } from '@gram/ui/tooltip/tooltip';
import { ThemeService } from './core/theme.service';
import { Home } from './features/home/home';
import { ModelPicker } from './features/model-picker/model-picker';
import { Sidebar } from './features/sidebar/sidebar';
import { ModelsStore } from './state/models.store';

/** Délai minimal entre deux relectures déclenchées par le focus de la fenêtre. */
export const FOCUS_REFRESH_MS = 2000;

interface ThemeChoice {
  value: ThemePref;
  label: string;
  icon: IconName;
}

const THEMES: readonly ThemeChoice[] = [
  { value: 'light', label: 'Clair', icon: 'sun' },
  { value: 'dark', label: 'Sombre', icon: 'moon' },
  { value: 'system', label: 'Celui du système', icon: 'monitor' },
];

/**
 * Coque de GRAM Harness : barre latérale (conversations), en-tête (sélecteur de modèle,
 * « Actualiser », thème) et zone centrale. La liste des modèles est lue au lancement, par
 * « Actualiser » (R) et quand la fenêtre reprend le focus.
 */
@Component({
  selector: 'app-root',
  imports: [Button, Icon, MENU, Tooltip, Home, ModelPicker, Sidebar],
  templateUrl: './app.html',
  styleUrl: './app.scss',
  host: { '(window:focus)': 'onWindowFocus()' },
})
export class App {
  protected readonly store = inject(ModelsStore);
  /** Créé avec la coque : le thème choisi s'applique avant le premier rendu. */
  protected readonly theme = inject(ThemeService);
  protected readonly themes = THEMES;
  protected readonly themeIcon = computed<IconName>(
    () => THEMES.find((t) => t.value === this.theme.preference())?.icon ?? 'monitor',
  );
  private lastFocusRefresh = 0;

  constructor() {
    void this.store.refresh();
    inject(DestroyRef).onDestroy(
      inject(ShortcutsService).register([{ key: 'r', run: () => this.refresh() }]),
    );
  }

  protected refresh(): void {
    void this.store.refresh();
  }

  /** De retour dans la fenêtre (après un ajout dans GRAM, un lancement d'Ollama…). */
  protected onWindowFocus(): void {
    const now = Date.now();
    if (now - this.lastFocusRefresh < FOCUS_REFRESH_MS) return;
    this.lastFocusRefresh = now;
    void this.store.refresh();
  }
}
