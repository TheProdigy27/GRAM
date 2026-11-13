import { enhance, renderMarkdown } from './markdown';

/** Markdown rendu, nettoyé puis enrichi, comme dans `app-markdown`. */
function render(markdown: string): HTMLElement {
  const el = document.createElement('div');
  el.innerHTML = renderMarkdown(markdown);
  enhance(el);
  return el;
}

describe('renderMarkdown', () => {
  it('GFM : titres, emphase, listes, tableaux, code dans le texte', () => {
    const el = render(
      '## Titre\n\n**gras** et *italique*, `code`\n\n- un\n- deux\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n~~barré~~',
    );
    expect(el.querySelector('h2')?.textContent).toBe('Titre');
    expect(el.querySelector('strong')?.textContent).toBe('gras');
    expect(el.querySelector('em')?.textContent).toBe('italique');
    expect(el.querySelector('p > code')?.textContent).toBe('code');
    expect([...el.querySelectorAll('li')].map((li) => li.textContent)).toEqual(['un', 'deux']);
    expect([...el.querySelectorAll('td')].map((td) => td.textContent)).toEqual(['1', '2']);
    expect(el.querySelector('del')?.textContent).toBe('barré');
  });

  it('le HTML écrit par le modèle s’affiche comme du texte : ni <script>, ni onerror', () => {
    const el = render(
      '<script>alert(1)</script>\n\nAvant <img src=x onerror="alert(2)"> après <b onclick="x()">gras</b>',
    );
    expect(el.querySelector('script, img, b')).toBeNull();
    expect(el.querySelectorAll('[onerror], [onclick]')).toHaveLength(0);
    expect(el.textContent).toContain('<script>alert(1)</script>');
    expect(el.textContent).toContain('<img src=x onerror="alert(2)">');
  });

  it('liens : http et https gardés, javascript: et les autres perdent leur adresse', () => {
    const el = render(
      '[ok](https://exemple.fr/page) [aussi](http://exemple.fr) [non](javascript:alert(1)) ' +
        '[pas](mailto:a@b.fr) [rien](file:///C:/x) <https://auto.fr>',
    );
    const links = [...el.querySelectorAll('a')];
    expect(links.map((a) => a.getAttribute('href'))).toEqual([
      'https://exemple.fr/page',
      'http://exemple.fr',
      null,
      null,
      null,
      'https://auto.fr',
    ]);
    // L'adresse en infobulle : le texte d'un lien peut mentir.
    expect(links[0].title).toBe('https://exemple.fr/page');
    expect(links[0].rel).toBe('noopener noreferrer');
    expect(el.innerHTML).not.toContain('javascript:');
  });

  it('images : un lien à la place (rien ne se charge)', () => {
    const el = render('![schéma](https://exemple.fr/a.png) ![x](javascript:alert(1))');
    expect(el.querySelector('img')).toBeNull();
    const links = [...el.querySelectorAll('a')];
    expect(links.map((a) => a.textContent)).toEqual(['Image : schéma', 'Image : x']);
    expect(links.map((a) => a.getAttribute('href'))).toEqual(['https://exemple.fr/a.png', null]);
  });

  it('blocs de code : coloration du langage connu, barre avec langage et bouton copier', () => {
    const el = render(
      '```python\ndef f():\n    return "<b>"\n```\n\n```inconnu\nx < y\n```\n\n```\nbrut\n```',
    );
    const blocks = [...el.querySelectorAll('.md-code')];
    expect(blocks).toHaveLength(3);
    const langs = blocks.map((b) => b.querySelector('.md-code__lang')?.textContent);
    expect(langs).toEqual(['python', 'inconnu', 'texte']);
    // Python coloré, texte intact (échappé par highlight.js).
    const python = blocks[0].querySelector('code')!;
    expect(python.classList).toContain('hljs');
    expect(python.querySelector('.hljs-keyword')?.textContent).toBe('def');
    expect(python.textContent).toBe('def f():\n    return "<b>"\n');
    expect(python.querySelector('b')).toBeNull();
    // Langage inconnu : laissé tel quel.
    expect(blocks[1].querySelector('code')!.classList).not.toContain('hljs');
    expect(blocks[1].querySelector('code')!.textContent).toBe('x < y\n');
    const copy = blocks[0].querySelector<HTMLButtonElement>('[data-testid="copy-code"]')!;
    expect(copy.getAttribute('aria-label')).toBe('Copier le code (python)');
    expect(copy.type).toBe('button');
    expect(blocks[0].querySelector('pre')!.tabIndex).toBe(0);
  });

  it('classes : seule celle du langage d’un bloc de code survit au nettoyage', () => {
    // `marked` ne garde que le premier mot, préfixé : aucune classe de l'interface empruntée.
    const el = render('```g-sr-only cdk-overlay-pane\nx\n```');
    expect(el.querySelector('code')!.className).toBe('language-g-sr-only');
    // Un langage farfelu est refusé.
    const odd = render('```a"b<c\nx\n```');
    expect(odd.querySelector('code')!.getAttribute('class')).toBeNull();
    expect(odd.querySelector('.md-code__lang')?.textContent).toBe('texte');
  });

  it('message en cours : un bloc de code pas encore fermé s’affiche déjà', () => {
    const el = render('Voici :\n\n```rust\nfn main() {');
    expect(el.querySelector('.md-code__lang')?.textContent).toBe('rust');
    expect(el.querySelector('code')?.textContent).toContain('fn main() {');
  });
});
