/**
 * The hover card: a symbol's signature and doc comment, rendered from the markdown a language server sends.
 *
 * The subset gopls emits is small — a fenced signature block, the doc text, `---` dividers, and markdown links
 * — so this parses exactly that instead of pulling in a markdown renderer.
 */

import type { RefObject } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { HoverState } from './use-symbol-hover';

export type HoverBlock =
  | { kind: 'code'; language: string; text: string }
  | { kind: 'text'; text: string }
  | { kind: 'rule' };

export function parseHoverMarkdown(value: string): HoverBlock[] {
  const blocks: HoverBlock[] = [];
  let buffer: string[] = [];
  let fence: { language: string; lines: string[] } | null = null;

  const flush = () => {
    const text = buffer.join('\n').trim();
    if (text) {
      blocks.push({ kind: 'text', text });
    }
    buffer = [];
  };

  for (const line of value.split('\n')) {
    if (line.startsWith('```')) {
      if (fence) {
        blocks.push({ kind: 'code', language: fence.language, text: fence.lines.join('\n') });
        fence = null;
      } else {
        flush();
        fence = { language: line.slice(3).trim(), lines: [] };
      }
      continue;
    }
    if (fence) {
      fence.lines.push(line);
      continue;
    }
    if (/^\s*---\s*$/.test(line)) {
      flush();
      blocks.push({ kind: 'rule' });
      continue;
    }
    buffer.push(line);
  }
  if (fence) {
    blocks.push({ kind: 'code', language: fence.language, text: fence.lines.join('\n') });
  }
  flush();

  // A leading or trailing divider carries no information.
  return blocks.filter((block, index) => !(block.kind === 'rule' && (index === 0 || index === blocks.length - 1)));
}

/** Text with `[label](url)` spans turned into links; the doc comment's pkg.go.dev link arrives that way. */
function renderText(text: string) {
  return text.split(/(\[[^\]]+\]\([^)]+\))/g).map((part, index) => {
    const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(part);
    if (!link) {
      return <span key={index}>{part}</span>;
    }
    return (
      <button
        key={index}
        type="button"
        className="cursor-pointer text-left underline decoration-dotted underline-offset-2 hover:text-accent"
        onClick={() => void openUrl(link[2]).catch(() => {})}
      >
        {link[1]}
      </button>
    );
  });
}

export function HoverCard(props: { card: HoverState | null; cardRef: RefObject<HTMLDivElement | null> }) {
  const { card, cardRef } = props;
  if (!card) {
    return null;
  }

  const width = 520;
  const blocks = parseHoverMarkdown(card.hover.value);
  // Pinned to the symbol's box rather than the cursor, and kept inside the viewport. Anchored by whichever
  // edge the hook chose, so a card above the symbol needs no height measurement first.
  const left = Math.max(8, Math.min(card.anchor.left, window.innerWidth - width - 16));
  const vertical =
    'top' in card.anchor
      ? { top: card.anchor.top, maxHeight: window.innerHeight - card.anchor.top - 16 }
      : { bottom: card.anchor.bottom, maxHeight: card.anchor.bottom - 16 };

  return (
    <div
      ref={cardRef}
      data-lsp-card
      className="fixed z-50 flex flex-col gap-1.5 rounded-lg border border-overlay-border bg-overlay px-3 py-2 text-xs text-text shadow-lg"
      style={{ left, ...vertical, maxWidth: width, overflow: 'hidden' }}
    >
      {blocks.map((block, index) => {
        if (block.kind === 'rule') {
          return <div key={index} className="border-t border-overlay-border" />;
        }
        if (block.kind === 'code') {
          return (
            <pre key={index} className="overflow-hidden font-mono text-[12px] whitespace-pre-wrap text-text">
              {block.text}
            </pre>
          );
        }
        return (
          <div key={index} className="whitespace-pre-wrap text-text-secondary">
            {renderText(block.text)}
          </div>
        );
      })}
    </div>
  );
}
