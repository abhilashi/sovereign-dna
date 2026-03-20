import { useRef } from 'react';
import * as Plot from '@observablehq/plot';
import { usePlot } from '../../hooks/usePlot';
import { colors } from '../tokens';

interface SparklineProps {
  data: number[];
  width?: number;
  height?: number;
  color?: string;
}

export default function Sparkline({
  data,
  width = 120,
  height = 32,
  color = colors.accent,
}: SparklineProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  usePlot(
    containerRef,
    () =>
      Plot.plot({
        width,
        height,
        axis: null,
        margin: 0,
        marginTop: 2,
        marginBottom: 2,
        marginLeft: 0,
        marginRight: 0,
        x: { axis: null },
        y: { axis: null, domain: [Math.min(...data) * 0.9, Math.max(...data) * 1.1] },
        marks: [
          Plot.line(data.map((d, i) => [i, d]), {
            stroke: color,
            strokeWidth: 1.5,
            curve: 'natural',
          }),
        ],
      }),
    [data, width, height, color],
  );

  return <div ref={containerRef} className="inline-block" />;
}
