import { Component, input, output } from '@angular/core';
import type { FitLevel } from '../../core/generated/FitLevel';
import type { VariantDetail } from '../../core/generated/VariantDetail';
import { decimal } from '../../shared/format';
import { FIT_LABELS } from '../../shared/labels';
import { CtxPipe, GbPipe, TokSPipe } from '../../shared/pipes';
import { scoreTooltip } from '../../shared/score';
import { Icon } from '../../shared/ui/icon/icon';
import type { IconName } from '../../shared/ui/icon/icons';
import { ModeBadge } from '../../shared/ui/mode-badge/mode-badge';
import { Tag } from '../../shared/ui/tag/tag';
import { Tooltip } from '../../shared/ui/tooltip/tooltip';

/** Adéquation mémoire d'une quantification : icône, ton et libellé. */
export interface FitMark {
  icon: IconName;
  tone: 'ok' | 'warn' | 'bad';
  label: string;
}

const FIT_ICONS: Record<FitLevel, IconName> = {
  perfect: 'check',
  good: 'circle',
  marginal: 'warning',
  tooTight: 'minus-circle',
};

export function fitMark(v: VariantDetail): FitMark {
  if (v.estimate.mode.kind === 'tooBig')
    return { icon: 'close', tone: 'bad', label: 'Ne tient pas' };
  const fit = v.estimate.fit;
  const tone = fit === 'perfect' || fit === 'good' ? 'ok' : fit === 'marginal' ? 'warn' : 'bad';
  return { icon: FIT_ICONS[fit], tone, label: FIT_LABELS[fit] };
}

/**
 * Vue tableau des quantifications de la fiche (à la place du graphique) : adéquation mémoire,
 * meilleure, installée, taille, bits par poids, mode, tok/s, contexte et score. Le nom de chaque
 * quantification est un bouton qui la choisit ; un clic sur la ligne aussi.
 */
@Component({
  selector: 'app-quant-table',
  imports: [Icon, ModeBadge, Tag, Tooltip, CtxPipe, GbPipe, TokSPipe],
  templateUrl: './quant-table.html',
  styleUrl: './quant-table.scss',
})
export class QuantTable {
  readonly variants = input.required<readonly VariantDetail[]>();
  readonly best = input<number | null>(null);
  readonly selected = input<string | null>(null);
  /** Titre de la section qui nomme le tableau. */
  readonly labelledBy = input('');
  readonly selectedChange = output<string>();

  protected readonly fitMark = fitMark;
  protected readonly scoreTooltip = scoreTooltip;
  protected readonly decimal = decimal;

  protected choose(event: Event, label: string): void {
    event.stopPropagation();
    this.selectedChange.emit(label);
  }
}
