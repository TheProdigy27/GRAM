// Mise en forme française des valeurs affichées (virgule décimale, espace insécable des
// milliers), alignée sur `gram-core::fmt` : Go = Gio, comme le Gestionnaire des tâches.

const GIB = 2 ** 30;

const oneDecimal = new Intl.NumberFormat('fr-FR', {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
});
const integer = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 0 });
const upToOneDecimal = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 1 });
const compact = new Intl.NumberFormat('fr-FR', { notation: 'compact' });
const relative = new Intl.RelativeTimeFormat('fr', { numeric: 'auto' });

/** Octets → « 15,9 » (Gio, sans unité). */
export function gbValue(bytes: number): string {
  return oneDecimal.format(bytes / GIB);
}

/** Octets → « 15,9 Go ». */
export function gb(bytes: number): string {
  return `${gbValue(bytes)} Go`;
}

/** Taille arrondie (RAM, VRAM, disques) → « 16 Go », « 1,8 To ». */
export function roundSize(bytes: number): string {
  const gib = bytes / GIB;
  return gib >= 1024 ? `${upToOneDecimal.format(gib / 1024)} To` : `${integer.format(gib)} Go`;
}

/** Vitesse → « ~38 », « ~5,6 » sous 10 tok/s, « — » si nulle (même règle que le backend). */
export function tokS(t: number): string {
  if (!Number.isFinite(t) || t <= 0) return '—';
  return t < 10 ? `~${oneDecimal.format(t)}` : `~${integer.format(Math.round(t))}`;
}

/** Contexte en tokens → « 32k » (multiples de 1 024), « 512 » en dessous, « — » si nul. */
export function ctxK(tokens: number): string {
  if (tokens <= 0) return '—';
  return tokens >= 1024 ? `${Math.floor(tokens / 1024)}k` : String(tokens);
}

/** Nombre de paramètres → « 8,2 Md », « 70 Md », « 600 M ». */
export function paramCount(n: number): string {
  if (n >= 1e9) {
    const b = n / 1e9;
    return `${b < 10 ? upToOneDecimal.format(b) : integer.format(Math.round(b))} Md`;
  }
  return `${integer.format(Math.round(n / 1e6))} M`;
}

/** Paramètres d'un modèle, avec les paramètres actifs d'un MoE. */
export function params(total: number, active: number | null): string {
  const t = paramCount(total);
  return active && active < total ? `${t} (${paramCount(active)} actifs)` : t;
}

/** Entier avec espace des milliers → « 1 165 ». */
export function int(n: number): string {
  return integer.format(n);
}

/** Compteur → « 1,2 M », « 35 k », « 950 ». */
export function count(n: number): string {
  return compact.format(n);
}

/** Bande passante → « ≈960 Go/s ». */
export function bandwidth(gbs: number): string {
  return `≈${integer.format(Math.round(gbs))} Go/s`;
}

/** Nombre décimal court → « 0,45 », « 3,2 ». */
export function decimal(x: number, digits = 2): string {
  return new Intl.NumberFormat('fr-FR', { maximumFractionDigits: digits }).format(x);
}

/** Date ISO → « il y a 2 heures », « hier », « il y a 3 mois ». */
export function relativeTime(iso: string, now: number = Date.now()): string {
  const seconds = (new Date(iso).getTime() - now) / 1000;
  if (!Number.isFinite(seconds)) return '';
  const abs = Math.abs(seconds);
  if (abs < 60) return 'à l’instant';
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ['year', 365 * 86400],
    ['month', 30 * 86400],
    ['day', 86400],
    ['hour', 3600],
    ['minute', 60],
  ];
  const [unit, size] = units.find(([, s]) => abs >= s) ?? ['minute', 60];
  return relative.format(Math.round(seconds / size), unit);
}

// ── Curseur de contexte : 2k → 128k, une puissance de deux par cran ─────────────────────────

export const CTX_STEPS = [2048, 4096, 8192, 16384, 32768, 65536, 131072] as const;

/** Cran le plus proche d'un contexte (échelle logarithmique). */
export function ctxToStep(ctx: number): number {
  const i = Math.round(Math.log2(Math.max(ctx, 1) / CTX_STEPS[0]));
  return Math.min(Math.max(i, 0), CTX_STEPS.length - 1);
}

export function stepToCtx(step: number): number {
  return CTX_STEPS[Math.min(Math.max(Math.round(step), 0), CTX_STEPS.length - 1)];
}

/** Go saisis → octets (Gio, comme l'affichage). */
export function gbToBytes(gbs: number): number {
  return Math.round(gbs * GIB);
}
