use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use log::debug;
use pathfinding::directed::{
    strongly_connected_components::strongly_connected_components,
    topological_sort::topological_sort,
};
use std::{cell::Cell, collections::HashMap, fmt::Write, ops::Range};

use super::super::{diag::Diag, sem, EmittedError};
use super::{
    analysis,
    bitsetgen::{self, BitsetTy},
    evalgen, fields, methods, paths, CompBuilderTy, CompSharedTy, CompStateTy, CompTy, Ctx,
    EventInnerSubList, FactorySetterForField, InnerValueField, SetterMethod, TempVar,
};
use crate::metadata;

#[derive(Debug)]
enum DepNode {
    Field { item_i: usize },
    // Actually, this doesn't have to be a node because it could be just
    // initialized as a part of `Field`. Nevertheless, it's represented as
    // a node for better reporting of a circular reference.
    ObjInitField { item_i: usize, field_i: usize },
    This,
}

#[derive(Debug)]
enum CommitNode {
    /// `FieldType::{Prop, Wire}` (`CompItemDef::Field`), or `CompItemDef::On`
    Item { item_i: usize },
    /// `prop`
    ObjInitField { item_i: usize, field_i: usize },
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum CommitTrigger {
    /// `prop`'s setter method is called.
    SetItem { item_i: usize },
    /// The value of a `prop` or `wire` field of the current component has
    /// changed. This can only happen as a result of the commitment of another
    /// field, thus it's said to be not *an initiator*.
    /// `item_i` is an index into `sem::CompDef::items`.
    WatchField { item_i: usize },
    /// An event is raised. `input` must refer to an event.
    Event { input: analysis::ItemInput },
}

pub struct DepAnalysis {
    nodes: Vec<DepNode>,
    item2node_map: Vec<usize>,
    ordered_node_i_list: Vec<usize>,

    commit_nodes: Vec<CommitNode>,
    triggers: Vec<CommitTrigger>,
    trigger2trigger_i: HashMap<CommitTrigger, usize>,
    trigger2commitnode_map: Vec<Vec<usize>>,
    /// Each `Vec<usize>` is sorted
    commitnode2trigger_map: Vec<Vec<usize>>,
    cdf2node_map: Vec<Vec<usize>>,
    cdf_triggers_cdf_map: Vec<Vec<usize>>,
    bit2cdf_map: Vec<usize>,
    cdf2bit_map: Vec<usize>,
    /// Each `Vec<usize>` comes from `commitnode2trigger_map`, thus is sorted.
    cdf2triggerset: Vec<Vec<usize>>,
    pub cdf_ty: BitsetTy,
}

impl DepAnalysis {
    pub fn new(
        analysis: &analysis::Analysis,
        ctx: &Ctx,
        item_meta2sem_map: &[usize],
        diag: &mut Diag,
    ) -> Result<Self, EmittedError> {
        analyze_dep(analysis, ctx, item_meta2sem_map, diag)
    }
}

/// Analyze dependencies between fields.
fn analyze_dep(
    analysis: &analysis::Analysis,
    ctx: &Ctx,
    item_meta2sem_map: &[usize],
    diag: &mut Diag,
) -> Result<DepAnalysis, EmittedError> {
    let comp = ctx.cur_comp;

    // Construct a dependency graph to find the initialization order
    // ----------------------------------------------------------------------
    let mut nodes = vec![DepNode::This];

    // Define nodes
    let mut item2node_map = Vec::with_capacity(comp.items.len());
    for (item_i, item) in comp.items.iter().enumerate() {
        item2node_map.push(nodes.len());

        match item {
            sem::CompItemDef::Field(item) => {
                nodes.push(DepNode::Field { item_i });

                if let Some(sem::DynExpr::ObjInit(init)) = &item.value {
                    // Add all fields
                    let num_fields = init.fields.len();
                    nodes.extend(
                        (0..num_fields).map(|field_i| DepNode::ObjInitField { item_i, field_i }),
                    );
                }
            }
            sem::CompItemDef::On(_) | sem::CompItemDef::Event(_) => {}
        }
    }

    // Find node dependencies
    let mut deps = Vec::new();

    let push_func_deps = |deps: &mut Vec<usize>, func: &sem::Func| {
        for func_input in func.inputs.iter() {
            match analysis.get_input(&func_input.input) {
                analysis::InputInfo::EventParam(_) => unreachable!(),
                analysis::InputInfo::Item(item_input) => {
                    let ind0 = item_input.indirections.first().unwrap();
                    let sem_item_i = item_meta2sem_map[ind0.item_i];
                    deps.push(item2node_map[sem_item_i]);
                }
                analysis::InputInfo::This => {
                    deps.push(0); // `DepNode::This`
                }
                analysis::InputInfo::Invalid => {}
            }
        }
    };

    let dep_ranges: Vec<Range<usize>> = nodes
        .iter()
        .enumerate()
        .map(|(node_i, node)| {
            let start = deps.len();

            match node {
                DepNode::This => {
                    // `this` depends on all fields
                    for (item_i, item) in comp.items.iter().enumerate() {
                        if let sem::CompItemDef::Field(_) = item {
                            deps.push(item2node_map[item_i]);
                        }
                    }
                }
                DepNode::Field { item_i } => {
                    match &comp.items[*item_i].field().unwrap().value {
                        None => {}
                        Some(sem::DynExpr::Func(func)) => {
                            push_func_deps(&mut deps, func);
                        }
                        Some(sem::DynExpr::ObjInit(_)) => {
                            // In `nodes`, this node is followed by zero or more
                            // `DepNode::ObjInitField` nodes
                            deps.extend((node_i + 1..nodes.len()).take_while(|&i| {
                                match &nodes[i] {
                                    DepNode::ObjInitField {
                                        item_i: item_i2, ..
                                    } if item_i2 == item_i => true,
                                    _ => false,
                                }
                            }));
                        }
                    }
                }
                DepNode::ObjInitField { item_i, field_i } => {
                    let field_item = comp.items[*item_i].field().unwrap();
                    let obj_init = field_item.value.as_ref().unwrap().obj_init().unwrap();
                    let field = &obj_init.fields[*field_i];
                    push_func_deps(&mut deps, &field.value);
                }
            }

            start..deps.len() // A range into `deps` representing `node`'s dependencies
        })
        .collect();

    let node_i_list: Vec<_> = (0..nodes.len()).collect();
    let node_depends_on = |&node_i: &usize| deps[dep_ranges[node_i].clone()].iter().copied();

    // Log the dependency
    if log::LevelFilter::Debug <= log::max_level() {
        debug!(
            "Planning field initialization for the component `{}`",
            comp.path
        );
        for (i, node) in nodes.iter().enumerate() {
            debug!(
                " [{}] {:?} → {:?}",
                i,
                node,
                node_depends_on(&i).collect::<Vec<_>>()
            );
        }
    }

    // Find a topological order
    let ordered_node_i_list = topological_sort(&node_i_list, node_depends_on);

    debug!("Initialization order = {:?}", ordered_node_i_list);

    let ordered_node_i_list = if let Ok(mut x) = ordered_node_i_list {
        x.reverse();
        x
    } else {
        // If none was found, find cycles and report them as an error.
        let sccs = strongly_connected_components(&node_i_list, node_depends_on);

        diag.emit(&[Diagnostic {
            level: Level::Error,
            message: format!(
                "A circular dependency was detected in the \
                 field initialization of `{}`",
                comp.path
            ),
            code: None,
            spans: comp
                .path
                .span
                .map(|span| SpanLabel {
                    span,
                    label: None,
                    style: SpanStyle::Primary,
                })
                .into_iter()
                .collect(),
        }]);

        let num_cycles = sccs.iter().filter(|scc| scc.len() > 1).count();

        for (i, scc) in sccs.iter().filter(|scc| scc.len() > 1).enumerate() {
            let codemap_spans: Vec<_> = scc
                .iter()
                .rev()
                .filter_map(|&x| match &nodes[x] {
                    DepNode::Field { item_i } => {
                        let field = comp.items[*item_i].field().unwrap();
                        Some((field.ident.span?, "initialization of this field"))
                    }
                    DepNode::ObjInitField { item_i, field_i } => {
                        let field = comp.items[*item_i].field().unwrap();
                        let obj_init = field.value.as_ref().unwrap().obj_init().unwrap();
                        let init_field = &obj_init.fields[*field_i];
                        Some((init_field.ident.span?, "initialization of this field"))
                    }
                    DepNode::This => Some((comp.path.span?, "`this` reference of the component")),
                })
                .enumerate()
                .map(|(i, (span, label))| SpanLabel {
                    span,
                    label: Some(format!("({}) {}", i + 1, label)),
                    style: SpanStyle::Primary,
                })
                .collect();

            diag.emit(&[Diagnostic {
                level: Level::Note,
                message: format!("Cycle (SCC) {} of {}", i + 1, num_cycles),
                code: None,
                spans: codemap_spans,
            }]);
        }

        let involves_this = sccs
            .iter()
            .filter(|scc| scc.len() > 1 && scc.contains(&0))
            .nth(0)
            .is_some();

        if involves_this {
            diag.emit(&[Diagnostic {
                level: Level::Note,
                message: "`this` is constructed after initializing all fields".to_string(),
                code: None,
                spans: vec![],
            }]);
        }

        return Err(EmittedError);
    };

    // The last node should be `this`
    assert_eq!(*ordered_node_i_list.last().unwrap(), 0);

    // Enumerate works to do in the committing function
    // ----------------------------------------------------------------------
    let mut commit_nodes = Vec::new();

    #[derive(Default)]
    struct TriggerInfo {
        triggers: Vec<CommitTrigger>,
        trigger2trigger_i: HashMap<CommitTrigger, usize>,
        trigger2commitnode_map: Vec<Vec<usize>>,
        commitnode2trigger_map: Vec<Vec<usize>>,
    }
    let mut trigger_info = TriggerInfo::default();
    let trigger_emitted = Cell::new(false); // set when `define_trigger` is called

    let define_trigger = |trigger_info: &mut TriggerInfo, node_i, trigger: CommitTrigger| {
        let TriggerInfo {
            triggers,
            trigger2commitnode_map,
            trigger2trigger_i,
            ..
        } = trigger_info;

        let trigger_i = *trigger2trigger_i.entry(trigger.clone()).or_insert_with(|| {
            let i = trigger2commitnode_map.len();
            triggers.push(trigger);
            trigger2commitnode_map.push(Vec::new());
            i
        });

        trigger2commitnode_map[trigger_i].push(node_i);
        trigger_emitted.set(true);
    };

    let define_input_trigger =
        |trigger_info: &mut _, diag: &mut Diag, input: &sem::Input, node_i, skip_events| {
            match analysis.get_input(input) {
                analysis::InputInfo::EventParam(_) => {}
                analysis::InputInfo::Item(item_input) => {
                    let ind0 = item_input.indirections.first().unwrap();

                    let local_delivery = item_input.indirections.len() == 1
                        && ctx.cur_meta_comp().items[ind0.item_i].field().is_some();

                    if local_delivery {
                        let sem_item_i = item_meta2sem_map[ind0.item_i];

                        let item = ind0.item(ctx.repo);
                        if item.field().unwrap().field_ty == metadata::FieldType::Const {
                            // `const` never changes
                            return;
                        }

                        define_trigger(
                            trigger_info,
                            node_i,
                            CommitTrigger::WatchField { item_i: sem_item_i },
                        );
                    } else {
                        let item_input = item_input.clone();

                        // Find the referred item
                        let ind_last = item_input.indirections.last().unwrap();
                        let item = ind_last.item(ctx.repo);

                        // If it's a field, find the event for watching the field
                        if let Some(field) = item.field() {
                            if item.field().unwrap().field_ty == metadata::FieldType::Const {
                                // `const` never changes
                                return;
                            }

                            if let Some(_) = field.accessors.watch {
                                // TODO: Find the event
                                diag.emit(&[Diagnostic {
                                    level: Level::Warning,
                                    message: "Watching remote prop is unimplemented".to_string(),
                                    code: None,
                                    spans: vec![],
                                }]);
                                return;
                            } else {
                                // TODO: We can't watch a prop without a `watch` accessor.
                                //       This should probably be checked in `analysis.rs`
                                diag.emit(&[Diagnostic {
                                    level: Level::Error,
                                    message: format!(
                                        "Prop `{}` does not have a `watch` accessor",
                                        field.ident
                                    ),
                                    code: None,
                                    spans: vec![],
                                }]);
                                return;
                            }
                        } else if skip_events {
                            return;
                        }

                        define_trigger(
                            trigger_info,
                            node_i,
                            CommitTrigger::Event { input: item_input },
                        );
                    }
                }
                analysis::InputInfo::This => {}
                analysis::InputInfo::Invalid => {}
            }
        };

    let define_func_trigger = |trigger_info: &mut _, diag: &mut Diag, func: &sem::Func, node_i| {
        for func_input in func.inputs.iter() {
            define_input_trigger(trigger_info, diag, &func_input.input, node_i, false);
        }
    };

    let define_on_trigger = |trigger_info: &mut _, diag: &mut Diag, on: &sem::OnDef, node_i| {
        for trigger in on.triggers.iter() {
            if let sem::Trigger::Input(input) = trigger {
                // `skip_events = true` because event triggers on `on` are
                // treated specially and are not handled here
                define_input_trigger(trigger_info, diag, input, node_i, true);
            }
        }
    };

    for (item_i, item) in comp.items.iter().enumerate() {
        item2node_map.push(nodes.len());

        match item {
            sem::CompItemDef::Field(item) => match item.field_ty {
                sem::FieldType::Const => {
                    if let Some(sem::DynExpr::ObjInit(init)) = &item.value {
                        for (field_i, field) in init.fields.iter().enumerate() {
                            if field.field_ty == sem::FieldType::Prop {
                                let node_i = commit_nodes.len();
                                trigger_emitted.set(false);
                                define_func_trigger(&mut trigger_info, diag, &field.value, node_i);
                                // Emit a node only if it has a trigger
                                if trigger_emitted.get() {
                                    commit_nodes.push(CommitNode::ObjInitField { item_i, field_i });
                                }
                            }
                        }
                    }
                }
                sem::FieldType::Prop => {
                    let node_i = commit_nodes.len();
                    define_trigger(&mut trigger_info, node_i, CommitTrigger::SetItem { item_i });
                    commit_nodes.push(CommitNode::Item { item_i });
                }
                sem::FieldType::Wire => {
                    let node_i = commit_nodes.len();
                    // `wire` must have a value. `DynExpr::ObjInit` is only allowed
                    // for `FieldType::Const`, so it must be `DynExpr::Func`.
                    let func = item.value.as_ref().unwrap().func().unwrap();

                    trigger_emitted.set(false);
                    define_func_trigger(&mut trigger_info, diag, func, node_i);

                    // Emit a node only if it has a trigger
                    if trigger_emitted.get() {
                        commit_nodes.push(CommitNode::Item { item_i });
                    }
                }
            },
            sem::CompItemDef::On(on) => {
                let node_i = commit_nodes.len();

                trigger_emitted.set(false);
                define_on_trigger(&mut trigger_info, diag, on, node_i);

                // Emit a node only if it has a trigger
                if trigger_emitted.get() {
                    commit_nodes.push(CommitNode::Item { item_i });
                }
            }
            sem::CompItemDef::Event(_) => {}
        }
    }

    // Initialize `commitnode2trigger_map`
    trigger_info
        .commitnode2trigger_map
        .resize_with(commit_nodes.len(), Vec::new);
    for (trigger_i, node_i_list) in trigger_info.trigger2commitnode_map.iter().enumerate() {
        for &node_i in node_i_list.iter() {
            trigger_info.commitnode2trigger_map[node_i].push(trigger_i);
        }
    }

    // Create dirty flags
    // ----------------------------------------------------------------------
    // `trigger2commitnode_map` defines the set `(t, n) ∈ T`. It can be viewed
    // as two multivalued functions, which can be quickly evaluated by indexing
    // into `trigger2commitnode_map` and `commitnode2trigger_map`, respectively.
    //
    // In some extension, evaluating one node *unconditionally* activates other
    // triggers, guaranteeing other nodes' evaluation. Example: Let
    // `n₁ = Item { item_i: i }`. Evaluating `n₁` activates the trigger
    // `t₂ = WatchField { item_i: i }`. If `T` had an element `(t₂, n₂)`, `n₂`
    // would be evaluated too. In this way, we could construct a closure `T*`.
    // **But** we actually don't use this extension (for now we *conditionally*
    // trigger a chain reaction), so `T*` is identical to `T`.
    //
    //  NOTE: The above premise (that we conditionally trigger a chain reaction)
    //        means that nodes in the same CDF have no dependencies to each
    //        other. If the premise changes, we will have to care about the
    //        nodes' ordering!
    //
    // Nodes `N'` with identical sets of triggers (to be more precise, [1]) can
    // be combined and have a single dirty flag, which we call a compressed
    // dirty flag (CDF). In other words, every distinct element of
    // `{{t|(t,n)∈T*}|n∈N}` (where `N` represents the universal set of nodes)
    // receives a CDF.
    //   [1]: ∀n₁,n₂∈N'. {t|(t,n₁)∈T*} = {t|(t,n₂)∈T}

    let mut cdf2triggerset: Vec<Vec<usize>> = Vec::new();
    let node2cdf: Vec<usize>;
    {
        let mut triggerset2cdf = HashMap::new();
        node2cdf = trigger_info
            .commitnode2trigger_map
            .iter()
            .map(|trigger_i_list| {
                let trigger_i_list = &trigger_i_list[..];

                assert_eq!(trigger_i_list.is_empty(), false);

                *triggerset2cdf.entry(trigger_i_list).or_insert_with(|| {
                    let i = cdf2triggerset.len();
                    cdf2triggerset.push(trigger_i_list.to_owned());
                    i
                })
            })
            .collect();
    }

    let mut cdf2node_map = vec![Vec::new(); cdf2triggerset.len()];
    for (node_i, &cdf_i) in node2cdf.iter().enumerate() {
        cdf2node_map[cdf_i].push(node_i);
    }

    // Sort the CDFs by the topological order of the relationship R_WF defined
    // by `WatchField`. The order is guaranteed to exist because of the
    // following preconditions:
    //
    //  - `R_WF` is actually a subset of the relationship that dictates the
    //    order of field initialization, which we've already checked that is
    //    acyclic.
    //  - An alternative way to do this is to construct a graph `(N, R_WF)` and
    //    merge two nodes having the same set of predecessors one by one.
    //    The resulting graph is still a DAG. This can be proven by showing that
    //    there exists a topologically-sorted list of `N` and the merging
    //    operation can be done simultaneously on the list while preserving the
    //    topological ordering.

    // `cdf_triggers_cdf_map[cdf_i]`: Suppose nodes belonging to `cdf_i` are updated.
    //     Through `CommitTrigger::WatchField`, another set of nodes is going to
    //     be updated. This function tells which CDF this new set of nodes
    //     belongs to.
    let cdf_triggers_cdf_map: Vec<Vec<usize>> = cdf2node_map
        .iter()
        .map(|node_i_list| {
            node_i_list
                .iter()
                .filter_map(|&node_i| match commit_nodes[node_i] {
                    CommitNode::Item { item_i } => Some(item_i),
                    CommitNode::ObjInitField { .. } => None,
                })
                .map(|item_i| CommitTrigger::WatchField { item_i })
                .filter_map(|trigger| trigger_info.trigger2trigger_i.get(&trigger))
                .flat_map(|&trigger_i| trigger_info.trigger2commitnode_map[trigger_i].iter())
                .map(|&node_i| node2cdf[node_i])
                .collect()
        })
        .collect();
    let cdf_triggers_cdf_map_fn = |i: &usize| cdf_triggers_cdf_map[*i].iter().cloned();

    // Find the topological order
    let cdf_i_list: Vec<usize> = (0..cdf2triggerset.len()).collect();
    let ordered_cdf_i = topological_sort(&cdf_i_list, cdf_triggers_cdf_map_fn).unwrap();

    // `ordered_cdf_i` defines the CDFs' actual bit positions (in `BitsetTy`).
    let bit2cdf_map = ordered_cdf_i;
    let mut cdf2bit_map = vec![0; bit2cdf_map.len()];
    for (bit_i, &cdf_i) in bit2cdf_map.iter().enumerate() {
        cdf2bit_map[cdf_i] = bit_i;
    }

    // Construct a `BitsetTy` that represents a run-time type large enough to
    // store all the CDFs.
    let cdf_ty = match BitsetTy::new(cdf2triggerset.len()) {
        Ok(x) => x,
        Err(bitsetgen::TooLargeError) => {
            diag.emit(&[Diagnostic {
                level: Level::Error,
                message: "The component requires more dirty flags than \
                          currently supported by the code generator"
                    .to_string(),
                code: None,
                spans: (comp.path.span)
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);

            BitsetTy::new(0).unwrap()
        }
    };

    Ok(DepAnalysis {
        nodes,
        item2node_map,
        ordered_node_i_list,

        commit_nodes,
        triggers: trigger_info.triggers,
        trigger2trigger_i: trigger_info.trigger2trigger_i,
        trigger2commitnode_map: trigger_info.trigger2commitnode_map,
        commitnode2trigger_map: trigger_info.commitnode2trigger_map,
        cdf2node_map,
        cdf_triggers_cdf_map,
        bit2cdf_map,
        cdf2bit_map,
        cdf2triggerset,
        cdf_ty,
    })
}

/// Generates construction code for a component. The generated expression
/// evaluates to the type named `CompTy(comp_ident)`.
///
/// Assumes settable fields are in `self` of type `xxxBuilder`.
pub fn gen_construct(
    analysis: &analysis::Analysis,
    dep_analysis: &DepAnalysis,
    ctx: &Ctx,
    item_meta2sem_map: &[usize],
    diag: &mut Diag,
    out: &mut String,
) {
    let comp = ctx.cur_comp;
    let comp_ident = &comp.ident.sym;

    let nodes = &dep_analysis.nodes[..];
    let item2node_map = &dep_analysis.item2node_map[..];
    let ordered_node_i_list = &dep_analysis.ordered_node_i_list[..];

    // Emit field initializers
    // ----------------------------------------------------------------------
    struct InitFuncInputGen<'a> {
        item2node_map: &'a [usize],
    }

    impl evalgen::FuncInputGen for InitFuncInputGen<'_> {
        fn gen_field_ref(&mut self, item_i: usize, by_ref: bool, out: &mut String) {
            let node_i = self.item2node_map[item_i];

            if by_ref {
                write!(out, "(&{})", TempVar(node_i)).unwrap();
            } else {
                write!(out, "{}::clone(&{})", paths::CLONE, TempVar(node_i)).unwrap();
            }
        }

        fn gen_this(&mut self, _out: &mut String) {
            // `this: ComponentType` is unavailable at this point
            unreachable!()
        }

        // `InitFuncInputGen` isn't used for event handlers, so the following
        // two methods are never called
        fn trigger_i(&mut self) -> usize {
            unreachable!()
        }
        fn gen_event_param(&mut self, _param_i: usize, _out: &mut String) {
            unreachable!()
        }
    }

    let mut func_input_gen = InitFuncInputGen {
        item2node_map: &item2node_map,
    };

    let var_state = TempVar("state");
    let var_shared = TempVar("shared");
    let var_this = TempVar(0); // `DepNode::This`
    for (i, node) in ordered_node_i_list.iter().map(|&i| (i, &nodes[i])) {
        let var = TempVar(i);
        match node {
            DepNode::This => {
                assert_eq!(var.0, var_this.0);

                // `struct ComponentTypeState`
                writeln!(out, "let {} = {} {{", var_state, CompStateTy(&comp_ident)).unwrap();
                for (i, item) in comp.items.iter().enumerate() {
                    let val = TempVar(item2node_map[i]);
                    match item {
                        sem::CompItemDef::Field(item) => match item.field_ty {
                            sem::FieldType::Const => {}
                            sem::FieldType::Wire | sem::FieldType::Prop => {
                                writeln!(
                                    out,
                                    "    {ident}: {val},",
                                    ident = InnerValueField(&item.ident.sym),
                                    val = val,
                                )
                                .unwrap();
                            }
                        },
                        _ => {}
                    }
                }
                writeln!(out, "}};").unwrap();

                // `struct ComponentTypeShared`
                writeln!(out, "let {} = {} {{", var_shared, CompSharedTy(&comp_ident)).unwrap();
                for (i, item) in comp.items.iter().enumerate() {
                    let val = TempVar(item2node_map[i]);
                    match item {
                        sem::CompItemDef::Field(item) => match item.field_ty {
                            sem::FieldType::Wire => {}
                            sem::FieldType::Prop => {
                                writeln!(
                                    out,
                                    "    {ident}: {def}::default(),",
                                    ident = InnerValueField(&item.ident.sym),
                                    def = paths::DEFAULT,
                                )
                                .unwrap();
                            }
                            sem::FieldType::Const => {
                                writeln!(
                                    out,
                                    "    {ident}: {val},",
                                    ident = InnerValueField(&item.ident.sym),
                                    val = val,
                                )
                                .unwrap();
                            }
                        },
                        sem::CompItemDef::Event(item) => {
                            writeln!(
                                out,
                                "    {ident}: {def}::default(),",
                                ident = EventInnerSubList(&item.ident.sym),
                                def = paths::DEFAULT,
                            )
                            .unwrap();
                        }
                        _ => {}
                    }
                }
                writeln!(
                    out,
                    "    {field}: {refcell}::new({val}),",
                    field = fields::STATE,
                    refcell = paths::REF_CELL,
                    val = var_state,
                )
                .unwrap();
                writeln!(
                    out,
                    "    {field}: {cell}::new({val}),",
                    field = fields::DIRTY,
                    cell = paths::CELL,
                    val = dep_analysis.cdf_ty.gen_empty(),
                )
                .unwrap();
                writeln!(out, "}};").unwrap();

                // `struct ComponentType`
                writeln!(out, "let {} = {} {{", var_this, CompTy(&comp_ident)).unwrap();
                writeln!(
                    out,
                    "    {field}: {rc}::new({shared})",
                    field = fields::SHARED,
                    rc = paths::RC,
                    shared = var_shared
                )
                .unwrap();
                writeln!(out, "}};").unwrap();
            } // DepNode::This

            DepNode::Field { item_i } => {
                let field = comp.items[*item_i].field().unwrap();
                write!(out, "let {} = ", var).unwrap();

                if field.value.is_none() {
                    // Mandatory field - the value is always available
                    // from `ComponentTypeBuilder`
                    writeln!(
                        out,
                        "self.{field};",
                        field = InnerValueField(&field.ident.sym)
                    )
                    .unwrap();
                    continue;
                }

                let is_settable = field.accessors.set.is_some();
                if is_settable {
                    // Check if the value is available from `ComponentTypeBuilder`
                    let var_tmp = TempVar("given_value");
                    writeln!(
                        out,
                        "if let {some}({t}) = self.{field} {{ {t} }} else {{",
                        some = paths::SOME,
                        t = var_tmp,
                        field = InnerValueField(&field.ident.sym)
                    )
                    .unwrap();
                }

                match field.value.as_ref().unwrap() {
                    sem::DynExpr::Func(func) => {
                        evalgen::gen_func_eval(
                            func,
                            analysis,
                            ctx,
                            item_meta2sem_map,
                            &mut func_input_gen,
                            out,
                        );
                    }
                    sem::DynExpr::ObjInit(init) => {
                        // Find the component we are constructing. The field's
                        // type is guaranteed to match the component's type
                        // because we do not allow explicitly specifying the type
                        // when `ObjInit` is in use.
                        let meta_item_i =
                            item_meta2sem_map.iter().position(|i| i == item_i).unwrap();
                        let meta_field = ctx.cur_meta_comp().items[meta_item_i].field().unwrap();

                        if let Some(ty) = &meta_field.ty {
                            let initer_map = check_obj_init(ctx.repo.comp_by_ref(ty), init, diag);

                            gen_obj_init(
                                ctx.repo.comp_by_ref(ty),
                                init,
                                analysis,
                                ctx,
                                item_meta2sem_map,
                                &mut func_input_gen,
                                &initer_map,
                                out,
                            );
                        } else {
                            diag.emit(&[Diagnostic {
                                level: Level::Error,
                                message: format!("`{}` does not refer to a component", init.path),
                                code: None,
                                spans: init
                                    .path
                                    .span
                                    .map(|span| SpanLabel {
                                        span,
                                        label: None,
                                        style: SpanStyle::Primary,
                                    })
                                    .into_iter()
                                    .collect(),
                            }]);

                            write!(out, "panic!(\"codegen failed\")").unwrap();
                        }
                    }
                }

                if is_settable {
                    writeln!(out, "\n}};").unwrap(); // close the `if` block
                } else {
                    writeln!(out, ";").unwrap();
                }
            } // DepNode::Field

            DepNode::ObjInitField { .. } => {
                // It's a part of `Field` and initialized in there
            } // DepNode::ObjInitField
        }
    }

    // TODO: Regisetr event handlers for event triggers of `on`
    // TODO: Rgeister event handlers for CDF triggers (`Event` in `trigger_info.triggers`)

    writeln!(out, "{}", var_this).unwrap();
}

/// Analyze `ObjInit` and report errors if any.
///
/// Returns a multi-map from indices into `comp.item` to indices into
/// `obj_init.fields`.
fn check_obj_init(
    comp: &metadata::CompDef,
    obj_init: &sem::ObjInit,
    diag: &mut Diag,
) -> Vec<Vec<usize>> {
    let mut initers = vec![Vec::new(); comp.items.len()];

    for (init_field_i, init_field) in obj_init.fields.iter().enumerate() {
        let item_i = comp.items.iter().position(|item| {
            item.field()
                .filter(|f| f.ident == init_field.ident.sym)
                .is_some()
        });

        let init_field_span = init_field.ident.span.map(|span| SpanLabel {
            span,
            label: None,
            style: SpanStyle::Primary,
        });

        if let Some(item_i) = item_i {
            if let Some(field) = comp.items[item_i].field() {
                if init_field.field_ty != field.field_ty {
                    diag.emit(&[Diagnostic {
                        level: Level::Error,
                        message: format!(
                            "Field type mismatch; the field `{}` is of type `{}`",
                            field.field_ty, init_field.field_ty
                        ),
                        code: None,
                        spans: init_field_span.into_iter().collect(),
                    }]);
                }

                initers[item_i].push(init_field_i);
            } else {
                diag.emit(&[Diagnostic {
                    level: Level::Error,
                    message: format!("`{}::{}` is not a field", comp.name(), init_field.ident.sym),
                    code: None,
                    spans: init_field_span.into_iter().collect(),
                }]);
            }
        } else {
            diag.emit(&[Diagnostic {
                level: Level::Error,
                message: format!(
                    "Component `{}` does not have a field named `{}`",
                    comp.name(),
                    init_field.ident.sym
                ),
                code: None,
                spans: init_field_span.into_iter().collect(),
            }]);
        }
    }

    // Report excessive or lack of initialization
    for (item, initers) in comp.items.iter().zip(initers.iter()) {
        let field = if let Some(x) = item.field() {
            x
        } else {
            continue;
        };

        if initers.len() > 1 {
            let codemap_spans: Vec<_> = initers
                .iter()
                .filter_map(|&i| obj_init.fields[i].ident.span)
                .map(|span| SpanLabel {
                    span,
                    label: None,
                    style: SpanStyle::Primary,
                })
                .collect();

            diag.emit(&[Diagnostic {
                level: Level::Error,
                message: format!("Too many initializers for the field `{}`", item.ident()),
                code: None,
                spans: codemap_spans,
            }]);
        }

        if !field.flags.contains(metadata::FieldFlags::OPTIONAL)
            && initers.is_empty()
            && field.field_ty != metadata::FieldType::Wire
        {
            diag.emit(&[Diagnostic {
                level: Level::Error,
                message: format!("Non-optional field `{}` is not initialized", field.ident),
                code: None,
                spans: obj_init
                    .path
                    .span
                    .map(|span| SpanLabel {
                        span,
                        label: None,
                        style: SpanStyle::Primary,
                    })
                    .into_iter()
                    .collect(),
            }]);
        }
    }

    initers
}

/// Generate an expression that instantiates a componen and evaluates to the
/// component's type.
///
/// `initer_map` is a multi-map from indices into `comp.item` to indices into
/// `obj_init.fields`, returned by `check_obj_init`, and may include errors
/// reported by `check_obj_init`.
fn gen_obj_init(
    comp: &metadata::CompDef,
    obj_init: &sem::ObjInit,
    analysis: &analysis::Analysis,
    ctx: &Ctx,
    item_meta2sem_map: &[usize],
    input_gen: &mut impl evalgen::FuncInputGen,
    initer_map: &[Vec<usize>],
    out: &mut String,
) {
    if comp.flags.contains(metadata::CompFlags::SIMPLE_BUILDER) {
        // Simple builder API
        let tmp_var = TempVar("built_component");
        writeln!(out, "{{").unwrap();
        writeln!(
            out,
            "    let {} = {}::new(",
            tmp_var,
            CompTy(&obj_init.path)
        )
        .unwrap();
        for (item, initers) in comp.items.iter().zip(initer_map.iter()) {
            let field = if let Some(x) = item.field() {
                x
            } else {
                continue;
            };

            // `const` is passed to `new`
            if field.field_ty == metadata::FieldType::Const
                && field.accessors.set.is_some()
                && initers.len() > 0
            {
                let obj_field = &obj_init.fields[initers[0]];
                evalgen::gen_func_eval(
                    &obj_field.value,
                    analysis,
                    ctx,
                    item_meta2sem_map,
                    input_gen,
                    out,
                );
                writeln!(out, "    ,").unwrap();
            }
        }
        writeln!(out, "    );").unwrap();

        for obj_field in obj_init
            .fields
            .iter()
            .filter(|f| f.field_ty == metadata::FieldType::Prop)
        {
            // `prop` is set through a setter method
            write!(
                out,
                "    {}.{}(",
                tmp_var,
                SetterMethod(&obj_field.ident.sym)
            )
            .unwrap();
            evalgen::gen_func_eval(
                &obj_field.value,
                analysis,
                ctx,
                item_meta2sem_map,
                input_gen,
                out,
            );
            writeln!(out, ");").unwrap();
        }

        writeln!(out, "    {}", tmp_var).unwrap();
        write!(out, "}}").unwrap();
    } else {
        // Standard builder API
        writeln!(out, "{}::new()", CompBuilderTy(&obj_init.path)).unwrap();
        for obj_field in obj_init.fields.iter() {
            write!(
                out,
                "    .{meth}(",
                meth = FactorySetterForField(&obj_field.ident.sym),
            )
            .unwrap();
            evalgen::gen_func_eval(
                &obj_field.value,
                analysis,
                ctx,
                item_meta2sem_map,
                input_gen,
                out,
            );
            writeln!(out, ")").unwrap();
        }
        write!(out, "    .build()").unwrap();
    }
}

/// Generate `xxxShared::set_dirty_flags` (`methods::SET_DIRTY_FLAGS`).
pub fn gen_set_dirty_flags(dep_analysis: &DepAnalysis, out: &mut String) {
    let arg_this = "this";
    let arg_flags = "flags";
    let cdf_ty = dep_analysis.cdf_ty;

    writeln!(
        out,
        "    fn {meth}({this}: &{rc}<Self>, {arg}: {ty}) {{",
        meth = methods::SET_DIRTY_FLAGS,
        this = arg_this,
        rc = paths::RC,
        arg = arg_flags,
        ty = cdf_ty.gen_ty(),
    )
    .unwrap();

    // If `xxxShared::dirty` is empty, schedule a next update
    writeln!(
        out,
        "        if {is_empty} {{",
        is_empty = cdf_ty.gen_is_empty(format_args!(
            "{this}.{dirty}.get()",
            this = arg_this,
            dirty = fields::DIRTY
        ),),
    )
    .unwrap();
    // TODO: call `WmExt::invoke_on_update`
    writeln!(out, "        }}",).unwrap();

    // Update `xxxShared::dirty`
    writeln!(
        out,
        "        {this}.{dirty}.set({new_flags});",
        this = arg_this,
        dirty = fields::DIRTY,
        new_flags = cdf_ty.gen_union(
            format_args!(
                "{this}.{dirty}.get()",
                this = arg_this,
                dirty = fields::DIRTY
            ),
            arg_flags,
        ),
    )
    .unwrap();

    writeln!(out, "    }}",).unwrap();
}

/// Generates a statament that activates the specified trigger.
pub fn gen_activate_trigger(
    dep_analysis: &DepAnalysis,
    ctx: &Ctx<'_>,
    trigger: &CommitTrigger,
    expr_shared: &impl std::fmt::Display,
    out: &mut String,
) {
    let comp_ident = &ctx.cur_comp.ident.sym;

    let trigger_i = if let Some(&x) = dep_analysis.trigger2trigger_i.get(trigger) {
        x
    } else {
        return;
    };

    let cdf_i_list = dep_analysis
        .cdf2triggerset
        .iter()
        .enumerate()
        .filter(|(_, triggerset)| triggerset.binary_search(&trigger_i).is_ok())
        .map(|x| x.0);

    let bit_i_list: Vec<_> = cdf_i_list.map(|i| dep_analysis.cdf2bit_map[i]).collect();

    if bit_i_list.is_empty() {
        return;
    }

    writeln!(
        out,
        "{ty}::{set_dirty_flags}({shared}, {flags});",
        ty = CompSharedTy(comp_ident),
        set_dirty_flags = methods::SET_DIRTY_FLAGS,
        shared = expr_shared,
        flags = dep_analysis.cdf_ty.gen_multi(bit_i_list),
    )
    .unwrap();
}
