// Méthodes que `marked` utilise et que le WebKit de Safari n'a qu'à partir de 15.4 (macOS 10.15
// peut avoir un WebKit 15.0 à 15.3). Importé en premier par `main.ts`.

if (!Array.prototype.at) {
  Object.defineProperty(Array.prototype, 'at', {
    configurable: true,
    writable: true,
    value: function at<T>(this: T[], index: number): T | undefined {
      const i = Math.trunc(index) || 0;
      return this[i < 0 ? this.length + i : i];
    },
  });
}

if (!String.prototype.at) {
  Object.defineProperty(String.prototype, 'at', {
    configurable: true,
    writable: true,
    value: function at(this: string, index: number): string | undefined {
      const i = Math.trunc(index) || 0;
      return this.charAt(i < 0 ? this.length + i : i) || undefined;
    },
  });
}

if (!Object.hasOwn) {
  Object.defineProperty(Object, 'hasOwn', {
    configurable: true,
    writable: true,
    value: (object: object, key: PropertyKey) => Object.prototype.hasOwnProperty.call(object, key),
  });
}

export {};
