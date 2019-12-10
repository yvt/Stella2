//! Metadata generation
use codemap_diagnostic::{Diagnostic, Level, SpanLabel, SpanStyle};
use std::collections::HashMap;

use super::{diag::Diag, sem};
use crate::metadata;

/// Analyze the given `CompDef`s, add `Crate` to `out_repo.crates`.
pub fn gen_and_push_crate(
    comps: &[sem::CompDef<'_>],
    imports_crate_i: &HashMap<&str, usize>,
    crate_name: String,
    out_repo: &mut metadata::Repo,
    diag: &mut Diag,
) {
    let mut ctx = Ctx {
        resolver: CompResolver {
            imports_crate_i,
            deps_crates: &out_repo.crates,
            local_crate_name: &crate_name,
            local_crate_i: out_repo.crates.len(),
            local_comps: &comps,
        },
        diag,
    };

    let new_crate = metadata::Crate {
        comps: comps.iter().map(|c| gen_comp(&mut ctx, c)).collect(),
        // TODO: probably should use a hash for reproducible builds
        uuid: uuid::Uuid::new_v4(),
        name: crate_name,
    };

    out_repo.main_crate_i = out_repo.crates.len();
    out_repo.crates.push(new_crate);
}

/// Replaces `Visibility::Restricted` with `Visibility::Private`.
pub struct DowngradeRestrictedVisibility;

impl metadata::visit_mut::VisitMut for DowngradeRestrictedVisibility {
    fn visit_visibility_mut(&mut self, i: &mut metadata::Visibility) {
        if let metadata::Visibility::Restricted(_) = i {
            *i = metadata::Visibility::Private;
        }
    }

    fn visit_path_mut(&mut self, _: &mut metadata::Path) {}
}

pub struct MapCrateIndex<'a>(pub &'a [usize]);

impl metadata::visit_mut::VisitMut for MapCrateIndex<'_> {
    fn visit_crate_i_mut(&mut self, i: &mut usize) {
        *i = self.0[*i];
    }
}

struct Ctx<'a> {
    resolver: CompResolver<'a>,
    diag: &'a mut Diag,
}

struct CompResolver<'a> {
    imports_crate_i: &'a HashMap<&'a str, usize>,
    deps_crates: &'a [metadata::Crate],
    /// The name of the current crate.
    local_crate_name: &'a str,
    local_crate_i: usize,
    /// Local components.
    local_comps: &'a [sem::CompDef<'a>],
}

impl CompResolver<'_> {
    fn find_crate_by_path(&self, path: &syn::Path) -> Option<usize> {
        let crate_name = path.segments[0].ident.to_string();
        if self.local_crate_name == crate_name
            || (crate_name == "crate" && path.leading_colon.is_none())
        {
            Some(self.local_crate_i)
        } else {
            assert!(path.leading_colon.is_some());

            // Based on the postcondition of a successful call to `resolve_paths`,
            // `path.segments[0].ident` always refers to a crate name
            Some(*self.imports_crate_i.get(&*crate_name)?)
        }
    }

    fn find_comp_by_path(&self, path: &syn::Path) -> Option<(usize, usize)> {
        let segments = &path.segments;

        // Paths with arguments never refer to a component
        if segments.iter().any(|s| !s.arguments.is_empty()) {
            return None;
        }

        let crate_name = &path.segments[0].ident;
        if *crate_name == self.local_crate_name
            || (*crate_name == "crate" && path.leading_colon.is_none())
        {
            // Search the local components
            let comp_i = self
                .local_comps
                .iter()
                .position(|comp: &sem::CompDef<'_>| {
                    // The first path segment represents a crate name. Skip that
                    // part because (1) we already know it has the correct crate
                    // name; and (2) one path might use `crate` while the other
                    // one is using the crate name.
                    let segs1 = comp.path.syn_path.segments.iter().skip(1).map(|s| &s.ident);
                    let segs2 = path.segments.iter().skip(1).map(|s| &s.ident);
                    segs1.eq(segs2)
                })?;

            return Some((self.local_crate_i, comp_i));
        }

        // Search the dependencies
        let crate_i = *self.imports_crate_i.get(&*crate_name.to_string())?;
        let kuleto = &self.deps_crates[crate_i];
        let comp_i = kuleto.comps.iter().position(|comp: &metadata::CompDef| {
            let segs1 = comp.paths[0].idents.iter();
            let segs2 = path.segments.iter().skip(1).map(|s| &s.ident);
            segs2.eq(segs1)
        })?;

        Some((crate_i, comp_i))
    }
}

fn gen_comp(ctx: &mut Ctx<'_>, comp: &sem::CompDef<'_>) -> metadata::CompDef {
    let path = gen_path(ctx, &comp.path);

    validate_comp_path(ctx, &path, &comp.path);

    metadata::CompDef {
        flags: comp.flags,
        vis: gen_vis(ctx, &comp.vis),
        paths: vec![gen_path(ctx, &comp.path)], // TODO: validate path
        items: comp
            .items
            .iter()
            .filter_map(|item| match item {
                sem::CompItemDef::Field(field) => Some(metadata::CompItemDef::Field(gen_field(
                    ctx, field, comp.flags,
                ))),
                sem::CompItemDef::Event(event) => {
                    Some(metadata::CompItemDef::Event(gen_event(ctx, event)))
                }
                // `on` is invisible to outside
                sem::CompItemDef::On(_) => None,
            })
            .collect(),
    }
}

fn validate_comp_path(ctx: &mut Ctx<'_>, path: &metadata::Path, orig_path: &sem::Path) {
    if path.crate_i != ctx.resolver.local_crate_i {
        ctx.diag.emit(&[Diagnostic {
            level: Level::Error,
            message: "Can't define a component outside the current crate".to_string(),
            code: None,
            spans: orig_path
                .span
                .into_iter()
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

fn gen_vis(ctx: &mut Ctx<'_>, vis: &sem::Visibility) -> metadata::Visibility {
    match vis {
        sem::Visibility::Inherited => metadata::Visibility::Private,
        sem::Visibility::Public { .. } => metadata::Visibility::Public,
        sem::Visibility::Crate { .. } => metadata::Visibility::Restricted(metadata::Path {
            crate_i: ctx.resolver.local_crate_i,
            idents: vec![],
        }),
        // TODO: validate `r`
        sem::Visibility::Restricted { path, .. } => {
            metadata::Visibility::Restricted(gen_path(ctx, path))
        }
    }
}

/// Assumes `path` is already rooted by `super::resolve`.
fn gen_path(ctx: &mut Ctx<'_>, path: &sem::Path) -> metadata::Path {
    // For now, `path` is actually not allowed to be anything from dependent
    // crates, but anyway...
    let crate_i = if let Some(i) = ctx.resolver.find_crate_by_path(&path.syn_path) {
        i
    } else {
        let crate_name = &path.syn_path.segments[0].ident;

        ctx.diag.emit(&[Diagnostic {
            level: Level::Error,
            message: format!("Can't find a crate named `{}`", crate_name),
            code: None,
            spans: path
                .span
                .map(|span| SpanLabel {
                    span,
                    label: None,
                    style: SpanStyle::Primary,
                })
                .into_iter()
                .collect(),
        }]);

        ctx.resolver.local_crate_i
    };

    let idents = path
        .syn_path
        .segments
        .iter()
        .skip(1)
        .map(|seg| gen_ident(&seg.ident))
        .collect();

    metadata::Path { crate_i, idents }
}

fn gen_field(
    ctx: &mut Ctx<'_>,
    field: &sem::FieldDef<'_>,
    comp_flags: metadata::CompFlags,
) -> metadata::FieldDef {
    let mut flags = field.flags;

    if field.field_ty != metadata::FieldType::Wire && field.value.is_some() {
        flags |= metadata::FieldFlags::OPTIONAL;
    }

    // See if `field.ty` refers to a known component.
    let comp_ty = match field.ty.as_ref().unwrap() {
        syn::Type::Path(syn::TypePath { qself: None, path }) => {
            if let Some((crate_i, comp_i)) = ctx.resolver.find_comp_by_path(path) {
                Some(metadata::CompRef { crate_i, comp_i })
            } else {
                None
            }
        }
        _ => None,
    };

    // `builder(simple)` puts some restriction.
    if comp_flags.contains(metadata::CompFlags::SIMPLE_BUILDER)
        && field.field_ty == metadata::FieldType::Const
        && field.value.is_some()
        && field.accessors.set.is_some()
    {
        ctx.diag.emit(&[Diagnostic {
            level: Level::Error,
            message: "`const` field may not have both of a default value and a \
                      setter if the component has `#[builder(simple)]`"
                .to_string(),
            code: None,
            spans: field
                .ident
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

    metadata::FieldDef {
        field_ty: field.field_ty,
        flags,
        ty: comp_ty,
        ident: gen_sem_ident(&field.ident),
        accessors: metadata::FieldAccessors {
            set: field.accessors.set.as_ref().map(|a| metadata::FieldSetter {
                vis: gen_vis(ctx, &a.vis),
            }),
            get: field.accessors.get.as_ref().map(|a| metadata::FieldGetter {
                vis: gen_vis(ctx, &a.vis),
                mode: a.mode,
            }),
            watch: field
                .accessors
                .watch
                .as_ref()
                .map(|a| metadata::FieldWatcher {
                    vis: gen_vis(ctx, &a.vis),
                    event: gen_sem_ident(&a.event),
                }),
        },
    }
}

fn gen_event(ctx: &mut Ctx<'_>, event: &sem::EventDef<'_>) -> metadata::EventDef {
    metadata::EventDef {
        vis: gen_vis(ctx, &event.vis),
        ident: gen_sem_ident(&event.ident),
        inputs: event
            .inputs
            .iter()
            .map(|fn_arg| {
                // TODO: Ensure `fn_arg` is `ident: Ty` at an earlier stage.
                match fn_arg {
                    syn::FnArg::Typed(syn::PatType { pat, .. }) => match &**pat {
                        syn::Pat::Ident(syn::PatIdent {
                            subpat: None,
                            ident,
                            ..
                        }) => gen_ident(ident),
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                }
            })
            .collect(),
    }
}

fn gen_ident(i: &syn::Ident) -> metadata::Ident {
    i.to_string()
}

fn gen_sem_ident(i: &sem::Ident) -> metadata::Ident {
    i.sym.clone()
}
