interface WaffleDataItem {
  name: string;
  percentage: number;
  color: string;
}

interface WaffleChartProps {
  data: WaffleDataItem[];
}

export default function WaffleChart({ data }: WaffleChartProps) {
  const totalCells = 100;
  const cells: string[] = [];

  const sorted = [...data].sort((a, b) => b.percentage - a.percentage);
  for (const item of sorted) {
    const count = Math.round((item.percentage / 100) * totalCells);
    for (let i = 0; i < count && cells.length < totalCells; i++) {
      cells.push(item.color);
    }
  }
  while (cells.length < totalCells) {
    cells.push('#E8E6E3');
  }

  const cellSize = 18;
  const gap = 2;
  const cols = 10;
  const rows = 10;
  const svgWidth = cols * (cellSize + gap) - gap;
  const svgHeight = rows * (cellSize + gap) - gap;

  return (
    <div>
      <svg
        width={svgWidth}
        height={svgHeight}
        viewBox={`0 0 ${svgWidth} ${svgHeight}`}
        className="block"
      >
        {cells.map((color, i) => {
          const col = i % cols;
          const row = Math.floor(i / cols);
          return (
            <rect
              key={i}
              x={col * (cellSize + gap)}
              y={row * (cellSize + gap)}
              width={cellSize}
              height={cellSize}
              rx={1}
              fill={color}
            />
          );
        })}
      </svg>
      <div className="mt-4 flex flex-wrap gap-x-5 gap-y-1.5">
        {sorted.map((item) => (
          <div key={item.name} className="flex items-center gap-2 text-xs">
            <span
              className="w-2.5 h-2.5 rounded-sm inline-block"
              style={{ backgroundColor: item.color }}
            />
            <span className="text-text-muted">{item.name}</span>
            <span className="font-mono text-text">{item.percentage.toFixed(1)}%</span>
          </div>
        ))}
      </div>
    </div>
  );
}
