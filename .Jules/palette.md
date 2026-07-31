## 2024-05-18 - Input Focus for Text Injectors
**Learning:** When text injectors (like 'Quick Prompts' or Karyogram finding clicks) populate an input but do not auto-submit, users must manually click into the input to submit or edit their query, which breaks the flow.
**Action:** Automatically focus the primary text input (e.g., using setTimeout) after populating it, allowing the user to seamlessly press Enter or immediately modify the text.
