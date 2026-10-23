## 2024-05-15 - Input Auto-focus for Quick Prompts
**Learning:** When using interactive text injectors (like quick prompts or karyogram findings) that populate an input field without auto-submitting, automatically focusing the input reduces user friction and allows immediate modification or submission.
**Action:** Always auto-focus primary text inputs (using `setTimeout` to ensure state updates complete) when programmatically populating them.
