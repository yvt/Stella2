use bitflags::bitflags;
use sorted_diff::{sorted_diff, In};
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};
use subscriber_list::{SubscriberList, UntypedSubscription as Sub};
use tcw3_pal::mt_lazy_static;

use super::{
    style::{ClassSet, ElemClassPath, Prop, PropValue},
    stylesheet::{DefaultStylesheet, RuleId, Stylesheet},
};
use crate::{pal, pal::prelude::*};

pub(crate) type SheetId = usize;

pub(crate) type ManagerCb = Box<dyn Fn(pal::WM, &Manager)>;

pub type ManagerNewSheetSetCb = Box<dyn Fn(pal::WM, &Manager, &mut NewSheetSetCtx<'_>)>;

/// The center of the theming system.
///
/// `Manager` stores the currently active stylesheet set ([`SheetSet`]), which
/// is usually applied to entire the application. When it's changed, it sends
/// out a notification via the callback functions registered via
/// `subscribe_sheet_set_changed`.
pub struct Manager {
    wm: pal::WM,
    sheet_set: RefCell<SheetSet>,
    set_change_handlers: RefCell<SubscriberList<ManagerCb>>,
    new_set_handlers: RefCell<SubscriberList<ManagerNewSheetSetCb>>,
}

impl fmt::Debug for Manager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Manager")
            .field("wm", &self.wm)
            .field("sheet_set", &())
            .field("set_change_handlers", &())
            .field("new_set_handlers", &())
            .finish()
    }
}

mt_lazy_static! {
    static ref GLOBAL_MANAGER: Manager => Manager::new;
}

impl Manager {
    fn new(wm: pal::WM) -> Self {
        let this = Self {
            wm,
            sheet_set: RefCell::new(SheetSet { sheets: Vec::new() }),
            set_change_handlers: RefCell::new(SubscriberList::new()),
            new_set_handlers: RefCell::new(SubscriberList::new()),
        };

        // Create the first `SheetSet`
        let sheet_set = this.new_sheet_set();
        *this.sheet_set.borrow_mut() = sheet_set;

        this
    }

    /// Get a global instance of `Manager`.
    pub fn global(wm: pal::WM) -> &'static Self {
        GLOBAL_MANAGER.get_with_wm(wm)
    }

    /// Register a handler function called when `sheet_set()` is updated with a
    /// new sheet set.
    pub(crate) fn subscribe_sheet_set_changed(&self, cb: ManagerCb) -> Sub {
        self.set_change_handlers.borrow_mut().insert(cb).untype()
    }

    /// Register a callback function called when a new stylesheet set is being
    /// created.
    ///
    /// The specified function is called when the stylesheet is updated for the
    /// next time, i.e., when the operating system's apperance setting is
    /// updated or `update_sheet_set` is called.
    pub fn subscribe_new_sheet_set(&self, cb: ManagerNewSheetSetCb) -> Sub {
        self.new_set_handlers.borrow_mut().insert(cb).untype()
    }

    /// Force the recreation the stylesheet set.
    pub fn update_sheet_set(&self) {
        let sheet_set = self.new_sheet_set();
        *self.sheet_set.borrow_mut() = sheet_set;

        // Notify the change
        for handler in self.set_change_handlers.borrow().iter() {
            handler(self.wm, self);
        }
    }

    /// Construct a new `SheetSet` using the default stylesheet and
    /// `new_set_handlers`.
    fn new_sheet_set(&self) -> SheetSet {
        let mut sheet_set = SheetSet {
            sheets: vec![Box::new(DefaultStylesheet)],
        };

        for handler in self.new_set_handlers.borrow().iter() {
            handler(
                self.wm,
                self,
                &mut NewSheetSetCtx {
                    sheet_set: &mut sheet_set,
                },
            );
        }

        sheet_set
    }

    /// Get the currently active sheet set.
    ///
    /// This may change throughout the application's lifecycle. Use
    /// `subscribe_sheet_set_changed` to get notified when it happens.
    pub(crate) fn sheet_set<'a>(&'a self) -> impl std::ops::Deref<Target = SheetSet> + 'a {
        self.sheet_set.borrow()
    }
}

/// The context type passed to callback functions of type [`ManagerNewSheetSetCb`].
pub struct NewSheetSetCtx<'a> {
    sheet_set: &'a mut SheetSet,
}

impl NewSheetSetCtx<'_> {
    /// Insert a new `Stylesheet`.
    pub fn insert_stylesheet(&mut self, stylesheet: impl Stylesheet + 'static) {
        self.sheet_set.sheets.push(Box::new(stylesheet));
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
    pub struct PropKindFlags: u16 {
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
    pub fn kind_flags(&self) -> PropKindFlags {
        // Make sure to synchronize these with the `prop!` macro - This is a
        // temporary restriction until `match` inside `const fn` is implemented:
        // <https://github.com/rust-lang/rust/issues/49146>
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

/// Represents a styled element.
///
/// This type tracks the currently-active rule set of a styled element. It
/// subscribes to [`Manager`]'s sheet set change handler and automatically
/// updates the active rule set whenever the sheet set is changed. It tracks
/// changes in properties, and calls the provided [`ElemChangeCb`] whenever
/// styling properties of the corresponding styled element are updated.
#[derive(Debug)]
pub struct Elem {
    inner: Rc<ElemInner>,
}

pub type ElemChangeCb = Box<dyn Fn(pal::WM, PropKindFlags)>;

struct ElemInner {
    sub: Cell<Option<Sub>>,
    style_manager: &'static Manager,
    rules: RefCell<ElemRules>,
    /// The function called when property values might have changed.
    change_handler: RefCell<ElemChangeCb>,
}

#[derive(Debug)]
struct ElemRules {
    class_path: Rc<ElemClassPath>,
    // Currently-active rules, sorted by a lexicographical order.
    rules_by_ord: Vec<(SheetId, RuleId)>,
    // Currently-active rules, sorted by an ascending order of priority.
    rules_by_prio: Vec<(SheetId, RuleId)>,
}

impl fmt::Debug for ElemInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ElemInner")
            .field("sub", &())
            .field("style_manager", &self.style_manager)
            .field("rules", &self.rules)
            .field("change_handler", &((&self.change_handler) as *const _))
            .finish()
    }
}

impl Drop for Elem {
    fn drop(&mut self) {
        if let Some(sub) = self.inner.sub.take().take() {
            sub.unsubscribe().unwrap();
        }
    }
}

impl Elem {
    /// Construct an `Elem`.
    pub fn new(style_manager: &'static Manager) -> Self {
        let this = Self {
            inner: Rc::new(ElemInner {
                sub: Cell::new(None),
                style_manager,
                rules: RefCell::new(ElemRules {
                    class_path: Rc::default(),
                    rules_by_ord: Vec::new(),
                    rules_by_prio: Vec::new(),
                }),
                change_handler: RefCell::new(Box::new(|_, _| {})),
            }),
        };

        // Watch for stylesheet set changes
        let inner = Rc::clone(&this.inner);
        let sub = style_manager.subscribe_sheet_set_changed(Box::new(move |wm, _| {
            // `sheet_set` was changed, update the ative rule set
            let manager = inner.style_manager;
            let sheet_set = manager.sheet_set();
            inner
                .rules
                .borrow_mut()
                .invalidate_rules_and_update(&sheet_set);

            // Notify that any of the properties might have changed
            inner.change_handler.borrow()(wm, PropKindFlags::all());
        }));
        this.inner.sub.set(Some(sub));

        this
    }

    /// Set a callback function called when property values might have changed.
    pub fn set_on_change(&self, handler: ElemChangeCb) {
        *self.inner.change_handler.borrow_mut() = handler;
    }

    /// Get the computed value of the specified styling property.
    pub fn compute_prop(&self, prop: Prop) -> PropValue {
        let manager = self.inner.style_manager;
        let sheet_set = manager.sheet_set();
        self.inner.rules.borrow().compute_prop(&sheet_set, prop)
    }

    /// Assign a new `ElemClassPath` and update the active rule set.
    ///
    /// This might internally call the `ElemChangeCb` registered by
    /// `set_on_change`.
    pub fn set_class_path(&self, new_class_path: Rc<ElemClassPath>) {
        self.update_class_path_with(|class_path| {
            *class_path = new_class_path;
        });
    }

    /// Set the class set and update the active rule set.
    ///
    /// This might internally call the `ElemChangeCb` registered by
    /// `set_on_change`.
    pub fn set_class_set(&self, class_set: ClassSet) {
        self.update_class_path_with(|class_path| {
            let class_path = Rc::make_mut(class_path);
            class_path.class_set = class_set;
        });
    }

    /// Set the parent class path and update the active rule set.
    ///
    /// This might internally call the `ElemChangeCb` registered by
    /// `set_on_change`.
    pub fn set_parent_class_path(&self, parent_class_path: Option<Rc<ElemClassPath>>) {
        self.update_class_path_with(|class_path| {
            let class_path = Rc::make_mut(class_path);
            class_path.tail = parent_class_path;
        });
    }

    /// Get the class set.
    pub fn class_set(&self) -> ClassSet {
        self.inner.rules.borrow().class_path.class_set
    }

    /// Get the class path.
    pub fn class_path(&self) -> Rc<ElemClassPath> {
        Rc::clone(&self.inner.rules.borrow().class_path)
    }

    /// Update the target `ElemClassPath` and update the active rule set.
    fn update_class_path_with(&self, f: impl FnOnce(&mut Rc<ElemClassPath>)) {
        let manager = self.inner.style_manager;
        let sheet_set = manager.sheet_set();

        // Update the active rule set
        let diff = {
            let mut rules = self.inner.rules.borrow_mut();
            f(&mut rules.class_path);
            rules.update(&sheet_set)
        };

        // Notify the change
        if !diff.is_empty() {
            self.inner.change_handler.borrow()(manager.wm, diff);
        }
    }
}

impl ElemRules {
    /// Get the computed value of the specified styling property.
    fn compute_prop(&self, sheet_set: &SheetSet, prop: Prop) -> PropValue {
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

    /// Recalculate the active rule set assuming the existing `rules_by_ord` is
    /// invalid.
    fn invalidate_rules_and_update(&mut self, sheet_set: &SheetSet) {
        // Replace all rules
        let rules_by_ord = &mut self.rules_by_ord;
        rules_by_ord.clear();
        sheet_set.match_rules(&self.class_path, &mut |sheet_id, rule_id| {
            rules_by_ord.push((sheet_id, rule_id));
        });

        rules_by_ord.sort_unstable();
        self.update_rules_by_prio(sheet_set);
    }

    /// Recalculate the active rule set.
    ///
    /// This method assumes that the stylesheet set haven't changed since the
    /// last time the active rule set was calculated. If it has changed,
    /// `set_class_path` must be used instead.
    ///
    /// Returns `PropKindFlags` indicating which property might have been
    /// changed.
    fn update(&mut self, sheet_set: &SheetSet) -> PropKindFlags {
        let mut new_rules = Vec::with_capacity(self.rules_by_ord.len());
        sheet_set.match_rules(&self.class_path, &mut |sheet_id, rule_id| {
            new_rules.push((sheet_id, rule_id));
        });
        new_rules.sort_unstable();

        // Calculate `PropKindFlags`
        let mut flags = PropKindFlags::empty();
        for diff in sorted_diff(self.rules_by_ord.iter(), new_rules.iter()) {
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

        self.rules_by_ord = new_rules;
        self.update_rules_by_prio(sheet_set);

        flags
    }

    /// Update `self.rules_by_prio` based on `self.rules_by_ord`.
    fn update_rules_by_prio(&mut self, sheet_set: &SheetSet) {
        self.rules_by_prio.clear();
        self.rules_by_prio.extend(self.rules_by_ord.iter().cloned());
        self.rules_by_prio
            .sort_unstable_by_key(|id| sheet_set.get_rule(*id).unwrap().priority());
    }
}
