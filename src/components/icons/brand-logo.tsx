import type { SVGProps } from 'react';
import { cn } from '../../lib/cn';

/**
 * The Diffract mark: a beam entering a prism and leaving as three rays, carrying the colour fringing a lens
 * would leave on it. The offsets are in mark units (1024 of them), so below roughly 64px they fall under a
 * pixel and the silhouette carries the small sizes on its own.
 */
const MARK_VIEW_BOX = '0 0 1024 1024';
const MARK_PATHS = [
  'M512 176 694 816 330 816Z',
  'M124 472h376v80h-376Z',
  'M560 428 884 228v76L560 504Z',
  'M560 480h328v64H560Z',
  'M560 568 884 720v76L560 632Z',
];

/** Fringe offset at mark scale; the axes are the mark's own 1024-unit grid. */
const FRINGE = 22;

export function BrandLogo(props: SVGProps<SVGSVGElement>) {
  const { className, ...rest } = props;

  return (
    <svg
      viewBox={MARK_VIEW_BOX}
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
      className={cn('text-text', className)}
      {...rest}
    >
      <g fill="#ff2d55" transform={`translate(${-FRINGE} ${-FRINGE / 2})`}>
        {MARK_PATHS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
      <g fill="#00b0ff" transform={`translate(${FRINGE} ${FRINGE / 2})`}>
        {MARK_PATHS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
      <g fill="currentColor">
        {MARK_PATHS.map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
    </svg>
  );
}
