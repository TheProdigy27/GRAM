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
import { ScrollbarsService } from './core/scrollbars.service';

registerLocaleData(localeFr);

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    { provide: LOCALE_ID, useValue: 'fr' },
    // Barres de défilement visibles pendant le défilement seulement (styles.scss).
    provideAppInitializer(() => void inject(ScrollbarsService)),
    // Superpositions CDK sans l'API popover (couche supérieure du navigateur) : le WebKit de
    // Safari 15 ne la connaît pas, et WebView2 se comporte ainsi comme lui. Les toasts
    // (z-index 1100) restent alors au-dessus des dialogues.
    { provide: OVERLAY_DEFAULT_CONFIG, useValue: { usePopover: false } },
  ],
};
