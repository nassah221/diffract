import { useState } from 'react';
import type { DiffLine } from '@/lib/diff-parser';
import type { LspInlayHint } from '@/lib/types';
import { WordDiff } from '../components/diff/word-diff';
import { spliceHints, type ContentPart } from '../features/lsp/inlay-hints';
import type { SyntaxToken } from '../lib/syntax-token';

/** Lines longer than this (minified code) show their start and a button for the rest. */
const TRUNCATE_AT = 1000;

function TruncatedLine(props: { content: string }) {
  const { content } = props;
  const [expanded, setExpanded] = useState(false);

  if (expanded) {
    return <span>{content}</span>;
  }
  const rest = content.length - TRUNCATE_AT;
  return (
    <>
      <span>{content.slice(0, TRUNCATE_AT)}</span>
      <button
        type="button"
        onMouseDown={(event) => event.stopPropagation()}
        onClick={(event) => {
          event.stopPropagation();
          setExpanded(true);
        }}
        className="ml-1.5 inline-flex h-[18px] items-center rounded px-1.5 align-baseline font-sans text-[11px] text-text-secondary bg-fill hover:bg-fill-hover hover:text-text cursor-pointer select-none"
        title="The rest of this line is hidden to keep the diff fast"
      >
        … {rest.toLocaleString()} more characters · Show all
      </button>
    </>
  );
}

export function renderContent(line: DiffLine, syntaxTokens?: SyntaxToken[], hints?: LspInlayHint[]) {
  if (line.wordDiff && line.wordDiff.length > 0) {
    // Word-diff re-renders the line as its own parts, so hint offsets would not line up; hints are left off.
    return <WordDiff line={line} syntaxTokens={syntaxTokens} />;
  }

  if (line.content.length > TRUNCATE_AT) {
    return <TruncatedLine content={line.content} />;
  }

  const parts: ContentPart[] = syntaxTokens && syntaxTokens.length > 0
    ? syntaxTokens.map((token) => ({ text: token.text, color: token.color }))
    : [{ text: line.content || '\n' }];

  const withHints = hints && hints.length > 0 ? spliceHints(parts, hints) : parts;
  return (
    <>
      {withHints.map((part, i) =>
        part.hint ? (
          // `data-lsp-hint` keeps this text out of every offset computation: it is not part of the line.
          <span key={i} data-lsp-hint className="pl-px font-sans text-[11px] italic text-text-muted select-none">
            {part.text}
          </span>
        ) : (
          <span key={i} style={part.color ? { color: part.color } : undefined}>
            {part.text}
          </span>
        ),
      )}
    </>
  );
}
