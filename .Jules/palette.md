## 2026-08-27 - Interactive Text Injector Input Focus
**Learning:** In chat-like interfaces where users can select 'Quick Prompts' or click items to populate an input field *without* auto-submitting, users must perform an extra manual click to focus the input field before they can review, edit, or submit the query.
**Action:** Always pair programmatic text injection (like `setQuery`) with a `setTimeout(() => inputRef.current?.focus(), 0)` to immediately transfer browser focus to the input, creating a seamless transition from selection to typing or submission.
