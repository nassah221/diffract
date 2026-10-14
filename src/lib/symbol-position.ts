/**
 * Maps a pointer position to the `(path, side, line, character)` a language server wants, and to the symbol
 * around it.
 *
 * The views render highlighted spans rather than an editor, so a position is recovered from the caret the
 * browser places in the text, combined with the file, line and side the markup declares about itself
 * (`data-find-scope`, `data-code-line`, `data-code-side`). Anything that is not a real symbol — the gutter,
 * cell padding, the truncation button, the phantom newline an empty line renders — resolves to null.
 *
 * Offsets are UTF-16 code units: what `Text.data.length` counts, and what LSP positions use.
 */

import { rangeFor, textSegments } from './find';

export type SymbolSide = 'old' | 'new';

export interface SymbolAtPoint {
  path: string;
  side: SymbolSide;
  line: number;
  /** 0-based offset into the line's text. */
  character: number;
  /** The rendered cell the symbol is in, so callers can measure or highlight the same text again. */
  cell: HTMLElement;
}

/** A half-open `[start, end)` span of a line's text. */
export interface WordSpan {
  start: number;
  end: number;
}

const WORD = /[A-Za-z0-9_]/;

/**
 * The identifier around `character`, or null when the caret is on something that is not one. Pure, so the
 * "did the pointer move to a different symbol" question does not need the DOM.
 */
export function wordAt(text: string, character: number): WordSpan | null {
  const position = Math.max(0, Math.min(character, text.length));
  let start = position;
  let end = position;
  while (start > 0 && WORD.test(text[start - 1] ?? '')) {
    start -= 1;
  }
  while (end < text.length && WORD.test(text[end] ?? '')) {
    end += 1;
  }
  return end > start ? { start, end } : null;
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
  return segment
    ? { path, side, line, character: segment.start + range.startOffset, cell }
    : null;
}

/** Where a card pinned to a symbol goes: anchored by whichever edge keeps it next to the symbol. */
export type CardAnchor = { left: number; top: number } | { left: number; bottom: number };

/** Below this much room under a symbol, the card goes above it instead. */
const FLIP_BELOW = 260;

export function placeCard(
  rect: { left: number; top: number; bottom: number },
  viewportHeight: number,
  flipBelow = FLIP_BELOW,
): CardAnchor {
  const roomBelow = viewportHeight - rect.bottom;
  return roomBelow < flipBelow && rect.top > flipBelow
    ? { left: rect.left, bottom: viewportHeight - rect.top + 6 }
    : { left: rect.left, top: rect.bottom + 6 };
}

/**
 * The DOM range for `[start, end)` of a cell's text, across the token spans it is split into. Used to measure a
 * symbol (so a card can sit still against it) and to highlight it. Inlay hints are skipped by `textSegments`,
 * so these offsets are code offsets.
 */
export function rangeForOffsets(cell: HTMLElement, start: number, end: number): Range | null {
  return rangeFor(textSegments(cell), start, end);
}
