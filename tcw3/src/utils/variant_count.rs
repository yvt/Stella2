/// Discards the first token and `;` and inserts the remaining input tokens.
macro_rules! replace {
    ($ignored:tt; $($rest:tt)*) => {$($rest)*};
}

/// Takes an `enum` and adds an associated constant named `VARIANT_COUNT`,
/// representing the number of variants it has.
macro_rules! variant_count {
    (
        $(#[$enum_meta:meta])*
        $enum_vis:vis
        enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident
                $(= $variant_value:expr)?
            ),*  $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        $enum_vis
        enum $name {
            $(
                $(#[$variant_meta])*
                $variant
                $(= $variant_value)?
            ),*
        }

        impl $name {
            const VARIANT_COUNT: usize = 0 $(+ replace!($variant; 1))*;
        }
    };
}
