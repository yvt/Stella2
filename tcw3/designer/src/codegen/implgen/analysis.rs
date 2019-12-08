//! Resolves what `Input` points to and provides the analysis result.
use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};

use super::super::{diag::Diag, sem};
use super::Ctx;
use crate::metadata;

pub struct Analysis {
    /// Results indexed by `sem::Input::index`.
    pub inputs: Vec<Option<InputInfo>>,
}

/// The analysis result for one `sem::Input`.
pub enum InputInfo {
    /// The input refers to an event parameter. E.g., `event.mouse_position`.
    EventParam(EventParamInput),
    /// The input refers to a field or event. E.g., `this.prop1`,
    /// `this.subcomponent.activated`.
    Item(ItemInput),
    /// The input refers to the enclosing component's instance. E.g., `this`.
    This,
    /// There was a semantic error encountered during the analysis.
    Invalid,
}

pub struct EventParamInput {
    /// The parameter index for each event trigger listed in the trigger part of
    /// `on (here) ...`. In other words, `param_i[i]` indicates the parameter
    /// index for the `i`-th trigger, which must refer to an event, to get the
    /// input value from.
    pub param_i: Vec<usize>,
}

pub struct ItemInput {
    /// Starts at the enclosing component. Must be non-empty.
    ///
    /// # Example: `this.subcomponent.event` in `Comp1`
    ///
    /// ```rust,no_compile
    /// vec![
    ///     // `(0, 4)` refers to `Comp1` in the current crate.
    ///     // `3` refers to a field named `subcomponent`.
    ///     ItemIndirection { comp_crate_i: 0, comp_i: 4, item_i: 3 },
    ///
    ///     // `(2, 1)` refers to `Comp2` in another crate.
    ///     // `0` refers to an event named `event`.
    ///     ItemIndirection { comp_crate_i: 2, comp_i: 1, item_i: 0 },
    /// ]
    /// ```
    pub indirections: Vec<ItemIndirection>,
}

#[derive(Clone, Copy)]
pub struct ItemIndirection {
    /// An index into `Ctx::crates`.
    pub comp_crate_i: usize,
    /// An index into `ctx.crates[comp_crate_i].comps`.
    pub comp_i: usize,
    /// An index into `ctx.crates[comp_crate_i].comps[comp_i].items`.
    pub item_i: usize,
}

struct AnalysisCtx<'a> {
    ctx: &'a Ctx<'a>,
    diag: &'a mut Diag,
    analysis: &'a mut Analysis,
    cur_comp: &'a metadata::CompDef,
    cur_comp_crate_i: usize,
    cur_comp_i: usize,
}

impl Analysis {
    pub fn new(
        comp: &sem::CompDef<'_>,
        meta_comp: &metadata::CompDef,
        ctx: &Ctx,
        diag: &mut Diag,
    ) -> Self {
        // Find the `(comp_crate_i, comp_i)` of the enclosing component
        // TODO: This value should be passed in lieu of `meta_comp`
        let cur_comp_crate_i = ctx.repo.main_crate_i;
        let cur_comp_i = ctx.repo.crates[cur_comp_crate_i]
            .comps
            .iter()
            .position(|c| std::ptr::eq(c, meta_comp))
            .unwrap();

        let mut this = Self { inputs: Vec::new() };

        let mut actx = AnalysisCtx {
            ctx,
            diag,
            analysis: &mut this,
            cur_comp: meta_comp,
            cur_comp_crate_i,
            cur_comp_i,
        };

        for item in comp.items.iter() {
            match item {
                sem::CompItemDef::Field(item) => match &item.value {
                    None => {}
                    Some(sem::DynExpr::Func(func)) => {
                        analyze_inputs(
                            &mut actx,
                            func.inputs.iter().map(|func_input| &func_input.input),
                            Err(EventTriggerUnavailableReason::NotEventHandler),
                        );
                    }
                    Some(sem::DynExpr::ObjInit(init)) => {
                        analyze_obj_init(&mut actx, init);
                    }
                },
                sem::CompItemDef::On(item) => {
                    analyze_on(&mut actx, item);
                }
                sem::CompItemDef::Event(_) => {}
            }
        }

        this
    }
}

fn analyze_on(actx: &mut AnalysisCtx<'_>, item: &sem::OnDef) {
    analyze_inputs(
        actx,
        item.triggers.iter().filter_map(|trigger| match &trigger {
            sem::Trigger::Init(_) => None,
            sem::Trigger::Input(input) => Some(input),
        }),
        Err(EventTriggerUnavailableReason::NotEventHandler),
    );

    // See if the triggers are event-based. But if there
    // was an error while analyzing the triggers, we also want
    // to know this is the case.
    let event_inputs = item
        .triggers
        .iter()
        .map(|trigger| match trigger {
            sem::Trigger::Init(trigger) => {
                // Definitely not event-based
                Ok(EventTrigger::NotEvent(trigger.span))
            }

            sem::Trigger::Input(input) => {
                let input_anal = actx.analysis.inputs[input.index].as_ref().unwrap();
                match input_anal {
                    // That's invalid as a trigger!
                    InputInfo::EventParam(_) => unreachable!(),

                    // It may be an event... or not.
                    InputInfo::Item(item_input) => {
                        // Find the item the input refers to
                        let ind = item_input.indirections.last().unwrap();
                        let item = &actx.ctx.repo.crates[ind.comp_crate_i].comps[ind.comp_i].items
                            [ind.item_i];

                        if let metadata::CompItemDef::Event(_) = item {
                            // Okay
                            Ok(EventTrigger::Event(*ind, input.span))
                        } else {
                            // Not event-based
                            Ok(EventTrigger::NotEvent(input.span))
                        }
                    }

                    // It's never an event.
                    InputInfo::This => Ok(EventTrigger::NotEvent(input.span)),

                    InputInfo::Invalid => {
                        // We can't give a concrete answer because of
                        // a semantic error
                        Err(EventTriggerUnavailableReason::SemError)
                    }
                }
            }
        })
        .collect::<Result<Vec<_>, _>>();

    let event_inputs = match &event_inputs {
        Ok(x) => Ok(x.as_slice()),
        Err(e) => Err(*e),
    };

    analyze_inputs(
        actx,
        item.func.inputs.iter().map(|func_input| &func_input.input),
        event_inputs,
    );
}

fn analyze_obj_init(actx: &mut AnalysisCtx<'_>, init: &sem::ObjInit) {
    for field in init.fields.iter() {
        analyze_inputs(
            actx,
            field
                .value
                .inputs
                .iter()
                .map(|func_input| &func_input.input),
            Err(EventTriggerUnavailableReason::NotEventHandler),
        );
    }
}

/// Used as an input to `analyze_inputs`. Describes why `[EventTrigger]` is
/// unavailable in a given context.
#[derive(Clone, Copy)]
enum EventTriggerUnavailableReason {
    SemError,

    /// The current position is not an event handler input. All event parameter
    /// references will be reported as an error by `analyze_inputs`.
    NotEventHandler,
}

/// Used as an input to `analyze_inputs`.
enum EventTrigger {
    /// The trigger is indeed an event-based trigger.
    Event(ItemIndirection, Option<codemap::Span>),

    /// The trigger is not event-based. The given span is used to
    /// display an error message if there is a dependency on event data.
    NotEvent(Option<codemap::Span>),
}

fn analyze_inputs<'a>(
    actx: &mut AnalysisCtx<'_>,
    inputs: impl IntoIterator<Item = &'a sem::Input> + Clone,
    event_triggers: Result<&[EventTrigger], EventTriggerUnavailableReason>,
) {
    for input in inputs.clone() {
        analyze_input(actx, input, event_triggers.map_err(|_| ()));
    }

    let non_event_triggers = match event_triggers {
        Ok(x) => x
            .iter()
            .filter_map(|et| match et {
                EventTrigger::Event(_, _) => None,
                EventTrigger::NotEvent(span) => Some(span),
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    let event_param_inputs: Vec<_> = inputs
        .into_iter()
        .filter(|input| input.origin == sem::InputOrigin::Event)
        .collect();

    if !event_param_inputs.is_empty() {
        let codemap_spans1 = event_param_inputs
            .iter()
            .filter_map(|input| input.span)
            .map(|span| SpanLabel {
                span,
                label: None,
                style: SpanStyle::Primary,
            })
            .collect();

        if !non_event_triggers.is_empty() {
            let codemap_spans2: Vec<_> = non_event_triggers
                .iter()
                .filter_map(|x| **x)
                .map(|span| SpanLabel {
                    span,
                    label: None,
                    style: SpanStyle::Primary,
                })
                .collect();

            actx.diag.emit(&[Diagnostic {
                level: Level::Error,
                message: "Can't reference event parameters if some of the \
                          triggers aren't events"
                    .to_string(),
                code: None,
                spans: codemap_spans1,
            }]);

            actx.diag.emit(&[Diagnostic {
                level: Level::Note,
                message: "These trigger(s) aren't events".to_string(),
                code: None,
                spans: codemap_spans2,
            }]);
        } else if let Err(EventTriggerUnavailableReason::NotEventHandler) = event_triggers {
            actx.diag.emit(&[Diagnostic {
                level: Level::Error,
                message: "Event parameters can be referenced only in an event \
                          handler (the last part of `on`)"
                    .to_string(),
                code: None,
                spans: codemap_spans1,
            }]);
        }
    }

    // TODO: The same event parameter should not appear twice
} // analyze_inputs

fn analyze_input(
    actx: &mut AnalysisCtx<'_>,
    input: &sem::Input,
    event_triggers: Result<&[EventTrigger], ()>,
) {
    let info = analyze_input_inner(actx, input, event_triggers);

    // Reserve the place to put the analysis result
    if actx.analysis.inputs.len() <= input.index {
        actx.analysis.inputs.resize_with(input.index + 1, || None);
    }

    let out_cell = &mut actx.analysis.inputs[input.index];
    assert!(out_cell.is_none());
    *out_cell = Some(info);
}

fn analyze_input_inner(
    actx: &mut AnalysisCtx<'_>,
    input: &sem::Input,
    event_triggers: Result<&[EventTrigger], ()>,
) -> InputInfo {
    match input.origin {
        sem::InputOrigin::Event => {
            let event_triggers = match event_triggers {
                Ok(t) => t,
                Err(()) => {
                    // This case is reported by `analyze_inputs`
                    return InputInfo::Invalid;
                }
            };

            let param_name = &input.selectors[0];

            // Resolve the event parameter reference for each event trigger
            let param_i_list = event_triggers
                .iter()
                .map(|et| match et {
                    EventTrigger::NotEvent(_) => {
                        // This case is reported by `analyze_inputs`. Ignore `NotEvent`
                        // as long as we are in `analyze_input`.
                        None
                    }
                    EventTrigger::Event(ind, trigger_span) => {
                        let item = &actx.ctx.repo.crates[ind.comp_crate_i].comps[ind.comp_i].items
                            [ind.item_i];
                        let event = item.event().unwrap();

                        Some(
                            event
                                .inputs
                                .iter()
                                .position(|ident| *ident == param_name.sym)
                                .ok_or(trigger_span),
                        )
                    }
                })
                .collect::<Vec<_>>();

            // Report resolution failure
            let incompatible_trigger_list = param_i_list
                .iter()
                .filter_map(|result| match *result {
                    Some(Err(span)) => Some(span),
                    _ => None,
                })
                .collect::<Vec<_>>();

            if !incompatible_trigger_list.is_empty() {
                let mut codemap_spans = incompatible_trigger_list
                    .iter()
                    .filter_map(|x| **x)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Secondary,
                    })
                    .collect::<Vec<_>>();

                if let Some(span) = input.span {
                    codemap_spans.push(SpanLabel {
                        span,
                        label: Some("referenced by this".to_string()),
                        style: SpanStyle::Primary,
                    });
                }

                actx.diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: format!(
                        "Some of the triggering event(s) \
                         do not have a parameter named `{}`",
                        param_name.sym
                    ),
                    code: None,
                    spans: codemap_spans,
                }]);
            } // !incompatible_trigger_list.is_empty()

            // Return the analysis result
            let param_i_list = param_i_list
                .into_iter()
                .collect::<Option<Result<Vec<_>, _>>>();

            if let Some(Ok(param_i)) = param_i_list {
                InputInfo::EventParam(EventParamInput { param_i })
            } else {
                InputInfo::Invalid
            }
        } // sem::InputOrigin::Event

        // `this` refers to `this`
        sem::InputOrigin::This if input.selectors.is_empty() => InputInfo::This,

        // `this.field1.field2` refers to something indirectly accessible
        // through `this`
        sem::InputOrigin::This => {
            enum State<'a> {
                Comp {
                    comp: &'a metadata::CompDef,
                    crate_i: usize,
                    comp_i: usize,
                },
                /// The last component refers to a field. Should be derefed to
                /// get `State::Comp`.
                Field(&'a metadata::FieldDef),
                /// The last component refers to a event. Can't be derefed.
                Event,
            }

            let mut state = State::Comp {
                comp: actx.cur_comp,
                crate_i: actx.cur_comp_crate_i,
                comp_i: actx.cur_comp_i,
            };

            let mut indirections = Vec::new();

            for sel in input.selectors.iter() {
                // Resolve the last component's target type as a component
                let (comp, crate_i, comp_i) = match state {
                    State::Comp {
                        comp,
                        crate_i,
                        comp_i,
                    } => (comp, crate_i, comp_i),
                    State::Field(field) => {
                        if let Some(metadata::CompRef { crate_i, comp_i }) = field.ty {
                            (
                                &actx.ctx.repo.crates[crate_i].comps[comp_i],
                                crate_i,
                                comp_i,
                            )
                        } else {
                            actx.diag.emit(&[Diagnostic {
                                level: Level::Error,
                                message: "Can't refer to a field of something \
                                          that is not a component"
                                    .to_string(),
                                code: None,
                                spans: sel
                                    .span
                                    .map(|span| SpanLabel {
                                        span,
                                        label: None,
                                        style: SpanStyle::Primary,
                                    })
                                    .into_iter()
                                    .collect(),
                            }]);

                            return InputInfo::Invalid;
                        }
                    }
                    State::Event => {
                        actx.diag.emit(&[Diagnostic {
                            level: Level::Error,
                            message: "Events do not have a field".to_string(),
                            code: None,
                            spans: sel
                                .span
                                .map(|span| SpanLabel {
                                    span,
                                    label: None,
                                    style: SpanStyle::Primary,
                                })
                                .into_iter()
                                .collect(),
                        }]);

                        return InputInfo::Invalid;
                    }
                }; // let (comp, crate_i, comp_i) = match state

                // Find the named item
                let find_result = comp.find_item_by_ident(&sel.sym);
                let (item_i, item) = if let Some(x) = find_result {
                    x
                } else {
                    actx.diag.emit(&[Diagnostic {
                        level: Level::Error,
                        message: format!(
                            "`{}` not have a field named `{}`",
                            comp.paths[0], sel.sym
                        ),
                        code: None,
                        spans: sel
                            .span
                            .map(|span| SpanLabel {
                                span,
                                label: None,
                                style: SpanStyle::Primary,
                            })
                            .into_iter()
                            .collect(),
                    }]);

                    return InputInfo::Invalid;
                };

                indirections.push(ItemIndirection {
                    comp_crate_i: crate_i,
                    comp_i,
                    item_i,
                });

                state = match item {
                    metadata::CompItemDef::Field(field) => State::Field(field),
                    metadata::CompItemDef::Event(_) => State::Event,
                };
            } // for sel in input.selectors.iter()

            assert!(!indirections.is_empty());
            InputInfo::Item(ItemInput { indirections })
        } // sem::InputOrigin::This
    } // match input.origin
} // analyze_input_inner
