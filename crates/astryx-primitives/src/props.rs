//! Builder setters for component props, without a proc-macro.
//!
//! Upstream components take 10–30 optional props. Each becomes a builder
//! method named after the prop. Three forms cover them:
//!
//! ```ignore
//! impl Text {
//!     astryx_primitives::setters! {
//!         /// Semantic text type (`type`).
//!         ty: TextType,
//!         /// Explicit size override (`size`).
//!         ?size: TextSize,
//!         /// Strikethrough decoration (`hasStrikethrough`).
//!         !strikethrough,
//!     }
//! }
//! ```
//!
//! - `name: T` sets a field of type `T` from `impl Into<T>`.
//! - `?name: T` sets an `Option<T>` field to `Some(..)`.
//! - `!name` sets a `bool` field; the setter takes the bool.

/// Generates builder setters; see the [module docs](self).
#[macro_export]
macro_rules! setters {
    () => {};
    ($(#[$doc:meta])* ?$name:ident : $ty:ty $(, $($rest:tt)*)?) => {
        $(#[$doc])*
        pub fn $name(mut self, value: impl Into<$ty>) -> Self {
            self.$name = Some(value.into());
            self
        }
        $($crate::setters!($($rest)*);)?
    };
    ($(#[$doc:meta])* !$name:ident $(, $($rest:tt)*)?) => {
        $(#[$doc])*
        pub fn $name(mut self, value: bool) -> Self {
            self.$name = value;
            self
        }
        $($crate::setters!($($rest)*);)?
    };
    ($(#[$doc:meta])* $name:ident : $ty:ty $(, $($rest:tt)*)?) => {
        $(#[$doc])*
        pub fn $name(mut self, value: impl Into<$ty>) -> Self {
            self.$name = value.into();
            self
        }
        $($crate::setters!($($rest)*);)?
    };
}

#[cfg(test)]
mod tests {
    #[derive(Default)]
    struct Props {
        label: String,
        size: Option<u8>,
        disabled: bool,
    }

    impl Props {
        setters! {
            /// A label.
            label: String,
            ?size: u8,
            !disabled,
        }
    }

    #[test]
    fn setters_cover_all_three_forms() {
        let props = Props::default().label("Save").size(3u8).disabled(true);
        assert_eq!(props.label, "Save");
        assert_eq!(props.size, Some(3));
        assert!(props.disabled);
    }
}
