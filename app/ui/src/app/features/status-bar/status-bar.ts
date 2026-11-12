import { Component, DestroyRef, computed, inject, signal } from '@angular/core';
import { int, relativeTime } from '../../shared/format';
import { DATA_SOURCE_LABELS, SYNC_PHASE_LABELS } from '../../shared/labels';
import { Icon } from '../../shared/ui/icon/icon';
import { Progress } from '../../shared/ui/progress/progress';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';
import { CatalogStore } from '../../state/catalog.store';
import { RankingStore } from '../../state/ranking.store';

/**
 * Barre d'état : source des données, nombre de modèles, progression de la synchronisation,
 * quota de l'API Hugging Face, marge des estimations.
 */
@Component({
  selector: 'app-status-bar',
  imports: [Icon, Progress, Tooltip],
  templateUrl: './status-bar.html',
  styleUrl: './status-bar.scss',
})
export class StatusBar {
  protected readonly catalog = inject(CatalogStore);
  private readonly ranking = inject(RankingStore);

  /** Heure courante, rafraîchie chaque minute pour « il y a 2 h ». */
  private readonly now = signal(Date.now());

  constructor() {
    const timer = setInterval(() => this.now.set(Date.now()), 60_000);
    inject(DestroyRef).onDestroy(() => clearInterval(timer));
  }

  protected readonly source = computed(() => {
    const s = this.ranking.response()?.dataSource;
    if (!s) return null;
    const label = DATA_SOURCE_LABELS[s.kind];
    return `${label.charAt(0).toUpperCase()}${label.slice(1)} · ${relativeTime(s.fetchedAt, this.now())}`;
  });

  protected readonly sourceTooltip = computed(() => {
    const s = this.ranking.response()?.dataSource;
    if (!s) return '';
    const when = new Date(s.fetchedAt).toLocaleString('fr-FR');
    return `Catalogue synchronisé le ${when}`;
  });

  protected readonly counts = computed(() => {
    const r = this.ranking.response();
    if (!r) return null;
    const total = r.totalItems + r.totalSlow + r.tooBigCount;
    return `${int(total)} modèles · ${int(r.totalItems + r.totalSlow)} exécutables`;
  });

  protected readonly progress = computed(() => {
    const p = this.catalog.progress();
    if (!p) return null;
    return {
      label: `${SYNC_PHASE_LABELS[p.phase]} ${p.done}/${p.total}`,
      done: p.done,
      total: Math.max(p.total, 1),
    };
  });

  protected readonly quota = computed(() => {
    const r = this.catalog.rateLimit();
    return r ? `Quota Hugging Face : ${r.remaining} requêtes` : null;
  });

  protected readonly quotaTooltip = computed(() => {
    const r = this.catalog.rateLimit();
    if (!r) return '';
    return `Remise à zéro dans ${Math.ceil(r.resetS / 60)} min`;
  });

  protected readonly syncWarnings = computed(() =>
    (this.catalog.summary()?.warnings ?? []).join('\n'),
  );
}
