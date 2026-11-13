import { Component, computed, inject } from '@angular/core';
import { Select } from '@gram/ui/select/select';
import { Tag } from '@gram/ui/tag/tag';
import { Tooltip } from '@gram/ui/tooltip/tooltip';
import { capabilities, modelOptions } from '../../shared/models';
import { ModelsStore } from '../../state/models.store';

/**
 * Sélecteur de modèle de l'en-tête : modèles de GRAM puis autres modèles d'Ollama, avec les
 * capacités du modèle choisi (vision, réflexion, outils) en étiquettes.
 */
@Component({
  selector: 'app-model-picker',
  imports: [Select, Tag, Tooltip],
  template: `
    <g-select
      class="select"
      ariaLabel="Modèle"
      data-testid="model-select"
      [placeholder]="store.list() || store.error() ? 'Aucun modèle' : 'Recherche…'"
      [options]="options()"
      [value]="store.selected()?.name ?? null"
      [disabled]="options().length === 0"
      (valueChange)="pick($event)"
    />
    @for (c of caps(); track c.key) {
      <g-tag
        size="sm"
        [icon]="c.icon"
        [gTooltip]="c.description"
        [attr.data-testid]="'capability-' + c.key"
        >{{ c.label }}</g-tag
      >
    }
  `,
  styles: `
    :host {
      display: flex;
      align-items: center;
      gap: 6px;
      min-width: 0;
    }
    .select {
      flex: 0 1 340px;
      min-width: 160px;
      margin-right: 4px;
    }
  `,
})
export class ModelPicker {
  protected readonly store = inject(ModelsStore);
  protected readonly options = computed(() => modelOptions(this.store.models()));
  protected readonly caps = computed(() => {
    const m = this.store.selected();
    return m ? capabilities(m) : [];
  });

  protected pick(name: string | null): void {
    if (name) this.store.select(name);
  }
}
