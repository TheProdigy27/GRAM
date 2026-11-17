import {
  Component,
  DestroyRef,
  ElementRef,
  Injector,
  PendingTasks,
  afterNextRender,
  computed,
  inject,
  input,
  viewChild,
} from '@angular/core';
import type { HarnessAddRequest } from '../../core/generated/HarnessAddRequest';
import type { HarnessEntry } from '../../core/generated/HarnessEntry';
import type { ModelDetail } from '../../core/generated/ModelDetail';
import type { VariantDetail } from '../../core/generated/VariantDetail';
import { TauriService, errorMessage } from '../../core/tauri.service';
import { ctxK, decimal, fileSize, int, roundSize, tokS } from '../../shared/format';
import { installPercent, installStep, sameVariant } from '../../shared/harness';
import { CtxPipe } from '../../shared/pipes';
import { Button } from '../../shared/ui/button/button';
import { Callout } from '../../shared/ui/callout/callout';
import { DialogService } from '../../shared/ui/dialog/dialog';
import { Icon } from '../../shared/ui/icon/icon';
import { Progress } from '../../shared/ui/progress/progress';
import { Skeleton } from '../../shared/ui/skeleton/skeleton';
import { Tag } from '../../shared/ui/tag/tag';
import { ToastService } from '../../shared/ui/toast/toast';
import { HardwareStore } from '../../state/hardware.store';
import { HarnessStore } from '../../state/harness.store';
import { RELEASES_URL } from '../../state/update.store';
import { CommandBlock } from './command-block';
import type { RemoveChoice } from './harness-remove-dialog';

/** État de la section pour la quantification affichée. */
type PanelState = 'absent' | 'stopped' | 'new' | 'installing' | 'added';

/**
 * « Discuter avec » : ajout de la quantification choisie à GRAM Harness, c'est-à-dire à Ollama
 * (téléchargée si besoin), réglée au contexte calculé par GRAM. Progression et annulation,
 * puis modèle dérivé `gram/…`, « Ouvrir dans GRAM Harness » (ou, s'il n'est pas installé,
 * « Télécharger GRAM Harness » et la commande `ollama run`), réglage d'un autre contexte et
 * retrait.
 */
@Component({
  selector: 'app-harness-panel',
  imports: [Button, Callout, CommandBlock, Icon, Progress, Skeleton, Tag, CtxPipe],
  templateUrl: './harness-panel.html',
  styleUrl: './harness-panel.scss',
})
export class HarnessPanel {
  readonly detail = input.required<ModelDetail>();
  readonly variant = input.required<VariantDetail>();

  protected readonly store = inject(HarnessStore);
  protected readonly hardware = inject(HardwareStore);
  private readonly tauri = inject(TauriService);
  private readonly toast = inject(ToastService);
  private readonly dialogs = inject(DialogService);
  private readonly pendingTasks = inject(PendingTasks);
  private readonly injector = inject(Injector);

  private destroyed = false;

  private readonly title = viewChild.required<ElementRef<HTMLElement>>('title');
  private readonly cancelButton = viewChild('cancelButton', { read: ElementRef });
  private readonly removeButton = viewChild('removeButton', { read: ElementRef });

  constructor() {
    inject(DestroyRef).onDestroy(() => (this.destroyed = true));
  }

  private readonly key = computed(() => ({
    repoId: this.detail().model.repoId,
    label: this.variant().variant.label,
  }));

  protected readonly ollama = computed(() => this.hardware.runtimes()?.ollama ?? null);
  protected readonly entry = computed(() => this.store.entryFor(this.key()) ?? null);
  private readonly installingHere = computed(() => sameVariant(this.store.install(), this.key()));
  /** Ajout en cours d'une autre quantification. */
  private readonly otherInstall = computed(() =>
    this.installingHere() ? null : this.store.install(),
  );
  protected readonly preflight = computed(() => {
    const p = this.store.preflight();
    return sameVariant(p, this.key()) ? p : null;
  });
  protected readonly error = computed(() => {
    const e = this.store.error();
    return sameVariant(e, this.key()) ? e!.message : null;
  });

  protected readonly state = computed<PanelState>(() => {
    if (this.installingHere()) return 'installing';
    const o = this.ollama();
    if (!o?.installed) return 'absent';
    if (!o.running) return 'stopped';
    return this.entry() ? 'added' : 'new';
  });

  /** « Télécharge 4,9 Go dans Ollama · 812 Go libres sur C:\ · contexte 16k ». */
  protected readonly facts = computed(() => {
    const p = this.preflight();
    if (!p) return '';
    const parts: string[] = [];
    if (p.download > 0) {
      parts.push(`Télécharge ${fileSize(p.download)} dans Ollama`);
      if (p.disk) parts.push(`${roundSize(p.disk.available)} libres sur ${p.disk.mount}`);
    } else {
      parts.push('Déjà dans Ollama');
    }
    parts.push(`contexte ${ctxK(p.ctx)}`);
    return parts.join(' · ');
  });

  /** Pourquoi « Ajouter » est désactivé (ajout en cours ailleurs, trop gros, place). */
  protected readonly blocked = computed(() => {
    const other = this.otherInstall();
    if (other) {
      return `Ajout de ${other.displayName} ${other.label} en cours : attendre sa fin, ou l’annuler.`;
    }
    return this.preflight()?.blocked ?? null;
  });

  protected readonly progress = computed(() => {
    const p = this.installingHere() ? this.store.install() : null;
    if (!p) return null;
    const percent = installPercent(p);
    return {
      step: installStep(p),
      percent,
      percentText: percent === null ? '' : `${Math.floor(percent)} %`,
    };
  });

  /** Ajoutée : « contexte 8k · ~95 tok/s estimés · 83 mesurés ». */
  protected readonly addedFacts = computed(() => {
    const m = this.entry()?.model;
    if (!m) return '';
    const parts = [`contexte ${ctxK(m.ctx)}`, `${tokS(m.estimatedTokS)} tok/s estimés`];
    const measured = m.measuredTokS;
    if (measured !== null) {
      parts.push(`${measured < 10 ? decimal(measured, 1) : int(measured)} mesurés`);
    }
    return parts.join(' · ');
  });

  protected readonly addedWarnings = computed(() => {
    const m = this.entry()?.model;
    return m ? (this.store.addedWarnings()[m.ollamaName] ?? []) : [];
  });

  /** Contexte que réglerait un nouvel ajout, quand il diffère de celui du modèle dérivé. */
  protected readonly ctxUpdate = computed(() => {
    const e = this.entry();
    const p = this.preflight();
    return e && p && !p.blocked && p.ctx > 0 && p.ctx !== e.model.ctx ? p.ctx : null;
  });

  protected readonly removing = computed(() => {
    const e = this.entry();
    return !!e && this.store.removing() === e.model.ollamaName;
  });

  private request(): HarnessAddRequest {
    const d = this.detail();
    return {
      repoId: d.model.repoId,
      label: this.variant().variant.label,
      useCase: d.useCase,
      ctx: d.ctx,
    };
  }

  /** Ajout, ou nouveau modèle dérivé à l'autre contexte (sans téléchargement). */
  protected async add(): Promise<void> {
    const done = this.store.add(this.request());
    // La progression remplace le bouton : le focus passe à « Annuler ».
    afterNextRender(() => this.cancelButton()?.nativeElement.focus(), { injector: this.injector });
    await done;
    this.focusTitleIfLost();
  }

  protected cancel(): void {
    void this.store.cancel();
  }

  /** GRAM Harness choisit le modèle dans une nouvelle conversation et passe au premier plan. */
  protected open(entry: HarnessEntry): void {
    void this.store.open(entry.model);
  }

  /** Page des releases (GRAM Harness n'est pas installé). */
  protected download(): void {
    this.tauri.openUrl(RELEASES_URL).catch((e: unknown) => {
      this.toast.show(`Navigateur non ouvert (${errorMessage(e)}) : aller sur ${RELEASES_URL}`, {
        kind: 'error',
      });
    });
  }

  /** « Retirer » : confirmation, avec la suppression de la base quand elle est permise. */
  protected remove(entry: HarnessEntry): void {
    // WebKit ne donne pas le focus à un bouton cliqué : il est rendu à ce bouton.
    const origin = this.removeButton()?.nativeElement as HTMLElement | undefined;
    this.pendingTasks.run(async () => {
      const { HarnessRemoveDialog } = await import('./harness-remove-dialog');
      const ref = this.dialogs.open<RemoveChoice, HarnessEntry>(HarnessRemoveDialog, {
        data: entry,
        width: '480px',
        restoreFocus: origin ?? true,
      });
      ref.closed.subscribe((choice) => {
        if (!choice) return;
        void this.store.remove(entry.model, choice.removeBase).then((r) => {
          if (r) this.focusTitleIfLost();
        });
      });
    });
  }

  /** Après `ollama pull` ou le lancement d'Ollama. */
  protected refresh(): void {
    void this.hardware.refreshRuntimes();
  }

  /** Le bouton qui avait le focus a disparu (fin d'ajout, retrait) : focus sur le titre. */
  private focusTitleIfLost(): void {
    if (this.destroyed) return;
    afterNextRender(
      () => {
        const active = document.activeElement;
        if (!active || active === document.body) this.title().nativeElement.focus();
      },
      { injector: this.injector },
    );
  }
}
