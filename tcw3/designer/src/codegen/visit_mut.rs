use syn::visit_mut::VisitMut;

use super::parser;

pub trait TcwdlVisitMut: VisitMut {
    fn visit_file_mut(&mut self, i: &mut parser::File) {
        visit_file_mut(self, i);
    }

    fn visit_item_mut(&mut self, i: &mut parser::Item) {
        visit_item_mut(self, i);
    }

    fn visit_comp_mut(&mut self, i: &mut parser::Comp) {
        visit_comp_mut(self, i);
    }

    fn visit_comp_item_mut(&mut self, i: &mut parser::CompItem) {
        visit_comp_item_mut(self, i);
    }

    fn visit_comp_item_field_mut(&mut self, i: &mut parser::CompItemField) {
        visit_comp_item_field_mut(self, i);
    }

    fn visit_field_accessor_mut(&mut self, i: &mut parser::FieldAccessor) {
        visit_field_accessor_mut(self, i);
    }

    fn visit_comp_item_init_mut(&mut self, i: &mut parser::CompItemInit) {
        visit_comp_item_init_mut(self, i);
    }

    fn visit_comp_item_watch_mut(&mut self, i: &mut parser::CompItemWatch) {
        visit_comp_item_watch_mut(self, i);
    }

    fn visit_comp_item_event_mut(&mut self, i: &mut parser::CompItemEvent) {
        visit_comp_item_event_mut(self, i);
    }

    fn visit_dyn_expr_mut(&mut self, i: &mut parser::DynExpr) {
        visit_dyn_expr_mut(self, i);
    }

    fn visit_func_mut(&mut self, i: &mut parser::Func) {
        visit_func_mut(self, i);
    }

    fn visit_func_input_mut(&mut self, i: &mut parser::FuncInput) {
        visit_func_input_mut(self, i);
    }

    fn visit_input_mut(&mut self, i: &mut parser::Input) {
        visit_input_mut(self, i);
    }

    fn visit_input_field_mut(&mut self, i: &mut parser::InputField) {
        visit_input_field_mut(self, i);
    }

    fn visit_obj_init_mut(&mut self, i: &mut parser::ObjInit) {
        visit_obj_init_mut(self, i);
    }

    fn visit_obj_init_field_mut(&mut self, i: &mut parser::ObjInitField) {
        visit_obj_init_field_mut(self, i);
    }
}

pub fn visit_file_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::File) {
    i.items
        .iter_mut()
        .for_each(|i| TcwdlVisitMut::visit_item_mut(v, i));
}

pub fn visit_item_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::Item) {
    match i {
        parser::Item::Import(i) => v.visit_lit_str_mut(i),
        parser::Item::Use(i) => v.visit_item_use_mut(i),
        parser::Item::Comp(i) => v.visit_comp_mut(i),
    }
}

pub fn visit_comp_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::Comp) {
    i.attrs.iter_mut().for_each(|i| v.visit_attribute_mut(i));
    v.visit_visibility_mut(&mut i.vis);
    v.visit_path_mut(&mut i.path);
    i.items.iter_mut().for_each(|i| v.visit_comp_item_mut(i));
}

pub fn visit_comp_item_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::CompItem) {
    match i {
        parser::CompItem::Field(i) => v.visit_comp_item_field_mut(i),
        parser::CompItem::Init(i) => v.visit_comp_item_init_mut(i),
        parser::CompItem::Watch(i) => v.visit_comp_item_watch_mut(i),
        parser::CompItem::Event(i) => v.visit_comp_item_event_mut(i),
    }
}

pub fn visit_comp_item_field_mut(
    v: &mut (impl TcwdlVisitMut + ?Sized),
    i: &mut parser::CompItemField,
) {
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

pub fn visit_field_accessor_mut(
    v: &mut (impl TcwdlVisitMut + ?Sized),
    i: &mut parser::FieldAccessor,
) {
    match i {
        parser::FieldAccessor::Set { set_token: _, vis } => {
            v.visit_visibility_mut(vis);
        }
        parser::FieldAccessor::Get {
            get_token: _,
            vis,
            mode: _,
        } => {
            v.visit_visibility_mut(vis);
        }
        parser::FieldAccessor::Watch {
            watch_token: _,
            vis,
            mode: _,
        } => {
            v.visit_visibility_mut(vis);
        }
    }
}

pub fn visit_comp_item_init_mut(
    v: &mut (impl TcwdlVisitMut + ?Sized),
    i: &mut parser::CompItemInit,
) {
    i.attrs.iter_mut().for_each(|i| v.visit_attribute_mut(i));
    v.visit_func_mut(&mut i.func);
}

pub fn visit_comp_item_watch_mut(
    v: &mut (impl TcwdlVisitMut + ?Sized),
    i: &mut parser::CompItemWatch,
) {
    i.attrs.iter_mut().for_each(|i| v.visit_attribute_mut(i));
    v.visit_func_mut(&mut i.func);
}

pub fn visit_comp_item_event_mut(
    v: &mut (impl TcwdlVisitMut + ?Sized),
    i: &mut parser::CompItemEvent,
) {
    i.attrs.iter_mut().for_each(|i| v.visit_attribute_mut(i));
    v.visit_visibility_mut(&mut i.vis);
    v.visit_ident_mut(&mut i.ident);
    i.inputs.iter_mut().for_each(|i| v.visit_fn_arg_mut(i));
}

pub fn visit_dyn_expr_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::DynExpr) {
    match i {
        parser::DynExpr::Func(i) => v.visit_func_mut(i),
        parser::DynExpr::ObjInit(i) => v.visit_obj_init_mut(i),
    }
}

pub fn visit_func_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::Func) {
    i.inputs.iter_mut().for_each(|i| v.visit_func_input_mut(i));
    v.visit_expr_mut(&mut i.body);
}

pub fn visit_func_input_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::FuncInput) {
    v.visit_input_mut(&mut i.input);
}

pub fn visit_input_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::Input) {
    match i {
        parser::Input::Field(i) => v.visit_input_field_mut(i),
        parser::Input::This(_) => {}
    }
}

pub fn visit_input_field_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::InputField) {
    v.visit_input_mut(&mut i.base);
    v.visit_ident_mut(&mut i.member);
}

pub fn visit_obj_init_mut(v: &mut (impl TcwdlVisitMut + ?Sized), i: &mut parser::ObjInit) {
    v.visit_path_mut(&mut i.path);
    i.fields
        .iter_mut()
        .for_each(|i| v.visit_obj_init_field_mut(i));
}

pub fn visit_obj_init_field_mut(
    v: &mut (impl TcwdlVisitMut + ?Sized),
    i: &mut parser::ObjInitField,
) {
    v.visit_ident_mut(&mut i.ident);
    v.visit_dyn_expr_mut(&mut i.dyn_expr);
}
