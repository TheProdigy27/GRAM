import { Injectable, inject } from '@angular/core';
import { ThemeController } from '@gram/theme';
import { PrefsStore } from '../state/prefs.store';
import { HarnessApi } from './harness-api.service';

/**
 * Thème de GRAM Harness (menu de l'en-tête) : celui du système, ou clair ou sombre, comme dans
 * GRAM, dont il reprend la logique (`ThemeController`).
 */
@Injectable({ providedIn: 'root' })
export class ThemeService extends ThemeController {
  constructor() {
    const api = inject(HarnessApi);
    super(inject(PrefsStore), (theme) => api.setWindowTheme(theme));
  }
}
