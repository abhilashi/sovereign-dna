export function formatNumber(value: number): string {
  return value.toLocaleString('en-US');
}

export function formatPercentage(value: number, decimals = 1): string {
  return `${(value * 100).toFixed(decimals)}%`;
}

export function formatRsid(rsid: string): string {
  if (rsid.startsWith('rs')) return rsid;
  return `rs${rsid}`;
}

export function formatChromosome(chr: string): string {
  const cleaned = chr.replace(/^chr/i, '');
  if (cleaned === '23') return 'X';
  if (cleaned === '24') return 'Y';
  if (cleaned === '25') return 'MT';
  return cleaned;
}

export function formatGenotype(genotype: string): string {
  if (genotype.length === 2) {
    return `${genotype[0]}/${genotype[1]}`;
  }
  return genotype;
}

export function formatDate(dateString: string): string {
  const date = new Date(dateString);
  return date.toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

export function formatCompactNumber(value: number): string {
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(1)}M`;
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(1)}K`;
  }
  return value.toString();
}

export function formatOddsRatio(or: number): string {
  return or.toFixed(2);
}
