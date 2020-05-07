use lazy_static::lazy_static;
use std::ops::Range;

use super::{
    manager::PropKindFlags,
    style::{elem_id, roles, ClassSet, ElemClassPath, Metrics, Prop, PropValue},
};

/// Represents a single stylesheet rule in [`Stylesheet`].
pub type RuleId = usize;

pub trait Stylesheet {
    /// Enumerate rules that apply to the specifed `ElemClassPath`.
    ///
    /// `out_rules` is called with a `RuleId` for each rule that applies to the
    /// specified `ElemClassPath`. The `RuleId` is specific to `self`.
    fn match_rules(&self, path: &ElemClassPath, out_rules: &mut dyn FnMut(RuleId));

    /// Get the priority of a stylesheet rule in this `Stylesheet`.
    ///
    /// Returns `None` if `id` is invalid.
    fn get_rule_priority(&self, id: RuleId) -> Option<i16>;

    /// Get a `PropKindFlags` representing an approximate set of styling
    /// properties specified by a stylesheet rule in this `Stylesheet`.
    ///
    /// Returns `None` if `id` is invalid.
    fn get_rule_prop_kinds(&self, id: RuleId) -> Option<PropKindFlags>;

    /// Get a property value for `Prop` specified by a stylesheet rule in this
    /// `Stylesheet`.
    ///
    /// Returns `None` if `id` is invalid; `Some(None)` if the value is not
    /// provided by the rule; `Some(Some(_))` otherwise.
    #[allow(clippy::option_option)]
    fn get_rule_prop_value(&self, id: RuleId, prop: &Prop) -> Option<Option<&PropValue>>;
}

// The following types are constructred by the `stylesheet!` marcro. However,
// they are implemntation details and I'd prefer not to expose them.

#[doc(hidden)]
#[derive(Debug)]
pub struct StylesheetMacroOutput {
    /// The static part (everything other than prop values) of the stylesheet.
    pub rules: &'static [Rule],
    /// The runtime part (prop values) of the stylesheet.
    pub props: Box<[(Prop, PropValue)]>,
}

#[doc(hidden)]
#[derive(Debug)]
pub struct Rule {
    pub priority: i16,
    pub prop_kinds: PropKindFlags,
    /// An index range into `StylesheetMacroOutput::props`.
    pub props_range_u16: Range<u16>,
    pub selector: Selector,
}

#[doc(hidden)]
#[derive(Debug)]
pub struct Selector {
    pub target: ElemCriteria,
    pub ancestors: &'static [(bool, ElemCriteria)],
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ElemCriteria {
    pub pos: ClassSet,
    pub neg: ClassSet,
}

impl Stylesheet for StylesheetMacroOutput {
    fn match_rules(&self, path: &ElemClassPath, out_rules: &mut dyn FnMut(RuleId)) {
        // TODO: optimize the selector matching using target class buckets or
        //       DFA + BDD
        for (i, rule) in self.rules.iter().enumerate() {
            if rule.selector.matches(path) {
                out_rules(i);
            }
        }
    }

    fn get_rule_priority(&self, id: RuleId) -> Option<i16> {
        self.rules.get(id).map(Rule::priority)
    }
    fn get_rule_prop_kinds(&self, id: RuleId) -> Option<PropKindFlags> {
        self.rules.get(id).map(Rule::prop_kinds)
    }
    fn get_rule_prop_value(&self, id: RuleId, prop: &Prop) -> Option<Option<&PropValue>> {
        self.rules.get(id).map(|r| {
            self.props[r.props_range()]
                .iter()
                .find(|p| p.0 == *prop)
                .map(|p| &p.1)
        })
    }
}

impl Rule {
    fn priority(&self) -> i16 {
        self.priority
    }

    fn prop_kinds(&self) -> PropKindFlags {
        self.prop_kinds
    }

    fn props_range(&self) -> Range<usize> {
        self.props_range_u16.start as usize..self.props_range_u16.end as usize
    }
}

impl Selector {
    fn matches(&self, path: &ElemClassPath) -> bool {
        let mut it = path.iter().rev();
        if !self.target.matches(&it.next().unwrap()) {
            return false;
        }

        let mut cur_maybe = it.next();

        for (direct, criteria) in self.ancestors.iter() {
            if *direct {
                if let Some(cur) = cur_maybe {
                    if !criteria.matches(&cur) {
                        return false;
                    }
                    cur_maybe = it.next();
                } else {
                    return false;
                }
            } else {
                loop {
                    if let Some(cur) = cur_maybe {
                        if criteria.matches(&cur) {
                            cur_maybe = it.next();
                            break;
                        } else {
                            cur_maybe = it.next();
                        }
                    } else {
                        return false;
                    }
                }
            }
        }

        true
    }
}

impl ElemCriteria {
    fn matches(&self, class_set: &ClassSet) -> bool {
        class_set.contains(self.pos) && !class_set.intersects(self.neg)
    }
}

// -----------------------------------------------------------------------------
//  Stylesheet definition macro

// Extract positive criterias of class names, and output them as the raw
// representation of `ClassSet`.
#[doc(hidden)]
#[macro_export]
macro_rules! elem_pos {
    (#$id:tt $($rest:tt)*) => {
        ($id).bits() | $crate::elem_pos!($($rest)*)
    };
    (.$cls:ident $($rest:tt)*) => {
        $crate::ui::theming::ClassSet::$cls.bits() | $crate::elem_pos!($($rest)*)
    };
    (:not(.$cls:ident) $($rest:tt)*) => {
        $crate::elem_pos!($($rest)*)
    };
    () => { 0 };
}

// Extract negative criterias of class names, and output them as the raw
// representation of `ClassSet`.
#[doc(hidden)]
#[macro_export]
macro_rules! elem_neg {
    (#$id:tt $($rest:tt)*) => {
        ($crate::ui::theming::ClassSet::ID_MASK.bits() ^ ($id).bits()) | $crate::elem_neg!($($rest)*)
    };
    (:not(.$cls:ident) $($rest:tt)*) => {
        $crate::ui::theming::ClassSet::$cls.bits() | $crate::elem_neg!($($rest)*)
    };
    (.$cls:ident $($rest:tt)*) => {
        $crate::elem_neg!($($rest)*)
    };
    () => { 0 };
}

/// Construct a `ElemCriteria`. Called inside a `static` statement.
#[doc(hidden)]
#[macro_export]
macro_rules! elem {
    ($($classes:tt)*) => {
        $crate::ui::theming::ElemCriteria {
            pos: $crate::ui::theming::ClassSet::from_bits_truncate($crate::elem_pos!($($classes)*)),
            neg: $crate::ui::theming::ClassSet::from_bits_truncate($crate::elem_neg!($($classes)*)),
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! sel_ancestor {
    (< $($rest:tt)*) => {(true, $crate::elem!($($rest)*))};
    (.. $($rest:tt)*) => {(false, $crate::elem!($($rest)*))};
}

#[doc(hidden)]
#[macro_export]
macro_rules! sel {
    (
        [$($cur:tt)*]
        $( $mode:tt [ $($ancestor:tt)* ] )*
    ) => {{
        $crate::ui::theming::Selector {
            target: $crate::elem!($($cur)*),
            ancestors: &[
                $( $crate::sel_ancestor!( $mode $($ancestor)* ) ),*
            ],
        }
    }};
}

/// This is an internal macro to be used by other macros defined in this module.
///
/// This macro accepts the following input forms:
///
///  - `prop!(@prop name[param])` produces a `Prop`.
///  - `prop!(@constvalue #[dyn] name[param]: value)` produces a `PropValue` for
///    the first pass.
///  - `prop!(@setdynvalue(store_to) #[dyn] name[param]: value)` produces an
///    expression assigning `PropValue` for the second pass.
///
/// The following input forms are internal:
///
///  - `prop!(@value name[param]: value)` produces a `PropValue`. If necessary,
///    boxing is done in a way suitable for compile-time evaluation.
///  - `prop!(@dynvalue name[param]: value)` produces a `PropValue`. If
///    necessary, boxing is done in a way suitable for runtime evaluation. This
///    will fallback to `@value` if boxing is not needed for the given prop.
///
#[doc(hidden)]
#[macro_export]
macro_rules! prop {
    // For the first pass, prop specifications with `#[dyn]` are replaced with
    // dummy values.
    (@constvalue #[dyn] $name:ident $($rest:tt)*) => {
        $crate::ui::theming::PropValue::Bool(false)
    };
    (@constvalue $name:ident $($rest:tt)*) => {
        $crate::prop!(@value $name $($rest)*)
    };

    // For the second pass, only the prop specifications with `#[dyn]` are
    // evaluated.
    (@setdynvalue($store_to:expr) #[dyn] $($rest:tt)*) => {
        ::std::mem::forget(::std::mem::replace(
            &mut $store_to,
            $crate::prop!(@dynvalue $($rest)*),
        ))
    };
    (@setdynvalue($store_to:expr) $name:ident $($rest:tt)*) => {};

    (@prop num_layers) => { $crate::ui::theming::Prop::NumLayers };
    (@value num_layers: $val:expr) => { $crate::ui::theming::PropValue::Usize($val) };

    (@prop layer_img[$i:expr]) => { $crate::ui::theming::Prop::LayerImg($i) };
    (@value layer_img[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::Himg($val) };

    (@prop layer_center[$i:expr]) => { $crate::ui::theming::Prop::LayerCenter($i) };
    (@value layer_center[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::Box2($val) };

    (@prop layer_xform[$i:expr]) => { $crate::ui::theming::Prop::LayerXform($i) };
    (@value layer_xform[$i:expr]: $val:expr) =>
        { $crate::ui::theming::PropValue::LayerXform($crate::rob::Rob::from_ref(&$val)) };
    (@dynvalue layer_xform[$i:expr]: $val:expr) =>
        { $crate::ui::theming::PropValue::LayerXform($crate::rob::Rob::from_box(Box::new($val))) };

    (@prop layer_opacity[$i:expr]) => { $crate::ui::theming::Prop::LayerOpacity($i) };
    (@value layer_opacity[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::Float($val) };

    (@prop layer_bg_color[$i:expr]) => { $crate::ui::theming::Prop::LayerBgColor($i) };
    (@value layer_bg_color[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::Rgbaf32($val) };

    (@prop layer_flags[$i:expr]) => { $crate::ui::theming::Prop::LayerFlags($i) };
    (@value layer_flags[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::LayerFlags($val) };

    (@prop layer_metrics[$i:expr]) => { $crate::ui::theming::Prop::LayerMetrics($i) };
    (@value layer_metrics[$i:expr]: $val:expr) =>
        { $crate::ui::theming::PropValue::Metrics($crate::rob::Rob::from_ref(&$val)) };
    (@dynvalue layer_metrics[$i:expr]: $val:expr) =>
        { $crate::ui::theming::PropValue::Metrics($crate::rob::Rob::from_box(Box::new($val))) };

    (@prop subview_layouter) => { $crate::ui::theming::Prop::SubviewLayouter };
    (@value subview_layouter: $val:expr) =>
        { $crate::ui::theming::PropValue::Layouter($val) };

    (@prop subview_padding) => { $crate::ui::theming::Prop::SubviewPadding };
    (@value subview_padding: $val:expr) =>
        { $crate::ui::theming::PropValue::F32x4($val) };

    (@prop subview_metrics[$i:expr]) => { $crate::ui::theming::Prop::SubviewMetrics($i) };
    (@value subview_metrics[$i:expr]: $val:expr) =>
        { $crate::ui::theming::PropValue::Metrics($crate::rob::Rob::from_ref(&$val)) };
    (@dynvalue subview_metrics[$i:expr]: $val:expr) =>
        { $crate::ui::theming::PropValue::Metrics($crate::rob::Rob::from_box(Box::new($val))) };

    (@prop subview_table_cell[$i:expr]) => { $crate::ui::theming::Prop::SubviewTableCell($i) };
    (@value subview_table_cell[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::U32x2($val) };

    (@prop subview_table_align[$i:expr]) => { $crate::ui::theming::Prop::SubviewTableAlign($i) };
    (@value subview_table_align[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::AlignFlags($val) };

    (@prop subview_visibility[$i:expr]) => { $crate::ui::theming::Prop::SubviewVisibility($i) };
    (@value subview_visibility[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::Bool($val) };

    (@prop subview_table_col_spacing[$i:expr]) => { $crate::ui::theming::Prop::SubviewTableColSpacing($i) };
    (@value subview_table_col_spacing[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::Float($val) };

    (@prop subview_table_row_spacing[$i:expr]) => { $crate::ui::theming::Prop::SubviewTableRowSpacing($i) };
    (@value subview_table_row_spacing[$i:expr]: $val:expr) => { $crate::ui::theming::PropValue::Float($val) };

    (@prop min_size) => { $crate::ui::theming::Prop::MinSize };
    (@value min_size: $val:expr) => { $crate::ui::theming::PropValue::Vector2($val) };

    (@prop allow_grow) => { $crate::ui::theming::Prop::AllowGrow };
    (@value allow_grow: $val:expr) => { $crate::ui::theming::PropValue::Bool2($val) };

    (@prop fg_color) => { $crate::ui::theming::Prop::FgColor };
    (@value fg_color: $val:expr) => { $crate::ui::theming::PropValue::Rgbaf32($val) };

    (@prop font) => { $crate::ui::theming::Prop::Font };
    (@value font: $val:expr) => { $crate::ui::theming::PropValue::SysFontType($val) };

    // `@dynvalue` falls back to `@value` if boxing is not necessary
    (@dynvalue $($rest:tt)*) => { $crate::prop!(@value $($rest)*) };
}

/// Produces an expression of type `Vec<(Prop, PropValue)>`.
///
/// # Attributes
///
/// Usually we use `$(#[$m:meta])*` for attributes, but we can't here
/// because the compiler gets confused about loop nesting. We intend to
/// support only zero or one attribute for each rule, so fortunately there's a
/// solution. For the cases where there's one attribute, the caller of this
/// macro specifies input tokens like `#[a][b]`. This macro captures `[a]` and
/// ignores `[b]`. For the cases where there are no attributes, the caller of
/// this macro specifies input tokens like `#[b]`. `[b]` is usually something
/// that will behave as if no attributes are specified at all. In this case,
/// this macro captures `[b]`, thus simulating the effect of not specifying any
/// attributes.
#[doc(hidden)]
#[macro_export]
macro_rules! props {
    (
        $(
            // See the discussion in the doc comment for why we ignore the
            // second token tree.
            meta = # $meta:tt $([$($ignored:tt)*])?;
            props = {
                $( $(#[$mod:tt])* $name:ident $([$param:expr])* : $value:expr ),* $(,)*
            };
        )*
    ) => {{
        const PROP_COUNT: usize = {
            let mut count = 0;
            // For each rule...
            $(
                #$meta
                {
                    count += $crate::prop_count!{ $($name $([$param])* : $value,)* };
                }
            )*
            count
        };

        // Pass 1: Create a static array containing all static property values
        const PROPS: [($crate::ui::theming::Prop, $crate::ui::theming::PropValue); PROP_COUNT] = [
            // For each rule...
            $(
                // (`$meta` is defined by each rule)

                // For each prop specification...
                $(
                    // Repeat `$meta` for all prop specifications in the rule
                    #$meta

                    // Emit an element
                    (
                        $crate::prop!( @prop $name $([$param])* ),
                        $crate::prop!( @constvalue $(#[$mod])* $name $([$param])* : $value ),
                    ),
                )*
            )*
        ];

        let mut props = Box::new(PROPS);

        // Pass 2: Assign runtime values
        let mut props_ptr = &mut props[..];

        // For each rule...
        $(
            #[allow(unused_assignments)]
            #$meta
            {
                // For each prop specification...
                $(
                    // Update `props_ptr[0]` if the prop specification
                    // has `#[dyn]`
                    $crate::prop!(
                        @setdynvalue(props_ptr[0].1)
                        $(#[$mod])* $name $([$param])* : $value
                    );
                    props_ptr = &mut props_ptr[1..];
                )*
            }
        )*

        props
    }};
}

/// Given prop specifications, emits an expression of type `PropKindFlags`
/// representing the union of `PropKindFlags` of the given prop specifications.
#[doc(hidden)]
#[macro_export]
macro_rules! prop_kinds {
    ($( $(#[$mod:tt])* $name:ident $([$param:expr])* : $value:expr ),* $(,)* ) => {
        $crate::ui::theming::PropKindFlags::from_bits_truncate(
            // 0 | x | y | z | ...
            0
            $(
                |
                $crate::prop!(@prop $name $([$param])*).kind_flags().bits()
            )*
        )
    };
}

/// Given prop specifications, emits an integer literal representing the number
/// of the prop specifications.
#[doc(hidden)]
#[macro_export]
macro_rules! prop_count {
    ($(#[$mod:tt])* $name:ident $([$param:expr])* : $value:expr $(, $(,)* $($rest:tt)*)?) => {
        1 + $crate::prop_count!($($($rest)*)?)
    };
    () => { 0 }
}

#[doc(hidden)]
#[macro_export]
macro_rules! rule {
    (
        // The counter variable that keep track of the current index into
        // `StylesheetMacroOutput::props`.
        $i:expr,
        ($($sel:tt)*) (priority = $pri:expr) {
            $($props:tt)*
        }
    ) => {{
        let start = $i;
        $i += $crate::prop_count! { $($props)* };
        let end = $i;

        $crate::ui::theming::Rule {
            priority: $pri,
            prop_kinds: $crate::prop_kinds! { $($props)* },
            props_range_u16: start..end,
            selector: $crate::sel!($($sel)*),
        }
    }};
}

/// Construct an `impl `[`Stylesheet`]` + 'static`.
///
/// [`Stylesheet`]: crate::ui::theming::Stylesheet
///
/// The produced expression is not a constant expression because it has to
/// support property values which are determined at runtime. However,
/// it utilizes `static` as much as possible for the constant part of the data.
///
/// # Examples
///
///     use tcw3::{stylesheet, pal::RGBAF32, ui::theming::ClassSet};
///
///     const CUSTOM_ID: ClassSet = ClassSet::id(42);
///
///     # #[tcw3::testing::use_testing_wm]
///     # fn inner(twm: &dyn tcw3::pal::testing::TestingWm) {
///     let stylesheet = stylesheet! {
///         // Selector are similar to CSS, but use predefined symbols instead.
///         //  - ID values (`CUSTOM_ID`) are constant expressions. They must be
///         //    a single token tree.
///         //  - Class bits (`LABEL``, etc.) are bare identifiers in `ClassSet`.
///         ([#CUSTOM_ID.LABEL] < [.BUTTON.ACTIVE]) (priority = 100) {
///             // Arbitrary expressions are permitted only as property values
///             // like the following:
///             fg_color: RGBAF32::new(1.0, 1.0, 1.0, 1.0),
///
///             // Non-constant expressions require `#[dyn]`.
///             #[dyn] layer_img[0]: Some(
///                 tcw3::images::himg_figures![rect([0.1, 0.4, 0.8, 1.0])]
///             ),
///         },
///
///         #[cfg(target_os = "windows")]
///         ([.BUTTON]) (priority = 10000) {
///             // This rule is compiled in only when targetting Windows
///             layer_opacity[0]: 0.5,
///         },
///     };
///     # }
///     # inner();
///
#[macro_export]
macro_rules! stylesheet {
    (
        // for each rule...
        $(
            // `#[cfg(...)]`, etc.
            $( #[$cfg:meta] )?
            // scope and priority
            $( ($( $meta:tt )*) )*
            // props
            { $( $rule:tt )* }
        ),*
        $(,)*
    ) => {{
        static RULES: &[$crate::ui::theming::Rule] = {
            let mut i = 0;
            &[
                $(
                    $( #[$cfg] )*
                    $crate::rule!( i, $(($($meta)*))* {$($rule)*} ),
                )*
            ]
        };
        $crate::ui::theming::StylesheetMacroOutput {
            rules: RULES,
            props: $crate::props!{$(
                meta = #$([$cfg])* [cfg(all())];
                props = { $($rule)* };
            )*}
        }
    }};
}

// -----------------------------------------------------------------------------
//  Default  stylesheet definition
//
// TODO: Make it dynamic (based on the operating system's configuration)
//
use crate::{
    images::{figures, himg_figures, himg_from_figures_with_size, HImg},
    pal::RGBAF32,
    stvg::StvgImg,
};
use cggeom::box2;
use cgmath::Vector2;
use std::f32::NAN;

mod assets {
    pub type Stvg = (&'static [u8], [f32; 2]);

    macro_rules! stvg {
        ($path:literal) => {
            stvg_macro::include_stvg!($path)
        };
    }

    pub static CHECKBOX_LIGHT: Stvg = stvg!("assets/checkbox_light.svg");
    pub static CHECKBOX_LIGHT_ACT: Stvg = stvg!("assets/checkbox_light_act.svg");
    pub static CHECKBOX_LIGHT_CHECKED: Stvg = stvg!("assets/checkbox_light_checked.svg");
    pub static CHECKBOX_LIGHT_CHECKED_ACT: Stvg = stvg!("assets/checkbox_light_checked_act.svg");

    pub static RADIO_LIGHT: Stvg = stvg!("assets/radio_light.svg");
    pub static RADIO_LIGHT_ACT: Stvg = stvg!("assets/radio_light_act.svg");
    pub static RADIO_LIGHT_CHECKED: Stvg = stvg!("assets/radio_light_checked.svg");
    pub static RADIO_LIGHT_CHECKED_ACT: Stvg = stvg!("assets/radio_light_checked_act.svg");
}

const BUTTON_CORNER_RADIUS: f32 = 2.0;

const CHECKBOX_IMG_SIZE: Vector2<f32> = Vector2::new(16.0, 16.0);

const SCROLLBAR_VISUAL_WIDTH: f32 = 6.0;
const SCROLLBAR_VISUAL_RADIUS: f32 = SCROLLBAR_VISUAL_WIDTH / 2.0;
const SCROLLBAR_MARGIN: f32 = 6.0;
const SCROLLBAR_LEN_MIN: f32 = 20.0;

const FIELD_HEIGHT: f32 = 20.0;

/// Replace blue with a global tint color, and create a `HImg`.
fn recolor_tint(data: &(&'static [u8], [f32; 2])) -> HImg {
    use alt_fp::fma;
    use packed_simd::{f32x4, shuffle};
    #[inline(never)]
    fn map_color(c: RGBAF32) -> RGBAF32 {
        let tint = f32x4::new(0.2, 0.5, 0.9, 1.0);
        let c: [f32; 4] = c.into();
        let c: f32x4 = c.into();
        // Equivalent to:
        //
        //     [
        //         lerp(c.g, c.b, tint.r),
        //         lerp(c.g, c.b, tint.g),
        //         lerp(c.g, c.b, tint.b),
        //         lerp(c.g, c.a, tint.a), // `tint.a` assumed to be `1`
        //     ]
        let c1 = f32x4::splat(c.extract(1));
        let c2: f32x4 = shuffle!(c, [2, 2, 2, 3]);
        let out_c = fma![c1 * (1.0 - tint) + (c2 * tint)];
        <[f32; 4]>::from(out_c).into()
    }
    StvgImg::new(*data).with_color_xform(map_color).into_himg()
}

// Import IDs (e.g., `#SHOW_MENU`) into the scope
use elem_id::*;

lazy_static! {
    static ref DEFAULT_STYLESHEET: StylesheetMacroOutput = stylesheet! {
        ([.BUTTON]) (priority = 100) {
            num_layers: 1,
            #[dyn] layer_img[0]: Some(himg_figures![
                // Shadow
                rect([0.0, 0.0, 0.0, 0.05])
                    .radius(BUTTON_CORNER_RADIUS + 0.5)
                    .margin([0.5; 4]),
                rect([0.0, 0.0, 0.0, 0.1])
                    .radius(BUTTON_CORNER_RADIUS + 0.5)
                    .margin([1.0, 0.5, 0.0, 0.5]),
                rect([0.0, 0.0, 0.0, 0.2])
                    .radius(BUTTON_CORNER_RADIUS)
                    .margin([1.5, 1.0, 0.5, 1.0]),
                // Button face
                rect([0.97, 0.97, 0.97, 1.0])
                    .radius(BUTTON_CORNER_RADIUS)
                    .margin([1.0; 4]),
            ]),
            layer_metrics[0]: Metrics {
                margin: [-1.0; 4],
                .. Metrics::default()
            },
            layer_center[0]: box2! { point: [0.5, 0.5] },
            layer_opacity[0]: 0.8,
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [3.0, 8.0, 3.0, 8.0],
                .. Metrics::default()
            },
        },
        ([.BUTTON.HOVER]) (priority = 200) {
            layer_opacity[0]: 1.0,
        },
        ([.BUTTON.ACTIVE]) (priority = 200) {
            #[dyn] layer_img[0]: Some(himg_figures![
                // Shadow
                rect([0.0, 0.0, 0.0, 0.05])
                    .radius(BUTTON_CORNER_RADIUS + 0.5)
                    .margin([0.5; 4]),
                rect([0.0, 0.0, 0.0, 0.1])
                    .radius(BUTTON_CORNER_RADIUS + 0.5)
                    .margin([1.0, 0.5, 0.0, 0.5]),
                rect([0.0, 0.0, 0.0, 0.2])
                    .radius(BUTTON_CORNER_RADIUS)
                    .margin([1.5, 1.0, 0.5, 1.0]),
                // Button face
                rect([0.90, 0.90, 0.90, 1.0])
                    .radius(BUTTON_CORNER_RADIUS)
                    .margin([1.0; 4]),
                // Obscure the button face layer completely except the topmost
                // 0.5px-wide area for a subtle highlight effect
                rect([0.85, 0.85, 0.85, 1.0])
                    .radius(BUTTON_CORNER_RADIUS)
                    .margin([1.5, 1.0, 1.0, 1.0]),
            ]),
        },
        // Button label
        ([] < [.BUTTON]) (priority = 100) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 1.0),
        },

        // Checkbox
        ([.CHECKBOX]) (priority = 100) {
            num_layers: 1,
            #[dyn] layer_img[0]: Some(recolor_tint(&assets::CHECKBOX_LIGHT)),
            layer_metrics[0]: Metrics {
                margin: [NAN, NAN, NAN, 4.0],
                size: CHECKBOX_IMG_SIZE,
            },
            layer_opacity[0]: 0.9,
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [3.0, 8.0, 3.0, 10.0 + CHECKBOX_IMG_SIZE.x],
                .. Metrics::default()
            },
        },
        ([.CHECKBOX.HOVER]) (priority = 200) {
            layer_opacity[0]: 1.0,
        },
        ([.CHECKBOX.ACTIVE]) (priority = 200) {
            #[dyn] layer_img[0]: Some(recolor_tint(&assets::CHECKBOX_LIGHT_ACT)),
        },
        ([.CHECKBOX.CHECKED]) (priority = 300) {
            #[dyn] layer_img[0]: Some(recolor_tint(&assets::CHECKBOX_LIGHT_CHECKED)),
        },
        ([.CHECKBOX.ACTIVE.CHECKED]) (priority = 400) {
            #[dyn] layer_img[0]: Some(recolor_tint(&assets::CHECKBOX_LIGHT_CHECKED_ACT)),
        },

        // Radio button (identical to checkbox except for images)
        ([.RADIO_BUTTON]) (priority = 100) {
            num_layers: 1,
            #[dyn] layer_img[0]: Some(recolor_tint(&assets::RADIO_LIGHT)),
            layer_metrics[0]: Metrics {
                margin: [NAN, NAN, NAN, 4.0],
                size: CHECKBOX_IMG_SIZE,
            },
            layer_opacity[0]: 0.9,
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [3.0, 8.0, 3.0, 10.0 + CHECKBOX_IMG_SIZE.x],
                .. Metrics::default()
            },
        },
        ([.RADIO_BUTTON.HOVER]) (priority = 200) {
            layer_opacity[0]: 1.0,
        },
        ([.RADIO_BUTTON.ACTIVE]) (priority = 200) {
            #[dyn] layer_img[0]: Some(recolor_tint(&assets::RADIO_LIGHT_ACT)),
        },
        ([.RADIO_BUTTON.CHECKED]) (priority = 300) {
            #[dyn] layer_img[0]: Some(recolor_tint(&assets::RADIO_LIGHT_CHECKED)),
        },
        ([.RADIO_BUTTON.ACTIVE.CHECKED]) (priority = 400) {
            #[dyn] layer_img[0]: Some(recolor_tint(&assets::RADIO_LIGHT_CHECKED_ACT)),
        },

        // Checkbox label
        ([] < [.CHECKBOX]) (priority = 100) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 1.0),
        },
        ([] < [.RADIO_BUTTON]) (priority = 100) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 1.0),
        },

        // Entry wrapper
        ([.ENTRY]) (priority = 100) {
            num_layers: 2,

            // Focus ring
            #[dyn] layer_img[0]: Some(himg_figures![rect([0.2, 0.4, 0.9, 1.0]).radius(5.0)]),
            layer_center[0]: box2! { point: [0.5, 0.5] },
            layer_opacity[0]: 0.0,
            layer_metrics[0]: Metrics {
                margin: [-2.0; 4],
                ..Metrics::default()
            },

            // Background
            #[dyn] layer_img[1]: Some(himg_figures![rect([1.0, 1.0, 1.0, 1.0]).radius(3.0)]),
            layer_center[1]: box2! { point: [0.5, 0.5] },
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [0.0; 4],
                size: Vector2::new(NAN, FIELD_HEIGHT),
            },
        },
        ([.ENTRY.FOCUS]) (priority = 200) {
            layer_opacity[0]: 0.5,
        },
        // Entry text
        ([] < [.ENTRY]) (priority = 100) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 1.0),
        },

        // Scrollbar
        ([.SCROLLBAR]) (priority = 100) {
            num_layers: 1,
            layer_metrics[0]: Metrics {
                margin: [SCROLLBAR_MARGIN; 4],
                .. Metrics::default()
            },
            layer_opacity[0]: 0.0,
        },
        ([.SCROLLBAR.HOVER]) (priority = 150) {
            layer_opacity[0]: 1.0,
        },
        ([.SCROLLBAR:not(.VERTICAL)]) (priority = 100) {
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [SCROLLBAR_MARGIN; 4],
                size: Vector2::new(NAN, SCROLLBAR_VISUAL_WIDTH),
            },
            #[dyn] layer_img[0]: Some(himg_from_figures_with_size(
                figures![
                    rect([0.5, 0.5, 0.5, 0.12]).radius(SCROLLBAR_VISUAL_RADIUS)
                ],
                [SCROLLBAR_VISUAL_WIDTH + 2.0, SCROLLBAR_VISUAL_WIDTH],
            )),
            layer_center[0]: box2! { min: [0.5, 0.0], max: [0.5, 1.0] },
        },
        ([.SCROLLBAR.VERTICAL]) (priority = 100) {
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [SCROLLBAR_MARGIN; 4],
                size: Vector2::new(SCROLLBAR_VISUAL_WIDTH, NAN),
            },
            #[dyn] layer_img[0]: Some(himg_from_figures_with_size(
                figures![
                    rect([0.5, 0.5, 0.5, 0.12]).radius(SCROLLBAR_VISUAL_RADIUS)
                ],
                [SCROLLBAR_VISUAL_WIDTH, SCROLLBAR_VISUAL_WIDTH + 2.0],
            )),
            layer_center[0]: box2! { min: [0.0, 0.5], max: [1.0, 0.5] },
        },
        // Scrollbar thumb
        ([] < [.SCROLLBAR]) (priority = 100) {
            num_layers: 1,
            layer_opacity[0]: 0.6,
        },
        ([] < [.SCROLLBAR:not(.VERTICAL)]) (priority = 100) {
            min_size: Vector2::new(SCROLLBAR_LEN_MIN, 0.0),
            #[dyn] layer_img[0]: Some(himg_from_figures_with_size(
                figures![
                    rect([0.5, 0.5, 0.5, 0.7]).radius(SCROLLBAR_VISUAL_RADIUS)
                ],
                [SCROLLBAR_VISUAL_WIDTH + 2.0, SCROLLBAR_VISUAL_WIDTH],
            )),
            layer_center[0]: box2! { min: [0.5, 0.0], max: [0.5, 1.0] },
        },
        ([] < [.SCROLLBAR.VERTICAL]) (priority = 100) {
            min_size: Vector2::new(0.0, SCROLLBAR_LEN_MIN),
            #[dyn] layer_img[0]: Some(himg_from_figures_with_size(
                figures![
                    rect([0.5, 0.5, 0.5, 0.7]).radius(SCROLLBAR_VISUAL_RADIUS)
                ],
                [SCROLLBAR_VISUAL_WIDTH, SCROLLBAR_VISUAL_WIDTH + 2.0],
            )),
            layer_center[0]: box2! { min: [0.0, 0.5], max: [1.0, 0.5] },
        },

        ([] < [.SCROLLBAR.HOVER]) (priority = 150) {
            layer_opacity[0]: 0.9,
        },
        ([] < [.SCROLLBAR.ACTIVE]) (priority = 200) {
            layer_opacity[0]: 1.0,
        },

        // Scroll container
        ([.SCROLL_CONTAINER]) (priority = 100) {
            subview_metrics[roles::GENERIC]: Metrics {
                margin: [0.0; 4],
                .. Metrics::default()
            },
            subview_metrics[roles::HORZ_SCROLLBAR]: Metrics {
                // Dock to the bottom side. Avoid the vertical scrollbar
                margin: [NAN, 16.0, 0.0, 0.0],
                .. Metrics::default()
            },
            subview_metrics[roles::VERT_SCROLLBAR]: Metrics {
                // Dock to the right side. Avoid the horizontal scrollbar
                margin: [0.0, 0.0, 16.0, NAN],
                .. Metrics::default()
            },
        },
        ([.SCROLL_CONTAINER:not(.HAS_HORIZONTAL_SCROLLBAR)]) (priority = 200) {
            subview_visibility[roles::HORZ_SCROLLBAR]: false,
            subview_metrics[roles::VERT_SCROLLBAR]: Metrics {
                // Dock to the right side
                margin: [0.0, 0.0, 0.0, NAN],
                .. Metrics::default()
            },
        },
        ([.SCROLL_CONTAINER:not(.HAS_VERTICAL_SCROLLBAR)]) (priority = 200) {
            subview_visibility[roles::VERT_SCROLLBAR]: false,
            subview_metrics[roles::HORZ_SCROLLBAR]: Metrics {
                // Dock to the bottom side
                margin: [NAN, 0.0, 0.0, 0.0],
                .. Metrics::default()
            },
        },

        // Splitter
        ([#SPLITTER]) (priority = 100) {
            num_layers: 1,
            layer_bg_color[0]: RGBAF32::new(0.5, 0.5, 0.5, 0.8),
            min_size: Vector2::new(1.0, 1.0),
        },
    };
}

pub(crate) struct DefaultStylesheet;

impl Stylesheet for DefaultStylesheet {
    fn match_rules(&self, path: &ElemClassPath, out_rules: &mut dyn FnMut(RuleId)) {
        DEFAULT_STYLESHEET.match_rules(path, out_rules)
    }

    fn get_rule_priority(&self, id: RuleId) -> Option<i16> {
        DEFAULT_STYLESHEET.get_rule_priority(id)
    }
    fn get_rule_prop_kinds(&self, id: RuleId) -> Option<PropKindFlags> {
        DEFAULT_STYLESHEET.get_rule_prop_kinds(id)
    }
    fn get_rule_prop_value(&self, id: RuleId, prop: &Prop) -> Option<Option<&PropValue>> {
        DEFAULT_STYLESHEET.get_rule_prop_value(id, prop)
    }
}
