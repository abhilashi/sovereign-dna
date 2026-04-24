## 2024-05-24 - Interactive Text Injectors UX
**Learning:** When users click a quick prompt or a visual finding (like a karyogram point) that pre-fills a search/chat input without automatically submitting, they often face friction because their focus remains on the clicked element.
**Action:** Always automatically shift focus to the primary text input (e.g., `setTimeout(() => inputRef.current?.focus(), 0)`) when populating it via a click interaction. This allows immediate keyboard editing or immediate submission via Enter, dramatically smoothing the flow.
