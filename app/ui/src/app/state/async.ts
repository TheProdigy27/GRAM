// Petits outils partagés par les stores.

/**
 * Attend `ms` millisecondes (debounce dans un loader de `resource`). Si la requête est
 * remplacée entre-temps, `signal` est annulé et la promesse rejetée : le loader s'arrête sans
 * appeler le backend.
 */
export function delay(ms: number, signal: AbortSignal): Promise<void> {
  if (ms <= 0 || signal.aborted) {
    return signal.aborted ? Promise.reject(signal.reason) : Promise.resolve();
  }
  return new Promise((resolve, reject) => {
    const timer = setTimeout(resolve, ms);
    signal.addEventListener(
      'abort',
      () => {
        clearTimeout(timer);
        reject(signal.reason);
      },
      { once: true },
    );
  });
}

/** Égalité structurelle de requêtes (objets construits toujours dans le même ordre). */
export function sameJson<T>(a: T, b: T): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
