import { useRef, useEffect, useCallback } from 'react';
import * as d3 from 'd3';
import { colors } from '../design-system/tokens';
import { CHROMOSOMES } from '../lib/constants';

interface ManhattanPoint {
  rsid: string;
  chromosome: string;
  position: number;
  pValue: number;
}

interface ManhattanPlotProps {
  data: ManhattanPoint[];
  width?: number;
  height?: number;
  threshold?: number;
  onPointClick?: (point: ManhattanPoint) => void;
}

const CHR_COLORS = [colors.accent, '#8B9DAF'];

export default function ManhattanPlot({
  data,
  width = 900,
  height = 320,
  threshold = 5e-8,
  onPointClick,
}: ManhattanPlotProps) {
  const svgRef = useRef<SVGSVGElement>(null);

  const render = useCallback(() => {
    const svg = d3.select(svgRef.current);
    svg.selectAll('*').remove();

    if (data.length === 0) return;

    const margin = { top: 16, right: 16, bottom: 40, left: 48 };
    const plotWidth = width - margin.left - margin.right;
    const plotHeight = height - margin.top - margin.bottom;

    const chrOrder = new Map(CHROMOSOMES.map((c, i) => [c, i]));
    const chrOffsets = new Map<string, number>();
    let cumOffset = 0;

    const grouped = d3.group(data, (d) => d.chromosome);
    for (const chr of CHROMOSOMES) {
      chrOffsets.set(chr, cumOffset);
      const chrData = grouped.get(chr);
      if (chrData) {
        const maxPos = d3.max(chrData, (d) => d.position) ?? 0;
        cumOffset += maxPos;
      }
    }

    const adjustedData = data.map((d) => ({
      ...d,
      adjustedPos: (chrOffsets.get(d.chromosome) ?? 0) + d.position,
      negLogP: -Math.log10(d.pValue),
      chrIndex: chrOrder.get(d.chromosome as typeof CHROMOSOMES[number]) ?? 0,
    }));

    const xScale = d3.scaleLinear()
      .domain([0, cumOffset])
      .range([0, plotWidth]);

    const maxY = d3.max(adjustedData, (d) => d.negLogP) ?? 10;
    const yScale = d3.scaleLinear()
      .domain([0, maxY * 1.1])
      .range([plotHeight, 0]);

    const g = svg
      .append('g')
      .attr('transform', `translate(${margin.left},${margin.top})`);

    // Y axis
    const yAxis = d3.axisLeft(yScale).ticks(5).tickSize(-plotWidth);
    g.append('g')
      .call(yAxis)
      .call((sel) => sel.select('.domain').remove())
      .call((sel) =>
        sel.selectAll('.tick line').attr('stroke', '#E8E6E3').attr('stroke-dasharray', '2,2'),
      )
      .call((sel) =>
        sel.selectAll('.tick text').attr('fill', '#6B6965').attr('font-size', '9px').attr('font-family', "'JetBrains Mono', monospace"),
      );

    // Y label
    g.append('text')
      .attr('transform', 'rotate(-90)')
      .attr('x', -plotHeight / 2)
      .attr('y', -36)
      .attr('text-anchor', 'middle')
      .attr('fill', '#6B6965')
      .attr('font-size', '9px')
      .attr('font-family', "'Inter', sans-serif")
      .text('-log\u2081\u2080(p)');

    // Chromosome labels
    for (const chr of CHROMOSOMES) {
      const chrData = grouped.get(chr);
      if (!chrData) continue;
      const offset = chrOffsets.get(chr) ?? 0;
      const maxPos = d3.max(chrData, (d) => d.position) ?? 0;
      const midX = xScale(offset + maxPos / 2);

      g.append('text')
        .attr('x', midX)
        .attr('y', plotHeight + 24)
        .attr('text-anchor', 'middle')
        .attr('fill', '#6B6965')
        .attr('font-size', '8px')
        .attr('font-family', "'JetBrains Mono', monospace")
        .text(chr);
    }

    // Threshold line
    const thresholdY = -Math.log10(threshold);
    if (thresholdY <= maxY * 1.1) {
      g.append('line')
        .attr('x1', 0)
        .attr('x2', plotWidth)
        .attr('y1', yScale(thresholdY))
        .attr('y2', yScale(thresholdY))
        .attr('stroke', colors.riskHigh)
        .attr('stroke-width', 1)
        .attr('stroke-dasharray', '4,3')
        .attr('opacity', 0.6);
    }

    // Points
    g.selectAll('circle')
      .data(adjustedData)
      .join('circle')
      .attr('cx', (d) => xScale(d.adjustedPos))
      .attr('cy', (d) => yScale(d.negLogP))
      .attr('r', 2)
      .attr('fill', (d) => CHR_COLORS[d.chrIndex % 2])
      .attr('opacity', 0.7)
      .style('cursor', onPointClick ? 'pointer' : 'default')
      .on('click', (_event, d) => {
        if (onPointClick) {
          onPointClick({
            rsid: d.rsid,
            chromosome: d.chromosome,
            position: d.position,
            pValue: d.pValue,
          });
        }
      });

    // Zoom behavior
    const zoom = d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([1, 20])
      .translateExtent([[0, 0], [plotWidth, plotHeight]])
      .extent([[0, 0], [plotWidth, plotHeight]])
      .on('zoom', (event) => {
        const newX = event.transform.rescaleX(xScale);
        g.selectAll('circle')
          .attr('cx', (d: any) => newX(d.adjustedPos));

        for (const chr of CHROMOSOMES) {
          const chrData = grouped.get(chr);
          if (!chrData) continue;
          const offset = chrOffsets.get(chr) ?? 0;
          const maxPos = d3.max(chrData, (d) => d.position) ?? 0;
          const midX = newX(offset + maxPos / 2);
          g.selectAll('text')
            .filter(function () {
              return d3.select(this).text() === chr;
            })
            .attr('x', midX);
        }
      });

    svg.call(zoom as any);
  }, [data, width, height, threshold, onPointClick]);

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
