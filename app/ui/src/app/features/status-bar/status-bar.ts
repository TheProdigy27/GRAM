import { Component, DestroyRef, computed, inject, signal } from '@angular/core';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatTooltipModule } from '@angular/material/tooltip';
import { int, relativeTime } from '../../shared/format';
import { DATA_SOURCE_LABELS, SYNC_PHASE_LABELS } from '../../shared/labels';
import { CatalogStore } from '../../state/catalog.store';
import { RankingStore } from '../../state/ranking.store';

/** Barre d'état : source des données, synchronisation, quota de l'API Hugging Face. */
@Component({
  selector: 'app-status-bar',
  imports: [MatIconModule, MatProgressBarModule, MatTooltipModule],
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
    return `${DATA_SOURCE_LABELS[s.kind]} · ${relativeTime(s.fetchedAt, this.now())}`;
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
    const pct = p.total > 0 ? (100 * p.done) / p.total : 0;
    return { label: `${SYNC_PHASE_LABELS[p.phase]} ${p.done}/${p.total}`, pct };
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
