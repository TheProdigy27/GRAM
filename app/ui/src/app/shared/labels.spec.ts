import { quantName, speedWord } from './labels';

describe('speedWord', () => {
  it('seuils fixes, puis le seuil de lenteur du cas d’usage', () => {
    expect(speedWord(106, 10)).toBe('instantané');
    expect(speedWord(80, 10)).toBe('instantané');
    expect(speedWord(33, 10)).toBe('très fluide');
    expect(speedWord(15, 10)).toBe('fluide');
    expect(speedWord(12, 10)).toBe('correct');
    expect(speedWord(10, 10)).toBe('correct');
    expect(speedWord(9.9, 10)).toBe('lent');
  });
});

describe('quantName', () => {
  const repo = 'huihui-ai/Huihui-Qwen3.8-27B-abliterated-GGUF';

  it('libellé ordinaire : inchangé', () => {
    expect(quantName({ label: 'Q4_K_M', repoId: repo })).toBe('Q4_K_M');
    expect(quantName({ label: 'UD-Q4_K_XL', repoId: 'unsloth/Qwen3.5-9B-GGUF' })).toBe(
      'UD-Q4_K_XL',
    );
  });

  it('nom de fichier exotique : sans le nom du modèle', () => {
    expect(
      quantName({
        label: 'Huihui-Qwen3.8-27B-abliterated-Underdog-Saluki-Swift-1.5-IQ2',
        repoId: repo,
      }),
    ).toBe('Underdog-Saluki-Swift-1.5-IQ2');
    expect(
      quantName({ label: 'qwen_qwen3-8b.custom', repoId: 'bartowski/Qwen_Qwen3-8B-GGUF' }),
    ).toBe('custom');
  });

  it('le nom du modèle doit être suivi d’un séparateur', () => {
    expect(quantName({ label: 'Huihui-Qwen3.8-27B-abliterated', repoId: repo })).toBe(
      'Huihui-Qwen3.8-27B-abliterated',
    );
    expect(quantName({ label: 'Huihui-Qwen3.8-27B-abliteratedX', repoId: repo })).toBe(
      'Huihui-Qwen3.8-27B-abliteratedX',
    );
    expect(quantName({ label: 'model-custom', repoId: 'test/repo-GGUF' })).toBe('model-custom');
  });
});
