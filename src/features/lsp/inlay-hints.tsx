/**
 * Inlay hints: the parameter names and inferred types gopls draws *inside* a line.
 *
 * They are inserted at a code offset, so they shift the text after them — which is why the hint spans carry
 * `data-lsp-hint`: `textSegments` skips them, and every offset the pointer code computes stays in code space
 * rather than counting hint characters.
 */

import { createContext, useContext, useMemo, type ReactNode } from 'react';
import type { LspInlayHint } from '../../lib/types';

/** A run of line content, either original text (with its syntax colour) or an inserted hint. */
export interface ContentPart {
  text: string;
  color?: string;
  hint?: boolean;
}

const EMPTY: LspInlayHint[] = [];
const InlayHintsContext = createContext<Map<number, LspInlayHint[]>>(new Map());

export function InlayHintsProvider(props: { hints: LspInlayHint[] | undefined; children: ReactNode }) {
  const { hints, children } = props;
  const byLine = useMemo(() => {
    const map = new Map<number, LspInlayHint[]>();
    for (const hint of hints ?? []) {
      const line = map.get(hint.line);
      if (line) {
        line.push(hint);
      } else {
        map.set(hint.line, [hint]);
      }
    }
    for (const line of map.values()) {
      line.sort((left, right) => left.character - right.character);
    }
    return map;
  }, [hints]);

  return <InlayHintsContext.Provider value={byLine}>{children}</InlayHintsContext.Provider>;
}

/** The hints for one 1-based line, in code order. */
export function useInlayHintsForLine(line: number): LspInlayHint[] {
  return useContext(InlayHintsContext).get(line) ?? EMPTY;
}

/**
 * Insert each hint into the line's parts at its character offset. Pure, and the only place hint text meets
 * rendered text.
 */
export function spliceHints(parts: ContentPart[], hints: LspInlayHint[]): ContentPart[] {
  if (hints.length === 0) {
    return parts;
  }
  const pending = [...hints].sort((left, right) => left.character - right.character);
  const out: ContentPart[] = [];
  let next = 0;
  // Character offset of the start of the part being walked.
  let cursor = 0;

  for (const part of parts) {
    let remaining = part.text;
    // Hints sitting at or before this part's start (or at the very start of the line).
    while (next < pending.length && pending[next].character <= cursor) {
      out.push({ text: pending[next].label, hint: true });
      next += 1;
    }
    while (next < pending.length && pending[next].character < cursor + remaining.length) {
      const at = pending[next].character - cursor;
      out.push({ text: remaining.slice(0, at), color: part.color });
      out.push({ text: pending[next].label, hint: true });
      remaining = remaining.slice(at);
      cursor = pending[next].character;
      next += 1;
    }
    if (remaining) {
      out.push({ text: remaining, color: part.color });
      cursor += remaining.length;
    }
  }

  // Anything left was positioned at or past the end of the line.
  while (next < pending.length) {
    out.push({ text: pending[next].label, hint: true });
    next += 1;
  }
  return out;
}
