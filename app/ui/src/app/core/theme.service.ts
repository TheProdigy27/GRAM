import { Injectable, inject } from '@angular/core';
import { DisplayStore } from '../state/display.store';
import { ThemeController } from './theme';
import { TauriService } from './tauri.service';

export { DARK_SCHEME_QUERY, type Theme } from './theme';

/**
 * Thème de GRAM (bouton de l'en-tête, palette), gardé dans les préférences d'affichage. La
 * logique, commune avec GRAM Harness, est dans `ThemeController` (`./theme`).
 */
@Injectable({ providedIn: 'root' })
export class ThemeService extends ThemeController {
  constructor() {
    const tauri = inject(TauriService);
    super(inject(DisplayStore), (theme) => tauri.setWindowTheme(theme));
  }
}
