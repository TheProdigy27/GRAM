import { makeDetail } from '../../../testing/fixtures';
import { nameTooltip } from './quant-table';

describe('nameTooltip', () => {
  const [q4] = makeDetail().variants;

  it('nom court sous la taille maximale : pas d’infobulle', () => {
    expect(nameTooltip(q4)).toBe('');
  });

  it('nom tronqué : en entier, puis l’avertissement de taille', () => {
    const exotic = {
      ...q4,
      variant: { ...q4.variant, label: 'Qwen3-8B-Underdog-Saluki-Swift-1.5-IQ2' },
      withinSize: false,
    };
    expect(nameTooltip(exotic)).toBe(
      'Underdog-Saluki-Swift-1.5-IQ2\nAu-dessus de la taille maximale demandée',
    );
  });
});
