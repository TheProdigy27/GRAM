import { AriaDescriber, FocusMonitor } from '@angular/cdk/a11y';
import { ESCAPE, hasModifierKey } from '@angular/cdk/keycodes';
import {
  type ConnectedPosition,
  type OverlayRef,
  createCloseScrollStrategy,
  createFlexibleConnectedPositionStrategy,
  createOverlayRef,
} from '@angular/cdk/overlay';
import { ComponentPortal } from '@angular/cdk/portal';
import {
  Component,
  type ComponentRef,
  DestroyRef,
  Directive,
  ElementRef,
  InjectionToken,
  Injector,
  booleanAttribute,
  effect,
  inject,
  input,
  untracked,
} from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import type { Subscription } from 'rxjs';

export type TooltipPosition = 'above' | 'below' | 'left' | 'right';

/** Délai avant l'apparition au survol ou au focus clavier. */
export const TOOLTIP_DELAY_MS = new InjectionToken<number>('TOOLTIP_DELAY_MS', {
  factory: () => 400,
});

const GAP = 6;
const POSITIONS: Record<TooltipPosition, ConnectedPosition> = {
  above: {
    originX: 'center',
    originY: 'top',
    overlayX: 'center',
    overlayY: 'bottom',
    offsetY: -GAP,
  },
  below: {
    originX: 'center',
    originY: 'bottom',
    overlayX: 'center',
    overlayY: 'top',
    offsetY: GAP,
  },
  left: { originX: 'start', originY: 'center', overlayX: 'end', overlayY: 'center', offsetX: -GAP },
  right: { originX: 'end', originY: 'center', overlayX: 'start', overlayY: 'center', offsetX: GAP },
};
const FALLBACKS: Record<TooltipPosition, TooltipPosition[]> = {
  above: ['above', 'below', 'right', 'left'],
  below: ['below', 'above', 'right', 'left'],
  left: ['left', 'right', 'above', 'below'],
  right: ['right', 'left', 'above', 'below'],
};

/** Bulle affichée (visuelle seulement : la description accessible passe par `AriaDescriber`). */
@Component({
  selector: 'g-tooltip-panel',
  template: `{{ message() }}
    @if (shortcut()) {
      <kbd class="g-kbd">{{ shortcut() }}</kbd>
    }`,
  host: { class: 'g-tooltip', 'aria-hidden': 'true' },
  styles: `
    :host {
      display: block;
      max-width: 320px;
      padding: 7px 10px;
      border-radius: var(--g-radius);
      background: var(--g-tooltip);
      color: var(--g-tooltip-fg);
      font: 12px/1.4 var(--g-font-sans);
      white-space: pre-line;
      box-shadow: var(--g-shadow-pop);
    }
    .g-kbd {
      margin-left: 6px;
      border-color: currentColor;
      background: transparent;
      color: inherit;
      opacity: 0.75;
    }
  `,
})
export class TooltipPanel {
  readonly message = input('');
  readonly shortcut = input('');
}

/**
 * Infobulle : `<button gTooltip="Ré-analyser le matériel" gTooltipShortcut="R">`.
 * Apparaît après 400 ms au survol ou au focus clavier, se ferme en sortant, au clic ou avec
 * Échap. Le texte est relié à l'élément par `aria-describedby` ; les sauts de ligne sont gardés.
 */
@Directive({
  selector: '[gTooltip]',
  host: {
    '(mouseenter)': 'scheduleShow()',
    '(mouseleave)': 'hide()',
    '(pointerdown)': 'hide()',
    '[attr.aria-keyshortcuts]': 'shortcut() || null',
  },
})
export class Tooltip {
  readonly message = input<string | null | undefined>('', { alias: 'gTooltip' });
  readonly position = input<TooltipPosition>('above', { alias: 'gTooltipPosition' });
  readonly disabled = input(false, { alias: 'gTooltipDisabled', transform: booleanAttribute });
  /** Raccourci clavier affiché dans la bulle et annoncé (`aria-keyshortcuts`). */
  readonly shortcut = input('', { alias: 'gTooltipShortcut' });

  private readonly injector = inject(Injector);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  private readonly describer = inject(AriaDescriber);
  private readonly delay = inject(TOOLTIP_DELAY_MS);
  private overlayRef: OverlayRef | null = null;
  private panel: ComponentRef<TooltipPanel> | null = null;
  private keydown: Subscription | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private described = '';

  constructor() {
    effect(() => {
      const text = this.disabled() ? '' : (this.message() ?? '').trim();
      const shortcut = this.shortcut();
      untracked(() => {
        this.describe(text);
        if (!text) this.hide();
        else if (this.panel) this.render(text, shortcut);
      });
    });

    const focusMonitor = inject(FocusMonitor);
    focusMonitor
      .monitor(this.host)
      .pipe(takeUntilDestroyed())
      .subscribe((origin) => {
        if (origin === 'keyboard') this.scheduleShow();
        else if (origin === null) this.hide();
      });

    inject(DestroyRef).onDestroy(() => {
      focusMonitor.stopMonitoring(this.host);
      this.hide();
      this.describe('');
      this.overlayRef?.dispose();
    });
  }

  /** La bulle est-elle affichée ? */
  get visible(): boolean {
    return !!this.panel;
  }

  protected scheduleShow(): void {
    if (this.panel || this.timer !== null || !this.described) return;
    if (this.delay <= 0) this.show();
    else this.timer = setTimeout(() => this.show(), this.delay);
  }

  show(): void {
    this.clearTimer();
    if (this.panel || !this.described) return;
    const position = createFlexibleConnectedPositionStrategy(this.injector, this.host)
      .withPositions(FALLBACKS[this.position()].map((p) => POSITIONS[p]))
      .withFlexibleDimensions(false)
      .withViewportMargin(8)
      .withPush(true);
    if (this.overlayRef) {
      this.overlayRef.updatePositionStrategy(position);
    } else {
      this.overlayRef = createOverlayRef(this.injector, {
        positionStrategy: position,
        scrollStrategy: createCloseScrollStrategy(this.injector),
        panelClass: 'g-tooltip-pane',
      });
      this.overlayRef.detachments().subscribe(() => this.cleanUp());
    }
    this.panel = this.overlayRef.attach(new ComponentPortal(TooltipPanel));
    this.render(this.described, this.shortcut());
    this.keydown = this.overlayRef.keydownEvents().subscribe((event) => {
      if (event.keyCode === ESCAPE && !hasModifierKey(event)) {
        event.preventDefault();
        event.stopPropagation();
        this.hide();
      }
    });
  }

  hide(): void {
    this.clearTimer();
    if (this.overlayRef?.hasAttached()) this.overlayRef.detach();
  }

  private render(message: string, shortcut: string): void {
    if (!this.panel) return;
    this.panel.setInput('message', message);
    this.panel.setInput('shortcut', shortcut);
    this.panel.changeDetectorRef.detectChanges();
    this.overlayRef?.updatePosition();
  }

  private cleanUp(): void {
    this.panel = null;
    this.keydown?.unsubscribe();
    this.keydown = null;
  }

  private describe(text: string): void {
    if (text === this.described) return;
    if (this.described) this.describer.removeDescription(this.host, this.described, 'tooltip');
    if (text) this.describer.describe(this.host, text, 'tooltip');
    this.described = text;
  }

  private clearTimer(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}
