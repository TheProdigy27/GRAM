import type { Calibration } from '../../core/generated/Calibration';
import type { EstimateParams } from '../../core/generated/EstimateParams';
import type { ScoreWeights } from '../../core/generated/ScoreWeights';
import type { Settings } from '../../core/generated/Settings';
import type { UseCase } from '../../core/generated/UseCase';
import { SCORE_AXES } from '../../shared/score';
import type { IconName } from '../../shared/ui/icon/icons';

const GIB = 2 ** 30;

export type SectionId = 'estimate' | 'useCases' | 'catalog' | 'updates' | 'about';

export interface SectionDef {
  id: SectionId;
  label: string;
  icon: IconName;
  description: string;
}

/** Sections des paramètres, dans l'ordre de la navigation. */
export const SECTIONS: readonly SectionDef[] = [
  {
    id: 'estimate',
    label: 'Estimation',
    icon: 'gauge',
    description:
      'Mémoire réellement utilisable et vitesse estimée, à ±30 % près. Les efficacités et le temps fixe par token se recalibrent d’après des mesures réelles avec Ollama.',
  },
  {
    id: 'useCases',
    label: 'Cas d’usage',
    icon: 'grid',
    description:
      'Ce que chaque usage privilégie dans le score, son contexte par défaut et sa vitesse minimale.',
  },
  {
    id: 'catalog',
    label: 'Catalogue',
    icon: 'database',
    description: 'Synchronisation avec Hugging Face, cache local et token.',
  },
  {
    id: 'updates',
    label: 'Mises à jour',
    icon: 'update',
    description: 'Nouvelles versions publiées sur GitHub.',
  },
  { id: 'about', label: 'À propos', icon: 'info', description: '' },
];

export type RowKind = 'number' | 'switch' | 'kvCache' | 'other';

/**
 * Un réglage : chemin dans `Settings` (`*` = cas d'usage modifié), libellé, aide et mots-clés
 * pour la recherche. Les réglages numériques indiquent leur unité et leur échelle d'affichage.
 */
export interface RowDef {
  id: string;
  section: SectionId;
  kind: RowKind;
  heading: string;
  help: string;
  keywords?: string;
  path?: string;
  testId?: string;
  /** Unité affichée dans le champ (« % », « Go »). */
  unit?: string;
  /** Valeur affichée = valeur enregistrée × `scale`. */
  scale?: number;
  /** Décimales affichées. */
  digits?: number;
  /** Valeur enregistrée arrondie à l'entier (octets, tokens, compteurs). */
  integer?: boolean;
}

export const ROWS: readonly RowDef[] = [
  {
    id: 'vramMargin',
    section: 'estimate',
    kind: 'number',
    path: 'estimate.vramMargin',
    testId: 'set-vram-margin',
    heading: 'Marge de sécurité VRAM',
    help: 'Part de la VRAM laissée à l’affichage du système et aux applications ouvertes.',
    keywords: 'carte graphique gpu réserve',
    unit: '%',
    scale: 100,
    digits: 1,
  },
  {
    id: 'ramShare',
    section: 'estimate',
    kind: 'number',
    path: 'estimate.ramShare',
    testId: 'set-ram-share',
    heading: 'Part de RAM utilisable',
    help: 'Part de la RAM totale qu’un modèle peut occuper, au plus la RAM libre.',
    keywords: 'mémoire vive',
    unit: '%',
    scale: 100,
    digits: 1,
  },
  {
    id: 'closeApps',
    section: 'estimate',
    kind: 'switch',
    path: 'estimate.closeApps',
    testId: 'set-close-apps',
    heading: 'Je ferme tout avant de lancer le modèle',
    help: 'Compte la RAM totale moins 4 Go et la VRAM entière, sans l’occupation actuelle.',
    keywords: 'applications mémoire',
  },
  {
    id: 'overhead',
    section: 'estimate',
    kind: 'number',
    path: 'estimate.overheadBytes',
    testId: 'set-overhead',
    heading: 'Surcoût du runtime',
    help: 'Contexte CUDA, Vulkan ou Metal et buffers de calcul.',
    keywords: 'mémoire vram',
    unit: 'Go',
    scale: 1 / GIB,
    digits: 2,
    integer: true,
  },
  {
    id: 'kvCache',
    section: 'estimate',
    kind: 'kvCache',
    path: 'estimate.kvCache',
    testId: 'set-kv-cache',
    heading: 'Type de cache KV',
    help: 'q8_0 rend le contexte deux fois plus léger, q4_0 quatre fois (qualité réduite). À régler aussi dans le runtime.',
    keywords: 'contexte quantification f16 q8_0 q4_0 ollama llama.cpp',
  },
  {
    id: 'gpuEfficiency',
    section: 'estimate',
    kind: 'number',
    path: 'estimate.gpuEfficiency',
    testId: 'set-gpu-efficiency',
    heading: 'Efficacité GPU',
    help: 'Part de la bande passante de la VRAM réellement exploitée.',
    keywords: 'vitesse calibration mesure recalibrée',
    digits: 2,
  },
  {
    id: 'ramEfficiency',
    section: 'estimate',
    kind: 'number',
    path: 'estimate.ramEfficiency',
    testId: 'set-ram-efficiency',
    heading: 'Efficacité RAM',
    help: 'Part de la bande passante de la RAM exploitée (CPU, hybride).',
    keywords: 'vitesse calibration mesure recalibrée mémoire vive',
    digits: 2,
  },
  {
    id: 'tokenOverhead',
    section: 'estimate',
    kind: 'number',
    path: 'estimate.tokenOverheadMs',
    testId: 'set-token-overhead',
    heading: 'Temps fixe par token',
    help: 'Lancement des calculs et synchronisations : compte surtout pour les MoE et les petits modèles.',
    keywords: 'vitesse calibration mesure recalibrée moe',
    unit: 'ms',
    digits: 1,
  },
  {
    id: 'minBpw',
    section: 'estimate',
    kind: 'number',
    path: 'estimate.minBpw',
    testId: 'set-min-bpw',
    heading: 'Bits par poids minimum',
    help: 'En dessous, une quantification n’est pas proposée.',
    keywords: 'quantification bpw',
    digits: 1,
  },
  {
    id: 'minCtx',
    section: 'estimate',
    kind: 'number',
    path: 'estimate.minCtx',
    testId: 'set-min-ctx',
    heading: 'Contexte minimal',
    help: 'Repli quand le contexte demandé ne tient pas en mémoire.',
    keywords: 'tokens',
    unit: 'tokens',
    digits: 0,
    integer: true,
  },
  {
    id: 'ctx',
    section: 'useCases',
    kind: 'other',
    path: 'useCases.*.ctx',
    heading: 'Contexte par défaut',
    help: 'Proposé à l’ouverture de ce cas d’usage.',
    keywords: 'tokens 8k 16k',
  },
  {
    id: 'slow',
    section: 'useCases',
    kind: 'other',
    path: 'useCases.*.slowTokS',
    heading: 'Lent sous',
    help: 'En dessous, un modèle passe dans « Exécutable mais lent ».',
    keywords: 'vitesse tok/s seuil lenteur',
  },
  {
    id: 'weights',
    section: 'useCases',
    kind: 'other',
    path: 'useCases.*.weights',
    heading: 'Poids du score',
    help: 'Seules les proportions comptent : la somme est libre.',
    keywords: 'qualité vitesse marge contexte score',
  },
  {
    id: 'minDownloads',
    section: 'catalog',
    kind: 'number',
    path: 'minDownloads',
    testId: 'set-min-downloads',
    heading: 'Téléchargements minimum',
    help: 'Un dépôt moins téléchargé est ignoré à la synchronisation, sauf s’il est en tendance.',
    keywords: 'hugging face popularité',
    digits: 0,
    integer: true,
  },
  {
    id: 'ttl',
    section: 'catalog',
    kind: 'number',
    path: 'catalogTtlHours',
    testId: 'set-ttl',
    heading: 'Durée de vie du catalogue',
    help: 'Au-delà, le lancement resynchronise le catalogue.',
    keywords: 'cache synchronisation heures',
    unit: 'h',
    digits: 0,
    integer: true,
  },
  {
    id: 'detailGroups',
    section: 'catalog',
    kind: 'number',
    path: 'detailGroups',
    testId: 'set-detail-groups',
    heading: 'Modèles détaillés à la synchronisation',
    help: 'Leurs tailles réelles sont chargées ; au-delà de 150, un token Hugging Face est conseillé.',
    keywords: 'quota api tailles',
    digits: 0,
    integer: true,
  },
  {
    id: 'pageSize',
    section: 'catalog',
    kind: 'number',
    path: 'pageSize',
    testId: 'set-page-size',
    heading: 'Lignes affichées',
    help: 'Lignes du classement avant « Afficher plus ».',
    keywords: 'classement pagination',
    digits: 0,
    integer: true,
  },
  {
    id: 'cache',
    section: 'catalog',
    kind: 'other',
    heading: 'Cache local',
    help: 'Le catalogue affiché reste utilisable ; la prochaine synchronisation repartira de zéro.',
    keywords: 'vider dossier disque',
  },
  {
    id: 'token',
    section: 'catalog',
    kind: 'other',
    heading: 'Token Hugging Face',
    help: 'Lecture seule suffit : il élargit les quotas de l’API. Rangé dans le trousseau du système, jamais affiché.',
    keywords: 'quota api clé',
  },
  {
    id: 'version',
    section: 'updates',
    kind: 'other',
    heading: 'Version installée',
    help: '',
    keywords: 'mise à jour',
  },
  {
    id: 'check',
    section: 'updates',
    kind: 'other',
    heading: 'Rechercher une mise à jour',
    help: 'GRAM cherche aussi quelques secondes après chaque lancement, sans rien afficher s’il n’y en a pas.',
    keywords: 'version release github installer',
  },
  {
    id: 'about',
    section: 'about',
    kind: 'other',
    heading: 'À propos de GRAM',
    help: '',
    keywords: 'version licence confidentialité données hugging face',
  },
];

/** Minuscules sans accents, pour comparer la recherche aux libellés. */
export function normalize(text: string): string {
  return text.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
}

/** Réglages trouvés par la recherche : chaque mot dans le libellé, l'aide ou les mots-clés. */
export function searchRows(query: string): Set<string> {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  const found = new Set<string>();
  for (const r of ROWS) {
    const haystack = normalize(`${r.heading} ${r.help} ${r.keywords ?? ''}`);
    if (words.every((w) => haystack.includes(w))) found.add(r.id);
  }
  return found;
}

// ── Valeurs par chemin ──────────────────────────────────────────────────────────────────────

/** Chemin concret : `*` remplacé par le cas d'usage modifié. */
export function rowPath(path: string, useCase: UseCase): string {
  return path.replace('*', useCase);
}

export function getPath(settings: Settings, path: string): unknown {
  let value: unknown = settings;
  for (const key of path.split('.')) value = (value as Record<string, unknown>)[key];
  return value;
}

/** Copie de `settings` avec `value` au chemin donné. */
export function setPath(settings: Settings, path: string, value: unknown): Settings {
  const set = (obj: Record<string, unknown>, keys: string[]): Record<string, unknown> => {
    const [key, ...rest] = keys;
    return {
      ...obj,
      [key]: rest.length ? set(obj[key] as Record<string, unknown>, rest) : value,
    };
  };
  return set(
    settings as unknown as Record<string, unknown>,
    path.split('.'),
  ) as unknown as Settings;
}

/** Deux nombres égaux aux arrondis près (f32 côté Rust : 0,1 revient en 0,10000000149…). */
function sameNumber(a: number, b: number): boolean {
  return Math.abs(a - b) <= 1e-6 * Math.max(1, Math.abs(a), Math.abs(b));
}

/** Valeurs égales, objets comparés champ à champ. */
export function sameValue(a: unknown, b: unknown): boolean {
  return countDifferences(a, b) === 0;
}

/** Nombre de valeurs (feuilles) qui diffèrent entre deux réglages. */
export function countDifferences(a: unknown, b: unknown): number {
  if (typeof a === 'number' && typeof b === 'number') return sameNumber(a, b) ? 0 : 1;
  if (a && b && typeof a === 'object' && typeof b === 'object') {
    const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
    let n = 0;
    for (const k of keys) {
      n += countDifferences((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]);
    }
    return n;
  }
  return a === b ? 0 : 1;
}

/** Réglages d'une section qui diffèrent des valeurs par défaut (repère de la navigation). */
export function sectionDifferences(
  section: SectionId,
  draft: Settings,
  defaults: Settings,
  skip: ReadonlySet<string> = new Set(),
): number {
  if (section === 'useCases') return countDifferences(draft.useCases, defaults.useCases);
  return ROWS.filter((r) => r.section === section && r.path && !skip.has(r.path)).reduce(
    (n, r) => n + countDifferences(getPath(draft, r.path!), getPath(defaults, r.path!)),
    0,
  );
}

// ── Calibration ─────────────────────────────────────────────────────────────────────────────

type CalibratedKey = 'gpuEfficiency' | 'ramEfficiency' | 'tokenOverheadMs';

/**
 * Réglages qui viennent des mesures : la calibration propose une valeur et l'estimation l'a
 * déjà (efficacités au centième près, temps fixe au dixième de ms). Renvoie leurs chemins.
 */
export function calibratedPaths(cal: Calibration | null, e: EstimateParams): Set<string> {
  const paths = new Set<string>();
  if (!cal || cal.used === 0) return paths;
  const check = (key: CalibratedKey, proposed: number | null, step: number) => {
    if (proposed !== null && Math.abs(e[key] - proposed) < step) paths.add(`estimate.${key}`);
  };
  check('gpuEfficiency', cal.gpuEfficiency, 0.005);
  check('ramEfficiency', cal.ramEfficiency, 0.005);
  check('tokenOverheadMs', cal.tokenOverheadMs, 0.05);
  return paths;
}

/** La calibration propose des valeurs que l'estimation n'a pas encore. */
export function calibrationPending(cal: Calibration | null, e: EstimateParams): boolean {
  if (!cal || cal.used === 0) return false;
  const proposed = [cal.gpuEfficiency, cal.ramEfficiency, cal.tokenOverheadMs].filter(
    (x) => x !== null,
  ).length;
  return proposed > calibratedPaths(cal, e).size;
}

/** Estimation avec les valeurs proposées par les mesures (celles qui existent). */
export function withCalibration(e: EstimateParams, cal: Calibration): EstimateParams {
  return {
    ...e,
    gpuEfficiency: cal.gpuEfficiency ?? e.gpuEfficiency,
    ramEfficiency: cal.ramEfficiency ?? e.ramEfficiency,
    tokenOverheadMs: cal.tokenOverheadMs ?? e.tokenOverheadMs,
  };
}

// ── Poids du score ──────────────────────────────────────────────────────────────────────────

export interface WeightSegment {
  key: keyof ScoreWeights;
  label: string;
  /** Part du total, en % arrondi. */
  percent: number;
  /** Largeur exacte, en %. */
  width: number;
  /** Texte dans le segment, selon la place : « Qualité 38 % », « 15 % » ou rien. */
  text: string;
}

export const WEIGHT_LABELS: Record<keyof ScoreWeights, string> = {
  quality: 'Qualité',
  speed: 'Vitesse',
  fit: 'Marge',
  context: 'Contexte',
};

/** Répartition des quatre poids (leur somme est libre : seules les proportions comptent). */
export function weightSegments(w: ScoreWeights): WeightSegment[] {
  const sum = SCORE_AXES.reduce((s, a) => s + Math.max(0, w[a.key]), 0);
  return SCORE_AXES.map(({ key }) => {
    const width = sum > 0 ? (100 * Math.max(0, w[key])) / sum : 0;
    const percent = Math.round(width);
    const label = WEIGHT_LABELS[key];
    const text = width >= 20 ? `${label} ${percent} %` : width >= 9 ? `${percent} %` : '';
    return { key, label, percent, width, text };
  });
}

/** « Répartition : qualité 38 %, vitesse 38 %, marge 15 %, contexte 9 % ». */
export function weightsLabel(segments: WeightSegment[]): string {
  return `Répartition : ${segments.map((s) => `${s.label.toLowerCase()} ${s.percent} %`).join(', ')}`;
}
