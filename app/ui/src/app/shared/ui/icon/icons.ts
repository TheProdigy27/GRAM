// Registre des icônes : tracés à trait sur une grille de 24 × 24, repris des maquettes (cercles et
// rectangles convertis en chemins pour n'avoir qu'un seul élément SVG à rendre).

/** Cercle de centre (cx, cy) et de rayon r. */
function circle(cx: number, cy: number, r: number): string {
  return `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
}

/** Ellipse de centre (cx, cy). */
function ellipse(cx: number, cy: number, rx: number, ry: number): string {
  return `M${cx - rx} ${cy}a${rx} ${ry} 0 1 0 ${2 * rx} 0a${rx} ${ry} 0 1 0 ${-2 * rx} 0`;
}

/** Rectangle aux coins arrondis de rayon r. */
function rect(x: number, y: number, w: number, h: number, r: number): string {
  const iw = w - 2 * r;
  const ih = h - 2 * r;
  return (
    `M${x + r} ${y}h${iw}a${r} ${r} 0 0 1 ${r} ${r}v${ih}a${r} ${r} 0 0 1 ${-r} ${r}` +
    `h${-iw}a${r} ${r} 0 0 1 ${-r} ${-r}v${-ih}a${r} ${r} 0 0 1 ${r} ${-r}Z`
  );
}

export const ICONS = {
  // Navigation et actions
  search: [circle(11, 11, 7), 'm20 20-3.5-3.5'],
  close: ['M18 6 6 18M6 6l12 12'],
  check: ['m5 12 5 5 9-10'],
  'chevron-down': ['m6 9 6 6 6-6'],
  'chevron-up': ['m18 15-6-6-6 6'],
  'chevron-right': ['m9 6 6 6-6 6'],
  'chevron-left': ['m15 6-6 6 6 6'],
  'arrow-down': ['M12 5v14M6 13l6 6 6-6'],
  'arrow-up': ['M12 19V5M6 11l6-6 6 6'],
  plus: ['M12 5v14M5 12h14'],
  minus: ['M5 12h14'],
  refresh: [
    'M3 12a9 9 0 0 1 15.5-6.3L21 8',
    'M21 3v5h-5',
    'M21 12a9 9 0 0 1-15.5 6.3L3 16',
    'M3 21v-5h5',
  ],
  reset: ['M3 12a9 9 0 1 0 3-6.7L3 8', 'M3 3v5h5'],
  download: ['M12 3v12', 'm7 10 5 5 5-5', 'M5 21h14'],
  sync: [
    'M17.5 19a4.5 4.5 0 1 0-1.4-8.8A6 6 0 0 0 4.5 12 4 4 0 0 0 6 19.8',
    'm9 15 3-3 3 3',
    'M12 12v9',
  ],
  update: [circle(12, 12, 9), 'm8 12 4-4 4 4', 'M12 16V8'],
  copy: [rect(9, 9, 12, 12, 2), 'M5 15V5a2 2 0 0 1 2-2h10'],
  external: [
    'M14 4h6v6',
    'M20 4 10 14',
    'M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5',
  ],
  settings: [
    'M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1',
    circle(15, 6, 2),
    circle(9, 12, 2),
    circle(17, 18, 2),
  ],
  edit: ['M4 20h4L19 9l-4-4L4 16v4Z'],
  filter: ['M3 5h18l-7 8v6l-4 2v-8L3 5Z'],
  trash: ['M4 7h16', 'M10 11v6M14 11v6', 'M6 7l1 13h10l1-13', 'M9 7V4h6v3'],
  play: ['M7 4v16l13-8L7 4Z'],
  // Cas d'usage
  grid: [
    rect(3, 3, 7, 7, 1.5),
    rect(14, 3, 7, 7, 1.5),
    rect(3, 14, 7, 7, 1.5),
    rect(14, 14, 7, 7, 1.5),
  ],
  code: ['m8 7-5 5 5 5M16 7l5 5-5 5'],
  bulb: [
    'M9 18h6M10 21h4',
    'M12 3a6 6 0 0 0-4 10.5c.8.8 1 1.5 1 2.5h6c0-1 .2-1.7 1-2.5A6 6 0 0 0 12 3Z',
  ],
  bolt: ['M13 2 4 14h7l-1 8 9-12h-7l1-8Z'],
  eye: ['M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z', circle(12, 12, 3)],
  // Objets
  gauge: ['M12 14l4-4', 'M3.3 19a10 10 0 1 1 17.4 0'],
  database: [
    ellipse(12, 5, 8, 3),
    'M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5',
    'M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3',
  ],
  context: ['M4 4h16v16H4z', 'M8 9h8M8 13h8'],
  file: ['M5 3h10l4 4v14H5z', 'M9 13h6M9 17h6'],
  table: [rect(3, 4, 18, 16, 2), 'M3 10h18M9 4v16'],
  chart: ['M4 20h16', 'M6 16v-5M10 16V6M14 16v-8M18 16v-3'],
  lock: [rect(5, 11, 14, 10, 2), 'M8 11V7a4 4 0 0 1 8 0v4'],
  star: ['M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9L12 3.5Z'],
  // États
  info: [circle(12, 12, 9), 'M12 11v5M12 8h.01'],
  warning: ['M12 3 2 20h20L12 3Z', 'M12 10v4M12 17h.01'],
  error: [circle(12, 12, 9), 'M12 8v5M12 16h.01'],
  circle: [circle(12, 12, 8)],
  'minus-circle': [circle(12, 12, 9), 'M8 12h8'],
} as const satisfies Record<string, readonly string[]>;

export type IconName = keyof typeof ICONS;
