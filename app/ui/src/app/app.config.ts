import { OVERLAY_DEFAULT_CONFIG } from '@angular/cdk/overlay';
import { registerLocaleData } from '@angular/common';
import localeFr from '@angular/common/locales/fr';
import {
  ApplicationConfig,
  LOCALE_ID,
  inject,
  provideAppInitializer,
  provideBrowserGlobalErrorListeners,
} from '@angular/core';
import { MatIconRegistry } from '@angular/material/icon';

registerLocaleData(localeFr);

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    { provide: LOCALE_ID, useValue: 'fr' },
    // Superpositions CDK sans l'API popover (couche supérieure du navigateur) : le WebKit de
    // Safari 15 ne la connaît pas, et WebView2 se comporte ainsi comme lui. Les toasts
    // (z-index 1100) restent alors au-dessus des dialogues.
    { provide: OVERLAY_DEFAULT_CONFIG, useValue: { usePopover: false } },
    // Icônes Material Symbols hébergées localement (paquet npm `material-symbols`).
    provideAppInitializer(() => {
      inject(MatIconRegistry).setDefaultFontSetClass('material-symbols-outlined');
    }),
  ],
};
