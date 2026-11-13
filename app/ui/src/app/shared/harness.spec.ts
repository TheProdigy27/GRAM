import { INSTALL_PROGRESS } from '../../testing/fixtures';
import type { InstallProgress } from '../core/generated/InstallProgress';
import { installPercent, installStatus, installStep, sameVariant } from './harness';

const MIB = 2 ** 20;
const at = (o: Partial<InstallProgress>): InstallProgress => ({ ...INSTALL_PROGRESS, ...o });

describe('ajout à GRAM Harness : textes', () => {
  it('même quantification : dépôt exact, étiquette sans tenir compte de la casse', () => {
    const q = { repoId: 'Qwen/Qwen3-8B-GGUF', label: 'Q4_K_M' };
    expect(sameVariant(q, { repoId: 'Qwen/Qwen3-8B-GGUF', label: 'q4_k_m' })).toBe(true);
    expect(sameVariant(q, { repoId: 'Qwen/Qwen3-8B-GGUF', label: 'Q8_0' })).toBe(false);
    expect(sameVariant(q, { repoId: 'bartowski/Qwen3-8B-GGUF', label: 'Q4_K_M' })).toBe(false);
    expect(sameVariant(q, null)).toBe(false);
    expect(sameVariant(undefined, q)).toBe(false);
  });

  it('téléchargement : quantités, débit et pour cent', () => {
    expect(installStep(INSTALL_PROGRESS)).toBe('Téléchargement · 3,1 / 8,1 Go · 38 Mo/s');
    expect(installPercent(INSTALL_PROGRESS)).toBeCloseTo(38.27, 2);
    expect(installStatus(INSTALL_PROGRESS)).toBe('Ajout de Qwen3-8B Q8_0 · 38 %');
    // Sous 1 Go, en Mo ; débit pas encore connu.
    expect(installStep(at({ completed: 120 * MIB, total: 800 * MIB, bytesPerS: null }))).toBe(
      'Téléchargement · 120 Mo / 800 Mo',
    );
    // Taille pas encore annoncée.
    const unknown = at({ completed: 0, total: 0, bytesPerS: null });
    expect(installStep(unknown)).toBe('Téléchargement');
    expect(installPercent(unknown)).toBeNull();
    expect(installStatus(unknown)).toBe('Ajout de Qwen3-8B Q8_0 · téléchargement');
  });

  it('étapes sans avancement mesurable', () => {
    expect(installStep(at({ phase: 'preparing' }))).toBe('Préparation…');
    expect(installStep(at({ phase: 'verifying' }))).toBe('Vérification…');
    expect(installStep(at({ phase: 'creating' }))).toBe('Réglage du contexte…');
    expect(installPercent(at({ phase: 'verifying' }))).toBeNull();
    expect(installStatus(at({ phase: 'creating' }))).toBe(
      'Ajout de Qwen3-8B Q8_0 · réglage du contexte',
    );
  });
});
