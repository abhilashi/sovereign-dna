import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Snp {
  rsid: string;
  chromosome: string;
  position: number;
  genotype: string;
}

interface SnpResponse {
  rows: Snp[];
  total: number;
  offset: number;
  limit: number;
}

interface UseSnpDataResult {
  snps: Snp[];
  total: number;
  loading: boolean;
  error: string | null;
  page: number;
  pageSize: number;
  setPage: (page: number) => void;
  setSearch: (search: string) => void;
  setChromosomeFilter: (chromosome: string | null) => void;
  search: string;
  chromosomeFilter: string | null;
}

export function useSnpData(genomeId: number | null, pageSize = 100): UseSnpDataResult {
  const [snps, setSnps] = useState<Snp[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [search, setSearch] = useState('');
  const [chromosomeFilter, setChromosomeFilter] = useState<string | null>(null);

  const fetchSnps = useCallback(async () => {
    if (genomeId === null) return;

    setLoading(true);
    setError(null);
    try {
      const result = await invoke<SnpResponse>('get_snps', {
        genomeId,
        offset: page * pageSize,
        limit: pageSize,
        search: search || null,
        chromosome: chromosomeFilter,
      });
      setSnps(result.rows);
      setTotal(result.total);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [genomeId, page, pageSize, search, chromosomeFilter]);

  useEffect(() => {
    fetchSnps();
  }, [fetchSnps]);

  const handleSetSearch = useCallback((value: string) => {
    setSearch(value);
    setPage(0);
  }, []);

  const handleSetChromosomeFilter = useCallback((value: string | null) => {
    setChromosomeFilter(value);
    setPage(0);
  }, []);

  return {
    snps,
    total,
    loading,
    error,
    page,
    pageSize,
    setPage,
    setSearch: handleSetSearch,
    setChromosomeFilter: handleSetChromosomeFilter,
    search,
    chromosomeFilter,
  };
}
