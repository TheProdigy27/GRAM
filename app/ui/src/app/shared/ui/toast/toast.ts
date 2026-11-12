import { DOCUMENT } from '@angular/common';
import {
  ApplicationRef,
  Component,
  DestroyRef,
  EnvironmentInjector,
  Injectable,
  computed,
  createComponent,
  inject,
  input,
  signal,
} from '@angular/core';
import { type Observable, Subject } from 'rxjs';
import { Icon } from '../icon/icon';
import type { IconName } from '../icon/icons';

export type ToastKind = 'info' | 'success' | 'error';

export interface ToastOptions {
  /** Libellé du bouton d'action (« Annuler », « OK »). */
  action?: string;
  kind?: ToastKind;
  /** Durée d'affichage en millisecondes (4 s par défaut) ; 0 : jusqu'à fermeture. */
  duration?: number;
}

export interface ToastDismissal {
  dismissedByAction: boolean;
}

/** Toast affiché : action (`onAction`), fermeture (`afterDismissed`, `dismiss`). */
export class ToastRef {
  private readonly action$ = new Subject<void>();
  private readonly dismissed$ = new Subject<ToastDismissal>();
  private done = false;

  constructor(
    readonly message: string,
    readonly options: Required<ToastOptions>,
    private readonly onDismiss: (ref: ToastRef) => void,
  ) {}

  onAction(): Observable<void> {
    return this.action$.asObservable();
  }

  afterDismissed(): Observable<ToastDismissal> {
    return this.dismissed$.asObservable();
  }

  dismiss(): void {
    this.finish(false);
  }

  /** Clic sur le bouton d'action. */
  triggerAction(): void {
    if (this.done) return;
    this.action$.next();
    this.finish(true);
  }

  private finish(dismissedByAction: boolean): void {
    if (this.done) return;
    this.done = true;
    this.onDismiss(this);
    this.action$.complete();
    this.dismissed$.next({ dismissedByAction });
    this.dismissed$.complete();
  }
}

const ICON_BY_KIND: Record<ToastKind, IconName> = {
  info: 'info',
  success: 'check',
  error: 'error',
};

/**
 * Affiche des messages brefs en bas de la fenêtre, un à la fois (le nouveau remplace l'ancien).
 * La région `aria-live="polite"` existe dès la création du service : les lecteurs d'écran
 * annoncent chaque message. Le minuteur s'arrête tant que la souris ou le focus est sur le toast.
 */
@Injectable({ providedIn: 'root' })
export class ToastService {
  readonly current = signal<ToastRef | null>(null);
  private timer: ReturnType<typeof setTimeout> | null = null;
  private remaining = 0;
  private startedAt = 0;

  constructor() {
    const document = inject(DOCUMENT);
    const appRef = inject(ApplicationRef);
    const region = createComponent(ToastRegion, {
      environmentInjector: inject(EnvironmentInjector),
    });
    region.setInput('service', this);
    appRef.attachView(region.hostView);
    document.body.appendChild(region.location.nativeElement);
    inject(DestroyRef).onDestroy(() => {
      this.clearTimer();
      appRef.detachView(region.hostView);
      region.destroy();
      region.location.nativeElement.remove();
    });
  }

  show(message: string, options: ToastOptions = {}): ToastRef {
    this.current()?.dismiss();
    const ref = new ToastRef(
      message,
      { action: '', kind: 'info', duration: 4000, ...options },
      (r) => this.onDismiss(r),
    );
    this.current.set(ref);
    this.remaining = ref.options.duration;
    this.resume();
    return ref;
  }

  /** Arrête le minuteur (survol, focus). */
  pause(): void {
    if (this.timer === null) return;
    this.clearTimer();
    this.remaining = Math.max(0, this.remaining - (Date.now() - this.startedAt));
  }

  resume(): void {
    const ref = this.current();
    if (!ref || this.timer !== null || ref.options.duration <= 0) return;
    this.startedAt = Date.now();
    this.timer = setTimeout(() => ref.dismiss(), this.remaining);
  }

  private onDismiss(ref: ToastRef): void {
    if (this.current() !== ref) return;
    this.clearTimer();
    this.current.set(null);
  }

  private clearTimer(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}

/** Région des toasts, ajoutée au `<body>` par `ToastService`. */
@Component({
  selector: 'g-toast-region',
  imports: [Icon],
  template: `
    @if (toast(); as t) {
      <div
        class="g-toast"
        [attr.data-kind]="t.options.kind"
        (mouseenter)="service().pause()"
        (mouseleave)="service().resume()"
        (focusin)="service().pause()"
        (focusout)="service().resume()"
      >
        <g-icon class="g-toast__icon" [name]="icon()" />
        <span class="g-toast__message">{{ t.message }}</span>
        @if (t.options.action) {
          <button
            type="button"
            class="g-toast__action"
            data-testid="toast-action"
            (click)="t.triggerAction()"
          >
            {{ t.options.action }}
          </button>
        }
      </div>
    }
  `,
  host: {
    class: 'g-toast-region',
    role: 'status',
    'aria-live': 'polite',
  },
  styles: `
    :host {
      position: fixed;
      left: 50%;
      bottom: 24px;
      z-index: 1100;
      transform: translateX(-50%);
      width: max-content;
      max-width: calc(100vw - 32px);
      font-family: var(--g-font-sans);
    }
    .g-toast {
      display: flex;
      align-items: center;
      gap: 10px;
      min-height: 40px;
      padding: 6px 8px 6px 12px;
      border-radius: 10px;
      background: var(--g-tooltip);
      color: var(--g-tooltip-fg);
      font-size: 12.5px;
      line-height: 1.4;
      box-shadow: var(--g-shadow-pop);
    }
    .g-toast__message {
      flex: 1;
      padding-right: 4px;
    }
    .g-toast__action {
      height: 32px;
      padding: 0 10px;
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: inherit;
      font: inherit;
      font-weight: 600;
      text-decoration: underline;
      text-underline-offset: 2px;
      cursor: pointer;
    }
    .g-toast__action:hover {
      background: rgba(127, 127, 127, 0.2);
    }
    .g-toast__action:focus-visible {
      outline: 2px solid currentColor;
      outline-offset: -2px;
    }
  `,
})
export class ToastRegion {
  readonly service = input.required<ToastService>();
  protected readonly toast = computed(() => this.service().current());
  protected readonly icon = computed(() => ICON_BY_KIND[this.toast()?.options.kind ?? 'info']);
}
