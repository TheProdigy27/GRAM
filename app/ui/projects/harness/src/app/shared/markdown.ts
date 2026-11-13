// Rendu des réponses des modèles, du texte non fiable : Markdown (GFM) par `marked`, puis
// nettoyage par DOMPurify (jamais de HTML brut), puis coloration des blocs de code par
// highlight.js (son cœur et quelques langages, embarqués : aucune ressource réseau).

import DOMPurify from 'dompurify';
import hljs from 'highlight.js/lib/core';
import bash from 'highlight.js/lib/languages/bash';
import c from 'highlight.js/lib/languages/c';
import cpp from 'highlight.js/lib/languages/cpp';
import csharp from 'highlight.js/lib/languages/csharp';
import css from 'highlight.js/lib/languages/css';
import diff from 'highlight.js/lib/languages/diff';
import go from 'highlight.js/lib/languages/go';
import java from 'highlight.js/lib/languages/java';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import markdown from 'highlight.js/lib/languages/markdown';
import plaintext from 'highlight.js/lib/languages/plaintext';
import powershell from 'highlight.js/lib/languages/powershell';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import sql from 'highlight.js/lib/languages/sql';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';
import { Marked, type Tokens } from 'marked';

const LANGUAGES = {
  bash,
  c,
  cpp,
  csharp,
  css,
  diff,
  go,
  java,
  javascript,
  json,
  markdown,
  plaintext,
  powershell,
  python,
  rust,
  sql,
  typescript,
  xml,
  yaml,
};
for (const [name, language] of Object.entries(LANGUAGES)) hljs.registerLanguage(name, language);

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

const marked = new Marked({
  gfm: true,
  async: false,
  renderer: {
    // Le HTML écrit par le modèle s'affiche tel quel, comme du texte.
    html(token: Tokens.HTML | Tokens.Tag): string {
      const text = escapeHtml(token.text);
      return 'block' in token && token.block ? `<p>${text}</p>\n` : text;
    },
    // Pas d'image distante (hors ligne, et rien ne se charge à l'insu de l'utilisateur) : un
    // lien à la place.
    image({ href, text }: Tokens.Image): string {
      const label = `Image : ${text || href}`;
      return `<a href="${escapeHtml(href)}">${escapeHtml(label)}</a>`;
    },
  },
});

/** Instance propre de DOMPurify : ses réglages ne touchent personne d'autre. */
const purify = DOMPurify(window);
purify.addHook('uponSanitizeAttribute', (node, data) => {
  // Seule la classe du langage d'un bloc de code est gardée : le modèle ne peut pas emprunter
  // les classes de l'interface.
  if (data.attrName === 'class') {
    data.keepAttr = node.nodeName === 'CODE' && /^language-[\w+#.-]{1,32}$/.test(data.attrValue);
  }
});

const PURIFY_CONFIG = {
  USE_PROFILES: { html: true },
  FORBID_TAGS: [
    'style',
    'img',
    'picture',
    'source',
    'video',
    'audio',
    'iframe',
    'form',
    'button',
    'textarea',
    'select',
    'option',
  ],
  FORBID_ATTR: ['style', 'id', 'name'],
  // Liens http et https seulement ; les autres perdent leur adresse.
  ALLOWED_URI_REGEXP: /^https?:/i,
};

/** Markdown d'une réponse → HTML nettoyé (sans coloration ni barre des blocs de code). */
export function renderMarkdown(text: string): string {
  const html = marked.parse(text) as string;
  return purify.sanitize(html, PURIFY_CONFIG);
}

/** Libellé du langage d'un bloc de code (`language-ts` → « ts »), « texte » sans langage. */
export function codeLanguage(code: Element): string | null {
  const match = /^language-(.+)$/.exec(code.className);
  return match ? match[1] : null;
}

/**
 * Après insertion du HTML nettoyé : coloration des blocs de code d'un langage connu (le texte
 * est relu dans le DOM et échappé par highlight.js), barre au-dessus de chaque bloc (langage,
 * bouton copier), adresse de chaque lien en infobulle (le texte d'un lien peut mentir).
 */
export function enhance(root: HTMLElement): void {
  for (const pre of Array.from(root.querySelectorAll('pre'))) {
    const code = pre.querySelector('code');
    if (!code) continue;
    const language = codeLanguage(code);
    if (language && hljs.getLanguage(language)) {
      code.innerHTML = hljs.highlight(code.textContent ?? '', {
        language,
        ignoreIllegals: true,
      }).value;
      code.classList.add('hljs');
    }
    const block = document.createElement('div');
    block.className = 'md-code';
    const bar = document.createElement('div');
    bar.className = 'md-code__bar';
    const label = document.createElement('span');
    label.className = 'md-code__lang';
    label.textContent = language ?? 'texte';
    const copy = document.createElement('button');
    copy.type = 'button';
    copy.className = 'md-code__copy';
    copy.dataset['copyCode'] = '';
    copy.dataset['testid'] = 'copy-code';
    copy.setAttribute('aria-label', `Copier le code${language ? ` (${language})` : ''}`);
    copy.textContent = 'Copier';
    bar.append(label, copy);
    pre.replaceWith(block);
    block.append(bar, pre);
    pre.tabIndex = 0;
  }
  for (const link of Array.from(root.querySelectorAll('a'))) {
    const href = link.getAttribute('href');
    if (href) link.title = href;
    link.rel = 'noopener noreferrer';
  }
}
