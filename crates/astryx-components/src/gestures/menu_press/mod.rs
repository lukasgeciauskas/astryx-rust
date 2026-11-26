//! The menu press model, upstream `hooks/useMenuPress.ts` over the pure
//! `hooks/menuPressGesture.ts` machine, with `menuPressValues.ts`'s UIKit number and
//! `menuPressOnItems.ts`'s rule (a press on caller content beside the rows stays that
//! content's: only wrapped rows are rows).

pub mod gesture;
mod hook;
pub mod values;

pub use gesture::{
    MENU_PRESS_HANDOFF_PX, MENU_PRESS_SETTLE_MS, MenuPressEffect, MenuPressEvent, MenuPressGesture,
    MenuPressPointerType, MenuPressTarget, menu_press_step,
};
pub use hook::{
    MENU_PRESS_AUTOSCROLL_ZONE_PX, MENU_PRESS_LONG_PRESS_MS, MenuPress, MenuPressOptions,
    MenuPressRow, MenuPressSurface, MenuRelease, use_menu_press,
};
pub use values::MENU_PRESS_TRIGGER_DRIFT_PX;
