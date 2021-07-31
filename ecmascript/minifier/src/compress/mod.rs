use self::drop_console::drop_console;
use self::hoist_decls::DeclHoisterConfig;
use self::optimize::optimizer;
use crate::analyzer::analyze;
use crate::analyzer::ProgramData;
use crate::compress::hoist_decls::decl_hoister;
use crate::debug::dump;
use crate::debug::invoke;
use crate::marks::Marks;
use crate::option::CompressOptions;
use crate::util::Optional;
#[cfg(feature = "pretty_assertions")]
use pretty_assertions::assert_eq;
use std::borrow::Cow;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::time::Instant;
use swc_common::comments::Comments;
use swc_common::pass::CompilerPass;
use swc_common::pass::Repeat;
use swc_common::pass::Repeated;
use swc_common::sync::Lrc;
use swc_common::{chain, SourceMap};
use swc_ecma_ast::*;
use swc_ecma_transforms::fixer;
use swc_ecma_transforms::optimization::simplify::dead_branch_remover;
use swc_ecma_transforms::optimization::simplify::expr_simplifier;
use swc_ecma_transforms::pass::JsPass;
use swc_ecma_transforms_base::ext::MapWithMut;
use swc_ecma_utils::StmtLike;
use swc_ecma_visit::as_folder;
use swc_ecma_visit::noop_visit_mut_type;
use swc_ecma_visit::FoldWith;
use swc_ecma_visit::VisitMut;
use swc_ecma_visit::VisitMutWith;

mod drop_console;
mod hoist_decls;
mod optimize;

pub(crate) fn compressor<'a>(
    cm: Lrc<SourceMap>,
    marks: Marks,
    options: &'a CompressOptions,
    comments: Option<&'a dyn Comments>,
) -> impl 'a + JsPass {
    let console_remover = Optional {
        enabled: options.drop_console,
        visitor: drop_console(),
    };
    let compressor = Compressor {
        cm,
        marks,
        options,
        comments,
        changed: false,
        pass: 0,
        data: None,
    };

    chain!(
        console_remover,
        Repeat::new(as_folder(compressor)),
        expr_simplifier()
    )
}

struct Compressor<'a> {
    cm: Lrc<SourceMap>,
    marks: Marks,
    options: &'a CompressOptions,
    comments: Option<&'a dyn Comments>,
    changed: bool,
    pass: usize,
    data: Option<ProgramData>,
}

impl CompilerPass for Compressor<'_> {
    fn name() -> Cow<'static, str> {
        "compressor".into()
    }
}

impl Repeated for Compressor<'_> {
    fn changed(&self) -> bool {
        self.changed
    }

    fn reset(&mut self) {
        self.changed = false;
        self.pass += 1;
        self.data = None;
    }
}

impl Compressor<'_> {
    fn handle_stmt_likes<T>(&mut self, stmts: &mut Vec<T>)
    where
        T: StmtLike,
        Vec<T>: VisitMutWith<Self> + for<'aa> VisitMutWith<hoist_decls::Hoister<'aa>>,
    {
        // Skip if `use asm` exists.
        if stmts.iter().any(|stmt| match stmt.as_stmt() {
            Some(v) => match v {
                Stmt::Expr(stmt) => match &*stmt.expr {
                    Expr::Lit(Lit::Str(Str {
                        value,
                        has_escape: false,
                        ..
                    })) => &**value == "use asm",
                    _ => false,
                },
                _ => false,
            },
            None => false,
        }) {
            return;
        }

        stmts.visit_mut_children_with(self);

        // TODO: drop unused
    }
}

impl VisitMut for Compressor<'_> {
    noop_visit_mut_type!();

    fn visit_mut_module(&mut self, n: &mut Module) {
        debug_assert!(self.data.is_none());
        self.data = Some(analyze(&*n));

        if self.options.passes != 0 && self.options.passes + 1 <= self.pass {
            let done = dump(&*n);
            log::debug!("===== Done =====\n{}", done);
            return;
        }

        // Temporary
        if self.pass > 30 {
            panic!("Infinite loop detected (current pass = {})", self.pass)
        }

        let start = if cfg!(feature = "debug") {
            let start = dump(&n.clone().fold_with(&mut fixer(None)));
            log::debug!("===== Start =====\n{}", start);
            start
        } else {
            String::new()
        };

        {
            log::info!(
                "compress: Running expression simplifier (pass = {})",
                self.pass
            );

            let start_time = Instant::now();

            let mut visitor = expr_simplifier();
            n.visit_mut_with(&mut visitor);
            self.changed |= visitor.changed();
            if visitor.changed() {
                log::debug!("compressor: Simplified expressions");
                if cfg!(feature = "debug") {
                    log::debug!("===== Simplified =====\n{}", dump(&*n));
                }
            }

            let end_time = Instant::now();

            log::info!(
                "compress: expr_simplifier took {:?} (pass = {})",
                end_time - start_time,
                self.pass
            );

            if cfg!(feature = "debug") && !visitor.changed() {
                let simplified = dump(&n.clone().fold_with(&mut fixer(None)));
                if start != simplified {
                    assert_eq!(
                        DebugUsingDisplay(&start),
                        DebugUsingDisplay(&simplified),
                        "Invalid state: expr_simplifier: The code is changed but changed is not \
                         setted to true",
                    )
                }
            }
        }

        {
            log::info!("compress: Running optimizer (pass = {})", self.pass);

            let start_time = Instant::now();
            // TODO: reset_opt_flags
            //
            // This is swc version of `node.optimize(this);`.
            let mut visitor = optimizer(
                self.cm.clone(),
                self.marks,
                self.options,
                self.comments,
                self.data.as_ref().unwrap(),
            );
            n.visit_mut_with(&mut visitor);
            self.changed |= visitor.changed();

            let end_time = Instant::now();

            log::info!(
                "compress: Optimizer took {:?} (pass = {})",
                end_time - start_time,
                self.pass
            );
            // let done = dump(&*n);
            // log::debug!("===== Result =====\n{}", done);
        }

        if self.options.conditionals || self.options.dead_code {
            let start = if cfg!(feature = "debug") {
                dump(&*n)
            } else {
                "".into()
            };

            let start_time = Instant::now();

            let mut v = dead_branch_remover();
            n.map_with_mut(|n| n.fold_with(&mut v));

            let end_time = Instant::now();

            if cfg!(feature = "debug") {
                let simplified = dump(&*n);

                if start != simplified {
                    log::debug!(
                        "===== Removed dead branches =====\n{}\n==== ===== ===== ===== ======\n{}",
                        start,
                        simplified
                    );
                }
            }

            log::info!(
                "compress: dead_branch_remover took {:?} (pass = {})",
                end_time - start_time,
                self.pass
            );

            self.changed |= v.changed();
        }

        n.visit_mut_children_with(self);

        invoke(&*n);
    }

    fn visit_mut_module_items(&mut self, stmts: &mut Vec<ModuleItem>) {
        {
            let mut v = decl_hoister(
                DeclHoisterConfig {
                    hoist_fns: self.options.hoist_fns,
                    hoist_vars: self.options.hoist_vars,
                    top_level: self.options.top_level(),
                },
                self.data.as_ref().unwrap(),
            );
            stmts.visit_mut_with(&mut v);
            self.changed |= v.changed();
        }

        self.handle_stmt_likes(stmts);

        stmts.retain(|stmt| match stmt {
            ModuleItem::Stmt(Stmt::Empty(..)) => false,
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl {
                decl: Decl::Var(VarDecl { decls, .. }),
                ..
            }))
            | ModuleItem::Stmt(Stmt::Decl(Decl::Var(VarDecl { decls, .. })))
                if decls.is_empty() =>
            {
                false
            }
            _ => true,
        });
    }

    fn visit_mut_script(&mut self, n: &mut Script) {
        debug_assert!(self.data.is_none());
        self.data = Some(analyze(&*n));

        {
            let mut v = decl_hoister(
                DeclHoisterConfig {
                    hoist_fns: self.options.hoist_fns,
                    hoist_vars: self.options.hoist_vars,
                    top_level: self.options.top_level(),
                },
                self.data.as_ref().unwrap(),
            );
            n.body.visit_mut_with(&mut v);
            self.changed |= v.changed();
        }

        n.visit_mut_children_with(self);
    }

    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        self.handle_stmt_likes(stmts);

        stmts.retain(|stmt| match stmt {
            Stmt::Empty(..) => false,
            Stmt::Decl(Decl::Var(VarDecl { decls, .. })) if decls.is_empty() => false,
            _ => true,
        });
    }
}

#[derive(PartialEq, Eq)]
struct DebugUsingDisplay<'a>(pub &'a str);

impl<'a> Debug for DebugUsingDisplay<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self.0, f)
    }
}