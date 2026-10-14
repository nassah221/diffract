/**
 * Shows what a symbol is while the pointer rests on it.
 *
 * Hover fires far more often than a click, so this is debounced, keyed on the resolved
 * `(path, line, character)` so moving along a line is free, and cached per position — a second look costs
 * nothing. Nothing is asked when the pointer is over the card itself, which would otherwise hide it.
 */

import { useCallback, useRef, useState } from 'react';
import { fetchHover } from '../../lib/api';
import type { DiffFile } from '../../lib/diff-parser';
import { symbolAtPoint } from '../../lib/symbol-position';
import type { LspHover } from '../../lib/types';
import { lineTextAt } from './use-definition-jump';

/** Long enough that sweeping the mouse across a line does not fire a request per glyph. */
const SETTLE_MS = 120;

export interface HoverState {
  x: number;
  y: number;
  hover: LspHover;
}

export function useSymbolHover(options: { ref: string; files: DiffFile[]; enabled: boolean }) {
  const { ref, files, enabled } = options;
  const [card, setCard] = useState<HoverState | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const cache = useRef(new Map<string, LspHover | null>());
  const timer = useRef<number | undefined>(undefined);
  const shownKey = useRef<string | null>(null);

  const hide = useCallback(() => {
    shownKey.current = null;
    window.clearTimeout(timer.current);
    setCard(null);
  }, []);

  const onMouseMove = useCallback(
    (event: React.MouseEvent) => {
      if (!enabled) {
        return;
      }
      if (cardRef.current?.contains(event.target as Node)) {
        return;
      }

      const target = symbolAtPoint(event.clientX, event.clientY);
      if (!target || !target.path.endsWith('.go')) {
        hide();
        return;
      }

      const key = `${target.path}:${target.line}:${target.character}`;
      if (shownKey.current === key) {
        return;
      }
      const lineText = lineTextAt(files, target.path, target.side, target.line);
      if (lineText === null) {
        hide();
        return;
      }

      const at = { x: event.clientX, y: event.clientY };
      window.clearTimeout(timer.current);
      shownKey.current = key;

      const cached = cache.current.get(key);
      if (cached !== undefined) {
        setCard(cached ? { ...at, hover: cached } : null);
        return;
      }

      timer.current = window.setTimeout(() => {
        // An empty line renders a phantom newline the caret can sit in, so the offset is bounded by the real
        // line length before it is sent.
        const character = Math.max(0, Math.min(target.character, lineText.length));
        void fetchHover({ ...target, ref, character, lineText })
          .then((hover) => {
            // ponytail: crude ceiling on a session-long memo; an LRU would be the next step if it matters.
            if (cache.current.size > 500) {
              cache.current.clear();
            }
            cache.current.set(key, hover);
            if (shownKey.current === key) {
              setCard(hover ? { ...at, hover } : null);
            }
          })
          .catch(() => hide());
      }, SETTLE_MS);
    },
    [enabled, files, hide, ref],
  );

  return { onMouseMove, onMouseLeave: hide, card, cardRef };
}
