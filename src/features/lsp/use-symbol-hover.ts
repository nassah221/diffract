/**
 * Symbol affordances on pointer move: the hover card, and the ⌘ highlight that previews a click.
 *
 * Both are keyed on the *symbol* rather than the caret, so moving along a word does nothing at all — the card
 * stays where it was put and no request is made. The card is anchored to the symbol's own box, not the cursor,
 * which is what keeps it from drifting as the pointer moves over the text.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchHover } from '../../lib/api';
import type { DiffFile } from '../../lib/diff-parser';
import { rangeForOffsets, symbolAtPoint, wordAt } from '../../lib/symbol-position';
import type { LspHover } from '../../lib/types';
import { lineTextAt } from './use-definition-jump';
import { setSymbolHighlight } from './symbol-highlight';

/** Long enough that sweeping the mouse across a line does not fire a request per glyph. */
const SETTLE_MS = 120;

/** Where the card sits: the symbol's box it is pinned to, so it does not follow the pointer. */
export interface HoverAnchor {
  left: number;
  top: number;
}

export interface HoverState {
  anchor: HoverAnchor;
  hover: LspHover;
}

export function useSymbolHover(options: { ref: string; files: DiffFile[]; enabled: boolean }) {
  const { ref, files, enabled } = options;
  const [card, setCard] = useState<HoverState | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const cache = useRef(new Map<string, LspHover | null>());
  const timer = useRef<number | undefined>(undefined);
  const shownKey = useRef<string | null>(null);
  /** The last pointer position, so pressing ⌘ highlights without needing the mouse to move. */
  const lastPoint = useRef<{ x: number; y: number } | null>(null);

  const hide = useCallback(() => {
    shownKey.current = null;
    window.clearTimeout(timer.current);
    setCard(null);
  }, []);

  /** The symbol under a point, with the word span and the box to pin things to. */
  const resolve = useCallback(
    (x: number, y: number) => {
      const target = symbolAtPoint(x, y);
      if (!target || !target.path.endsWith('.go')) {
        return null;
      }
      const lineText = lineTextAt(files, target.path, target.side, target.line);
      if (lineText === null) {
        return null;
      }
      const word = wordAt(lineText, Math.max(0, Math.min(target.character, lineText.length)));
      if (!word) {
        return null;
      }
      const range = rangeForOffsets(target.cell, word.start, word.end);
      const box = range?.getBoundingClientRect() ?? null;
      return { target, lineText, word, range, box };
    },
    [files],
  );

  const onMouseMove = useCallback(
    (event: React.MouseEvent) => {
      lastPoint.current = { x: event.clientX, y: event.clientY };
      if (!enabled) {
        return;
      }
      if (cardRef.current?.contains(event.target as Node)) {
        return;
      }

      const found = resolve(event.clientX, event.clientY);
      if (!found) {
        setSymbolHighlight(null);
        hide();
        return;
      }

      // ⌘ previews the jump: underline the symbol the click would follow.
      setSymbolHighlight(event.metaKey ? found.range : null);

      const key = `${found.target.path}:${found.target.line}:${found.word.start}`;
      if (shownKey.current === key) {
        // Same symbol: leave the card exactly where it is.
        return;
      }

      const { target, lineText, box } = found;
      window.clearTimeout(timer.current);
      shownKey.current = key;
      const place = (): HoverAnchor => {
        const rect = box ?? target.cell.getBoundingClientRect();
        return { left: rect.left, top: rect.bottom + 6 };
      };

      const cached = cache.current.get(key);
      if (cached !== undefined) {
        setCard(cached ? { anchor: place(), hover: cached } : null);
        return;
      }

      timer.current = window.setTimeout(() => {
        const character = Math.max(0, Math.min(target.character, lineText.length));
        void fetchHover({ ...target, ref, character, lineText })
          .then((hover) => {
            // ponytail: crude ceiling on a session-long memo; an LRU would be the next step if it matters.
            if (cache.current.size > 500) {
              cache.current.clear();
            }
            cache.current.set(key, hover);
            if (shownKey.current === key) {
              setCard(hover ? { anchor: place(), hover } : null);
            }
          })
          .catch(() => hide());
      }, SETTLE_MS);
    },
    [enabled, hide, ref, resolve],
  );

  /** Leaving the code, or the window, drops both affordances. */
  const onMouseLeave = useCallback(() => {
    lastPoint.current = null;
    setSymbolHighlight(null);
    hide();
  }, [hide]);

  // ⌘ can be pressed or released without the pointer moving, so both keys are watched.
  useEffect(() => {
    const sync = (metaKey: boolean) => {
      const point = lastPoint.current;
      if (!metaKey || !enabled || !point) {
        setSymbolHighlight(null);
        return;
      }
      setSymbolHighlight(resolve(point.x, point.y)?.range ?? null);
    };
    const onKeyDown = (event: KeyboardEvent) => event.key === 'Meta' && sync(true);
    const onKeyUp = (event: KeyboardEvent) => event.key === 'Meta' && sync(false);
    const onBlur = () => sync(false);
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
      setSymbolHighlight(null);
    };
  }, [enabled, resolve]);

  return { onMouseMove, onMouseLeave, card, cardRef };
}
