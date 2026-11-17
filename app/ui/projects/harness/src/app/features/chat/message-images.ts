import { Component, computed, inject, input } from '@angular/core';
import { Icon } from '@gram/ui/icon/icon';
import { ImagesStore } from '../../state/images.store';

/**
 * Images d'un message de l'utilisateur, en vignettes : lues une fois par image (`get_image`),
 * ou adresses `data:` d'un message pas encore enregistré.
 */
@Component({
  selector: 'app-message-images',
  imports: [Icon],
  template: `
    <ul class="list" aria-label="Images jointes">
      @for (image of images(); track image.name; let i = $index) {
        <li class="item" data-testid="message-image">
          @if (image.url) {
            <img class="img" [src]="image.url" [alt]="'Image jointe ' + (i + 1)" />
          } @else if (image.url === null) {
            <span class="missing" data-testid="message-image-missing">
              <g-icon name="image" [size]="18" />
              Image introuvable
            </span>
          } @else {
            <span class="loading" role="img" [attr.aria-label]="'Image jointe ' + (i + 1)"></span>
          }
        </li>
      }
    </ul>
  `,
  styles: `
    .list {
      display: flex;
      flex-wrap: wrap;
      justify-content: flex-end;
      gap: 8px;
      max-width: min(80%, 620px);
      margin: 0;
      padding: 0;
      list-style: none;
    }
    .item {
      display: flex;
    }
    .img,
    .loading,
    .missing {
      max-width: 220px;
      max-height: 220px;
      border: 1px solid var(--g-line);
      border-radius: 10px;
      background: var(--g-surface-2);
    }
    .img {
      display: block;
      object-fit: contain;
    }
    .loading,
    .missing {
      width: 120px;
      height: 90px;
    }
    .missing {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 4px;
      font-size: 11.5px;
      color: var(--g-muted);
    }
  `,
})
export class MessageImages {
  readonly conversationId = input('');
  readonly names = input.required<readonly string[]>();
  private readonly store = inject(ImagesStore);

  protected readonly images = computed(() =>
    this.names().map((name) => ({ name, url: this.store.url(this.conversationId(), name) })),
  );
}
