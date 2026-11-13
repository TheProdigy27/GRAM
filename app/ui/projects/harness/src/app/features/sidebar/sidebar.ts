import { Component, ElementRef, PendingTasks, computed, inject } from '@angular/core';
import { ShortcutsService } from '@gram/shortcuts';
import { Button } from '@gram/ui/button/button';
import { DialogService } from '@gram/ui/dialog/dialog';
import { Icon } from '@gram/ui/icon/icon';
import { MENU } from '@gram/ui/menu/menu';
import { Spinner } from '@gram/ui/spinner/spinner';
import { ToastService } from '@gram/ui/toast/toast';
import { Tooltip } from '@gram/ui/tooltip/tooltip';
import type { ConversationSummary } from '@gram/types/ConversationSummary';
import { errorMessage } from '../../core/harness-api.service';
import { groupConversations } from '../../shared/conversations';
import { ChatStore } from '../../state/chat.store';
import { ModelsStore } from '../../state/models.store';

/**
 * Barre latérale : marque, « Nouvelle conversation », conversations groupées par date
 * (renommer et supprimer par menu) et état d'Ollama en pied.
 */
@Component({
  selector: 'app-sidebar',
  imports: [Button, Icon, MENU, Spinner, Tooltip],
  templateUrl: './sidebar.html',
  styleUrl: './sidebar.scss',
})
export class Sidebar {
  protected readonly store = inject(ModelsStore);
  protected readonly chat = inject(ChatStore);
  private readonly dialogs = inject(DialogService);
  private readonly toast = inject(ToastService);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  private readonly pendingTasks = inject(PendingTasks);
  protected readonly modN = inject(ShortcutsService).modShortcut('N');

  protected readonly groups = computed(() => groupConversations(this.chat.conversations()));
  /** Conversation dont la réponse est en cours. */
  protected readonly streamingId = computed(() => this.chat.streaming()?.conversationId ?? null);

  /** « Ollama en cours · 3 modèles », « Ollama arrêté »… ; vide avant la première lecture. */
  protected readonly status = computed(() => {
    if (this.store.error()) return 'Ollama : réponse illisible';
    switch (this.store.ollama()) {
      case 'running': {
        const n = this.store.models().length;
        return `Ollama en cours · ${n} modèle${n > 1 ? 's' : ''}`;
      }
      case 'stopped':
        return 'Ollama arrêté';
      case 'absent':
        return 'Ollama introuvable';
      default:
        return '';
    }
  });

  /** Pastille : vert si Ollama répond, orange s'il est arrêté, rouge s'il manque ou échoue. */
  protected readonly tone = computed(() => {
    if (this.store.error()) return 'error';
    switch (this.store.ollama()) {
      case 'running':
        return 'ok';
      case 'stopped':
        return 'warn';
      default:
        return 'error';
    }
  });

  protected newConversation(): void {
    this.chat.newConversation();
  }

  protected open(id: string): void {
    void this.chat.open(id);
  }

  /**
   * Bouton du menu d'une conversation : le focus y revient à la fermeture d'un dialogue (sous
   * WebKit, un dialogue ouvert par un clic le rendrait sinon au document).
   */
  private menuButton(id: string): HTMLElement | undefined {
    return this.host.querySelector<HTMLElement>(`li[data-id="${id}"] .item__menu`) ?? undefined;
  }

  /** Conversation telle qu'elle est maintenant (titre changé depuis…). */
  private find(id: string): ConversationSummary | undefined {
    return this.chat.conversations().find((c) => c.id === id);
  }

  /** Dialogues chargés à la demande ; tâche suivie par Angular (tests : `whenStable`). */
  protected rename(id: string): void {
    const c = this.find(id);
    if (!c) return;
    const origin = this.menuButton(id);
    this.pendingTasks.run(async () => {
      const { RenameDialog } = await import('./conversation-dialogs');
      const ref = this.dialogs.open<string>(RenameDialog, {
        data: c,
        width: '460px',
        autoFocus: '[data-testid="rename-input"]',
        restoreFocus: origin ?? true,
      });
      ref.closed.subscribe((title) => {
        if (!title || title === c.title) return;
        this.chat.rename(c.id, title).catch((e: unknown) => {
          this.toast.show(`Conversation non renommée : ${errorMessage(e)}`, { kind: 'error' });
        });
      });
    });
  }

  protected remove(id: string): void {
    const c = this.find(id);
    if (!c) return;
    const origin = this.menuButton(id);
    this.pendingTasks.run(async () => {
      const { DeleteDialog } = await import('./conversation-dialogs');
      const ref = this.dialogs.open<boolean>(DeleteDialog, {
        data: c,
        width: '460px',
        restoreFocus: origin ?? true,
      });
      ref.closed.subscribe((confirmed) => {
        if (!confirmed) return;
        this.chat.remove(c.id).then(
          () => this.toast.show('Conversation supprimée', { kind: 'success' }),
          (e: unknown) =>
            this.toast.show(`Conversation non supprimée : ${errorMessage(e)}`, { kind: 'error' }),
        );
      });
    });
  }
}
