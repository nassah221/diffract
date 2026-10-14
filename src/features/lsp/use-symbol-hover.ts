/**
 * Symbol affordances on pointer move: the hover card, and the ⌘ highlight that previews a click.
 *
 * Timing follows an editor rather than a tooltip-on-hover:
 *
 * - the pointer must **dwell** on a symbol before anything is asked for, so sweeping across a line costs
 *   nothing and never flashes a card;
 * - once shown, the card **stays put until the next symbol's content is ready**, so moving between symbols
 *   updates it in one step instead of jumping;
 * - leaving a symbol hides it only after a short grace period, so crossing a space does not blink it away;
 * - moving *within* a symbol is a no-op — no request, no reposition.
 *
 * The card is anchored to the symbol's own box, not the cursor.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchHover } from '../../lib/api';
import type { DiffFile } from '../../lib/diff-parser';
import { placeCard, rangeForOffsets, symbolAtPoint, wordAt, type CardAnchor } from '../../lib/symbol-position';
import type { LspHover } from '../../lib/types';
import { lineTextAt } from './use-definition-jump';
import { setSymbolHighlight } from './symbol-highlight';

/** How long the pointer must rest on a symbol before a card is asked for. */
const DWELL_MS = 350;
/** How long a shown card survives after the pointer leaves a symbol. */
const GRACE_MS = 220;

export interface HoverState {
  anchor: CardAnchor;
  hover: LspHover;
}

export function useSymbolHover(options: { ref: string; files: DiffFile[]; enabled: boolean }) {
  const { ref, files, enabled } = options;
  const [card, setCard] = useState<HoverState | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const cache = useRef(new Map<string, LspHover | null>());
  /** Only ever set while the card on screen belongs to that key. */
  const shown = useRef<string | null>(null);
  /** The key the pointer is currently over, so a timer that fires late can tell it moved on. */
  const over = useRef<string | null>(null);
  const dwellTimer = useRef<number | undefined>(undefined);
  const graceTimer = useRef<number | undefined>(undefined);
  /** The last pointer position, so pressing ⌘ highlights without needing the mouse to move. */
  const lastPoint = useRef<{ x: number; y: number } | null>(null);

  const cancelDwell = () => window.clearTimeout(dwellTimer.current);
  const cancelGrace = () => window.clearTimeout(graceTimer.current);

  const hide = useCallback(() => {
    cancelDwell();
    cancelGrace();
    over.current = null;
    shown.current = null;
    setCard(null);
  }, []);

  /** After the grace period, unless the pointer came back to a symbol in the meantime. */
  const scheduleHide = useCallback(() => {
    if (!shown.current) {
      cancelDwell();
      return;
    }
    cancelDwell();
    cancelGrace();
    graceTimer.current = window.setTimeout(() => {
      shown.current = null;
      setCard(null);
    }, GRACE_MS);
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
      return { target, lineText, range, box, key: `${target.path}:${target.line}:${word.start}` };
    },
    [files],
  );

  /**
   * Pin to the symbol, flipping above it when there is no room below. Anchoring by the card's bottom edge in
   * that case means the height never has to be measured first, so the first frame is already in place.
   */
  const place = useCallback((box: DOMRect | null, cell: HTMLElement): CardAnchor => {
    return placeCard(box ?? cell.getBoundingClientRect(), window.innerHeight);
  }, []);

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
        over.current = null;
        setSymbolHighlight(null);
        scheduleHide();
        return;
      }

      cancelGrace();
      // ⌘ previews the jump, and that affordance is immediate — it answers "what would a click do".
      setSymbolHighlight(event.metaKey ? found.range : null);

      const { target, lineText, key } = found;
      if (over.current === key) {
        return;
      }
      over.current = key;

      if (shown.current === key) {
        // Already on screen for this symbol: leave it exactly where it is.
        cancelDwell();
        return;
      }

      // Hold the current card while the next symbol settles, so moving between symbols never flashes.
      cancelDwell();
      dwellTimer.current = window.setTimeout(() => {
        if (over.current !== key) {
          return;
        }
        const anchor: CardAnchor = place(found.box, target.cell);
        const cached = cache.current.get(key);
        if (cached !== undefined) {
          shown.current = cached ? key : null;
          setCard(cached ? { anchor, hover: cached } : null);
          return;
        }
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
            if (over.current === key) {
              shown.current = hover ? key : null;
              setCard(hover ? { anchor, hover } : null);
            }
          })
          .catch(() => hide());
      }, DWELL_MS);
    },
    [enabled, hide, place, ref, resolve, scheduleHide],
  );

  /** Leaving the code entirely is a real "gone" signal, so it does not wait out the grace period. */
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
