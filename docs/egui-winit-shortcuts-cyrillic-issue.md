# Bug report draft for `egui-winit`

## Title
Keyboard shortcuts with non-Latin layouts (e.g. Russian) may produce no `Event::Key`, making `Ctrl+Z`-style shortcuts impossible to detect

## Affected versions
- `egui = 0.27.x`
- `eframe = 0.27.x`
- `egui-winit = 0.27.x`
- OS: Linux (also likely affects other platforms depending on IME/layout)

## Summary
When using a Cyrillic layout (e.g. Russian), pressing shortcuts like `Ctrl+Я` (physical `Z` key on QWERTY position) does not produce any keyboard event in `RawInput.events` / `InputState.events`.

As a result, app-level shortcut handling cannot detect undo/redo via keyboard under such layouts.

## Expected behavior
- On non-Latin layouts, shortcut detection should still receive a keyboard event so apps can map shortcuts by physical key fallback.
- Typical expectation: `Ctrl+Я` can be interpreted as undo (`Ctrl+Z`) if app uses physical key fallback.

## Actual behavior
`InputState` shows modifier state updates (`ctrl: true`) but `raw.events` and `events` are empty during the keypress frame.

Example (trimmed):
- `modifiers.ctrl = true`
- `raw.events = []`
- `events = []`
- `keys_down = {}`

So there is no `Event::Key` to process.

## Reproduction
1. Create a minimal `eframe` app.
2. In `update`, log `ctx.input(|i| i.clone())`.
3. Switch to Russian layout.
4. Press `Ctrl+Я` (same physical key position as `Z` on QWERTY).
5. Observe no keyboard event in `raw.events/events`.

## Suspected cause
In `egui-winit` keyboard handling, `Event::Key` is pushed only when `logical_key` is mapped to `egui::Key`.

Relevant flow (from `egui-winit/src/lib.rs`):
- `logical_key = key_from_winit_key(...)`
- `physical_key = key_from_key_code(...)`
- `if let Some(logical_key) = logical_key { ... push Event::Key { key: logical_key, physical_key, ... } }`

For many Cyrillic `Key::Character(...)` values, `egui::Key::from_name(...)` returns `None`, so this branch is skipped and no `Event::Key` is emitted, even when `physical_key` is available.

## Proposed fix
Emit `Event::Key` when either logical OR physical key is available.

Suggested approach:
- Keep copy/cut/paste checks on logical key only.
- For generic key event emission, use fallback:
  - `effective_key = logical_key.or(physical_key)`
  - if `effective_key.is_some()`, push `Event::Key`.

Pseudo-code:
```rust
let logical_key = key_from_winit_key(logical);
let physical_key = key_from_key_code(physical);

if let Some(logical) = logical_key {
    if pressed && is_cut_copy_paste(modifiers, logical) {
        ...
        return;
    }
}

if let Some(effective) = logical_key.or(physical_key) {
    self.egui_input.events.push(egui::Event::Key {
        key: effective,
        physical_key,
        pressed,
        repeat: false,
        modifiers: self.egui_input.modifiers,
    });
}
```

## Why this matters
Cross-layout shortcuts are a common UX expectation. Without key events for non-Latin layouts, apps cannot implement robust shortcut behavior even with explicit physical-key fallback logic.
