import { useRef, useEffect, useCallback } from 'react';
import * as d3 from 'd3';

interface TraitDistributionProps {
  mean: number;
  stdDev: number;
  userValue: number;
  label: string;
  unit?: string;
  width?: number;
  height?: number;
}

export default function TraitDistribution({
  mean,
  stdDev,
  userValue,
  label,
  unit = '',
  width = 300,
  height = 120,
}: TraitDistributionProps) {
  const svgRef = useRef<SVGSVGElement>(null);

  const render = useCallback(() => {
    const svg = d3.select(svgRef.current);
    svg.selectAll('*').remove();

    const margin = { top: 20, right: 16, bottom: 28, left: 16 };
    const plotWidth = width - margin.left - margin.right;
    const plotHeight = height - margin.top - margin.bottom;

    const g = svg
      .append('g')
      .attr('transform', `translate(${margin.left},${margin.top})`);

    // Generate normal distribution curve
    const xMin = mean - 3.5 * stdDev;
    const xMax = mean + 3.5 * stdDev;
    const xScale = d3.scaleLinear().domain([xMin, xMax]).range([0, plotWidth]);

    const normalPdf = (x: number) => {
      const z = (x - mean) / stdDev;
      return (1 / (stdDev * Math.sqrt(2 * Math.PI))) * Math.exp(-0.5 * z * z);
    };

    const points: [number, number][] = [];
    const steps = 100;
    for (let i = 0; i <= steps; i++) {
      const x = xMin + (i / steps) * (xMax - xMin);
      points.push([x, normalPdf(x)]);
    }

    const maxY = d3.max(points, (d) => d[1]) ?? 1;
    const yScale = d3.scaleLinear().domain([0, maxY * 1.15]).range([plotHeight, 0]);

    // Area under curve
    const area = d3.area<[number, number]>()
      .x((d) => xScale(d[0]))
      .y0(plotHeight)
      .y1((d) => yScale(d[1]))
      .curve(d3.curveBasis);

    g.append('path')
      .datum(points)
      .attr('d', area)
      .attr('fill', '#2D5F8A')
      .attr('fill-opacity', 0.08);

    // Curve line
    const line = d3.line<[number, number]>()
      .x((d) => xScale(d[0]))
      .y((d) => yScale(d[1]))
      .curve(d3.curveBasis);

    g.append('path')
      .datum(points)
      .attr('d', line)
      .attr('fill', 'none')
      .attr('stroke', '#2D5F8A')
      .attr('stroke-width', 1.5)
      .attr('opacity', 0.6);

    // Baseline
    g.append('line')
      .attr('x1', 0)
      .attr('x2', plotWidth)
      .attr('y1', plotHeight)
      .attr('y2', plotHeight)
      .attr('stroke', '#E8E6E3')
      .attr('stroke-width', 1);

    // User marker
    const userX = xScale(userValue);
    const userY = yScale(normalPdf(userValue));

    g.append('line')
      .attr('x1', userX)
      .attr('x2', userX)
      .attr('y1', plotHeight)
      .attr('y2', userY)
      .attr('stroke', '#A94442')
      .attr('stroke-width', 1.5)
      .attr('stroke-dasharray', '3,2');

    g.append('circle')
      .attr('cx', userX)
      .attr('cy', userY)
      .attr('r', 3.5)
      .attr('fill', '#A94442');

    // User label
    g.append('text')
      .attr('x', userX)
      .attr('y', userY - 8)
      .attr('text-anchor', 'middle')
      .attr('fill', '#A94442')
      .attr('font-size', '9px')
      .attr('font-family', "'JetBrains Mono', monospace")
      .attr('font-weight', '600')
      .text(`${userValue}${unit}`);

    // Title
    svg
      .append('text')
      .attr('x', margin.left)
      .attr('y', 12)
      .attr('fill', '#1A1A1A')
      .attr('font-size', '10px')
      .attr('font-family', "'Inter', sans-serif")
      .attr('font-weight', '600')
      .text(label);

    // X axis ticks (just mean and +/- 2sd)
    const ticks = [mean - 2 * stdDev, mean, mean + 2 * stdDev];
    for (const tick of ticks) {
      const tx = xScale(tick);
      if (tx < 0 || tx > plotWidth) continue;
      g.append('line')
        .attr('x1', tx)
        .attr('x2', tx)
        .attr('y1', plotHeight)
        .attr('y2', plotHeight + 4)
        .attr('stroke', '#6B6965');

      g.append('text')
        .attr('x', tx)
        .attr('y', plotHeight + 14)
        .attr('text-anchor', 'middle')
        .attr('fill', '#6B6965')
        .attr('font-size', '8px')
        .attr('font-family', "'JetBrains Mono', monospace")
        .text(tick.toFixed(1));
    }
  }, [mean, stdDev, userValue, label, unit, width, height]);

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
