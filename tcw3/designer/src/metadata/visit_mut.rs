use super::*;

pub trait VisitMut {
    fn visit_crate_mut(&mut self, i: &mut Crate) {
        visit_crate_mut(self, i);
    }

    fn visit_visibility_mut(&mut self, i: &mut Visibility) {
        visit_visibility_mut(self, i);
    }

    fn visit_path_mut(&mut self, i: &mut Path) {
        visit_path_mut(self, i);
    }

    fn visit_ident_mut(&mut self, i: &mut Ident) {
        visit_ident_mut(self, i);
    }

    fn visit_comp_def_mut(&mut self, i: &mut CompDef) {
        visit_comp_def_mut(self, i);
    }

    fn visit_comp_item_def_mut(&mut self, i: &mut CompItemDef) {
        visit_comp_item_def_mut(self, i);
    }

    fn visit_field_def_mut(&mut self, i: &mut FieldDef) {
        visit_field_def_mut(self, i);
    }

    fn visit_comp_ref_mut(&mut self, i: &mut CompRef) {
        visit_comp_ref_mut(self, i);
    }

    fn visit_field_accessors_mut(&mut self, i: &mut FieldAccessors) {
        visit_field_accessors_mut(self, i);
    }

    fn visit_field_getter_mut(&mut self, i: &mut FieldGetter) {
        visit_field_getter_mut(self, i);
    }

    fn visit_field_setter_mut(&mut self, i: &mut FieldSetter) {
        visit_field_setter_mut(self, i);
    }

    fn visit_field_watcher_mut(&mut self, i: &mut FieldWatcher) {
        visit_field_watcher_mut(self, i);
    }

    fn visit_event_def_mut(&mut self, i: &mut EventDef) {
        visit_event_def_mut(self, i);
    }

    fn visit_crate_i_mut(&mut self, i: &mut usize) {
        let _ = i;
    }

    fn visit_comp_i_mut(&mut self, i: &mut usize) {
        let _ = i;
    }
}

pub fn visit_repo_mut(v: &mut (impl VisitMut + ?Sized), i: &mut Repo) {
    i.crates.iter_mut().for_each(|i| v.visit_crate_mut(i));
}

pub fn visit_crate_mut(v: &mut (impl VisitMut + ?Sized), i: &mut Crate) {
    i.comps.iter_mut().for_each(|i| v.visit_comp_def_mut(i));
}

pub fn visit_visibility_mut(v: &mut (impl VisitMut + ?Sized), i: &mut Visibility) {
    match i {
        Visibility::Private | Visibility::Public => {}
        Visibility::Restricted(path) => v.visit_path_mut(path),
    }
}

pub fn visit_path_mut(v: &mut (impl VisitMut + ?Sized), i: &mut Path) {
    v.visit_crate_i_mut(&mut i.crate_i);
    i.idents.iter_mut().for_each(|i| v.visit_ident_mut(i));
}

pub fn visit_ident_mut(_: &mut (impl VisitMut + ?Sized), _: &mut Ident) {}

pub fn visit_comp_def_mut(v: &mut (impl VisitMut + ?Sized), i: &mut CompDef) {
    v.visit_visibility_mut(&mut i.vis);
    i.paths.iter_mut().for_each(|i| v.visit_path_mut(i));
    i.items
        .iter_mut()
        .for_each(|i| v.visit_comp_item_def_mut(i));
}

pub fn visit_comp_item_def_mut(v: &mut (impl VisitMut + ?Sized), i: &mut CompItemDef) {
    match i {
        CompItemDef::Field(i) => v.visit_field_def_mut(i),
        CompItemDef::Event(i) => v.visit_event_def_mut(i),
    }
}

pub fn visit_field_def_mut(v: &mut (impl VisitMut + ?Sized), i: &mut FieldDef) {
    v.visit_ident_mut(&mut i.ident);
    if let Some(i) = &mut i.ty {
        v.visit_comp_ref_mut(i);
    }
    v.visit_field_accessors_mut(&mut i.accessors);
}

pub fn visit_comp_ref_mut(v: &mut (impl VisitMut + ?Sized), i: &mut CompRef) {
    v.visit_crate_i_mut(&mut i.crate_i);
    v.visit_comp_i_mut(&mut i.comp_i);
}

pub fn visit_field_accessors_mut(v: &mut (impl VisitMut + ?Sized), i: &mut FieldAccessors) {
    if let Some(i) = &mut i.get {
        v.visit_field_getter_mut(i);
    }
    if let Some(i) = &mut i.set {
        v.visit_field_setter_mut(i);
    }
    if let Some(i) = &mut i.watch {
        v.visit_field_watcher_mut(i);
    }
}

pub fn visit_field_getter_mut(v: &mut (impl VisitMut + ?Sized), i: &mut FieldGetter) {
    v.visit_visibility_mut(&mut i.vis);
}

pub fn visit_field_setter_mut(v: &mut (impl VisitMut + ?Sized), i: &mut FieldSetter) {
    v.visit_visibility_mut(&mut i.vis);
}

pub fn visit_field_watcher_mut(v: &mut (impl VisitMut + ?Sized), i: &mut FieldWatcher) {
    v.visit_visibility_mut(&mut i.vis);
    v.visit_ident_mut(&mut i.event);
}

pub fn visit_event_def_mut(v: &mut (impl VisitMut + ?Sized), i: &mut EventDef) {
    v.visit_visibility_mut(&mut i.vis);
    v.visit_ident_mut(&mut i.ident);
    i.inputs.iter_mut().for_each(|i| v.visit_ident_mut(i));
}
