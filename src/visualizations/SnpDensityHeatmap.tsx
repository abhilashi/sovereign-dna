import { useRef } from 'react';
import * as Plot from '@observablehq/plot';
import { usePlot } from '../hooks/usePlot';
import { CHROMOSOMES } from '../lib/constants';

interface DensityBin {
  chromosome: string;
  binStart: number;
  binEnd: number;
  count: number;
}

interface SnpDensityHeatmapProps {
  data: DensityBin[];
  width?: number;
  height?: number;
}

export default function SnpDensityHeatmap({
  data,
  width = 800,
  height = 400,
}: SnpDensityHeatmapProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  usePlot(
    containerRef,
    () => {
      if (data.length === 0) {
        const emptyDiv = document.createElement('div');
        emptyDiv.textContent = 'No density data available';
        emptyDiv.style.color = '#6B6965';
        emptyDiv.style.fontSize = '12px';
        emptyDiv.style.padding = '20px';
        return emptyDiv as HTMLElement & { remove?: () => void };
      }

      const maxCount = Math.max(...data.map((d) => d.count));

      // Assign a numeric row index per chromosome
      const chrIndex = new Map(CHROMOSOMES.map((c: string, i: number) => [c, i]));

      const plotData = data.map((d) => ({
        ...d,
        chrY: chrIndex.get(d.chromosome) ?? 0,
        binMid: (d.binStart + d.binEnd) / 2,
        normalized: d.count / maxCount,
      }));

      return Plot.plot({
        width,
        height,
        marginLeft: 40,
        marginBottom: 40,
        marginTop: 16,
        marginRight: 16,
        x: {
          label: 'Position (Mb)',
          transform: (d: number) => d / 1e6,
          axis: 'bottom',
        },
        y: {
          domain: CHROMOSOMES.filter((c) => data.some((d) => d.chromosome === c)),
          label: null,
          padding: 0.1,
        },
        color: {
          type: 'sequential',
          scheme: 'blues',
          domain: [0, maxCount],
          label: 'SNP Count',
        },
        marks: [
          Plot.cell(plotData, {
            x: 'binMid',
            y: 'chromosome',
            fill: 'count',
            inset: 0.5,
            tip: true,
            title: (d: typeof plotData[number]) =>
              `Chr ${d.chromosome}\n${(d.binStart / 1e6).toFixed(1)}-${(d.binEnd / 1e6).toFixed(1)} Mb\n${d.count} SNPs`,
          }),
          Plot.frame({ stroke: '#E8E6E3' }),
        ],
        style: {
          fontFamily: "'JetBrains Mono', monospace",
          fontSize: '9px',
          color: '#6B6965',
          background: 'transparent',
        },
      });
    },
    [data, width, height],
  );

  return (
    <div>
      <p className="text-xs uppercase tracking-wider text-text-muted font-semibold mb-3">
        SNP Density Across Chromosomes
      </p>
      <div ref={containerRef} className="overflow-x-auto" />
    </div>
  );
}
