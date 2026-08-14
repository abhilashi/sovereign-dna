## 2024-05-24 - Quick Prompt Auto-focus
**Learning:** When text inputs are populated by clicking "Quick Prompt" buttons (or inline findings), users often want to immediately hit 'Enter' to submit or edit the text. If focus is lost or not transferred, they are forced to click the input manually, breaking the flow.
**Action:** Automatically focus the target input using `setTimeout(() => inputRef.current?.focus(), 0)` when the prompt is clicked.
