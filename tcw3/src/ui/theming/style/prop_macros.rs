//! Implements `def_prop!`, which is the macro that defines `Prop` and other
//! relevant items.

/// Defines `Prop` and relevant items at once.
macro_rules! def_prop {
    (
        $(#[$meta:meta])*
        pub enum Prop {
            // For each variant...
            $(
                // Documentation comments
                $( #[doc = $doc:tt] )*
                // Snake-cased name
                #[snake_case($snake_name:ident)]
                // Default value - this also specifies the value type
                #[default(PropValue::$val_variant:ident($default_val:expr))]
                $name:ident
                $( ($param_ty:ty) )?
            ),*
            $(,)*
        }
    ) => {
        doc_comment! {
            $(#[$meta])*
            @[doc = concat!(
                "\n",
                // Emit a Markdown table
                "| `Prop` | [`stylesheet!`] syntax | [`PropValue`] variant |\n",
                "| ------ | ---------------------- | --------------------- |\n",
                $(
                    // `Prop`
                    "| [`",
                    stringify!($name),
                    $( "(", stringify!($param_ty), ")", )?
                    "`](#variant.",
                    stringify!($name),
                    ") ",

                    // `stylesheet!` syntax
                    "| `",
                    stringify!($snake_name),
                    $( "[", stringify!($param_ty), "]", )?
                    ": value` ",

                    // `PropValue` variant
                    "| [`", stringify!($val_variant), "`]",
                    "(PropValue::", stringify!($val_variant), ")",

                    " | \n",
                )*
            )]
            pub enum Prop {
                $(
                    $( #[doc = $doc] )*
                    $name $(($param_ty))?
                ),*
            }
        }

        impl PropValue {
            /// Get the default value of a prop.
            pub fn default_for_prop(prop: &Prop) -> Self {
                match prop {
                    $(
                        prop_pattern!($name$(($param_ty))?) =>
                            PropValue::$val_variant($default_val)
                    ),*
                }
            }
        }

        /// Helper items for `stylesheet!`. Reexports `Prop`'s constructors
        /// under snake-cased names.
        #[doc(hidden)]
        pub mod mk_prop_by_snake_name {
            $( pub use super::Prop::$name as $snake_name; )*
        }

        /// Helper functions for `stylesheet!`.
        ///
        /// Reexports the constructors of `PropValue` under snake-cased names of
        /// the corresponding props.
        #[doc(hidden)]
        pub mod mk_prop_value_by_prop_snake_name {
            $( pub use super::PropValue::$val_variant as $snake_name; )*
        }

        /// Helper functions for `stylesheet!`.
        ///
        /// For each prop, defines a `const fn` with the snake-cased name of the
        /// prop.
        ///
        /// If the corresponding variant of `PropValue` contains `Rob`, the
        /// function in this module automatically wraps a given `'static`
        /// reference. Otherwise, it simply unwraps the reference.
        #[doc(hidden)]
        pub mod mk_wrap_value_by_prop_snake_name {
            use super::*;
            pub const fn deref<T: Copy>(x: &T) -> T { *x }
            $( def_wrap_value!(@value PropValue::$val_variant as $snake_name); )*
        }

        /// Helper functions for `stylesheet!`.
        ///
        /// Similar to `mk_wrap_value_by_prop_snake_name`, but the functions
        /// are not `const fn` and take a runtime value.
        #[doc(hidden)]
        pub mod mk_wrap_dynvalue_by_prop_snake_name {
            use super::*;
            $( def_wrap_value!(@dynvalue PropValue::$val_variant as $snake_name); )*
        }
    };
}

macro_rules! doc_comment {
    (
        $(#[$m:meta])*
        @[doc = $x:expr]
        $($tt:tt)*
    ) => {
        $(#[$m])*
        #[doc = $x]
        $($tt)*
    };
}

/// Map `X` → `Prop::X`, `X(u32)` → `Prop::X(_)`.  Used by `def_prop`.
macro_rules! prop_pattern {
    ($name:ident) => { Prop::$name };
    ($name:ident($t:ty)) => { Prop::$name(_) };
}

/// Generates a function that preprocesses a value before passing it to
/// `PropValue`'s constructor. Used by `def_prop`.
macro_rules! def_wrap_value {
    // For these prop value types, the inner values are wrapped with `Rob`
    (@value PropValue::LayerXform as $alias:ident) => {
        pub const fn $alias(value: &'static LayerXform) -> Rob<'static, LayerXform> {
            Rob::from_ref(value)
        }
    };
    (@dynvalue PropValue::LayerXform as $alias:ident) => {
        pub fn $alias(value: LayerXform) -> Rob<'static, LayerXform> {
            Rob::from_box(Box::new(value))
        }
    };

    (@value PropValue::Metrics as $alias:ident) => {
        pub const fn $alias(value: &'static Metrics) -> Rob<'static, Metrics> {
            Rob::from_ref(value)
        }
    };
    (@dynvalue PropValue::Metrics as $alias:ident) => {
        pub fn $alias(value: Metrics) -> Rob<'static, Metrics> {
            Rob::from_box(Box::new(value))
        }
    };

    // Default
    (@value PropValue::$name:ident as $alias:ident) => {
        pub use deref as $alias;
    };
    (@dynvalue PropValue::$name:ident as $alias:ident) => {
        pub use ::std::convert::identity as $alias;
    };
}
