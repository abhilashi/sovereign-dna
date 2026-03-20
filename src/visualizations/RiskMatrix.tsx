import { useMemo } from 'react';
import { getRiskColor, type RiskLevel } from '../design-system/tokens';

interface RiskMatrixItem {
  condition: string;
  category: string;
  riskLevel: RiskLevel;
  score: number;
}

interface RiskMatrixProps {
  data: RiskMatrixItem[];
}

const RISK_LEVELS: RiskLevel[] = ['low', 'moderate', 'elevated', 'high'];
const RISK_LABELS: Record<RiskLevel, string> = {
  low: 'Low',
  moderate: 'Mod',
  elevated: 'Elev',
  high: 'High',
};

export default function RiskMatrix({ data }: RiskMatrixProps) {
  const categories = useMemo(() => {
    const cats = [...new Set(data.map((d) => d.category))];
    return cats.sort();
  }, [data]);

  const matrix = useMemo(() => {
    const m: Record<string, Record<RiskLevel, RiskMatrixItem[]>> = {};
    for (const cat of categories) {
      m[cat] = { low: [], moderate: [], elevated: [], high: [] };
    }
    for (const item of data) {
      m[item.category]?.[item.riskLevel]?.push(item);
    }
    return m;
  }, [data, categories]);

  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse">
        <thead>
          <tr>
            <th className="text-left text-[10px] uppercase tracking-wider text-text-muted font-semibold pb-2 pr-4 w-32">
              Category
            </th>
            {RISK_LEVELS.map((level) => (
              <th
                key={level}
                className="text-center text-[10px] uppercase tracking-wider font-semibold pb-2 px-2"
                style={{ color: getRiskColor(level) }}
              >
                {RISK_LABELS[level]}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {categories.map((category) => (
            <tr key={category} className="border-t border-border">
              <td className="text-xs text-text py-2 pr-4">{category}</td>
              {RISK_LEVELS.map((level) => {
                const items = matrix[category]?.[level] ?? [];
                const color = getRiskColor(level);
                return (
                  <td key={level} className="py-2 px-2 text-center">
                    {items.length > 0 ? (
                      <div className="flex flex-wrap justify-center gap-1">
                        {items.map((item) => (
                          <div
                            key={item.condition}
                            className="w-4 h-4 rounded-sm"
                            style={{
                              backgroundColor: color,
                              opacity: 0.3 + item.score * 0.7,
                            }}
                            title={`${item.condition}: ${(item.score * 100).toFixed(0)}%`}
                          />
                        ))}
                      </div>
                    ) : (
                      <span className="text-[10px] text-text-muted">&mdash;</span>
                    )}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
