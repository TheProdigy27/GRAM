import { Component, input, output } from '@angular/core';
import type { FitLevel } from '../../core/generated/FitLevel';
import type { VariantDetail } from '../../core/generated/VariantDetail';
import { decimal } from '../../shared/format';
import { GLOSSARY } from '../../shared/glossary';
import { FIT_LABELS, quantName } from '../../shared/labels';
import { CtxPipe, GbPipe, QuantNamePipe, TokSPipe } from '../../shared/pipes';
import { scoreTooltip } from '../../shared/score';
import { Icon } from '../../shared/ui/icon/icon';
import type { IconName } from '../../shared/ui/icon/icons';
import { InfoTip } from '../../shared/ui/info-tip/info-tip';
import { ModeBadge } from '../../shared/ui/mode-badge/mode-badge';
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

/**
 * Caractères au-delà desquels le nom peut être tronqué dans une fiche étroite (la colonne Quant
 * prend la place qui reste) : 11 suffisent aux libellés ordinaires, jusqu'à « UD-Q2_K_XXS ».
 */
const NAME_FITS_CHARS = 11;

/** Infobulle du nom : le nom entier s'il risque d'être tronqué, puis l'avertissement de taille. */
export function nameTooltip(v: VariantDetail): string {
  const name = quantName(v.variant);
  return [
    ...(name.length > NAME_FITS_CHARS ? [name] : []),
    ...(v.withinSize ? [] : ['Au-dessus de la taille maximale demandée']),
  ].join('\n');
}

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
  imports: [Icon, InfoTip, ModeBadge, Tooltip, CtxPipe, GbPipe, QuantNamePipe, TokSPipe],
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
  protected readonly nameTooltip = nameTooltip;
  protected readonly scoreTooltip = scoreTooltip;
  protected readonly decimal = decimal;
  protected readonly glossary = GLOSSARY;

  protected choose(event: Event, label: string): void {
    event.stopPropagation();
    this.selectedChange.emit(label);
  }
}
