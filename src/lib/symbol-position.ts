/**
 * Maps a pointer position to the `(path, side, line, character)` a language server wants.
 *
 * The views render highlighted spans rather than an editor, so a position is recovered from the caret the
 * browser places in the text, combined with the file, line and side the markup declares about itself
 * (`data-find-scope`, `data-code-line`, `data-code-side`). Anything that is not a real symbol — the gutter,
 * cell padding, the truncation button, the phantom newline an empty line renders — resolves to null.
 *
 * Offsets are UTF-16 code units: what `Text.data.length` counts, and what LSP positions use.
 */

import { textSegments } from './find';

export type SymbolSide = 'old' | 'new';

export interface SymbolAtPoint {
  path: string;
  side: SymbolSide;
  line: number;
  /** 0-based offset into the line's text. */
  character: number;
}

/** WebKit's caret API is not in the DOM typings, so naming it needs the extra optional member. */
const caretDocument = document as Document & { caretRangeFromPoint?: (x: number, y: number) => Range | null };

/**
 * Bounding box of the cell's text. The caret API snaps to the nearest character, so without this a click in the
 * cell's padding — or right of a short line — would read as a click on that line's first or last character.
 */
function textBounds(cell: HTMLElement): DOMRect | null {
  if (!cell.textContent) {
    return null;
  }
  const range = document.createRange();
  range.selectNodeContents(cell);
  const bounds = range.getBoundingClientRect();
  return bounds.width > 0 ? bounds : null;
}

export function symbolAtPoint(x: number, y: number): SymbolAtPoint | null {
  const hit = document.elementFromPoint(x, y);
  const cell = hit?.closest<HTMLElement>('[data-code-cell]');
  const path = hit?.closest('[data-find-scope]')?.getAttribute('data-find-scope');
  if (!cell || !path) {
    return null;
  }

  const side = cell.dataset.codeSide;
  const line = Number(cell.dataset.codeLine);
  if ((side !== 'old' && side !== 'new') || !Number.isInteger(line) || line < 1) {
    return null;
  }

  const bounds = textBounds(cell);
  if (!bounds || x < bounds.left - 1 || x > bounds.right + 1) {
    return null;
  }

  const pointAt = caretDocument.caretRangeFromPoint;
  const range = pointAt ? pointAt.call(document, x, y) : null;
  if (!range || !cell.contains(range.startContainer)) {
    return null;
  }

  // `textSegments` already skips `<button>` text (the truncation button lives inside the cell but is not part
  // of the line), so a caret in it fails the lookup and the click resolves to nothing.
  const segment = textSegments(cell).find((candidate) => candidate.node === range.startContainer);
  return segment ? { path, side, line, character: segment.start + range.startOffset } : null;
}
