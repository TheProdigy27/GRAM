// Ajout à GRAM Harness : textes de la progression, partagés par la fiche et la barre d'état.

import type { InstallPhase } from '../core/generated/InstallPhase';
import type { InstallProgress } from '../core/generated/InstallProgress';
import { fileSize, gbValue } from './format';

const GIB = 2 ** 30;

/** Quantification d'un modèle du catalogue (dépôt canonique et étiquette). */
export interface VariantRef {
  repoId: string;
  label: string;
}

/** Même quantification (les étiquettes sans tenir compte de la casse). */
export function sameVariant(
  a: VariantRef | null | undefined,
  b: VariantRef | null | undefined,
): boolean {
  return !!a && !!b && a.repoId === b.repoId && a.label.toLowerCase() === b.label.toLowerCase();
}

/** Étapes sans avancement mesurable. */
const STEP_LABELS: Record<Exclude<InstallPhase, 'downloading'>, string> = {
  preparing: 'Préparation…',
  verifying: 'Vérification…',
  creating: 'Réglage du contexte…',
};

/** Étape en bref, pour la barre d'état. */
const SHORT_STEPS: Record<Exclude<InstallPhase, 'downloading'>, string> = {
  preparing: 'préparation',
  verifying: 'vérification',
  creating: 'réglage du contexte',
};

/** Avancement du téléchargement en pour cent ; `null` hors téléchargement ou taille inconnue. */
export function installPercent(p: InstallProgress): number | null {
  if (p.phase !== 'downloading' || p.total <= 0) return null;
  return Math.min(100, Math.max(0, (p.completed / p.total) * 100));
}

/** « 2,1 / 4,9 Go », « 120 Mo / 800 Mo » sous 1 Go. */
function amounts(completed: number, total: number): string {
  return total >= GIB
    ? `${gbValue(completed)} / ${gbValue(total)} Go`
    : `${fileSize(completed)} / ${fileSize(total)}`;
}

/**
 * Étape en cours : « Téléchargement · 2,1 / 4,9 Go · 38 Mo/s », puis « Vérification… » et
 * « Réglage du contexte… ».
 */
export function installStep(p: InstallProgress): string {
  if (p.phase !== 'downloading') return STEP_LABELS[p.phase];
  const parts = ['Téléchargement'];
  if (p.total > 0) parts.push(amounts(p.completed, p.total));
  if (p.bytesPerS !== null && p.bytesPerS > 0) parts.push(`${fileSize(p.bytesPerS)}/s`);
  return parts.join(' · ');
}

/** Barre d'état : « Ajout de Qwen3-8B Q4_K_M · 38 % », « … · vérification ». */
export function installStatus(p: InstallProgress): string {
  const percent = installPercent(p);
  const step =
    p.phase === 'downloading'
      ? percent === null
        ? 'téléchargement'
        : `${Math.floor(percent)} %`
      : SHORT_STEPS[p.phase];
  return `Ajout de ${p.displayName} ${p.label} · ${step}`;
}
