/** Message lisible d'une erreur : le backend rejette `invoke` avec une chaîne en français. */
export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  return String(e);
}

/** Rejette toujours une `Error` dont le message est en français. */
export function rethrow(e: unknown): never {
  throw e instanceof Error ? e : new Error(errorMessage(e));
}
