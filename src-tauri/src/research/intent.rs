// ── Query Intent ────────────────────────────────────────────────

#[derive(Debug)]
pub enum QueryIntent {
    RsidLookup(String),
    GeneLookup(String),
    ConditionRisk(String),
    DrugResponse(String),
    TraitQuery(String),
    CarrierQuery(String),
    ChromosomeQuery(String),
    GeneralSummary,
    Unknown(String),
}

// ── Constants ───────────────────────────────────────────────────

pub const KNOWN_GENES: &[&str] = &[
    "brca1", "brca2", "mthfr", "cyp2d6", "cyp2c19", "cyp2c9", "cyp3a4",
    "apoe", "fto", "tcf7l2", "pparg", "bdnf", "comt", "vkorc1",
    "slco1b1", "dpyd", "tpmt", "ugt1a1", "ankk1", "drd2", "herc2",
    "oca2", "mc1r", "slc24a5", "edar", "abcc11", "aldh2", "adh1b",
    "actn3", "ace", "il2ra", "il7r", "ptpn22", "sh2b3", "cftr",
    "hbb", "hexa", "gba", "smpd1", "f5", "f2", "agtr1", "gckr",
    "pcsk9", "ldlr", "chrna3", "chrna5",
];

pub const CONDITION_KEYWORDS: &[(&str, &str)] = &[
    ("diabetes", "diabetes"),
    ("cancer", "cancer"),
    ("breast cancer", "breast cancer"),
    ("prostate cancer", "prostate cancer"),
    ("colorectal", "colorectal cancer"),
    ("heart", "heart disease"),
    ("coronary", "coronary artery disease"),
    ("cardiovascular", "cardiovascular disease"),
    ("alzheimer", "alzheimer's disease"),
    ("parkinson", "parkinson's disease"),
    ("celiac", "celiac disease"),
    ("crohn", "crohn's disease"),
    ("lupus", "lupus"),
    ("rheumatoid", "rheumatoid arthritis"),
    ("arthritis", "arthritis"),
    ("multiple sclerosis", "multiple sclerosis"),
    ("obesity", "obesity"),
    ("hypertension", "hypertension"),
    ("blood pressure", "hypertension"),
    ("thrombosis", "venous thromboembolism"),
    ("clotting", "venous thromboembolism"),
    ("depression", "depression"),
    ("bipolar", "bipolar disorder"),
    ("schizophrenia", "schizophrenia"),
    ("asthma", "asthma"),
    ("macular degeneration", "macular degeneration"),
    ("glaucoma", "glaucoma"),
    ("osteoporosis", "osteoporosis"),
    ("autoimmune", "autoimmune disease"),
    ("migraine", "migraine"),
    ("stroke", "stroke"),
    ("atrial fibrillation", "atrial fibrillation"),
];

pub const DRUG_KEYWORDS: &[(&str, &str)] = &[
    ("warfarin", "warfarin"),
    ("clopidogrel", "clopidogrel"),
    ("caffeine", "caffeine"),
    ("codeine", "codeine"),
    ("omeprazole", "omeprazole"),
    ("simvastatin", "simvastatin"),
    ("statin", "statins"),
    ("metformin", "metformin"),
    ("ibuprofen", "ibuprofen"),
    ("tamoxifen", "tamoxifen"),
    ("fluorouracil", "fluorouracil"),
    ("5-fu", "fluorouracil"),
    ("azathioprine", "azathioprine"),
    ("mercaptopurine", "mercaptopurine"),
    ("tacrolimus", "tacrolimus"),
    ("citalopram", "citalopram"),
    ("sertraline", "sertraline"),
    ("fluoxetine", "fluoxetine"),
    ("amitriptyline", "amitriptyline"),
    ("tramadol", "tramadol"),
    ("morphine", "morphine"),
    ("oxycodone", "oxycodone"),
    ("metoprolol", "metoprolol"),
    ("pantoprazole", "pantoprazole"),
    ("lansoprazole", "lansoprazole"),
    ("voriconazole", "voriconazole"),
    ("escitalopram", "escitalopram"),
];

pub const TRAIT_KEYWORDS: &[(&str, &str)] = &[
    ("eye color", "eye color"),
    ("eye colour", "eye color"),
    ("hair color", "hair color"),
    ("hair colour", "hair color"),
    ("red hair", "hair color"),
    ("muscle", "muscle composition"),
    ("sprint", "muscle composition"),
    ("endurance", "endurance"),
    ("lactose", "lactose intolerance"),
    ("milk", "lactose intolerance"),
    ("bitter", "bitter taste perception"),
    ("cilantro", "cilantro taste"),
    ("coriander", "cilantro taste"),
    ("earwax", "earwax type"),
    ("alcohol flush", "alcohol flush reaction"),
    ("asparagus", "asparagus odor detection"),
    ("freckling", "freckling"),
    ("freckles", "freckling"),
    ("baldness", "male pattern baldness"),
    ("hair loss", "male pattern baldness"),
    ("height", "height"),
    ("skin color", "skin pigmentation"),
    ("skin pigment", "skin pigmentation"),
    ("dimples", "dimples"),
    ("cleft chin", "cleft chin"),
    ("unibrow", "unibrow"),
    ("wisdom teeth", "wisdom teeth"),
    ("sneeze", "photic sneeze reflex"),
    ("sun sneeze", "photic sneeze reflex"),
];

pub const CARRIER_KEYWORDS: &[(&str, &str)] = &[
    ("cystic fibrosis", "cystic fibrosis"),
    ("cf", "cystic fibrosis"),
    ("sickle cell", "sickle cell disease"),
    ("tay-sachs", "tay-sachs disease"),
    ("tay sachs", "tay-sachs disease"),
    ("gaucher", "gaucher disease"),
    ("niemann-pick", "niemann-pick disease"),
    ("niemann pick", "niemann-pick disease"),
    ("hemochromatosis", "hereditary hemochromatosis"),
    ("phenylketonuria", "phenylketonuria"),
    ("pku", "phenylketonuria"),
    ("galactosemia", "galactosemia"),
    ("thalassemia", "thalassemia"),
    ("hemophilia", "hemophilia"),
];

// ── Question Parser ─────────────────────────────────────────────

pub fn parse_question(question: &str) -> QueryIntent {
    let q = question.to_lowercase();
    let q = q.trim();

    // 1. Check for rsID lookup: "rs" followed by digits
    if let Some(rsid) = extract_rsid(q) {
        return QueryIntent::RsidLookup(rsid);
    }

    // 2. Check for gene names
    for &gene in KNOWN_GENES {
        if word_match(q, gene) {
            return QueryIntent::GeneLookup(gene.to_uppercase());
        }
    }

    // 3. Check for carrier queries -- check before condition risk
    if q.contains("carrier") {
        for &(keyword, condition) in CARRIER_KEYWORDS {
            if q.contains(keyword) {
                return QueryIntent::CarrierQuery(condition.to_string());
            }
        }
        return QueryIntent::CarrierQuery("all".to_string());
    }
    for &(keyword, condition) in CARRIER_KEYWORDS {
        if q.contains(keyword) {
            return QueryIntent::CarrierQuery(condition.to_string());
        }
    }

    // 4. Check for drug response
    if q.contains("metabolize") || q.contains("metabolise") || q.contains("drug")
        || q.contains("medication") || q.contains("pharmacogenomic")
        || q.contains("medicine")
    {
        for &(keyword, drug) in DRUG_KEYWORDS {
            if q.contains(keyword) {
                return QueryIntent::DrugResponse(drug.to_string());
            }
        }
        return QueryIntent::DrugResponse("all".to_string());
    }
    for &(keyword, drug) in DRUG_KEYWORDS {
        if q.contains(keyword) {
            return QueryIntent::DrugResponse(drug.to_string());
        }
    }

    // 5. Check for trait queries
    for &(keyword, trait_name) in TRAIT_KEYWORDS {
        if q.contains(keyword) {
            return QueryIntent::TraitQuery(trait_name.to_string());
        }
    }

    // 6. Check for condition risk
    if q.contains("risk") || q.contains("chance") || q.contains("predispos")
        || q.contains("susceptib") || q.contains("likely")
    {
        for &(keyword, condition) in CONDITION_KEYWORDS {
            if q.contains(keyword) {
                return QueryIntent::ConditionRisk(condition.to_string());
            }
        }
        return QueryIntent::ConditionRisk("all".to_string());
    }
    for &(keyword, condition) in CONDITION_KEYWORDS {
        if q.contains(keyword) {
            return QueryIntent::ConditionRisk(condition.to_string());
        }
    }

    // 7. Check for chromosome queries
    if let Some(chr) = extract_chromosome(q) {
        return QueryIntent::ChromosomeQuery(chr);
    }

    // 8. Check for general summary
    if q.contains("summary") || q.contains("summarize") || q.contains("summarise")
        || q.contains("overview") || q.contains("what should i know")
        || q.contains("tell me about my genome")
        || q.contains("highlights") || q.contains("key findings")
    {
        return QueryIntent::GeneralSummary;
    }

    QueryIntent::Unknown(question.to_string())
}

pub fn extract_rsid(q: &str) -> Option<String> {
    let bytes = q.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'r' && bytes[i + 1] == b's' {
            if i > 0 && (bytes[i - 1] as char).is_alphanumeric() {
                i += 1;
                continue;
            }
            let mut j = i + 2;
            while j < len && (bytes[j] as char).is_ascii_digit() {
                j += 1;
            }
            if j > i + 2 {
                return Some(format!("rs{}", &q[i + 2..j]));
            }
        }
        i += 1;
    }
    None
}

pub fn extract_chromosome(q: &str) -> Option<String> {
    let patterns = ["chromosome ", "chr "];
    for pat in patterns {
        if let Some(pos) = q.find(pat) {
            let rest = &q[pos + pat.len()..];
            let chr_val: String = rest.chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            if !chr_val.is_empty() {
                return normalize_chromosome(&chr_val);
            }
        }
    }

    if let Some(pos) = q.find("chr") {
        let rest = &q[pos + 3..];
        if !rest.starts_with("omosome") {
            let chr_val: String = rest.chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            if !chr_val.is_empty() {
                return normalize_chromosome(&chr_val);
            }
        }
    }

    None
}

pub fn normalize_chromosome(val: &str) -> Option<String> {
    let v = val.to_uppercase();
    match v.as_str() {
        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "10"
        | "11" | "12" | "13" | "14" | "15" | "16" | "17" | "18" | "19"
        | "20" | "21" | "22" => Some(v),
        "X" | "Y" | "MT" => Some(v),
        _ => None,
    }
}

pub fn word_match(text: &str, word: &str) -> bool {
    let text_bytes = text.as_bytes();
    let tlen = text_bytes.len();
    let wlen = word.len();

    if wlen > tlen {
        return false;
    }

    let mut i = 0;
    while i + wlen <= tlen {
        if &text[i..i + wlen] == word {
            let before_ok = i == 0 || !(text_bytes[i - 1] as char).is_alphanumeric();
            let after_ok = i + wlen >= tlen || !(text_bytes[i + wlen] as char).is_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}
