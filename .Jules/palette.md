
## 2024-04-19 - Auto-focus Inputs After Text Injection
**Learning:** Users experience friction when using "Quick Prompts" or interactive text injections (like Karyogram finding clicks) if the input doesn't auto-focus. They often have to manually click the input to hit Enter or modify the query.
**Action:** When implementing interactive text injectors that populate an input but do not auto-submit, automatically focus the primary text input (e.g., using `setTimeout(() => inputRef.current?.focus(), 0)`). This allows users to seamlessly press Enter or modify the query without repositioning focus manually.
