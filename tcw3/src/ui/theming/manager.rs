use bitflags::bitflags;
use sorted_diff::{sorted_diff, In};
use std::{cell::RefCell, fmt};
use subscriber_list::{SubscriberList, UntypedSubscription as Sub};

use super::{
    style::{ElemClassPath, Prop, PropValue},
    stylesheet::{DefaultStylesheet, RuleId, Stylesheet},
};
use crate::{pal, pal::prelude::*};

pub(crate) type SheetId = usize;

pub(crate) type ManagerEvtHandler = Box<dyn Fn(pal::WM, &Manager)>;

/// The center of the theming system.
///
/// `Manager` stores the currently active stylesheet set ([`SheetSet`]), which
/// is usually applied to entire the application. When it's changed, it sends
/// out a notification via the callback functions registered via
/// `subscribe_sheet_set_changed`.
pub struct Manager {
    wm: pal::WM,
    sheet_set: SheetSet,
    set_change_handlers: RefCell<SubscriberList<ManagerEvtHandler>>,
}

impl fmt::Debug for Manager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Manager")
            .field("wm", &self.wm)
            .field("sheet_set", &())
            .field("set_change_handlers", &())
            .finish()
    }
}

// TODO: Call `set_change_handlers` when `sheet_set` is updated

mt_lazy_static! {
    static ref GLOBAL_MANAGER: Manager => Manager::new;
}

impl Manager {
    fn new(wm: pal::WM) -> Self {
        Self {
            wm,
            sheet_set: SheetSet {
                sheets: vec![Box::new(DefaultStylesheet)],
            },
            set_change_handlers: RefCell::new(SubscriberList::new()),
        }
    }

    /// Get a global instance of `Manager`.
    pub fn global(wm: pal::WM) -> &'static Self {
        GLOBAL_MANAGER.get_with_wm(wm)
    }

    /// Register a handler function called when `sheet_set()` is updated with a
    /// new sheet set.
    pub(crate) fn subscribe_sheet_set_changed(&self, cb: ManagerEvtHandler) -> Sub {
        self.set_change_handlers.borrow_mut().insert(cb).untype()
    }

    /// Get the currently active sheet set.
    ///
    /// This may change throughout the application's lifecycle. Use
    /// `subscribe_sheet_set_changed` to get notified when it happens.
    pub(crate) fn sheet_set<'a>(&'a self) -> impl std::ops::Deref<Target = SheetSet> + 'a {
        &self.sheet_set
    }
}

/// A stylesheet set.
pub(crate) struct SheetSet {
    sheets: Vec<Box<dyn Stylesheet>>,
}

impl SheetSet {
    fn match_rules(&self, path: &ElemClassPath, out_rules: &mut dyn FnMut(SheetId, RuleId)) {
        for (i, sheet) in self.sheets.iter().enumerate() {
            sheet.match_rules(path, &mut |rule_id| out_rules(i, rule_id));
        }
    }

    fn get_rule(&self, id: (SheetId, RuleId)) -> Option<Rule<'_>> {
        self.sheets.get(id.0).and_then(|stylesheet| {
            if stylesheet.get_rule_priority(id.1).is_some() {
                Some(Rule {
                    stylesheet: &**stylesheet,
                    rule_id: id.1,
                })
            } else {
                None
            }
        })
    }
}

#[derive(Clone, Copy)]
struct Rule<'a> {
    stylesheet: &'a dyn Stylesheet,
    rule_id: RuleId,
}

impl Rule<'_> {
    fn priority(&self) -> i32 {
        self.stylesheet.get_rule_priority(self.rule_id).unwrap()
    }
    fn prop_kinds(&self) -> PropKindFlags {
        self.stylesheet.get_rule_prop_kinds(self.rule_id).unwrap()
    }
    fn get_prop_value(&self, prop: &Prop) -> Option<&PropValue> {
        self.stylesheet
            .get_rule_prop_value(self.rule_id, prop)
            .unwrap()
    }
}

bitflags! {
    /// Represents categories of updated styling properties.
    ///
    /// When an `Elem`'s class set is updated, we must figure out which
    /// properties have to be recomputed. It would be inefficient to precisely
    /// track every property, so we categorize the properties into coarse groups
    /// and track changes in this unit.
    pub(crate) struct PropKindFlags: u16 {
        const NUM_LAYERS = 1 << 0;
        const LAYER_IMG = 1 << 1;
        const LAYER_BOUNDS = 1 << 2;
        const LAYER_BG_COLOR = 1 << 3;
        const LAYER_OPACITY = 1 << 4;
        const LAYER_CENTER = 1 << 5;
        const LAYER_XFORM = 1 << 6;
        /// Any properties of decorative layers.
        const LAYER_ALL = Self::NUM_LAYERS.bits |
            Self::LAYER_IMG.bits |
            Self::LAYER_BOUNDS.bits |
            Self::LAYER_BG_COLOR.bits |
            Self::LAYER_OPACITY.bits |
            Self::LAYER_CENTER.bits |
            Self::LAYER_XFORM.bits;
        const CLIP_LAYER = 1 << 7;
        const LAYOUT = 1 << 8;
        const FONT = 1 << 9;
        const FG_COLOR = 1 << 10;
    }
}

impl Prop {
    pub(crate) fn kind_flags(&self) -> PropKindFlags {
        match *self {
            Prop::NumLayers => PropKindFlags::LAYER_ALL,
            Prop::LayerImg(_) => PropKindFlags::LAYER_IMG,
            Prop::LayerBgColor(_) => PropKindFlags::LAYER_BG_COLOR,
            Prop::LayerMetrics(_) => PropKindFlags::LAYER_BOUNDS,
            Prop::LayerOpacity(_) => PropKindFlags::LAYER_OPACITY,
            Prop::LayerCenter(_) => PropKindFlags::LAYER_CENTER,
            Prop::LayerXform(_) => PropKindFlags::LAYER_XFORM,
            Prop::SubviewMetrics(_) => PropKindFlags::LAYOUT,
            Prop::MinSize => PropKindFlags::LAYOUT,
            Prop::ClipMetrics => PropKindFlags::CLIP_LAYER,
            Prop::FgColor => PropKindFlags::FG_COLOR,
            Prop::Font => PropKindFlags::FONT,
        }
    }
}

// TODO: Flesh out the interface of `Elem` and make it `pub`

/// Represents a styled element.
///
/// This type tracks the currently-active rule set of a styled element.
#[derive(Debug)]
pub(crate) struct Elem {
    // Currently-active rules, sorted by a lexicographical order.
    rules: Vec<(SheetId, RuleId)>,
    // Currently-active rules, sorted by an ascending order of priority.
    rules_by_prio: Vec<(SheetId, RuleId)>,
}

impl Elem {
    /// Construct an `Elem`.
    pub(crate) fn new() -> Self {
        Self {
            rules: Vec::new(),
            rules_by_prio: Vec::new(),
        }
    }

    /// Assign a new `ElemClassPath` and recalculate the active rule set.
    pub(crate) fn set_class_path(&mut self, sheet_set: &SheetSet, new_class_path: &ElemClassPath) {
        self.rules.clear();
        sheet_set.match_rules(new_class_path, &mut |sheet_id, rule_id| {
            self.rules.push((sheet_id, rule_id));
        });
        self.rules.sort_unstable();
        self.update_rules_by_prio(sheet_set);
    }

    /// Assign a new `ElemClassPath` and recalculate the active rule set.
    ///
    /// This method assumes that the stylesheet set haven't changed since the
    /// last time the active rule set was calculated. If it has changed,
    /// `set_class_path` must be used instead.
    ///
    /// Returns `PropKindFlags` indicating which property might have been
    /// changed.
    pub(crate) fn set_and_diff_class_path(
        &mut self,
        sheet_set: &SheetSet,
        new_class_path: &ElemClassPath,
    ) -> PropKindFlags {
        let mut new_rules = Vec::with_capacity(self.rules.len());
        sheet_set.match_rules(new_class_path, &mut |sheet_id, rule_id| {
            new_rules.push((sheet_id, rule_id));
        });
        new_rules.sort_unstable();

        // Calculate `PropKindFlags`
        let mut flags = PropKindFlags::empty();
        for diff in sorted_diff(self.rules.iter(), new_rules.iter()) {
            match diff {
                In::Left(&id) | In::Right(&id) => {
                    flags |= sheet_set.get_rule(id).unwrap().prop_kinds();
                }
                In::Both(_, _) => {}
            }
        }

        if flags.is_empty() {
            // No changes
            return flags;
        }

        self.rules = new_rules;
        self.update_rules_by_prio(sheet_set);

        flags
    }

    /// Update `self.rules_by_prio`.
    fn update_rules_by_prio(&mut self, sheet_set: &SheetSet) {
        self.rules_by_prio.clear();
        self.rules_by_prio.extend(self.rules.iter().cloned());
        self.rules_by_prio
            .sort_unstable_by_key(|id| sheet_set.get_rule(*id).unwrap().priority());
    }

    /// Get the computed value of the specified styling property.
    pub(crate) fn compute_prop(&self, sheet_set: &SheetSet, prop: Prop) -> PropValue {
        let mut computed_value = PropValue::default_for_prop(&prop);
        let kind = prop.kind_flags();

        for &id in self.rules_by_prio.iter() {
            let rule = sheet_set.get_rule(id).unwrap();
            if rule.prop_kinds().intersects(kind) {
                if let Some(specified_value) = rule.get_prop_value(&prop) {
                    computed_value = specified_value.clone();
                }
            }
        }

        computed_value
    }
}
