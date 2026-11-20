use moxy_token::{Group, MAX_NESTED_NODES, TokenStream};

use super::*;

// Walks a tree no deeper than `MAX_NESTED_NODES` nested nodes, and records
// whether it found one deeper
#[derive(Default)]
struct DepthCheck {
    nodes: usize,
    exceeded: bool,
}

impl DepthCheck {
    fn nest(&mut self, walk: impl FnOnce(&mut Self)) {
        if self.exceeded || self.nodes == MAX_NESTED_NODES {
            self.exceeded = true;
            return;
        }

        self.nodes += 1;
        walk(self);
        self.nodes -= 1;
    }

    fn tokens(&mut self, tokens: &TokenStream) {
        self.exceeded = self.exceeded || tokens.nests_deeper_than(MAX_NESTED_NODES - self.nodes);
    }

    fn group(&mut self, group: &Group) {
        self.nest(|check| check.tokens(&group.tokens));
    }
}

impl<'ast> Visit<'ast> for DepthCheck {
    fn visit_expr(&mut self, node: &'ast Expr) {
        self.nest(|check| match node {
            Expr::Verbatim(tokens) => check.tokens(tokens),
            _ => walk_expr(check, node),
        });
    }

    fn visit_type(&mut self, node: &'ast Type) {
        self.nest(|check| walk_type(check, node));
    }

    fn visit_pattern(&mut self, node: &'ast Pattern) {
        self.nest(|check| walk_pattern(check, node));
    }

    fn visit_item(&mut self, node: &'ast Item) {
        self.nest(|check| walk_item(check, node));
    }

    fn visit_stmt_block(&mut self, node: &'ast StmtBlock) {
        self.nest(|check| walk_stmt_block(check, node));
    }

    fn visit_macro_call(&mut self, node: &'ast MacroCall) {
        self.group(&node.body);
        walk_macro_call(self, node);
    }

    fn visit_item_macro_rules(&mut self, node: &'ast ItemMacroRules) {
        self.group(&node.body);
        walk_item_macro_rules(self, node);
    }

    fn visit_item_decl_macro(&mut self, node: &'ast ItemDeclMacro) {
        if let Some(params) = &node.params {
            self.group(params);
        }

        self.group(&node.body);
        walk_item_decl_macro(self, node);
    }

    fn visit_meta_content(&mut self, node: &'ast MetaContent) {
        match node {
            MetaContent::Unit => {}
            MetaContent::List(group) => self.group(group),
            MetaContent::Expr { expr, .. } => self.tokens(expr),
        }
    }
}

macro_rules! depth_checks {
    ($($ty:ty => $visit:ident;)*) => {$(
        impl $ty {
            /// Whether the tree nests past [`MAX_DEPTH`](moxy_token::MAX_DEPTH), as a tree built in
            /// code can: whether more than [`MAX_NESTED_NODES`] expressions,
            /// types, patterns, items, blocks, and token groups nest in one
            /// another, a limit the parser's trees stay below. Every
            /// traversal of a tree that does not runs on the stack
            /// `MAX_DEPTH` is sized for. The check stops descending once the
            /// tree is past the limit, so it runs on a tree nested any depth.
            pub fn nests_past_max_depth(&self) -> bool {
                let mut check = DepthCheck::default();
                check.$visit(self);
                check.exceeded
            }
        }
    )*};
}

depth_checks! {
    Item => visit_item;
    Expr => visit_expr;
    Type => visit_type;
    Pattern => visit_pattern;
    Stmt => visit_stmt;
    StmtBlock => visit_stmt_block;
}

impl File {
    /// Whether the file nests past [`MAX_DEPTH`](moxy_token::MAX_DEPTH), as
    /// [`Item::nests_past_max_depth`] counts it.
    pub fn nests_past_max_depth(&self) -> bool {
        let mut check = DepthCheck::default();
        check.visit_attributes(&self.attrs);
        self.items.iter().for_each(|item| check.visit_item(item));
        check.exceeded
    }
}
