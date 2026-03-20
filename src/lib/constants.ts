export const CHROMOSOMES = [
  '1', '2', '3', '4', '5', '6', '7', '8', '9', '10',
  '11', '12', '13', '14', '15', '16', '17', '18', '19', '20',
  '21', '22', 'X', 'Y', 'MT',
] as const;

export const RISK_LEVELS = ['low', 'moderate', 'elevated', 'high'] as const;

export const HEALTH_CATEGORIES = [
  'All',
  'Cardiovascular',
  'Neurological',
  'Metabolic',
  'Oncological',
  'Autoimmune',
  'Musculoskeletal',
  'Other',
] as const;

export const TRAIT_CATEGORIES = [
  'Physical',
  'Behavioral',
  'Athletic',
  'Sensory',
] as const;

export const DRUG_CATEGORIES = [
  'Analgesics',
  'Cardiovascular',
  'Psychiatry',
  'Oncology',
  'Anticoagulants',
  'Other',
] as const;

export const METABOLIZER_STATUS = [
  'ultrarapid',
  'normal',
  'intermediate',
  'poor',
] as const;

export const NAV_ITEMS = [
  { label: 'Research', path: '/ask' },
  { label: 'Dashboard', path: '/' },
  { label: 'Genome Map', path: '/map' },
  { label: 'Import', path: '/import' },
  { label: 'Health Risks', path: '/health' },
  { label: 'Pharmacogenomics', path: '/pharma' },
  { label: 'Traits', path: '/traits' },
  { label: 'Ancestry', path: '/ancestry' },
  { label: 'Carrier Status', path: '/carrier' },
  { label: 'Research Feed', path: '/research' },
  { label: 'SNP Explorer', path: '/explorer' },
  { label: 'Reports', path: '/reports' },
  { label: 'Settings', path: '/settings' },
] as const;

export const LAYER_COLORS: Record<string, string> = {
  health: '#A94442',
  pharma: '#2D5F8A',
  traits: '#4A7C59',
  carrier: '#C4953A',
};
