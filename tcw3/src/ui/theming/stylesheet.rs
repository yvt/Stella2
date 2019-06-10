use lazy_static::lazy_static;

use super::{
    manager::PropKindFlags,
    style::{ClassSet, ElemClassPath, Metrics, Prop, PropValue, Role},
};

/// Represents a single stylesheet rule in [`Stylesheet`].
pub type RuleId = usize;

pub(crate) trait Stylesheet {
    /// Enumerate rules that apply to the specifed `ElemClassPath`.
    ///
    /// `out_rules` is called with a `RuleId` for each rule that applies to the
    /// specified `ElemClassPath`. The `RuleId` is specific to `self`.
    fn match_rules(&self, path: &ElemClassPath, out_rules: &mut dyn FnMut(RuleId));

    /// Get the priority of a stylesheet rule in this `Stylesheet`.
    ///
    /// Returns `None` if `id` is invalid.
    fn get_rule_priority(&self, id: RuleId) -> Option<i32>;

    /// Get a `PropKindFlags` representing an approximate set of styling
    /// properties specified by a stylesheet rule in this `Stylesheet`.
    ///
    /// Returns `None` if `id` is invalid.
    fn get_rule_prop_kinds(&self, id: RuleId) -> Option<PropKindFlags>;

    /// Get a property value for `Prop` specified by a stylesheet rule in this
    /// `Stylesheet`.
    fn get_rule_prop_value(&self, id: RuleId, prop: &Prop) -> Option<Option<&PropValue>>;
}

#[derive(Debug)]
struct StaticStylesheet {
    rules: &'static [StaticRule],
}

#[derive(Debug)]
struct StaticRule {
    priority: i32,
    prop_kinds: PropKindFlags,
    selector: Selector,
    props: &'static [RuleProp],
}

#[derive(Debug)]
struct Selector {
    target: ElemCriteria,
    ancestors: &'static [(bool, ElemCriteria)],
}

#[derive(Debug)]
struct ElemCriteria {
    pos: ClassSet,
    neg: ClassSet,
}

#[derive(Debug)]
struct RuleProp {
    prop: Prop,
    value: PropValue,
}

impl Stylesheet for StaticStylesheet {
    fn match_rules(&self, path: &ElemClassPath, out_rules: &mut dyn FnMut(RuleId)) {
        // TODO: optimize the selector matching using target class buckets or
        //       DFA + BDD
        for (i, rule) in self.rules.iter().enumerate() {
            if rule.selector.matches(path) {
                out_rules(i);
            }
        }
    }

    fn get_rule_priority(&self, id: RuleId) -> Option<i32> {
        self.rules.get(id).map(StaticRule::priority)
    }
    fn get_rule_prop_kinds(&self, id: RuleId) -> Option<PropKindFlags> {
        self.rules.get(id).map(StaticRule::prop_kinds)
    }
    fn get_rule_prop_value(&self, id: RuleId, prop: &Prop) -> Option<Option<&PropValue>> {
        self.rules.get(id).map(|r| r.get_prop_value(prop))
    }
}

impl StaticRule {
    fn priority(&self) -> i32 {
        self.priority
    }

    fn prop_kinds(&self) -> PropKindFlags {
        self.prop_kinds
    }

    fn get_prop_value(&self, prop: &Prop) -> Option<&PropValue> {
        // TODO: Use binary search?
        self.props
            .iter()
            .find(|p| p.prop == *prop)
            .map(|p| &p.value)
    }
}

impl Selector {
    fn matches(&self, path: &ElemClassPath) -> bool {
        if !self.target.matches(&path.class_set) {
            return false;
        }

        let mut cur_maybe = &path.tail;

        for (direct, criteria) in self.ancestors.iter() {
            if *direct {
                if let Some(cur) = cur_maybe {
                    if !criteria.matches(&cur.class_set) {
                        return false;
                    }
                    cur_maybe = &cur.tail;
                } else {
                    return false;
                }
            } else {
                loop {
                    if let Some(cur) = cur_maybe {
                        if criteria.matches(&cur.class_set) {
                            cur_maybe = &cur.tail;
                            break;
                        } else {
                            cur_maybe = &cur.tail;
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
//  Default stylesheet definition
//
// TODO: Make it dynamic (based on the operating system's configuration)
macro_rules! elem_pos {
    (.$cls:ident $($rest:tt)*) => {
        ClassSet::$cls | elem_pos!($($rest)*)
    };
    (:not(.$cls:ident) $($rest:tt)*) => {
        elem_pos!($($rest)*)
    };
    () => {
        ClassSet::empty()
    };
}

macro_rules! elem_neg {
    (:not(.$cls:ident) $($rest:tt)*) => {
        ClassSet::$cls | elem_neg!($($rest)*)
    };
    (.$cls:ident $($rest:tt)*) => {
        elem_neg!($($rest)*)
    };
    () => {
        ClassSet::empty()
    };
}

macro_rules! elem {
    ($($classes:tt)*) => {ElemCriteria {
        pos: elem_pos!($($classes)*),
        neg: elem_neg!($($classes)*),
    }};
}

macro_rules! sel_ancestor {
    (< $($rest:tt)*) => {(true, elem!($($rest)*))};
    (.. $($rest:tt)*) => {(false, elem!($($rest)*))};
}

macro_rules! count {
    ($e:tt $($rest:tt)*) => {1 + count!($($rest)*)};
    () => {0};
}

macro_rules! sel {
    (
        [$($cur:tt)*]
        $( $mode:tt [ $($ancestor:tt)* ] )*
    ) => {{
        lazy_static! {
            static ref ANCESTORS: [(bool, ElemCriteria); count!($($mode)*)] = [
                $( sel_ancestor!( $mode $($ancestor)* ) ),*
            ];
        }
        Selector {
            target: elem!($($cur)*),
            ancestors: &*ANCESTORS,
        }
    }};
}

macro_rules! prop {
    (num_layers: $val:expr) => {
        RuleProp {
            prop: Prop::NumLayers,
            value: PropValue::Usize($val),
        }
    };
    (layer_img[$i:expr]: $val:expr) => {
        RuleProp {
            prop: Prop::LayerImg($i),
            value: PropValue::Himg($val),
        }
    };
    (layer_center[$i:expr]: $val:expr) => {
        RuleProp {
            prop: Prop::LayerCenter($i),
            value: PropValue::Box2($val),
        }
    };
    (layer_bg_color[$i:expr]: $val:expr) => {
        RuleProp {
            prop: Prop::LayerBgColor($i),
            value: PropValue::Rgbaf32($val),
        }
    };
    (subview_metrics[$i:expr]: $val:expr) => {
        RuleProp {
            prop: Prop::SubviewMetrics($i),
            value: PropValue::Metrics($val),
        }
    };
    (fg_color: $val:expr) => {
        RuleProp {
            prop: Prop::FgColor,
            value: PropValue::Rgbaf32($val),
        }
    };
}

macro_rules! props {
    ($( $name:ident $([$param:expr])* : $value:expr ),* $(,)* ) => {{
        lazy_static! {
            static ref PROPS: [RuleProp; count!($( $name )*)] = [
                $( prop!($name $([$param])* : $value ), )*
            ];
        }
        &*PROPS
    }};
}

macro_rules! rule {
    (
        ($($sel:tt)*) (priority = $pri:expr) {
            $($props:tt)*
        }
    ) => {{
        let props = props! { $($props)* };

        StaticRule {
            priority: $pri,
            prop_kinds: props.iter()
                .map(|p| p.prop.kind_flags())
                .fold(PropKindFlags::empty(), |x, y| x | y),
            selector: sel!($($sel)*),
            props,
        }
    }};
}

macro_rules! stylesheet {
    ($( $( ($( $meta:tt )*) )* { $( $rule:tt )* } ),* $(,)*) => {{
        lazy_static! {
            static ref RULES: [StaticRule; count!($({ $($rule)* })*)] = [
                $( rule!( $(($($meta)*))* {$($rule)*} ), )*
            ];
        }
        StaticStylesheet { rules: &*RULES }
    }};
}

use crate::{pal::RGBAF32, ui::images::himg_from_rounded_rect};
use cggeom::box2;

lazy_static! {
    static ref DEFAULT_STYLESHEET: StaticStylesheet = stylesheet! {
        ([.BUTTON]) (priority = 1) {
            num_layers: 1,
            layer_img[0]: Some(himg_from_rounded_rect(
                RGBAF32::new(0.7, 0.7, 0.7, 1.0), [[4.0; 2]; 4]
            )),
            layer_center[0]: box2! { point: [0.5, 0.5] },
            subview_metrics[Role::Generic]: Metrics {
                margin: [4.0; 4],
                .. Metrics::default()
            },
        },
        ([.BUTTON.ACTIVE]) (priority = 100) {
            layer_img[0]: Some(himg_from_rounded_rect(
                RGBAF32::new(0.2, 0.4, 0.9, 1.0), [[4.0; 2]; 4]
            )),
        },
        // Button label
        ([] < [.BUTTON]) (priority = 1) {
            fg_color: RGBAF32::new(0.0, 0.0, 0.0, 1.0),
        },
        ([] < [.BUTTON.ACTIVE]) (priority = 100) {
            fg_color: RGBAF32::new(1.0, 1.0, 1.0, 1.0),
        },
    };
}

pub(crate) struct DefaultStylesheet;

impl Stylesheet for DefaultStylesheet {
    fn match_rules(&self, path: &ElemClassPath, out_rules: &mut dyn FnMut(RuleId)) {
        DEFAULT_STYLESHEET.match_rules(path, out_rules)
    }

    fn get_rule_priority(&self, id: RuleId) -> Option<i32> {
        DEFAULT_STYLESHEET.get_rule_priority(id)
    }
    fn get_rule_prop_kinds(&self, id: RuleId) -> Option<PropKindFlags> {
        DEFAULT_STYLESHEET.get_rule_prop_kinds(id)
    }
    fn get_rule_prop_value(&self, id: RuleId, prop: &Prop) -> Option<Option<&PropValue>> {
        DEFAULT_STYLESHEET.get_rule_prop_value(id, prop)
    }
}
