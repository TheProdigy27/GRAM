import { MAX_IMAGE_BYTES, base64Of, imageProblem, readDataUrl } from './attachments';

describe('Images jointes', () => {
  it('PNG, JPEG ou WebP de 10 Mo au plus', () => {
    for (const type of ['image/png', 'image/jpeg', 'image/webp']) {
      expect(imageProblem({ name: 'a', type, size: MAX_IMAGE_BYTES })).toBeNull();
    }
    expect(imageProblem({ name: 'anim.gif', type: 'image/gif', size: 10 })).toBe(
      'anim.gif : format non pris en charge (PNG, JPEG ou WebP)',
    );
    expect(imageProblem({ name: 'notes.txt', type: '', size: 10 })).toBe(
      'notes.txt : format non pris en charge (PNG, JPEG ou WebP)',
    );
    expect(imageProblem({ name: 'scan.png', type: 'image/png', size: 14.2 * 2 ** 20 })).toBe(
      'scan.png : 14 Mo, 10 Mo au plus',
    );
  });

  it('lecture en adresse data:, puis base64 envoyé', async () => {
    const url = await readDataUrl(new Blob(['abc'], { type: 'image/png' }));
    expect(url).toBe('data:image/png;base64,YWJj');
    expect(base64Of(url)).toBe('YWJj');
  });
});
