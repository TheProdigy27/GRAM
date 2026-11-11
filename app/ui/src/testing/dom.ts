// Helpers de test sans harness Material : on agit sur le DOM comme l'utilisateur (clic, saisie,
// touches) puis on attend la stabilité d'Angular. Les cibles se donnent par `data-testid` ou
// directement par élément. Les superpositions CDK (listes, menus) sont dans `document.body`.

import type { ComponentFixture } from '@angular/core/testing';

type Target = string | Element;
type Fixture = ComponentFixture<unknown>;

/** Texte d'un élément, espaces normalisés. */
export function text(el: Element | null | undefined): string {
  return el?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
}

/** Élément `data-testid` (dans `root`, par défaut tout le document) ; erreur s'il manque. */
export function byTestId<T extends Element = HTMLElement>(
  id: string,
  root: ParentNode = document,
): T {
  const el = root.querySelector<T>(`[data-testid="${id}"]`);
  if (!el) throw new Error(`Aucun élément data-testid="${id}"`);
  return el;
}

/** Élément `data-testid`, ou `null`. */
export function queryTestId<T extends Element = HTMLElement>(
  id: string,
  root: ParentNode = document,
): T | null {
  return root.querySelector<T>(`[data-testid="${id}"]`);
}

/** Tous les éléments `data-testid`. */
export function allByTestId<T extends Element = HTMLElement>(
  id: string,
  root: ParentNode = document,
): T[] {
  return [...root.querySelectorAll<T>(`[data-testid="${id}"]`)];
}

function resolve<T extends Element = HTMLElement>(fixture: Fixture, target: Target): T {
  return typeof target === 'string'
    ? byTestId<T>(target, fixture.nativeElement as HTMLElement)
    : (target as T);
}

/** Premier élément correspondant à `selector`, l'élément lui-même compris. */
function self<T extends Element>(el: Element, selector: string): T {
  const found = el.matches(selector) ? el : el.querySelector(selector);
  if (!found) throw new Error(`Aucun ${selector} dans ${el.outerHTML.slice(0, 120)}`);
  return found as T;
}

/** Attend la stabilité d'Angular (rendu, tâches en attente). */
export async function stable(fixture: Fixture): Promise<void> {
  await fixture.whenStable();
}

/** Clique sur un élément (sans effet sur un contrôle désactivé, comme dans le navigateur). */
export async function click(fixture: Fixture, target: Target): Promise<void> {
  const el = resolve<Element>(fixture, target);
  if (el instanceof HTMLElement) el.click();
  else el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
  await stable(fixture);
}

/** `keyCode` des touches courantes : les key managers du CDK le lisent encore. */
const KEY_CODES: Record<string, number> = {
  Tab: 9,
  Enter: 13,
  Escape: 27,
  ' ': 32,
  End: 35,
  Home: 36,
  ArrowLeft: 37,
  ArrowUp: 38,
  ArrowRight: 39,
  ArrowDown: 40,
};

/** Envoie une touche (`keydown`) à un élément : « Enter », « ArrowDown », « Escape », « k »… */
export async function press(
  fixture: Fixture,
  target: Target | Document,
  key: string,
  init: KeyboardEventInit = {},
): Promise<KeyboardEvent> {
  const el = target instanceof Document ? target : resolve(fixture, target);
  const keyCode = KEY_CODES[key] ?? (key.length === 1 ? key.toUpperCase().charCodeAt(0) : 0);
  const event = new KeyboardEvent('keydown', {
    key,
    keyCode,
    bubbles: true,
    cancelable: true,
    ...init,
  });
  el.dispatchEvent(event);
  await stable(fixture);
  return event;
}

/**
 * Saisit `value` dans un champ (`input`, `textarea` ou un `g-field` qui en contient un) :
 * remplace le contenu puis émet `input` et `change`.
 */
export async function typeInto(fixture: Fixture, target: Target, value: string): Promise<void> {
  const input = self<HTMLInputElement>(resolve(fixture, target), 'input, textarea');
  input.focus();
  input.value = value;
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
  await stable(fixture);
}

/** Bouton « Effacer » d'un `g-field`. */
export async function clearField(fixture: Fixture, target: Target): Promise<void> {
  await click(fixture, byTestId('field-clear', resolve(fixture, target)));
}

/** Bascule un interrupteur (`g-switch` ou `[role=switch]`) ; renvoie son nouvel état. */
export async function toggleSwitch(fixture: Fixture, target: Target): Promise<boolean> {
  const button = self<HTMLButtonElement>(resolve(fixture, target), '[role="switch"]');
  await click(fixture, button);
  return button.getAttribute('aria-checked') === 'true';
}

/** Coche ou décoche une case (`g-checkbox` ou `input[type=checkbox]`). */
export async function setChecked(
  fixture: Fixture,
  target: Target,
  checked: boolean,
): Promise<void> {
  const input = self<HTMLInputElement>(resolve(fixture, target), 'input[type="checkbox"]');
  if (input.checked !== checked) await click(fixture, input);
}

/**
 * Choisit l'option `label` : dans un groupe de boutons (`g-toggle-group`, `g-tabs`), le bouton de
 * ce libellé ; dans un `g-select`, ouvre la liste puis clique l'option.
 */
export async function chooseOption(fixture: Fixture, target: Target, label: string): Promise<void> {
  const el = resolve(fixture, target);
  const trigger = el.matches('[aria-haspopup="listbox"]')
    ? el
    : el.querySelector<HTMLElement>('[aria-haspopup="listbox"]');
  if (trigger) {
    if (trigger.getAttribute('aria-expanded') !== 'true') await click(fixture, trigger);
    const list = document.getElementById(trigger.getAttribute('aria-controls') ?? '');
    if (!list) throw new Error('Liste non ouverte');
    await click(fixture, findByText(list, '[role="option"]', label));
    return;
  }
  await click(fixture, findByText(el, 'button', label));
}

/** Libellés des options d'un `g-select` ouvert, ou des boutons d'un groupe. */
export function optionLabels(root: Element): string[] {
  return [...root.querySelectorAll('[role="option"], button')].map((o) =>
    text(o.querySelector('.g-select__option-label') ?? o),
  );
}

/** Ouvre le menu d'un déclencheur ; renvoie ses éléments. */
export async function openMenu(fixture: Fixture, target: Target): Promise<HTMLElement[]> {
  const trigger = resolve(fixture, target);
  if (trigger.getAttribute('aria-expanded') !== 'true') await click(fixture, trigger);
  return menuItems();
}

/** Éléments du menu ouvert (le dernier ouvert s'il y en a plusieurs). */
export function menuItems(): HTMLElement[] {
  const menus = document.querySelectorAll('[role="menu"]');
  const menu = menus[menus.length - 1];
  return menu ? [...menu.querySelectorAll<HTMLElement>('[role="menuitem"]')] : [];
}

/** Ouvre le menu puis déclenche l'élément `label`. */
export async function chooseMenuItem(
  fixture: Fixture,
  target: Target,
  label: string,
): Promise<void> {
  const items = await openMenu(fixture, target);
  const item = items.find((i) => text(i).includes(label));
  if (!item) throw new Error(`Aucun élément de menu « ${label} » : ${items.map(text).join(', ')}`);
  await click(fixture, item);
}

/** Attend la fin d'un debounce (`ms`, 0 par défaut comme FILTER_DEBOUNCE_MS en test). */
export async function waitDebounce(fixture: Fixture, ms = 0): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
  await stable(fixture);
}

function findByText(root: Element, selector: string, label: string): HTMLElement {
  const all = [...root.querySelectorAll<HTMLElement>(selector)];
  const found =
    all.find((e) => text(e.querySelector('.g-select__option-label') ?? e) === label) ??
    all.find((e) => text(e).includes(label));
  if (!found) throw new Error(`Aucun ${selector} « ${label} » : ${all.map(text).join(', ')}`);
  return found;
}
