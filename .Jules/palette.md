## 2024-05-18 - Input Focus Pattern
**Learning:** In chat interfaces, when quick prompts populate an input but do not auto-submit (like Karyogram findings or Research Workbench prompts), failing to auto-focus the input forces an extra click.
**Action:** Always call `inputRef.current?.focus()` after setting the query via a quick prompt if auto-submit is not intended.
