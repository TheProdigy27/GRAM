// Modèles installés dans Ollama, pour la liste « Modèles installés » : ceux ajoutés à GRAM
// Harness (modèle dérivé et base en une ligne), puis les autres.

import type { HarnessEntry } from '../core/generated/HarnessEntry';
import type { OllamaModel } from '../core/generated/OllamaModel';

/** Même modèle d'Ollama : casse et espaces autour ignorés (comme `gram_link::same_model`). */
export function sameOllamaName(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

/** Un modèle ajouté à GRAM Harness et présent dans Ollama. */
export interface HarnessRow {
  entry: HarnessEntry;
  /** Taille sur le disque d'après Ollama (le modèle dérivé partage le fichier de sa base). */
  size: number;
}

/** Un autre modèle d'Ollama, installé sans GRAM Harness. */
export interface OtherRow {
  model: OllamaModel;
  /** « Qwen3-1.7B Q4_K_M » pour `hf.co/unsloth/Qwen3-1.7B-GGUF:Q4_K_M`, sinon le nom. */
  title: string;
}

export interface InstalledModels {
  harness: HarnessRow[];
  others: OtherRow[];
  /** Nombre de lignes : un modèle de GRAM Harness et sa base comptent pour un. */
  count: number;
  /** Total des tailles affichées. */
  size: number;
}

const collator = new Intl.Collator('fr', { sensitivity: 'base', numeric: true });

/** Nom lisible d'un modèle venu de Hugging Face : dépôt sans propriétaire ni « -GGUF ». */
export function otherTitle(m: OllamaModel): string {
  if (!m.repoId) return m.name;
  const repo = m.repoId.slice(m.repoId.lastIndexOf('/') + 1).replace(/[-_.]gguf$/i, '');
  return m.quant ? `${repo} ${m.quant}` : repo;
}

/**
 * Lignes de la liste. Une entrée du manifeste n'est gardée que si son modèle dérivé est dans
 * Ollama ; sa base, si elle y est, ne fait pas de ligne à part (« Retirer » propose de la
 * supprimer quand c'est permis, sinon elle réapparaît parmi les autres une fois le modèle
 * retiré). Tri : GRAM Harness par nom lisible puis quantification, les autres par nom.
 */
export function installedModels(
  ollama: readonly OllamaModel[],
  entries: readonly HarnessEntry[],
): InstalledModels {
  const find = (name: string) => ollama.find((m) => sameOllamaName(m.name, name));
  const hidden = new Set<OllamaModel>();
  const harness: HarnessRow[] = [];
  for (const entry of entries) {
    const derived = find(entry.model.ollamaName);
    if (!derived) continue;
    hidden.add(derived);
    const base = find(entry.model.baseName);
    if (base) hidden.add(base);
    harness.push({ entry, size: derived.size });
  }
  harness.sort(
    (a, b) =>
      collator.compare(a.entry.model.displayName, b.entry.model.displayName) ||
      collator.compare(a.entry.model.quant, b.entry.model.quant),
  );
  const others = ollama
    .filter((m) => !hidden.has(m))
    .map((model) => ({ model, title: otherTitle(model) }))
    .sort((a, b) => collator.compare(a.model.name, b.model.name));
  const size =
    harness.reduce((sum, r) => sum + r.size, 0) + others.reduce((sum, r) => sum + r.model.size, 0);
  return { harness, others, count: harness.length + others.length, size };
}

/** « 1 modèle », « 4 modèles ». */
export function modelCount(n: number): string {
  return `${n} modèle${n > 1 ? 's' : ''}`;
}
