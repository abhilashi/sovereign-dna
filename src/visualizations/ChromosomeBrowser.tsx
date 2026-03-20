import { useRef, useEffect, useCallback } from 'react';
import * as d3 from 'd3';

interface SnpPosition {
  rsid: string;
  position: number;
  gene?: string;
  highlight?: boolean;
}

interface ChromosomeBrowserProps {
  chromosome: string;
  length: number;
  snps: SnpPosition[];
  width?: number;
  height?: number;
  onSnpClick?: (snp: SnpPosition) => void;
  onRegionClick?: (start: number, end: number) => void;
}

export default function ChromosomeBrowser({
  chromosome,
  length,
  snps,
  width = 800,
  height = 120,
  onSnpClick,
  onRegionClick,
}: ChromosomeBrowserProps) {
  const svgRef = useRef<SVGSVGElement>(null);

  const render = useCallback(() => {
    const svg = d3.select(svgRef.current);
    svg.selectAll('*').remove();

    const margin = { top: 24, right: 20, bottom: 32, left: 20 };
    const plotWidth = width - margin.left - margin.right;
    const plotHeight = height - margin.top - margin.bottom;

    const g = svg
      .append('g')
      .attr('transform', `translate(${margin.left},${margin.top})`);

    const xScale = d3.scaleLinear().domain([0, length]).range([0, plotWidth]);

    // Chromosome body
    const chrHeight = 16;
    const chrY = plotHeight / 2 - chrHeight / 2;

    g.append('rect')
      .attr('x', 0)
      .attr('y', chrY)
      .attr('width', plotWidth)
      .attr('height', chrHeight)
      .attr('rx', chrHeight / 2)
      .attr('fill', '#E8E6E3');

    // Centromere (approximate at 40% for visual)
    const centromereX = plotWidth * 0.4;
    g.append('ellipse')
      .attr('cx', centromereX)
      .attr('cy', chrY + chrHeight / 2)
      .attr('rx', 4)
      .attr('ry', chrHeight / 2)
      .attr('fill', '#D0CEC9');

    // SNP markers
    g.selectAll('.snp-marker')
      .data(snps)
      .join('line')
      .attr('class', 'snp-marker')
      .attr('x1', (d) => xScale(d.position))
      .attr('x2', (d) => xScale(d.position))
      .attr('y1', chrY - 4)
      .attr('y2', chrY + chrHeight + 4)
      .attr('stroke', (d) => (d.highlight ? '#A94442' : '#2D5F8A'))
      .attr('stroke-width', (d) => (d.highlight ? 1.5 : 0.5))
      .attr('opacity', (d) => (d.highlight ? 0.9 : 0.4))
      .style('cursor', onSnpClick ? 'pointer' : 'default')
      .on('click', (_event, d) => {
        if (onSnpClick) onSnpClick(d);
      })
      .append('title')
      .text((d) => `${d.rsid}${d.gene ? ` (${d.gene})` : ''} - pos: ${d.position.toLocaleString()}`);

    // Position axis
    const axisScale = d3.axisBottom(xScale)
      .ticks(8)
      .tickFormat((d) => {
        const val = d as number;
        if (val >= 1e6) return `${(val / 1e6).toFixed(1)}M`;
        if (val >= 1e3) return `${(val / 1e3).toFixed(0)}K`;
        return String(val);
      });

    g.append('g')
      .attr('transform', `translate(0,${chrY + chrHeight + 12})`)
      .call(axisScale)
      .call((sel) => sel.select('.domain').attr('stroke', '#E8E6E3'))
      .call((sel) =>
        sel.selectAll('.tick line').attr('stroke', '#E8E6E3'),
      )
      .call((sel) =>
        sel.selectAll('.tick text')
          .attr('fill', '#6B6965')
          .attr('font-size', '8px')
          .attr('font-family', "'JetBrains Mono', monospace"),
      );

    // Label
    g.append('text')
      .attr('x', 0)
      .attr('y', -8)
      .attr('fill', '#1A1A1A')
      .attr('font-size', '11px')
      .attr('font-family', "'JetBrains Mono', monospace")
      .attr('font-weight', '600')
      .text(`Chromosome ${chromosome}`);

    g.append('text')
      .attr('x', plotWidth)
      .attr('y', -8)
      .attr('text-anchor', 'end')
      .attr('fill', '#6B6965')
      .attr('font-size', '9px')
      .attr('font-family', "'JetBrains Mono', monospace")
      .text(`${snps.length} SNPs`);

    // Brush for region selection
    if (onRegionClick) {
      const brush = d3.brushX<unknown>()
        .extent([[0, chrY - 8], [plotWidth, chrY + chrHeight + 8]])
        .on('end', (event) => {
          if (!event.selection) return;
          const [x0, x1] = event.selection as [number, number];
          const start = Math.round(xScale.invert(x0));
          const end = Math.round(xScale.invert(x1));
          onRegionClick(start, end);
          (g.select('.brush') as any).call(brush.move, null);
        });

      g.append('g')
        .attr('class', 'brush')
        .call(brush)
        .selectAll('.selection')
        .attr('fill', '#2D5F8A')
        .attr('fill-opacity', 0.1)
        .attr('stroke', '#2D5F8A');
    }
  }, [chromosome, length, snps, width, height, onSnpClick, onRegionClick]);

  useEffect(() => {
    render();
  }, [render]);

  return (
    <svg
      ref={svgRef}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className="block"
      preserveAspectRatio="xMinYMin meet"
    />
  );
}
