use std::fmt::Write;

use super::{
    fields, initgen::DepAnalysis, known_fields, paths, CompSharedTy, Ctx, InnerValueField,
};

/// Generate `impl Drop for ComponentTypeShared`.
pub fn gen_shared_drop(ctx: &Ctx<'_>, dep_analysis: &DepAnalysis, out: &mut String) {
    let comp_ident = &ctx.cur_comp.ident.sym;

    let num_subs = dep_analysis.num_subs();
    if num_subs == 0 {
        // Nothing to do in `drop`
        return;
    }

    writeln!(
        out,
        "impl {} for {} {{",
        paths::TRAIT_DROP,
        CompSharedTy(comp_ident)
    )
    .unwrap();
    writeln!(out, "    fn drop(&mut self) {{").unwrap();
    writeln!(out, "        unsafe {{").unwrap();
    writeln!(
        out,
        "            {meth}(self.{wm}, &mut self.{subs});",
        meth = ctx.path_unsubscribe_subs_unchecked(),
        wm = InnerValueField(known_fields::WM),
        subs = fields::SUBS,
    )
    .unwrap();
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();
}
