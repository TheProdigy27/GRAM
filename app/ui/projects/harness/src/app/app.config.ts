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
import { ScrollbarsService } from '@gram/scrollbars';
import { UPDATED_APP } from '@gram/updates';

registerLocaleData(localeFr);

/** Mêmes réglages que GRAM (`src/app/app.config.ts`). */
export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    { provide: LOCALE_ID, useValue: 'fr' },
    // Barres de défilement visibles pendant le défilement seulement.
    provideAppInitializer(() => void inject(ScrollbarsService)),
    // Superpositions CDK sans l'API popover, que le WebKit de Safari 15 ne connaît pas.
    { provide: OVERLAY_DEFAULT_CONFIG, useValue: { usePopover: false } },
    // Textes du dialogue de mise à jour, partagé avec GRAM.
    {
      provide: UPDATED_APP,
      useValue: { name: 'GRAM Harness', kept: 'vos conversations et paramètres' },
    },
  ],
};
