//! The snap-point drag's tests, one module per upstream test file:
//!
//! - `release_policy` and `factory` hold `createDragToSnapPoints.test.ts`;
//! - `fine_pointer`, `createDragToSnapPoints.finePointer.test.ts`;
//! - `reduced_motion`, `createDragToSnapPoints.reducedMotion.test.ts`;
//! - `snap_drag`, `createSnapDrag.test.ts`;
//! - `hook`, `hooks/useDragToSnapPoints.test.tsx`;
//! - `templates`, `snapTemplates.test.tsx`.
//!
//! Left out, being about the DOM, the server or the bundle rather than behavior:
//! `createDragToSnapPoints.server.test.ts` (importing does no DOM work: Rust has no
//! import-time code), `publicSurface.test.ts` and `__tests__/snapBundle.ts` (the
//! package's export surface: `create_snap_drag` is `pub(crate)`, which the compiler
//! checks), the listener-count half of "destroy removes every listener" (no DOM
//! listeners; destroy's detaching the pan is checked instead), the plain-JavaScript
//! template's DOM teardown and inline-style checks, and the Chromium interaction spec
//! (`__tests__/createDragToSnapPoints.interaction.browser.spec.ts`).

mod factory;
mod fine_pointer;
mod harness;
mod hook;
mod reduced_motion;
mod release_policy;
mod snap_drag;
mod templates;
