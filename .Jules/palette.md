## 2024-06-18 - Quick Prompts and Interactive Query Injectors
**Learning:** When building interfaces that inject text into a primary input (e.g., Karyogram chart elements or Quick Prompts) without auto-submitting the form, users lose interaction flow if the focus remains on the clicked element.
**Action:** Always append `setTimeout(() => inputRef.current?.focus(), 0)` after the state update to shift keyboard focus to the input. This seamlessly allows users to press Enter immediately or make modifications to the query without manual repositioning.
