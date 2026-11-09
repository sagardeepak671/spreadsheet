/**
 * Inline SVG icon set (24x24, stroke = currentColor). One consistent family
 * for toolbar, menus, home screen and dialogs; no emoji, no icon font.
 */

const PATHS: Record<string, string> = {
  new: '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6"/><path d="M12 12v6M9 15h6"/>',
  open: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  save: '<path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><path d="M17 21v-8H7v8M7 3v5h8"/>',
  csv: '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6"/><path d="M8 13h8M8 17h8"/>',
  import: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5M12 15V3"/>',
  download: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5M12 15V3"/>',
  home: '<path d="M3 11l9-8 9 8"/><path d="M5 10v10h5v-6h4v6h5V10"/>',
  close: '<path d="M18 6L6 18M6 6l12 12"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  trash: '<path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6"/><path d="M10 11v6M14 11v6"/>',
  edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"/>',
  undo: '<path d="M9 14L4 9l5-5"/><path d="M4 9h11a5 5 0 0 1 0 10h-3"/>',
  redo: '<path d="M15 14l5-5-5-5"/><path d="M20 9H9a5 5 0 0 0 0 10h3"/>',
  cut: '<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M20 4L8.1 15.9M14.5 14.5L20 20M8.1 8.1L12 12"/>',
  copy: '<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>',
  paste: '<path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><rect x="8" y="2" width="8" height="4" rx="1"/>',
  bold: '<path d="M7 4h7a4 4 0 0 1 0 8H7z"/><path d="M7 12h8a4 4 0 0 1 0 8H7z"/>',
  italic: '<path d="M19 4h-9M14 20H5M15 4L9 20"/>',
  underline: '<path d="M6 3v7a6 6 0 0 0 12 0V3"/><path d="M4 21h16"/>',
  "align-left": '<path d="M17 10H3M21 6H3M21 14H3M17 18H3"/>',
  "align-center": '<path d="M18 10H6M21 6H3M21 14H3M18 18H6"/>',
  "align-right": '<path d="M21 10H7M21 6H3M21 14H3M21 18H7"/>',
  "text-color": '<path d="M5 19l7-15 7 15M8.5 13h7"/><path d="M4 22h16"/>',
  fill: '<path d="M19 11L8.7 21.3a2 2 0 0 1-2.8 0l-3.2-3.2a2 2 0 0 1 0-2.8L13 5"/><path d="M10 2l9 9"/><path d="M20 16s2 2.2 2 3.6a2 2 0 0 1-4 0c0-1.4 2-3.6 2-3.6z"/>',
  "font-size": '<path d="M3 7V4h12v3M9 20V4M6 20h6"/><path d="M15 13v-2h6v2M18 20v-9M16.5 20h3"/>',
  "clear-format": '<path d="M3 7V4h12v3M9 20V4M6 20h6"/><path d="M14 14l6 6M20 14l-6 6"/>',
  chart: '<path d="M3 3v18h18"/><path d="M7 15l4-5 3 3 5-7"/>',
  "chart-bar": '<path d="M3 3v18h18"/><path d="M8 17v-6M13 17V7M18 17v-9"/>',
  "chart-scatter": '<path d="M3 3v18h18"/><circle cx="8" cy="14" r="1.5"/><circle cx="12" cy="8" r="1.5"/><circle cx="16" cy="12" r="1.5"/><circle cx="18" cy="6" r="1.5"/>',
  find: '<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.3-4.3"/>',
  sort: '<path d="M11 5h10M11 9h7M11 13h4"/><path d="M3 17l3 3 3-3M6 20V4"/>',
  sheet: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18"/>',
  theme: '<circle cx="12" cy="12" r="9"/><path d="M12 3a9 9 0 0 1 0 18z" fill="currentColor" stroke="none"/>',
  check: '<path d="M20 6L9 17l-5-5"/>',
  keyboard: '<rect x="2" y="6" width="20" height="12" rx="2"/><path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M8 14h8"/>',
  menu: '<path d="M3 6h18M3 12h18M3 18h18"/>',
  "chevron-down": '<path d="M6 9l6 6 6-6"/>',
  logo: '<rect x="3" y="3" width="18" height="18" rx="3"/><path d="M3 9h18M3 15h18M9 3v18M15 3v18"/>',
  grid: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18M15 3v18"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 16v-4M12 8h.01"/>',
  warning: '<path d="M12 3l10 18H2z"/><path d="M12 10v4M12 18h.01"/>',
  "panel-right": '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/>',
};

export type IconName = keyof typeof PATHS;

export const ICON_NAMES = Object.keys(PATHS) as IconName[];

/** SVG markup for an icon (safe to use with innerHTML: all paths are static). */
export function icon(name: IconName, size = 16): string {
  const body = PATHS[name];
  return `<svg class="icon icon-${name}" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">${body}</svg>`;
}

/** Creates an element holding the icon. */
export function iconEl(name: IconName, size = 16): HTMLSpanElement {
  const span = document.createElement("span");
  span.className = "icon-wrap";
  span.innerHTML = icon(name, size);
  return span;
}
