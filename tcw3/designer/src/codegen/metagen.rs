//! Metadata generation
use super::sem;
use crate::metadata;

pub fn metagen_crate(comps: &[sem::CompDef<'_>]) -> metadata::Crate {
    metadata::Crate {
        comps: comps.iter().map(metagen_comp).collect(),
    }
}

fn metagen_comp(comp: &sem::CompDef<'_>) -> metadata::CompDef {
    metadata::CompDef {
        flags: comp.flags,
        vis: metagen_vis(&comp.vis),
        paths: vec![metagen_path(&comp.path)],
        items: comp
            .items
            .iter()
            .filter_map(|item| match item {
                sem::CompItemDef::Field(field) => {
                    Some(metadata::CompItemDef::Field(metagen_field(field)))
                }
                sem::CompItemDef::Event(event) => {
                    Some(metadata::CompItemDef::Event(metagen_event(event)))
                }
                // `on` is invisible to outside
                sem::CompItemDef::On(_) => None,
            })
            .collect(),
    }
}

fn metagen_vis(vis: &syn::Visibility) -> metadata::Visibility {
    match vis {
        syn::Visibility::Inherited => metadata::Visibility::Private,
        syn::Visibility::Public(_) => metadata::Visibility::Public,
        syn::Visibility::Crate(_) => metadata::Visibility::Restricted(metadata::Path {
            root: metadata::PathRoot::Crate,
            idents: vec![],
        }),
        // TODO: validate `r`
        syn::Visibility::Restricted(r) => metadata::Visibility::Restricted(metagen_path(&r.path)),
    }
}

/// Assumes `path` is already rooted by `super::resolve`.
fn metagen_path(path: &syn::Path) -> metadata::Path {
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
        .map(|seg| metagen_ident(&seg.ident))
        .collect();

    metadata::Path { root, idents }
}

fn metagen_field(field: &sem::FieldDef<'_>) -> metadata::FieldDef {
    metadata::FieldDef {
        field_ty: field.field_ty,
        flags: field.flags,
        ident: metagen_sem_ident(&field.ident),
        accessors: metadata::FieldAccessors {
            set: field.accessors.set.as_ref().map(|a| metadata::FieldSetter {
                vis: metagen_vis(&a.vis),
            }),
            get: field.accessors.get.as_ref().map(|a| metadata::FieldGetter {
                vis: metagen_vis(&a.vis),
                mode: a.mode,
            }),
            watch: field
                .accessors
                .watch
                .as_ref()
                .map(|a| metadata::FieldWatcher {
                    vis: metagen_vis(&a.vis),
                    event: metagen_sem_ident(&a.event),
                }),
        },
    }
}

fn metagen_event(event: &sem::EventDef<'_>) -> metadata::EventDef {
    metadata::EventDef {
        vis: metagen_vis(&event.vis),
        ident: metagen_sem_ident(&event.ident),
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
                        }) => metagen_ident(ident),
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                }
            })
            .collect(),
    }
}

fn metagen_ident(i: &syn::Ident) -> metadata::Ident {
    i.to_string()
}

fn metagen_sem_ident(i: &sem::Ident) -> metadata::Ident {
    i.sym.clone()
}
