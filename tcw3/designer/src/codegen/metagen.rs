//! Metadata generation
use super::sem;
use crate::metadata;

pub fn gen_crate(comps: &[sem::CompDef<'_>]) -> metadata::Crate {
    metadata::Crate {
        comps: comps.iter().map(gen_comp).collect(),
    }
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

fn gen_comp(comp: &sem::CompDef<'_>) -> metadata::CompDef {
    metadata::CompDef {
        flags: comp.flags,
        vis: gen_vis(&comp.vis),
        paths: vec![gen_path(&comp.path)],
        items: comp
            .items
            .iter()
            .filter_map(|item| match item {
                sem::CompItemDef::Field(field) => {
                    Some(metadata::CompItemDef::Field(gen_field(field)))
                }
                sem::CompItemDef::Event(event) => {
                    Some(metadata::CompItemDef::Event(gen_event(event)))
                }
                // `on` is invisible to outside
                sem::CompItemDef::On(_) => None,
            })
            .collect(),
    }
}

fn gen_vis(vis: &syn::Visibility) -> metadata::Visibility {
    match vis {
        syn::Visibility::Inherited => metadata::Visibility::Private,
        syn::Visibility::Public(_) => metadata::Visibility::Public,
        syn::Visibility::Crate(_) => metadata::Visibility::Restricted(metadata::Path {
            root: metadata::PathRoot::Crate,
            idents: vec![],
        }),
        // TODO: validate `r`
        syn::Visibility::Restricted(r) => metadata::Visibility::Restricted(gen_path(&r.path)),
    }
}

/// Assumes `path` is already rooted by `super::resolve`.
fn gen_path(path: &syn::Path) -> metadata::Path {
    if path.leading_colon.is_some() {
        unimplemented!(
            "This function is not supposed to see a rooted path \
             because it's only used for local paths for now, but this \
             might change in a potential future"
        );
    }

    let root = if path.segments[0].ident.to_string() == "crate" {
        metadata::PathRoot::Crate
    } else {
        // The postcondition of a successful call to `resolve_paths`
        unreachable!();
    };

    let idents = path
        .segments
        .iter()
        .skip(1)
        .map(|seg| gen_ident(&seg.ident))
        .collect();

    metadata::Path { root, idents }
}

fn gen_field(field: &sem::FieldDef<'_>) -> metadata::FieldDef {
    let mut flags = field.flags;

    if field.field_ty == metadata::FieldType::Const && field.value.is_some() {
        flags |= metadata::FieldFlags::OPTIONAL;
    }

    metadata::FieldDef {
        field_ty: field.field_ty,
        flags,
        ty: None, // TODO
        ident: gen_sem_ident(&field.ident),
        accessors: metadata::FieldAccessors {
            set: field.accessors.set.as_ref().map(|a| metadata::FieldSetter {
                vis: gen_vis(&a.vis),
            }),
            get: field.accessors.get.as_ref().map(|a| metadata::FieldGetter {
                vis: gen_vis(&a.vis),
                mode: a.mode,
            }),
            watch: field
                .accessors
                .watch
                .as_ref()
                .map(|a| metadata::FieldWatcher {
                    vis: gen_vis(&a.vis),
                    event: gen_sem_ident(&a.event),
                }),
        },
    }
}

fn gen_event(event: &sem::EventDef<'_>) -> metadata::EventDef {
    metadata::EventDef {
        vis: gen_vis(&event.vis),
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
