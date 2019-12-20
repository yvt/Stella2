use syn::visit_mut::VisitMut;

use super::*;

pub trait TcwdlVisitMut: VisitMut {
    fn visit_file_mut(&mut self, i: &mut File) {
        visit_file_mut(self, i);
    }

    fn visit_item_mut(&mut self, i: &mut Item) {
        visit_item_mut(self, i);
    }

    fn visit_comp_mut(&mut self, i: &mut Comp) {
        visit_comp_mut(self, i);
    }

    fn visit_comp_item_mut(&mut self, i: &mut CompItem) {
        visit_comp_item_mut(self, i);
    }

    fn visit_comp_item_field_mut(&mut self, i: &mut CompItemField) {
        visit_comp_item_field_mut(self, i);
    }

    fn visit_field_accessor_mut(&mut self, i: &mut FieldAccessor) {
        visit_field_accessor_mut(self, i);
    }

    fn visit_comp_item_on_mut(&mut self, i: &mut CompItemOn) {
        visit_comp_item_on_mut(self, i);
    }

    fn visit_comp_item_event_mut(&mut self, i: &mut CompItemEvent) {
        visit_comp_item_event_mut(self, i);
    }

    fn visit_dyn_expr_mut(&mut self, i: &mut DynExpr) {
        visit_dyn_expr_mut(self, i);
    }

    fn visit_func_mut(&mut self, i: &mut Func) {
        visit_func_mut(self, i);
    }

    fn visit_trigger_mut(&mut self, i: &mut Trigger) {
        visit_trigger_mut(self, i);
    }

    fn visit_func_input_mut(&mut self, i: &mut FuncInput) {
        visit_func_input_mut(self, i);
    }

    fn visit_input_mut(&mut self, i: &mut Input) {
        visit_input_mut(self, i);
    }

    fn visit_input_selector_mut(&mut self, i: &mut InputSelector) {
        visit_input_selector_mut(self, i);
    }

    fn visit_obj_init_mut(&mut self, i: &mut ObjInit) {
        visit_obj_init_mut(self, i);
    }

    fn visit_obj_init_field_mut(&mut self, i: &mut ObjInitField) {
        visit_obj_init_field_mut(self, i);
    }
}

pub fn visit_file_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut File) {
    i.items
        .iter_mut()
        .for_each(|i| TcwdlVisitMut::visit_item_mut(v, i));
}

pub fn visit_item_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut Item) {
    match i {
        Item::Import(i) => v.visit_lit_str_mut(i),
        Item::Use(i) => v.visit_item_use_mut(i),
        Item::Comp(i) => v.visit_comp_mut(i),
    }
}

pub fn visit_comp_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut Comp) {
    i.attrs.iter_mut().for_each(|i| v.visit_attribute_mut(i));
    v.visit_visibility_mut(&mut i.vis);
    v.visit_path_mut(&mut i.path);
    i.items.iter_mut().for_each(|i| v.visit_comp_item_mut(i));
}

pub fn visit_comp_item_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut CompItem) {
    match i {
        CompItem::Field(i) => v.visit_comp_item_field_mut(i),
        CompItem::On(i) => v.visit_comp_item_on_mut(i),
        CompItem::Event(i) => v.visit_comp_item_event_mut(i),
    }
}

pub fn visit_comp_item_field_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut CompItemField) {
    i.attrs.iter_mut().for_each(|i| v.visit_attribute_mut(i));
    v.visit_visibility_mut(&mut i.vis);
    v.visit_ident_mut(&mut i.ident);
    if let Some(i) = &mut i.ty {
        v.visit_type_mut(i);
    }
    if let Some(accessors) = &mut i.accessors {
        accessors
            .iter_mut()
            .for_each(|i| v.visit_field_accessor_mut(i));
    }
    if let Some(i) = &mut i.dyn_expr {
        v.visit_dyn_expr_mut(i);
    }
}

pub fn visit_field_accessor_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut FieldAccessor) {
    match i {
        FieldAccessor::Set { set_token: _, vis } => {
            v.visit_visibility_mut(vis);
        }
        FieldAccessor::Get {
            get_token: _,
            vis,
            mode: _,
        } => {
            v.visit_visibility_mut(vis);
        }
        FieldAccessor::Watch {
            watch_token: _,
            vis,
            mode: _,
        } => {
            v.visit_visibility_mut(vis);
        }
    }
}

pub fn visit_comp_item_on_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut CompItemOn) {
    i.attrs.iter_mut().for_each(|i| v.visit_attribute_mut(i));
    i.triggers.iter_mut().for_each(|i| v.visit_trigger_mut(i));
    v.visit_func_mut(&mut i.func);
}

pub fn visit_comp_item_event_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut CompItemEvent) {
    i.attrs.iter_mut().for_each(|i| v.visit_attribute_mut(i));
    v.visit_visibility_mut(&mut i.vis);
    v.visit_ident_mut(&mut i.ident);
    i.inputs.iter_mut().for_each(|i| v.visit_fn_arg_mut(i));
}

pub fn visit_dyn_expr_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut DynExpr) {
    match i {
        DynExpr::Func(i) => v.visit_func_mut(i),
        DynExpr::ObjInit(i) => v.visit_obj_init_mut(i),
    }
}

pub fn visit_func_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut Func) {
    i.inputs.iter_mut().for_each(|i| v.visit_func_input_mut(i));
    v.visit_expr_mut(&mut i.body);
}

pub fn visit_trigger_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut Trigger) {
    match i {
        Trigger::Init(_) => {}
        Trigger::Input(i) => v.visit_input_mut(i),
    }
}

pub fn visit_func_input_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut FuncInput) {
    v.visit_input_mut(&mut i.input);
}

pub fn visit_input_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut Input) {
    i.selectors
        .iter_mut()
        .for_each(|i| v.visit_input_selector_mut(i));
}

pub fn visit_input_selector_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut InputSelector) {
    match i {
        InputSelector::Field {
            dot_token: _,
            ident,
        } => {
            v.visit_ident_mut(ident);
        }
    }
}

pub fn visit_obj_init_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut ObjInit) {
    v.visit_path_mut(&mut i.path);
    i.fields
        .iter_mut()
        .for_each(|i| v.visit_obj_init_field_mut(i));
}

pub fn visit_obj_init_field_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut ObjInitField) {
    v.visit_ident_mut(&mut i.ident);
    v.visit_dyn_expr_mut(&mut i.dyn_expr);
}
