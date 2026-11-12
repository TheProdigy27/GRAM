import { type DialogConfig, DialogRef } from '@angular/cdk/dialog';
import { createGlobalPositionStrategy } from '@angular/cdk/overlay';
import {
  Component,
  ElementRef,
  Injector,
  afterNextRender,
  computed,
  inject,
  resource,
  signal,
  viewChild,
} from '@angular/core';
import { AppCommands } from '../../app-commands';
import type { RankRequest } from '../../core/generated/RankRequest';
import type { RunMode } from '../../core/generated/RunMode';
import { ShortcutsService } from '../../core/shortcuts.service';
import { TauriService } from '../../core/tauri.service';
import { ThemeService } from '../../core/theme.service';
import { CTX_STEPS, ctxK, fileSize, int, tokS } from '../../shared/format';
import { MODE_LABELS, USE_CASES, USE_CASE_ICONS } from '../../shared/labels';
import { Icon } from '../../shared/ui/icon/icon';
import type { IconName } from '../../shared/ui/icon/icons';
import { ModeBadge } from '../../shared/ui/mode-badge/mode-badge';
import { delay, sameJson } from '../../state/async';
import { CatalogStore } from '../../state/catalog.store';
import { DetailStore, type RecentModel } from '../../state/detail.store';
import { FILTER_DEBOUNCE_MS, RankingStore } from '../../state/ranking.store';
import {
  MAX_MODELS,
  type PaletteCommand,
  type PaletteModel,
  type PaletteOption,
  foundCount,
  foundModels,
  paletteGroups,
  searchWords,
} from './palette-model';

/**
 * Réglages du dialogue : en haut de la fenêtre, 680 px au plus, focus dans le champ, rendu à
 * `origin` (sinon à l'élément qui l'avait) à la fermeture.
 */
export function paletteConfig(
  injector: Injector,
  origin?: HTMLElement,
): DialogConfig<unknown, DialogRef<unknown, Palette>> {
  return {
    ariaLabel: 'Rechercher un modèle ou une commande',
    ariaLabelledBy: null,
    autoFocus: 'input',
    restoreFocus: origin ?? true,
    width: '680px',
    maxHeight: 'calc(100vh - 104px)',
    positionStrategy: createGlobalPositionStrategy(injector).top('72px').centerHorizontally(),
  };
}

/** Filtres du classement sans la recherche ni la pagination. */
type BaseRequest = Omit<RankRequest, 'search' | 'limit'>;

/** Modèles affichés et nombre trouvé (`null` sans saisie), pour la saisie `query`. */
interface Results {
  query: string;
  models: PaletteModel[];
  count: number | null;
}

/** Une option prête à afficher. */
interface OptionView {
  key: string;
  id: string;
  index: number;
  option: PaletteOption;
  label: string;
  hint: string;
  icon: IconName;
  shortcut: string;
  /** Modèle : rang, sous-ligne, mode, vitesse, score. */
  rank: string;
  sub: string;
  mode: RunMode | null;
  tokS: string;
  score: string;
  ariaLabel: string | null;
}

interface GroupView {
  id: string;
  label: string;
  options: OptionView[];
}

let nextId = 0;

function optionKey(o: PaletteOption): string {
  if (o.kind === 'filter') return 'filter';
  return o.kind === 'model' ? `model:${o.model.repoId}` : `command:${o.command.id}`;
}

/**
 * Palette Ctrl F : recherche de modèles (même règle et même rang que le classement, calculés
 * par le backend), commandes, et « Filtrer le classement » qui applique la saisie au classement
 * comme l'ancien champ de l'en-tête. Sans saisie : fiches ouvertes pendant la session et
 * commandes principales. Le focus reste dans le champ ; ↑ ↓ choisissent l'option active
 * (`aria-activedescendant`), Entrée l'ouvre, Ctrl Entrée filtre, Échap ferme.
 */
@Component({
  selector: 'app-palette',
  imports: [Icon, ModeBadge],
  templateUrl: './palette.html',
  styleUrl: './palette.scss',
  host: { class: 'palette' },
})
export class Palette {
  private readonly ref = inject(DialogRef);
  private readonly tauri = inject(TauriService);
  private readonly ranking = inject(RankingStore);
  private readonly detail = inject(DetailStore);
  private readonly catalog = inject(CatalogStore);
  private readonly commands = inject(AppCommands);
  private readonly theme = inject(ThemeService);
  private readonly shortcuts = inject(ShortcutsService);
  private readonly debounceMs = inject(FILTER_DEBOUNCE_MS);
  private readonly injector = inject(Injector);

  private readonly input = viewChild.required<ElementRef<HTMLInputElement>>('input');

  protected readonly uid = `palette-${nextId++}`;
  protected readonly listId = `${this.uid}-list`;
  /** « Ctrl Entrée » ou « ⌘Entrée ». */
  protected readonly filterShortcut = this.shortcuts.modShortcut('Entrée');

  /** Saisie ; la palette s'ouvre sur la recherche active du classement, sélectionnée. */
  protected readonly query = signal(this.ranking.search());
  private readonly words = computed(() => searchWords(this.query()).join(' '));
  protected readonly hasQuery = computed(() => this.words() !== '');

  constructor() {
    afterNextRender(() => {
      const input = this.input().nativeElement;
      input.focus();
      input.select();
    });
  }

  // ── Modèles ───────────────────────────────────────────────────────────────────────────────

  private readonly base = computed<BaseRequest>(
    () => {
      const { search: _search, limit: _limit, ...base } = this.ranking.request();
      return base;
    },
    { equal: sameJson },
  );

  /** Derniers résultats reçus : gardés pendant la recherche suivante (pas de clignotement). */
  private readonly last = signal<Results | null>(null);

  private readonly results = resource({
    params: () => ({ query: this.words(), base: this.base(), recent: this.detail.recent() }),
    loader: async ({ params, abortSignal, previous }) => {
      let results: Results;
      if (params.query) {
        if (previous.status !== 'idle') await delay(this.debounceMs, abortSignal);
        const r = await this.tauri.searchRanking({
          ...params.base,
          search: params.query,
          limit: MAX_MODELS,
        });
        results = { query: params.query, models: foundModels(r), count: foundCount(r) };
      } else {
        const models = await Promise.all(params.recent.map((m) => this.recent(m, params.base)));
        results = { query: '', models, count: null };
      }
      if (!abortSignal.aborted) this.last.set(results);
      return results;
    },
  });

  /**
   * Un modèle récent, avec son rang et ses estimations pour les filtres actuels. Son dépôt
   * (avec « / ») est cherché tel quel par le backend ; absent : il n'est plus classé.
   */
  private async recent(m: RecentModel, base: BaseRequest): Promise<PaletteModel> {
    const missing: PaletteModel = { ...m, item: null, section: null };
    try {
      const r = await this.tauri.searchRanking({
        ...base,
        search: m.repoId,
        showTooBig: true,
        limit: 20,
      });
      return foundModels(r, Infinity).find((x) => x.repoId === m.repoId) ?? missing;
    } catch {
      return missing;
    }
  }

  /** Résultats de la même sorte que la saisie (recherche ou récents), même un peu anciens. */
  private readonly current = computed(() => {
    const last = this.last();
    return last && (last.query === '') === !this.hasQuery() ? last : null;
  });

  // ── Commandes ─────────────────────────────────────────────────────────────────────────────

  private readonly commandList = computed<PaletteCommand[]>(() => {
    const c = this.commands;
    const list: PaletteCommand[] = [];
    const search = this.ranking.search().trim();
    if (search) {
      list.push({
        id: 'clear-search',
        label: `Effacer la recherche « ${search} »`,
        hint: 'classement complet',
        icon: 'close',
        keys: [],
        run: () => this.ranking.setSearch(''),
      });
    }
    if (c.canExport()) {
      list.push(
        {
          id: 'export-md',
          label: 'Exporter le classement en Markdown',
          hint: '.md',
          icon: 'file',
          keys: ['E', 'M'],
          keywords: 'export',
          run: () => void c.exportRanking('md'),
        },
        {
          id: 'export-csv',
          label: 'Exporter le classement en CSV',
          hint: 'Excel',
          icon: 'table',
          keys: ['E', 'C'],
          keywords: 'export tableur',
          run: () => void c.exportRanking('csv'),
        },
      );
    }
    list.push({
      id: 'rescan',
      label: 'Ré-analyser le matériel',
      hint: 'GPU, RAM, runtimes',
      icon: 'refresh',
      keys: ['R'],
      keywords: 'détecter détection',
      run: () => c.rescan(),
    });
    if (!this.catalog.syncing()) {
      list.push({
        id: 'refresh-catalog',
        label: 'Actualiser les modèles',
        hint: 'Hugging Face',
        icon: 'sync',
        keys: [],
        keywords: 'synchroniser catalogue',
        run: () => c.refreshCatalog(),
      });
    }
    list.push(
      {
        id: 'corrections',
        label: 'Corriger ou simuler le matériel',
        hint: 'autre GPU, autre RAM',
        icon: 'edit',
        keys: [],
        keywords: 'simulation corrections',
        run: () => c.openCorrections(),
      },
      {
        id: 'settings',
        label: 'Ouvrir les paramètres',
        hint: '',
        icon: 'settings',
        keys: [],
        keywords: 'réglages préférences',
        run: () => c.openSettings(),
      },
      {
        id: 'shortcuts',
        label: 'Raccourcis clavier',
        hint: 'aide',
        icon: 'keyboard',
        keys: ['?'],
        keywords: '? touches',
        run: () => c.openShortcuts(),
      },
    );
    const dark = this.theme.theme() === 'dark';
    list.push({
      id: 'theme',
      label: dark ? 'Passer au thème clair' : 'Passer au thème sombre',
      hint: 'apparence',
      icon: dark ? 'sun' : 'moon',
      keys: [],
      keywords: 'thème mode clair sombre jour nuit couleurs',
      searchOnly: true,
      run: () => this.theme.toggle(),
    });
    if (this.theme.preference() !== 'system') {
      list.push({
        id: 'theme-system',
        label: 'Suivre le thème du système',
        hint: 'clair ou sombre selon le système',
        icon: 'monitor',
        keys: [],
        keywords: 'thème mode apparence automatique clair sombre',
        searchOnly: true,
        run: () => this.theme.set('system'),
      });
    }
    const useCase = this.ranking.useCase();
    USE_CASES.forEach((u, i) =>
      list.push({
        id: `use-${u.value}`,
        label: `Usage : ${u.label}`,
        hint: u.value === useCase ? 'actuel' : 'filtre',
        icon: USE_CASE_ICONS[u.value],
        keys: [String(i + 1)],
        keywords: 'cas',
        searchOnly: true,
        run: () => this.ranking.setUseCase(u.value),
      }),
    );
    const ctx = this.ranking.effectiveCtx();
    for (const step of CTX_STEPS) {
      list.push({
        id: `ctx-${step}`,
        label: `Contexte : ${ctxK(step)}`,
        hint: step === ctx ? 'actuel' : 'filtre',
        icon: 'context',
        keys: [],
        keywords: 'ctx tokens',
        searchOnly: true,
        run: () => this.ranking.setCtx(step),
      });
    }
    if (this.ranking.ctx() !== null) {
      list.push({
        id: 'ctx-default',
        label: 'Contexte par défaut',
        hint: 'celui de l’usage',
        icon: 'context',
        keys: [],
        keywords: 'ctx',
        searchOnly: true,
        run: () => this.ranking.setCtx(null),
      });
    }
    return list;
  });

  // ── Options ───────────────────────────────────────────────────────────────────────────────

  protected readonly groups = computed<GroupView[]>(() => {
    const current = this.current();
    const groups = paletteGroups(
      this.query(),
      current?.models ?? [],
      this.commandList(),
      current && current.query === this.words() ? current.count : null,
    );
    let index = 0;
    return groups.map((g, gi) => ({
      id: `${this.uid}-group-${gi}`,
      label: g.label,
      options: g.options.map((o) => this.view(o, index++)),
    }));
  });

  private readonly options = computed(() => this.groups().flatMap((g) => g.options));

  /** Option active (clé stable : elle reste active quand les résultats arrivent). */
  private readonly activeKey = signal<string | null>(null);
  protected readonly active = computed(() => {
    const options = this.options();
    const found = options.find((o) => o.key === this.activeKey());
    return found ?? options[0] ?? null;
  });

  protected readonly status = computed(() => {
    if (this.results.error()) return 'Recherche impossible : réessayer dans un instant';
    if (this.hasQuery() && this.results.isLoading()) return 'Recherche…';
    const models = this.current()?.models.length ?? 0;
    if (models === 0) return '';
    return this.hasQuery()
      ? 'Rang dans le classement complet'
      : 'Fiches ouvertes pendant la session';
  });

  private view(option: PaletteOption, index: number): OptionView {
    const base: OptionView = {
      key: optionKey(option),
      id: `${this.uid}-option-${index}`,
      index,
      option,
      label: '',
      hint: '',
      icon: 'search',
      shortcut: '',
      rank: '',
      sub: '',
      mode: null,
      tokS: '',
      score: '',
      ariaLabel: null,
    };
    switch (option.kind) {
      case 'filter': {
        const n = option.count;
        return {
          ...base,
          label: `Filtrer le classement sur « ${option.query} »`,
          hint:
            n === null
              ? 'recherche par nom'
              : n === 0
                ? 'aucun modèle'
                : `${int(n)} modèle${n > 1 ? 's' : ''}`,
          icon: 'filter',
          shortcut: this.filterShortcut,
        };
      }
      case 'command': {
        const c = option.command;
        return { ...base, label: c.label, hint: c.hint, icon: c.icon, shortcut: c.keys.join(' ') };
      }
      case 'model':
        return { ...base, ...modelView(option.model) };
    }
  }

  // ── Actions ───────────────────────────────────────────────────────────────────────────────

  protected onInput(value: string): void {
    this.query.set(value);
    this.activeKey.set(null);
  }

  protected onKeydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    const mod = this.shortcuts.isMac ? event.metaKey : event.ctrlKey;
    switch (event.key) {
      case 'ArrowDown':
      case 'ArrowUp':
        event.preventDefault();
        this.move(event.key === 'ArrowDown' ? 1 : -1);
        break;
      case 'Enter': {
        event.preventDefault();
        const active = this.active();
        if (mod) this.applyFilter();
        else if (active) this.run(active.option);
        break;
      }
      case 'f':
      case 'F':
      case 'k':
      case 'K':
        // Ctrl F (ou Ctrl K) dans la palette : la saisie est sélectionnée, pour la remplacer.
        if (mod) {
          event.preventDefault();
          this.input().nativeElement.select();
        }
        break;
    }
  }

  protected hover(option: OptionView): void {
    if (this.active()?.key !== option.key) this.activeKey.set(option.key);
  }

  private move(step: 1 | -1): void {
    const options = this.options();
    if (options.length === 0) return;
    const i = this.active()?.index ?? 0;
    const next = options[Math.min(Math.max(i + step, 0), options.length - 1)];
    this.activeKey.set(next.key);
    afterNextRender(
      () => document.getElementById(next.id)?.scrollIntoView?.({ block: 'nearest' }),
      { injector: this.injector },
    );
  }

  protected run(option: PaletteOption): void {
    switch (option.kind) {
      case 'filter':
        this.applyFilter();
        break;
      case 'model':
        this.ref.close();
        this.detail.open(option.model.repoId);
        break;
      case 'command':
        // Fermée d'abord : un dialogue ouvert ensuite rend le focus là où il était.
        this.ref.close();
        option.command.run();
        break;
    }
  }

  /** Ctrl Entrée et « Filtrer le classement » : la saisie devient la recherche du classement. */
  private applyFilter(): void {
    const words = this.words();
    if (!words) return;
    this.ranking.setSearch(words);
    this.ref.close();
  }
}

/** Rang, sous-ligne (quant · taille, installé, approximatif, lent), mode, vitesse, score. */
function modelView(m: PaletteModel): Partial<OptionView> {
  const item = m.item;
  if (!item) {
    return {
      label: m.displayName,
      sub: 'hors du classement avec ces filtres',
      ariaLabel: `${m.displayName}, hors du classement avec ces filtres`,
    };
  }
  const { variant, estimate } = item.best;
  const sub = [
    `${variant.label} · ${fileSize(variant.sizeBytes)}`,
    ...(item.installed.includes(variant.label) ? ['installé'] : []),
    ...(estimate.approximate ? ['approximatif'] : []),
    ...(m.section === 'slow' ? ['lent'] : []),
  ].join(' · ');
  const rankText =
    m.section === 'slow'
      ? `${item.rank}e des lents`
      : m.section === 'tooBig'
        ? `${item.rank}e des trop gros`
        : `rang ${item.rank}`;
  const speed = tokS(estimate.tokS);
  const score = String(Math.round(item.score.total));
  return {
    label: item.displayName,
    rank: `#${item.rank}`,
    sub,
    mode: estimate.mode,
    tokS: speed === '—' ? '—' : `${speed} tok/s`,
    score,
    ariaLabel: `${item.displayName}, ${rankText}, ${sub}, ${MODE_LABELS[estimate.mode.kind]}, ${speed} tok/s, score ${score}`,
  };
}
