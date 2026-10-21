
## 2023-10-24 - Auto-focus inputs for populated queries
**Learning:** When users click a quick prompt or a UI element (like a karyogram finding) that populates an input field without automatically submitting, they often want to edit or immediately press Enter. Forcing them to manually click the input field again breaks their flow.
**Action:** Automatically focus the main input field (`inputRef.current?.focus()`, usually wrapped in a `setTimeout` to allow state updates) whenever pre-filling its content to keep interaction seamless.
