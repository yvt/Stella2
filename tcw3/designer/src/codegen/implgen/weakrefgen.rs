use std::fmt::Write;

use super::{fields, methods, paths, CompSharedTy, CompTy, Ctx, WeakCompTy};

/// Generate `WeakComponent`, `Compoent::downgrade`, and
/// `WeakComponent::upgrade`.
pub fn gen_weakref_items(ctx: &Ctx<'_>, out: &mut String) {
    let comp = ctx.cur_comp;
    let comp_ident = &comp.ident.sym;

    writeln!(out, "#[allow(dead_code)]").unwrap();
    writeln!(
        out,
        "{vis} struct {ty} {{",
        vis = comp.vis,
        ty = WeakCompTy(comp_ident)
    )
    .unwrap();
    writeln!(
        out,
        "    {field}: {weak}<{ty}>,",
        field = fields::SHARED,
        weak = paths::WEAK,
        ty = CompSharedTy(comp_ident)
    )
    .unwrap();
    writeln!(out, "}}").unwrap();

    // `ComponentType::downgrade`
    writeln!(out, "#[allow(dead_code)]").unwrap();
    writeln!(out, "impl {} {{", CompTy(comp_ident)).unwrap();
    writeln!(
        out,
        "    {vis} fn {ident}(&self) -> {weakty} {{",
        vis = comp.vis,
        ident = methods::DOWNGRADE,
        weakty = WeakCompTy(comp_ident),
    )
    .unwrap();
    writeln!(
        out,
        "        {weakty} {{ {field}: {rc}::downgrade(&self.{field}) }}",
        weakty = WeakCompTy(comp_ident),
        field = fields::SHARED,
        rc = paths::RC,
    )
    .unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();

    // `WeakComponentType::upgrade`
    writeln!(out, "#[allow(dead_code)]").unwrap();
    writeln!(out, "impl {} {{", WeakCompTy(comp_ident)).unwrap();
    writeln!(
        out,
        "    {vis} fn {ident}(&self) -> {o}<{ty}> {{",
        vis = comp.vis,
        ident = methods::UPGRADE,
        o = paths::OPTION,
        ty = CompTy(comp_ident),
    )
    .unwrap();
    writeln!(
        out,
        "        self.{field}.upgrade().map(|{field}| {ty} {{ {field} }})",
        field = fields::SHARED,
        ty = CompTy(comp_ident),
    )
    .unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();
}
