import { Component, computed, inject } from '@angular/core';
import { Button } from '@gram/ui/button/button';
import { Icon } from '@gram/ui/icon/icon';
import { ModelsStore } from '../../state/models.store';

/**
 * Barre latérale : marque, « Nouvelle conversation », conversations (vides jusqu'à la
 * discussion) et état d'Ollama en pied.
 */
@Component({
  selector: 'app-sidebar',
  imports: [Button, Icon],
  templateUrl: './sidebar.html',
  styleUrl: './sidebar.scss',
})
export class Sidebar {
  protected readonly store = inject(ModelsStore);

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
}
