import { useState, useEffect, useMemo } from 'react';
import {
  getGenomeLayout, getAnalysisOverlay,
  getHealthRisks, getPharmacogenomics, getTraits, getCarrierStatus,
} from '../lib/tauri-bridge';
import type {
  GenomeLayout, OverlayMarker,
  HealthRiskResult, PharmaResult, TraitResult, CarrierResult,
} from '../lib/tauri-bridge';
import type { KaryogramFinding } from '../visualizations/CompactKaryogram';
import { LAYER_COLORS } from '../lib/constants';

export function useKaryogramData(genomeId: number | null) {
  const [layout, setLayout] = useState<GenomeLayout | null>(null);
  const [markers, setMarkers] = useState<OverlayMarker[]>([]);
  const [healthRisks, setHealthRisks] = useState<HealthRiskResult[]>([]);
  const [pharma, setPharma] = useState<PharmaResult[]>([]);
  const [traits, setTraits] = useState<TraitResult[]>([]);
  const [carrier, setCarrier] = useState<CarrierResult[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    if (!genomeId) {
      if (!cancelled) setLoading(false);
      return;
    }

    setLoading(true);

    Promise.all([
      getGenomeLayout(genomeId),
      getAnalysisOverlay(genomeId),
      getHealthRisks(genomeId).catch(() => []),
      getPharmacogenomics(genomeId).catch(() => []),
      getTraits(genomeId).catch(() => []),
      getCarrierStatus(genomeId).catch(() => []),
    ]).then(([layoutData, overlayData, healthData, pharmaData, traitData, carrierData]) => {
      if (cancelled) return;
      setLayout(layoutData);
      setMarkers(overlayData);
      setHealthRisks(healthData);
      setPharma(pharmaData);
      setTraits(traitData);
      setCarrier(carrierData);
      setLoading(false);
    }).catch(() => {
      if (!cancelled) setLoading(false);
    });

    return () => { cancelled = true; };
  }, [genomeId]);

  // Build findings from analysis data
  const findings = useMemo((): KaryogramFinding[] => {
    const result: KaryogramFinding[] = [];

    // Find marker position for a given rsid or gene
    const findMarkerPosition = (rsid?: string, gene?: string) => {
      if (rsid) {
        const m = markers.find((mk) => mk.rsid === rsid);
        if (m) return { chromosome: m.chromosome, position: m.position };
      }
      if (gene) {
        const m = markers.find((mk) => mk.label === gene);
        if (m) return { chromosome: m.chromosome, position: m.position };
      }
      return { chromosome: null, position: null };
    };

    for (const risk of healthRisks) {
      const topSnp = risk.contributingSnps[0];
      const pos = findMarkerPosition(topSnp?.rsid, topSnp?.gene);
      result.push({
        id: `health-${risk.condition}`,
        category: 'health',
        title: risk.condition,
        subtitle: topSnp?.gene || risk.category,
        detail: topSnp
          ? `${topSnp.rsid} ${topSnp.genotype} — ${topSnp.effect}`
          : `${risk.category} risk assessment`,
        chromosome: pos.chromosome,
        position: pos.position,
        rsid: topSnp?.rsid || null,
        significance: risk.riskLevel,
        color: LAYER_COLORS.health,
      });
    }

    for (const p of pharma) {
      const pos = findMarkerPosition(undefined, p.gene);
      const topDrug = p.affectedDrugs[0];
      result.push({
        id: `pharma-${p.gene}`,
        category: 'pharma',
        title: `${p.gene} — ${p.phenotype} metabolizer`,
        subtitle: p.starAllele,
        detail: topDrug
          ? `Affects ${p.affectedDrugs.map((d) => d.name).join(', ')}`
          : `Clinical actionability: ${p.clinicalActionability}`,
        chromosome: pos.chromosome,
        position: pos.position,
        rsid: null,
        significance: p.phenotype,
        color: LAYER_COLORS.pharma,
      });
    }

    for (const t of traits) {
      const topSnp = t.contributingSnps[0];
      const pos = findMarkerPosition(topSnp?.rsid);
      result.push({
        id: `trait-${t.name}`,
        category: 'traits',
        title: t.name,
        subtitle: t.prediction,
        detail: topSnp
          ? `${topSnp.rsid} ${topSnp.genotype} — ${t.description}`
          : t.description,
        chromosome: pos.chromosome,
        position: pos.position,
        rsid: topSnp?.rsid || null,
        significance: `${Math.round(t.confidence * 100)}% confidence`,
        color: LAYER_COLORS.traits,
      });
    }

    for (const c of carrier) {
      const topVariant = c.variantsChecked[0];
      const pos = findMarkerPosition(topVariant?.rsid, c.gene);
      result.push({
        id: `carrier-${c.condition}`,
        category: 'carrier',
        title: c.condition,
        subtitle: `${c.gene} — ${c.status === 'not_carrier' ? 'Not a carrier' : c.status === 'carrier' ? 'Carrier' : 'Affected'}`,
        detail: topVariant
          ? `${topVariant.rsid} ${topVariant.genotype} · ${c.inheritancePattern}`
          : c.description,
        chromosome: pos.chromosome,
        position: pos.position,
        rsid: topVariant?.rsid || null,
        significance: c.status,
        color: LAYER_COLORS.carrier,
      });
    }

    return result;
  }, [healthRisks, pharma, traits, carrier, markers]);

  const findingsWithPositions = useMemo(
    () => findings.filter((f) => f.chromosome !== null),
    [findings],
  );

  return {
    layout,
    findings,
    findingsWithPositions,
    healthRisks,
    pharma,
    traits,
    carrier,
    loading,
  };
}
