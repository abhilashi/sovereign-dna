## 2024-05-08 - Accessible Zoom Map Controls
**Learning:** Adding `aria-live="polite"` to dynamic text indicators that update without receiving user focus (e.g. map zoom level displays) ensures that screen readers announce these state changes. Additionally, providing `aria-label` to icon-only control buttons (`+`, `-`) improves overall accessibility without visual clutter.
**Action:** When implementing custom interactive maps or data visualizers with status indicators, apply `aria-live` to the indicator block and `aria-label` to the corresponding non-text control buttons.
