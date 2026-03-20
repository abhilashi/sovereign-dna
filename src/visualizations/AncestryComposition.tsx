import { colors } from '../design-system/tokens';
import WaffleChart from '../design-system/components/WaffleChart';

interface AncestrySegment {
  name: string;
  percentage: number;
  color: string;
}

interface AncestryCompositionProps {
  data: AncestrySegment[];
}

const PALETTE = [
  colors.ancestry.ochre,
  colors.ancestry.terracotta,
  colors.ancestry.forest,
  colors.ancestry.slate,
  colors.ancestry.rust,
  '#8B7355',
  '#6B8E9B',
  '#9B7B6B',
  '#7B8F6B',
  '#9B6B8E',
];

export default function AncestryComposition({ data }: AncestryCompositionProps) {
  const sorted = [...data].sort((a, b) => b.percentage - a.percentage);

  const waffleData = sorted.map((d, i) => ({
    name: d.name,
    percentage: d.percentage,
    color: d.color || PALETTE[i % PALETTE.length],
  }));

  const barTotal = sorted.reduce((sum, d) => sum + d.percentage, 0);

  return (
    <div className="space-y-6">
      {/* Stacked horizontal bar */}
      <div>
        <p className="text-xs uppercase tracking-wider text-text-muted font-semibold mb-3">
          Composition
        </p>
        <div className="flex h-6 rounded-sm overflow-hidden">
          {sorted.map((segment, i) => (
            <div
              key={segment.name}
              className="relative group"
              style={{
                width: `${(segment.percentage / barTotal) * 100}%`,
                backgroundColor: segment.color || PALETTE[i % PALETTE.length],
              }}
            >
              <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 hidden group-hover:block z-10">
                <div className="bg-text text-surface text-[10px] px-2 py-1 rounded-sm whitespace-nowrap font-mono">
                  {segment.name}: {segment.percentage.toFixed(1)}%
                </div>
              </div>
            </div>
          ))}
        </div>
        <div className="flex flex-wrap gap-x-4 gap-y-1 mt-2">
          {sorted.map((segment, i) => (
            <div key={segment.name} className="flex items-center gap-1.5 text-[10px]">
              <span
                className="w-2 h-2 rounded-sm"
                style={{ backgroundColor: segment.color || PALETTE[i % PALETTE.length] }}
              />
              <span className="text-text-muted">{segment.name}</span>
              <span className="font-mono text-text">{segment.percentage.toFixed(1)}%</span>
            </div>
          ))}
        </div>
      </div>

      {/* Waffle chart */}
      <div>
        <p className="text-xs uppercase tracking-wider text-text-muted font-semibold mb-3">
          Proportional Grid
        </p>
        <WaffleChart data={waffleData} />
      </div>

      {/* Breakdown */}
      <div>
        <p className="text-xs uppercase tracking-wider text-text-muted font-semibold mb-3">
          Breakdown
        </p>
        <div className="space-y-1.5">
          {sorted.map((segment, i) => (
            <div key={segment.name} className="flex items-center gap-3">
              <span
                className="w-2 h-2 rounded-sm shrink-0"
                style={{ backgroundColor: segment.color || PALETTE[i % PALETTE.length] }}
              />
              <span className="text-xs text-text flex-1">{segment.name}</span>
              <div className="w-24 h-1 bg-border rounded-full overflow-hidden">
                <div
                  className="h-full rounded-full"
                  style={{
                    width: `${segment.percentage}%`,
                    backgroundColor: segment.color || PALETTE[i % PALETTE.length],
                  }}
                />
              </div>
              <span className="text-xs font-mono text-text w-12 text-right">
                {segment.percentage.toFixed(1)}%
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
