use lazy_static::lazy_static;

use super::{
    manager::PropKindFlags,
    style::{ClassSet, ElemClassPath, Metrics, Prop, PropValue, Role},
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
    /// Each element corresponds an element in `rules` with an identical index.
    pub ruleprops: Vec<RuleProps>,
}

/// The properties specified by a rule.
#[doc(hidden)]
#[derive(Debug)]
pub struct RuleProps {
    pub props: Vec<(Prop, PropValue)>,
}

#[doc(hidden)]
#[derive(Debug)]
pub struct Rule {
    pub priority: i16,
    pub prop_kinds: PropKindFlags,
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
        self.ruleprops.get(id).map(|r| r.get_prop_value(prop))
    }
}

impl Rule {
    fn priority(&self) -> i16 {
        self.priority
    }

    fn prop_kinds(&self) -> PropKindFlags {
        self.prop_kinds
    }
}

impl RuleProps {
    pub fn new(items: Vec<(Prop, PropValue)>) -> Self {
        Self { props: items }
    }

    fn get_prop_value(&self, prop: &Prop) -> Option<&PropValue> {
        // TODO: Use binary search?
        self.props.iter().find(|p| p.0 == *prop).map(|p| &p.1)
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

/// This macro is used for two purposes:
///  - To create `PropKindFlags`.
///  - To create `(Prop, PropValue)`.
#[doc(hidden)]
#[macro_export]
macro_rules! prop {
    (@kind num_layers) => {
        $crate::ui::theming::Prop::NumLayers.kind_flags()
    };
    (num_layers: $val:expr) => {
        (
            $crate::ui::theming::Prop::NumLayers,
            $crate::ui::theming::PropValue::Usize($val),
        )
    };

    (@kind layer_img[$i:expr]) => {
        $crate::ui::theming::Prop::LayerImg($i).kind_flags()
    };
    (layer_img[$i:expr]: $val:expr) => {
        (
            $crate::ui::theming::Prop::LayerImg($i),
            $crate::ui::theming::PropValue::Himg($val),
        )
    };

    (@kind layer_center[$i:expr]) => {
        $crate::ui::theming::Prop::LayerCenter($i).kind_flags()
    };
    (layer_center[$i:expr]: $val:expr) => {
        (
            $crate::ui::theming::Prop::LayerCenter($i),
            $crate::ui::theming::PropValue::Box2($val),
        )
    };

    (@kind layer_xform[$i:expr]) => {
        $crate::ui::theming::Prop::LayerXform($i).kind_flags()
    };
    (layer_xform[$i:expr]: $val:expr) => {
        (
            $crate::ui::theming::Prop::LayerXform($i),
            $crate::ui::theming::PropValue::LayerXform($val),
        )
    };

    (@kind layer_opacity[$i:expr]) => {
        $crate::ui::theming::Prop::LayerOpacity($i).kind_flags()
    };
    (layer_opacity[$i:expr]: $val:expr) => {
        (
            $crate::ui::theming::Prop::LayerOpacity($i),
            $crate::ui::theming::PropValue::Float($val),
        )
    };

    (@kind layer_bg_color[$i:expr]) => {
        $crate::ui::theming::Prop::LayerBgColor($i).kind_flags()
    };
    (layer_bg_color[$i:expr]: $val:expr) => {
        (
            $crate::ui::theming::Prop::LayerBgColor($i),
            $crate::ui::theming::PropValue::Rgbaf32($val),
        )
    };

    (@kind layer_metrics[$i:expr]) => {
        $crate::ui::theming::Prop::LayerMetrics($i).kind_flags()
    };
    (layer_metrics[$i:expr]: $val:expr) => {
        (
            $crate::ui::theming::Prop::LayerMetrics($i),
            $crate::ui::theming::PropValue::Metrics($val),
        )
    };

    (@kind subview_metrics[$i:expr]) => {
        $crate::ui::theming::Prop::SubviewMetrics($i).kind_flags()
    };
    (subview_metrics[$i:expr]: $val:expr) => {
        (
            $crate::ui::theming::Prop::SubviewMetrics($i),
            $crate::ui::theming::PropValue::Metrics($val),
        )
    };

    (@kind subview_visibility[$i:expr]) => {
        $crate::ui::theming::Prop::SubviewVisibility($i).kind_flags()
    };
    (subview_visibility[$i:expr]: $val:expr) => {
        (
            $crate::ui::theming::Prop::SubviewVisibility($i),
            $crate::ui::theming::PropValue::Bool($val),
        )
    };

    (@kind min_size) => {
        $crate::ui::theming::Prop::MinSize.kind_flags()
    };
    (min_size: $val:expr) => {
        (
            $crate::ui::theming::Prop::MinSize,
            $crate::ui::theming::PropValue::Vector2($val),
        )
    };

    (@kind fg_color) => {
        $crate::ui::theming::Prop::FgColor.kind_flags()
    };
    (fg_color: $val:expr) => {
        (
            $crate::ui::theming::Prop::FgColor,
            $crate::ui::theming::PropValue::Rgbaf32($val),
        )
    };

    (@kind font) => {
        $crate::ui::theming::Prop::Font.kind_flags()
    };
    (font: $val:expr) => {
        (
            $crate::ui::theming::Prop::Font,
            $crate::ui::theming::PropValue::SysFontType($val),
        )
    };
}

/// Construct a `Vec<(Prop, PropValue)>`.
#[doc(hidden)]
#[macro_export]
macro_rules! props {
    ($( $name:ident $([$param:expr])* : $value:expr ),* $(,)* ) => {
        vec![
            $( $crate::prop!($name $([$param])* : $value ), )*
        ]
    };
}

/// Accepts the same syntax as `props`, but produces `PropKindFlags` instead.
#[doc(hidden)]
#[macro_export]
macro_rules! prop_kinds {
    ($( $name:ident $([$param:expr])* : $value:expr ),* $(,)* ) => {
        $crate::ui::theming::PropKindFlags::from_bits_truncate(
            // 0 | x | y | z | ...
            0
            $(
                |
                $crate::prop!(@kind $name $([$param])*).bits()
            )*
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! rule {
    (
        ($($sel:tt)*) (priority = $pri:expr) {
            $($props:tt)*
        }
    ) => {
        $crate::ui::theming::Rule {
            priority: $pri,
            prop_kinds: $crate::prop_kinds! { $($props)* },
            selector: $crate::sel!($($sel)*),
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! ruleprops {
    (
        ($($sel:tt)*) (priority = $pri:expr) {
            $($props:tt)*
        }
    ) => {{
        $crate::ui::theming::RuleProps {
            props: $crate::props! { $($props)* },
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
///     let stylesheet = stylesheet! {
///         // Selector are similar to CSS, but use predefined symbols instead.
///         //  - ID values (`CUSTOM_ID`) are constant expressions. They must be
///         //    a single token tree.
///         //  - Class bits (`LABEL``, etc.) are bare identifiers in `ClassSet`.
///         ([#CUSTOM_ID.LABEL] < [.BUTTON.ACTIVE]) (priority = 100) {
///             // Arbitrary expressions are permitted only as property values
///             // like the following:
///             fg_color: RGBAF32::new(1.0, 1.0, 1.0, 1.0),
///         },
///     };
///
#[macro_export]
macro_rules! stylesheet {
    ($( $( ($( $meta:tt )*) )* { $( $rule:tt )* } ),* $(,)*) => {{
        static RULES: &[$crate::ui::theming::Rule] = &[
            $( $crate::rule!( $(($($meta)*))* {$($rule)*} ), )*
        ];
        $crate::ui::theming::StylesheetMacroOutput {
            rules: RULES,
            ruleprops: std::vec![
                $( $crate::ruleprops!( $(($($meta)*))* {$($rule)*} ), )*
            ],
        }
    }};
}

// -----------------------------------------------------------------------------
//  Default  stylesheet definition
//
// TODO: Make it dynamic (based on the operating system's configuration)
//
use crate::{images::himg_from_rounded_rect, pal::RGBAF32};
use cggeom::box2;
use std::f32::NAN;

lazy_static! {
    static ref DEFAULT_STYLESHEET: StylesheetMacroOutput = stylesheet! {
        ([.BUTTON]) (priority = 100) {
            num_layers: 1,
            layer_img[0]: Some(himg_from_rounded_rect(
                RGBAF32::new(0.7, 0.7, 0.7, 1.0), [[4.0; 2]; 4]
            )),
            layer_center[0]: box2! { point: [0.5, 0.5] },
            layer_opacity[0]: 0.8,
            subview_metrics[Role::Generic]: Metrics {
                margin: [4.0; 4],
                .. Metrics::default()
            },
        },
        ([.BUTTON.HOVER]) (priority = 200) {
            layer_opacity[0]: 1.0,
        },
        ([.BUTTON.ACTIVE]) (priority = 200) {
            layer_img[0]: Some(himg_from_rounded_rect(
                RGBAF32::new(0.2, 0.4, 0.9, 1.0), [[4.0; 2]; 4]
            )),
        },
        // Button label
        ([] < [.BUTTON]) (priority = 100) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 1.0),
        },
        ([] < [.BUTTON.ACTIVE]) (priority = 200) {
            fg_color: RGBAF32::new(1.0, 1.0, 1.0, 1.0),
        },
        // Scrollbar
        ([.SCROLLBAR]) (priority = 100) {
            num_layers: 1,
            layer_img[0]: Some(himg_from_rounded_rect(
                RGBAF32::new(0.5, 0.5, 0.5, 0.12), [[4.0; 2]; 4]
            )),
            layer_metrics[0]: Metrics {
                margin: [4.0; 4],
                .. Metrics::default()
            },
            layer_center[0]: box2! { point: [0.5, 0.5] },
            layer_opacity[0]: 0.0,
        },
        ([.SCROLLBAR.HOVER]) (priority = 150) {
            layer_opacity[0]: 1.0,
        },
        ([.SCROLLBAR:not(.VERTICAL)]) (priority = 100) {
            subview_metrics[Role::Generic]: Metrics {
                margin: [4.0; 4],
                size: [NAN, 8.0].into(),
            },
        },
        ([.SCROLLBAR.VERTICAL]) (priority = 100) {
            subview_metrics[Role::Generic]: Metrics {
                margin: [4.0; 4],
                size: [8.0, NAN].into(),
            },
        },
        // Scrollbar thumb
        ([] < [.SCROLLBAR]) (priority = 100) {
            num_layers: 1,
            layer_img[0]: Some(himg_from_rounded_rect(
                RGBAF32::new(0.5, 0.5, 0.5, 0.7), [[4.0; 2]; 4]
            )),
            layer_center[0]: box2! { point: [0.5, 0.5] },
            layer_opacity[0]: 0.6,
        },
        ([] < [.SCROLLBAR:not(.VERTICAL)]) (priority = 100) {
            min_size: [20.0, 0.0].into(),
        },
        ([] < [.SCROLLBAR.VERTICAL]) (priority = 100) {
            min_size: [0.0, 20.0].into(),
        },

        ([] < [.SCROLLBAR.HOVER]) (priority = 150) {
            layer_opacity[0]: 0.9,
        },
        ([] < [.SCROLLBAR.ACTIVE]) (priority = 200) {
            layer_opacity[0]: 1.0,
        },

        // Scroll container
        ([.SCROLL_CONTAINER]) (priority = 100) {
            subview_metrics[Role::Generic]: Metrics {
                margin: [0.0; 4],
                .. Metrics::default()
            },
            subview_metrics[Role::HorizontalScrollbar]: Metrics {
                // Dock to the bottom side. Avoid the vertical scrollbar
                margin: [NAN, 16.0, 0.0, 0.0],
                .. Metrics::default()
            },
            subview_metrics[Role::VerticalScrollbar]: Metrics {
                // Dock to the right side. Avoid the horizontal scrollbar
                margin: [0.0, 0.0, 16.0, NAN],
                .. Metrics::default()
            },
        },
        ([.SCROLL_CONTAINER:not(.HAS_HORIZONTAL_SCROLLBAR)]) (priority = 200) {
            subview_visibility[Role::HorizontalScrollbar]: false,
            subview_metrics[Role::VerticalScrollbar]: Metrics {
                // Dock to the right side
                margin: [0.0, 0.0, 0.0, NAN],
                .. Metrics::default()
            },
        },
        ([.SCROLL_CONTAINER:not(.HAS_VERTICAL_SCROLLBAR)]) (priority = 200) {
            subview_visibility[Role::VerticalScrollbar]: false,
            subview_metrics[Role::HorizontalScrollbar]: Metrics {
                // Dock to the bottom side
                margin: [NAN, 0.0, 0.0, 0.0],
                .. Metrics::default()
            },
        },

        // Splitter
        ([.SPLITTER]) (priority = 100) {
            num_layers: 1,
            layer_bg_color[0]: [0.5, 0.5, 0.5, 0.8].into(),
            min_size: [1.0, 1.0].into(),
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
