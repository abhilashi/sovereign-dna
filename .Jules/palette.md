## 2026-09-02 - Auto-focus inputs for text injectors
**Learning:** When providing users with "Quick Prompts" or interactive text injectors (like Karyogram findings) that populate an input but do not auto-submit, failing to auto-focus the input creates friction. Users have to manually click the input to add text or press Enter.
**Action:** Always auto-focus the primary text input (e.g., `setTimeout(() => inputRef.current?.focus(), 0)`) when a user interacts with a component that populates the input but leaves it in an un-submitted state. This allows for immediate editing or submission.
