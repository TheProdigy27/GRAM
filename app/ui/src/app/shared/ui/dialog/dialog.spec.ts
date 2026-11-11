import { DIALOG_DATA, DialogRef } from '@angular/cdk/dialog';
import { ApplicationRef, Component, inject } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom } from 'rxjs';
import { byTestId, text } from '../../../../testing/dom';
import { Button } from '../button/button';
import { DialogService, GDialog } from './dialog';

@Component({
  imports: [GDialog, Button],
  template: `<g-dialog heading="Paramètres" width="520px">
    <p>Contenu : {{ data.name }}</p>
    <button gDialogHeader type="button">Chercher</button>
    <div gDialogActions>
      <button gButton data-testid="cancel" (click)="ref.close('annulé')">Annuler</button>
    </div>
  </g-dialog>`,
})
class Settings {
  readonly data = inject<{ name: string }>(DIALOG_DATA);
  readonly ref = inject<DialogRef<string>>(DialogRef);
}

describe('DialogService et g-dialog', () => {
  const stable = () => TestBed.inject(ApplicationRef).whenStable();

  async function open() {
    const ref = TestBed.inject(DialogService).open<string>(Settings, { data: { name: 'GRAM' } });
    await stable();
    const container = document.querySelector<HTMLElement>('[role="dialog"]')!;
    return { ref, container };
  }

  afterEach(() => TestBed.inject(DialogService)['dialog'].closeAll());

  it('dialogue modal titré, focus sur le titre, voile', async () => {
    const { container } = await open();
    expect(container.getAttribute('aria-modal')).toBe('true');
    const title = container.querySelector('h2')!;
    expect(title.textContent?.trim()).toBe('Paramètres');
    expect(container.getAttribute('aria-labelledby')).toBe(title.id);
    expect(document.activeElement).toBe(title);
    expect(document.querySelector('.cdk-overlay-backdrop')?.classList).toContain('g-backdrop');
    expect(text(container.querySelector('.g-dialog__body'))).toBe('Contenu : GRAM');
    expect(container.querySelector('.g-dialog__header [gDialogHeader]')).not.toBeNull();
    expect(container.querySelector('.g-dialog__actions [data-testid="cancel"]')).not.toBeNull();
    expect((container.querySelector('g-dialog') as HTMLElement).style.width).toBe('520px');
  });

  it('bouton Fermer, actions et Échap ferment le dialogue', async () => {
    let { ref, container } = await open();
    let closed = firstValueFrom(ref.closed);
    byTestId('dialog-close', container).click();
    expect(await closed).toBeUndefined();

    ({ ref, container } = await open());
    closed = firstValueFrom(ref.closed);
    byTestId('cancel', container).click();
    expect(await closed).toBe('annulé');

    ({ ref, container } = await open());
    closed = firstValueFrom(ref.closed);
    document.body.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', keyCode: 27, bubbles: true }),
    );
    expect(await closed).toBeUndefined();
    await stable();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
});
