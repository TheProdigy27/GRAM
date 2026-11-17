import {
  Component,
  DestroyRef,
  booleanAttribute,
  computed,
  inject,
  input,
  signal,
} from '@angular/core';
import { Button } from '@gram/ui/button/button';
import { Callout } from '@gram/ui/callout/callout';
import { Icon } from '@gram/ui/icon/icon';
import { Spinner } from '@gram/ui/spinner/spinner';
import { Tag } from '@gram/ui/tag/tag';
import { ToastService } from '@gram/ui/toast/toast';
import { Tooltip } from '@gram/ui/tooltip/tooltip';
import type { ChatMessage } from '@gram/types/ChatMessage';
import { HarnessApi, errorMessage } from '../../core/harness-api.service';
import { modelLabel } from '../../shared/conversations';
import { duration, gramEstimate, speedFacts } from '../../shared/stats';
import { ChatStore } from '../../state/chat.store';
import { ModelsStore } from '../../state/models.store';
import { MarkdownView } from './markdown-view';
import { MessageImages } from './message-images';

/** Durée du ✓ après une copie. */
const COPIED_MS = 2000;

let nextId = 0;

/**
 * Un message du fil : celui de l'utilisateur à droite (images, puis texte brut), une réponse en
 * pleine largeur (Markdown), avec le nom de son modèle, sa réflexion (ouverte pendant qu'elle
 * s'écrit, repliée dès que la réponse commence, « Réflexion · 12 s »), son échec, sa vitesse et
 * ses actions (copier ; régénérer ou réessayer pour la dernière).
 */
@Component({
  selector: 'app-message',
  imports: [Button, Callout, Icon, MarkdownView, MessageImages, Spinner, Tag, Tooltip],
  templateUrl: './message.html',
  styleUrl: './message.scss',
  host: { '[attr.data-role]': 'message().role' },
})
export class Message {
  readonly message = input.required<ChatMessage>();
  /** Conversation du message (images enregistrées). */
  readonly conversationId = input('');
  /** Dernière réponse de la conversation : « Régénérer », « Réessayer ». */
  readonly last = input(false, { transform: booleanAttribute });
  /** Réponse en cours. */
  readonly streaming = input(false, { transform: booleanAttribute });

  protected readonly store = inject(ChatStore);
  private readonly models = inject(ModelsStore);
  private readonly api = inject(HarnessApi);
  private readonly toast = inject(ToastService);
  protected readonly thinkingId = `thinking-${nextId++}`;

  protected readonly model = computed(() => {
    const name = this.message().model;
    return name ? modelLabel(this.models.models(), name) : 'Modèle inconnu';
  });
  /** Régénérer est possible : aucune réponse en cours, un modèle choisi. */
  protected readonly canRegenerate = computed(
    () => !this.store.busy() && this.models.selected() !== null,
  );

  /** La réflexion s'écrit encore (la réponse n'a pas commencé). */
  protected readonly thinkingLive = computed(() => this.streaming() && !this.message().content);
  /** Choix de l'utilisateur ; sans choix, ouverte seulement pendant qu'elle s'écrit. */
  private readonly thinkingChoice = signal<boolean | null>(null);
  protected readonly thinkingOpen = computed(() => this.thinkingChoice() ?? this.thinkingLive());
  /** « Réflexion… 4 s » pendant qu'elle s'écrit, puis « Réflexion · 12 s ». */
  protected readonly thinkingLabel = computed(() => {
    const ms = this.message().thinkingMs;
    if (this.thinkingLive()) return ms === null ? 'Réflexion…' : `Réflexion… ${duration(ms)}`;
    return ms === null ? 'Réflexion' : `Réflexion · ${duration(ms)}`;
  });

  /** « 128 tokens · 41 tok/s · chargement 2,1 s ». */
  protected readonly speed = computed(() => {
    const stats = this.message().stats;
    return stats ? speedFacts(stats).join(' · ') : '';
  });
  /** Infobulle de la vitesse : l'estimation de GRAM pour un modèle ajouté depuis GRAM. */
  protected readonly estimate = computed(() =>
    gramEstimate(this.models.models().find((m) => m.name === this.message().model)),
  );

  protected readonly copied = signal(false);
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    inject(DestroyRef).onDestroy(() => {
      if (this.timer !== null) clearTimeout(this.timer);
    });
  }

  protected toggleThinking(): void {
    this.thinkingChoice.set(!this.thinkingOpen());
  }

  protected regenerate(): void {
    void this.store.regenerate();
  }

  protected async copy(): Promise<void> {
    try {
      await this.api.copyText(this.message().content);
      this.toast.show('Réponse copiée', { kind: 'success' });
      if (this.timer !== null) clearTimeout(this.timer);
      this.copied.set(true);
      this.timer = setTimeout(() => this.copied.set(false), COPIED_MS);
    } catch (e) {
      this.toast.show(`Copie impossible : ${errorMessage(e)}`, { kind: 'error' });
    }
  }
}
