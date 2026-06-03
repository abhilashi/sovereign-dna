## 2024-06-25 - Dynamic Map Zoom Level Displays
**Learning:** When displaying a textual map zoom level that updates dynamically (e.g. "Genome", "Chromosome", "Region", "SNP"), screen readers need `aria-live="polite"` to correctly announce the change to the user, as the text element does not receive user focus.
**Action:** Always add `aria-live="polite"` to dynamic text indicators that provide critical context without receiving focus.
