// Images jointes à un message : formats, taille et nombre acceptés (comme
// `app/harness-tauri/src/images.rs`, qui vérifie de nouveau), lecture en adresse `data:` pour
// les vignettes, puis en base64 pour l'envoi.

import { fileSize } from '@gram/format';

export const IMAGE_TYPES: readonly string[] = ['image/png', 'image/jpeg', 'image/webp'];
/** `accept` du sélecteur de fichiers. */
export const IMAGE_ACCEPT = IMAGE_TYPES.join(',');
/** 10 Mo (1 Mo = 2^20 octets, comme les tailles de GRAM). */
export const MAX_IMAGE_BYTES = 10 * 2 ** 20;
export const MAX_IMAGES = 4;

/** Image jointe au message en cours de saisie. */
export interface Attachment {
  /** Clé de la vignette. */
  id: number;
  name: string;
  size: number;
  /** Adresse `data:` : vignette, puis contenu envoyé. */
  url: string;
}

/** Raison du refus d'un fichier, ou `null` s'il est accepté. */
export function imageProblem(file: Pick<File, 'name' | 'type' | 'size'>): string | null {
  if (!IMAGE_TYPES.includes(file.type)) {
    return `${file.name} : format non pris en charge (PNG, JPEG ou WebP)`;
  }
  if (file.size > MAX_IMAGE_BYTES) {
    return `${file.name} : ${fileSize(file.size)}, 10 Mo au plus`;
  }
  return null;
}

/** Contenu en base64 d'une adresse `data:` (ce qu'attend le backend). */
export function base64Of(url: string): string {
  return url.slice(url.indexOf(',') + 1);
}

/** Lit un fichier en adresse `data:`. */
export function readDataUrl(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error ?? new Error('lecture impossible'));
    reader.readAsDataURL(file);
  });
}

/** Fichiers glissés ou collés. */
export function filesOf(data: DataTransfer | null | undefined): File[] {
  return data ? Array.from(data.files) : [];
}

/** Un glisser apporte-t-il des fichiers (et pas du texte, un lien…) ? */
export function draggingFiles(event: DragEvent): boolean {
  return Array.from(event.dataTransfer?.types ?? []).includes('Files');
}
