# Porting a component

Repeat this for every upstream component.

1. **Upstream API:** `rustryx component <Name>` (add `--dense` for a short form) shows
   upstream's props, types, defaults and guidance, and what is ported so far.
2. **Read upstream:**
   - `../Astryx/packages/core/src/<Name>/*.tsx`;
   - `../Astryx/docs/specs/components/<Name>/<Name>.spec.md` (numbered FR/AR items);
   - the family contract in `../Astryx/docs/families/`;
   - the Playwright specs in `__tests__/`.
3. **Implement** in `crates/rustryx-components/src/<name>/`:
   - every prop, or a `RustApi::Waived` reason;
   - token styles;
   - theming targets through `theme.refine`;
   - keyboard behavior through actions, `key_context` and default key bindings;
   - accessibility roles from the AR items.
4. **Examples:** write `examples.rs` in that folder. The gallery renders it, and it
   becomes the `example` in the component's `ComponentDoc`.
5. **Register:** add the `ComponentDoc` (prop → Rust mapping) to `doc::REGISTRY`.
6. **Tests:** `#[gpui::test]` cases for the FR items: keyboard, pointer, state.
7. **Check:** compare the gallery with Astryx Storybook (`pnpm storybook` in `../Astryx`)
   in light and dark, neutral plus one other theme. Capture the native gallery with
   `tools/capture-window.ps1`.
8. **Parity:** `rustryx parity` must list the component as ported, with no unmapped props.
