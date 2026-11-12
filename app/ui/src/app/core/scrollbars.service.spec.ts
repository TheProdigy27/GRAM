import { TestBed } from '@angular/core/testing';
import { SCROLLBAR_HIDE_DELAY_MS, ScrollbarsService } from './scrollbars.service';

describe('ScrollbarsService', () => {
  let outer: HTMLElement;
  let inner: HTMLElement;

  beforeEach(() => {
    vi.useFakeTimers();
    TestBed.configureTestingModule({
      providers: [{ provide: SCROLLBAR_HIDE_DELAY_MS, useValue: 500 }],
    });
    TestBed.inject(ScrollbarsService);
    outer = document.createElement('div');
    inner = document.createElement('div');
    outer.appendChild(inner);
    document.body.appendChild(outer);
  });

  afterEach(() => {
    outer.remove();
    document.documentElement.classList.remove('g-scrollbar', 'g-scrolling');
    vi.useRealTimers();
  });

  const scroll = (target: EventTarget) => target.dispatchEvent(new Event('scroll'));
  const classes = (el: Element) => [...el.classList];

  it('montre la barre de l’élément qui défile, et de lui seul, jusqu’à peu après la fin', () => {
    scroll(inner);
    expect(classes(inner)).toEqual(['g-scrollbar', 'g-scrolling']);
    expect(classes(outer)).toEqual([]);

    // Chaque défilement repousse l’effacement.
    vi.advanceTimersByTime(400);
    scroll(inner);
    vi.advanceTimersByTime(400);
    expect(classes(inner)).toContain('g-scrolling');

    vi.advanceTimersByTime(100);
    // `g-scrollbar` reste : elle porte le fondu de sortie.
    expect(classes(inner)).toEqual(['g-scrollbar']);
  });

  it('chaque élément a son propre délai', () => {
    scroll(outer);
    vi.advanceTimersByTime(300);
    scroll(inner);
    vi.advanceTimersByTime(200);
    expect(classes(outer)).toEqual(['g-scrollbar']);
    expect(classes(inner)).toContain('g-scrolling');
  });

  it('la page : la barre de <html>', () => {
    scroll(document);
    expect(classes(document.documentElement)).toContain('g-scrolling');
    vi.advanceTimersByTime(500);
    expect(classes(document.documentElement)).not.toContain('g-scrolling');
  });

  it('s’arrête avec l’application', () => {
    TestBed.resetTestingModule();
    scroll(inner);
    expect(classes(inner)).toEqual([]);
  });
});
