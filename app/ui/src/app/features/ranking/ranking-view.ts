import { Component, computed, inject } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatExpansionModule } from '@angular/material/expansion';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { tokS } from '../../shared/format';
import { runtimeLabel } from '../../shared/labels';
import { DetailStore } from '../../state/detail.store';
import { RankingStore } from '../../state/ranking.store';
import { RankingTable } from './ranking-table';

/**
 * Zone principale : remarques du runtime, modèles exécutables, section repliable
 * « Exécutable mais lent », modèles trop gros (sur demande).
 */
@Component({
  selector: 'app-ranking-view',
  imports: [MatButtonModule, MatExpansionModule, MatIconModule, MatProgressBarModule, RankingTable],
  templateUrl: './ranking-view.html',
  styleUrl: './ranking-view.scss',
})
export class RankingView {
  protected readonly ranking = inject(RankingStore);
  protected readonly detail = inject(DetailStore);

  protected readonly response = this.ranking.response;

  protected readonly runtimeName = computed(() => runtimeLabel(this.ranking.effectiveRuntime()));

  protected readonly slowThreshold = computed(() => {
    const r = this.response();
    return r ? tokS(r.slowTokS).replace('~', '') : '';
  });

  protected readonly moreItems = computed(() => {
    const r = this.response();
    return r ? r.totalItems - r.items.length : 0;
  });

  protected readonly moreSlow = computed(() => {
    const r = this.response();
    return r ? r.totalSlow - r.slow.length : 0;
  });

  protected readonly moreTooBig = computed(() => {
    const r = this.response();
    return r && r.tooBig.length > 0 ? r.tooBigCount - r.tooBig.length : 0;
  });

  protected open(repoId: string): void {
    this.detail.open(repoId);
  }
}
