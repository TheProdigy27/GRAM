import { Component, DestroyRef, PendingTasks, computed, inject } from '@angular/core';
import { ShortcutsService } from '@gram/shortcuts';
import type { ThemePref } from '@gram/theme';
import { Button } from '@gram/ui/button/button';
import { DialogService } from '@gram/ui/dialog/dialog';
import { Icon } from '@gram/ui/icon/icon';
import type { IconName } from '@gram/ui/icon/icons';
import { MENU } from '@gram/ui/menu/menu';
import { Tooltip } from '@gram/ui/tooltip/tooltip';
import { ThemeService } from './core/theme.service';
import { ChatView } from './features/chat/chat-view';
import { ContextGauge } from './features/context-gauge/context-gauge';
import { ModelPicker } from './features/model-picker/model-picker';
import { Sidebar } from './features/sidebar/sidebar';
import { ChatStore } from './state/chat.store';
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
 * Coque de GRAM Harness : barre latérale (conversations), en-tête (sélecteur de modèle, jauge
 * de contexte, « Actualiser », thème, paramètres) et discussion. La liste des modèles est lue
 * au lancement, par « Actualiser » (R) et quand la fenêtre reprend le focus ; celle des
 * conversations au lancement. Raccourcis : Ctrl N, Ctrl , (paramètres), Échap (arrêter la
 * réponse), ? (aide), R.
 */
@Component({
  selector: 'app-root',
  imports: [Button, Icon, MENU, Tooltip, ChatView, ContextGauge, ModelPicker, Sidebar],
  templateUrl: './app.html',
  styleUrl: './app.scss',
  host: {
    '(window:focus)': 'onWindowFocus()',
    // Un fichier lâché hors de la zone de dépôt ne doit pas remplacer la page par le fichier.
    '(window:dragover)': 'blockFileDrop($event)',
    '(window:drop)': 'blockFileDrop($event)',
  },
})
export class App {
  protected readonly store = inject(ModelsStore);
  private readonly chat = inject(ChatStore);
  private readonly dialogs = inject(DialogService);
  private readonly pendingTasks = inject(PendingTasks);
  /** Créé avec la coque : le thème choisi s'applique avant le premier rendu. */
  protected readonly theme = inject(ThemeService);
  protected readonly themes = THEMES;
  protected readonly themeIcon = computed<IconName>(
    () => THEMES.find((t) => t.value === this.theme.preference())?.icon ?? 'monitor',
  );
  protected readonly modComma = inject(ShortcutsService).modShortcut(',');
  private lastFocusRefresh = 0;

  constructor() {
    void this.store.refresh();
    void this.chat.load();
    inject(DestroyRef).onDestroy(
      inject(ShortcutsService).register([
        { key: 'r', run: () => this.refresh() },
        { key: 'n', mod: true, inInputs: true, run: () => this.chat.newConversation() },
        { key: ',', mod: true, inInputs: true, run: () => this.openSettings() },
        { key: '?', run: () => this.openShortcuts() },
        {
          key: 'Escape',
          inInputs: true,
          run: () => {
            if (!this.chat.streamingHere()) return false;
            void this.chat.stop();
            return true;
          },
        },
      ]),
    );
  }

  /** Aide des raccourcis, chargée à la demande (tâche suivie par Angular). */
  protected openShortcuts(): void {
    this.pendingTasks.run(async () => {
      const { ShortcutsHelp } = await import('./features/shortcuts-help/shortcuts-help');
      this.dialogs.open(ShortcutsHelp, { width: '720px' });
    });
  }

  /** Paramètres, chargés à la demande (tâche suivie par Angular). */
  protected openSettings(): void {
    this.pendingTasks.run(async () => {
      const { SettingsDialog } = await import('./features/settings/settings-dialog');
      this.dialogs.open(SettingsDialog, { width: '560px' });
    });
  }

  protected blockFileDrop(event: DragEvent): void {
    if (Array.from(event.dataTransfer?.types ?? []).includes('Files')) event.preventDefault();
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
