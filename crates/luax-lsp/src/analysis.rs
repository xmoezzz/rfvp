use std::collections::{BTreeMap, HashMap, HashSet};

use crate::config::{ConfigGlobal, ConfigSyscall, LuaxConfig};
use crate::syntax::{
    parse, Expr, ExprKind, FuncName, GlobalEntry, Name, ParseError, Program, Span, Stmt, StmtKind, TableField,
    Token,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKindLite {
    Local,
    Global,
    Function,
    Parameter,
    Field,
    Module,
    ConfigGlobal,
    Syscall,
}

#[derive(Debug, Clone)]
pub struct SymbolDef {
    pub id: usize,
    pub name: String,
    pub kind: SymbolKindLite,
    pub span: Span,
    pub visible: Span,
    pub detail: String,
    pub documentation: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct SymbolRef {
    pub name: String,
    pub span: Span,
    pub def_id: Option<usize>,
    pub is_write: bool,
}

#[derive(Debug, Clone)]
pub struct MemberAccess {
    pub owner_def: Option<usize>,
    pub owner_module: Option<String>,
    pub name: String,
    pub span: Span,
    pub def_id: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    pub span: Span,
    pub message: String,
    pub severity: DiagnosticSeverityLite,
}

#[derive(Debug, Clone, Copy)]
pub enum DiagnosticSeverityLite {
    Error,
    Warning,
    Hint,
}

#[derive(Debug, Clone)]
pub struct OutlineItem {
    pub name: String,
    pub kind: SymbolKindLite,
    pub span: Span,
    pub selection: Span,
    pub children: Vec<OutlineItem>,
}

#[derive(Debug, Clone)]
pub struct AliasModule {
    pub name: String,
    pub module_name: String,
    pub visible: Span,
}

#[derive(Debug, Clone)]
pub struct DocumentAnalysis {
    pub module_name: Option<String>,
    pub program: Program,
    pub parse_errors: Vec<ParseError>,
    pub tokens: Vec<Token>,
    pub defs: Vec<SymbolDef>,
    pub refs: Vec<SymbolRef>,
    pub member_accesses: Vec<MemberAccess>,
    pub diagnostics: Vec<LintDiagnostic>,
    pub outlines: Vec<OutlineItem>,
    pub aliases: Vec<AliasModule>,
}

impl DocumentAnalysis {
    pub fn symbol_def_at(&self, offset: usize) -> Option<&SymbolDef> {
        self.defs.iter().find(|def| contains(def.span, offset))
    }

    pub fn symbol_ref_at(&self, offset: usize) -> Option<&SymbolRef> {
        self.refs.iter().find(|r| contains(r.span, offset))
    }

    pub fn member_at(&self, offset: usize) -> Option<&MemberAccess> {
        self.member_accesses.iter().find(|r| contains(r.span, offset))
    }

    pub fn visible_defs(&self, offset: usize) -> Vec<&SymbolDef> {
        let mut out: Vec<&SymbolDef> = self
            .defs
            .iter()
            .filter(|def| contains(def.visible, offset))
            .collect();
        out.sort_by_key(|def| (def.span.start, def.id));
        out
    }
}

pub fn analyze_document(text: &str, module_name: Option<String>, config: Option<&LuaxConfig>) -> DocumentAnalysis {
    let (program, parse_errors, tokens) = parse(text);
    let mut analyzer = Analyzer::new(module_name.clone(), config.cloned().unwrap_or_default(), program.clone());
    analyzer.run();
    let outlines = analyzer.build_outline();
    DocumentAnalysis {
        module_name,
        program,
        parse_errors,
        tokens,
        defs: analyzer.defs,
        refs: analyzer.refs,
        member_accesses: analyzer.member_accesses,
        diagnostics: analyzer.diagnostics,
        outlines,
        aliases: analyzer.aliases,
    }
}

struct Analyzer {
    module_name: Option<String>,
    config: LuaxConfig,
    program: Program,
    defs: Vec<SymbolDef>,
    refs: Vec<SymbolRef>,
    member_accesses: Vec<MemberAccess>,
    diagnostics: Vec<LintDiagnostic>,
    aliases: Vec<AliasModule>,
    scopes: Vec<HashMap<String, usize>>,
    next_id: usize,
    config_globals: HashMap<String, ConfigGlobal>,
    config_syscalls: HashMap<String, ConfigSyscall>,
    emitted_duplicate: HashSet<(String, usize)>,
}

impl Analyzer {
    fn new(module_name: Option<String>, config: LuaxConfig, program: Program) -> Self {
        let config_globals = config.global_map();
        let config_syscalls = config.syscall_map();
        Self {
            module_name,
            config,
            program,
            defs: Vec::new(),
            refs: Vec::new(),
            member_accesses: Vec::new(),
            diagnostics: Vec::new(),
            aliases: Vec::new(),
            scopes: vec![HashMap::new()],
            next_id: 0,
            config_globals,
            config_syscalls,
            emitted_duplicate: HashSet::new(),
        }
    }

    fn run(&mut self) {
        let file_span = Span {
            start: 0,
            end: self.program.eof,
        };
        if let Some(module) = self.module_name.clone() {
            let id = self.alloc_symbol(
                module.clone(),
                SymbolKindLite::Module,
                Span { start: 0, end: 0 },
                file_span,
                "module".to_string(),
                String::new(),
                None,
            );
            self.scopes[0].insert(module, id);
        }

        for cfg in self.config.globals.clone() {
            self.alloc_symbol(
                cfg.name.clone(),
                SymbolKindLite::ConfigGlobal,
                Span { start: 0, end: 0 },
                file_span,
                if cfg.ty.is_empty() {
                    if cfg.volatile {
                        "config volatile global".to_string()
                    } else {
                        "config global".to_string()
                    }
                } else if cfg.volatile {
                    format!("config volatile global: {}", cfg.ty)
                } else {
                    format!("config global: {}", cfg.ty)
                },
                cfg.doc,
                None,
            );
        }

        for syscall in self.config.syscalls.clone() {
            let detail = if syscall.params.is_empty() {
                format!("syscall #{}", syscall.id)
            } else {
                format!("syscall #{}({})", syscall.id, syscall.params.join(", "))
            };
            self.alloc_symbol(
                syscall.name.clone(),
                SymbolKindLite::Syscall,
                Span { start: 0, end: 0 },
                file_span,
                detail,
                syscall.doc,
                None,
            );
        }

        let stmts = self.program.stmts.clone();
        self.walk_block(&stmts, file_span, None);

        if let Some(entry) = &self.config.entry {
            let found = self.defs.iter().any(|def| def.name == *entry && def.kind == SymbolKindLite::Function);
            if !found {
                self.diagnostics.push(LintDiagnostic {
                    span: file_span,
                    message: format!("entry function '{}' is not defined", entry),
                    severity: DiagnosticSeverityLite::Warning,
                });
            }
        }
    }

    fn build_outline(&self) -> Vec<OutlineItem> {
        let mut roots = Vec::new();
        let mut children: BTreeMap<usize, Vec<OutlineItem>> = BTreeMap::new();
        for def in self.defs.iter().filter(|d| matches!(d.kind, SymbolKindLite::Function | SymbolKindLite::Global | SymbolKindLite::Field | SymbolKindLite::Local)) {
            let item = OutlineItem {
                name: def.name.clone(),
                kind: def.kind,
                span: def.visible,
                selection: def.span,
                children: Vec::new(),
            };
            if let Some(parent) = def.parent {
                children.entry(parent).or_default().push(item);
            } else {
                roots.push(item);
            }
        }
        fn attach(items: &mut [OutlineItem], children: &mut BTreeMap<usize, Vec<OutlineItem>>, defs: &[SymbolDef]) {
            for item in items {
                if let Some(def) = defs.iter().find(|d| d.name == item.name && d.span == item.selection) {
                    if let Some(mut cs) = children.remove(&def.id) {
                        attach(&mut cs, children, defs);
                        item.children = cs;
                    }
                }
            }
        }
        attach(&mut roots, &mut children, &self.defs);
        roots
    }

    fn walk_block(&mut self, stmts: &[Stmt], visible: Span, parent: Option<usize>) {
        self.scopes.push(HashMap::new());
        for stmt in stmts {
            self.walk_stmt(stmt, visible, parent);
        }
        self.scopes.pop();
    }

    fn walk_stmt(&mut self, stmt: &Stmt, block_visible: Span, parent: Option<usize>) {
        match &stmt.kind {
            StmtKind::Local { names, values } => {
                for value in values {
                    self.walk_expr(value, false);
                }
                for name in names {
                    let id = self.define_in_current_scope(
                        name,
                        SymbolKindLite::Local,
                        Span {
                            start: name.span.start,
                            end: block_visible.end,
                        },
                        "local".to_string(),
                        String::new(),
                        parent,
                    );
                    if let Some(expr) = values.get(names.iter().position(|n| n.span == name.span).unwrap_or(0)) {
                        self.register_value_shape(id, expr, block_visible.end);
                        self.capture_require_alias(name, expr, block_visible.end);
                    }
                }
            }
            StmtKind::Global { entries } => {
                for GlobalEntry {
                    name,
                    is_volatile,
                    value,
                    ..
                } in entries
                {
                    if let Some(v) = value {
                        self.walk_expr(v, false);
                    }
                    let detail = if *is_volatile { "volatile global" } else { "global" };
                    let id = self.define_global(
                        name,
                        SymbolKindLite::Global,
                        Span {
                            start: name.span.start,
                            end: self.program.eof,
                        },
                        detail.to_string(),
                        String::new(),
                        parent,
                    );
                    if let Some(v) = value {
                        self.register_value_shape(id, v, self.program.eof);
                    }
                }
            }
            StmtKind::Assign { targets, values } => {
                for value in values {
                    self.walk_expr(value, false);
                }
                for (idx, target) in targets.iter().enumerate() {
                    self.walk_expr(target, true);
                    if let Some(value) = values.get(idx) {
                        self.register_assignment_shape(target, value, block_visible.end);
                    }
                }
            }
            StmtKind::FunctionDecl {
                name,
                params,
                body,
                is_local,
            } => {
                let function_id = self.define_function_name(name, *is_local, stmt.span, block_visible, parent);
                self.scopes.push(HashMap::new());
                for param in params {
                    self.define_in_current_scope(
                        param,
                        SymbolKindLite::Parameter,
                        Span {
                            start: param.span.start,
                            end: stmt.span.end,
                        },
                        "parameter".to_string(),
                        String::new(),
                        Some(function_id),
                    );
                }
                for inner in body {
                    self.walk_stmt(inner, Span { start: stmt.span.start, end: stmt.span.end }, Some(function_id));
                }
                self.scopes.pop();
            }
            StmtKind::If { arms, else_body } => {
                for (cond, body, _) in arms {
                    self.walk_expr(cond, false);
                    self.walk_block(body, Span { start: stmt.span.start, end: stmt.span.end }, parent);
                }
                if let Some(body) = else_body {
                    self.walk_block(body, Span { start: stmt.span.start, end: stmt.span.end }, parent);
                }
            }
            StmtKind::While { cond, body } => {
                self.walk_expr(cond, false);
                self.walk_block(body, Span { start: stmt.span.start, end: stmt.span.end }, parent);
            }
            StmtKind::Repeat { body, cond } => {
                self.walk_block(body, Span { start: stmt.span.start, end: stmt.span.end }, parent);
                self.walk_expr(cond, false);
            }
            StmtKind::NumericFor {
                var,
                start,
                end,
                step,
                body,
            } => {
                self.walk_expr(start, false);
                self.walk_expr(end, false);
                if let Some(step) = step {
                    self.walk_expr(step, false);
                }
                self.scopes.push(HashMap::new());
                self.define_in_current_scope(
                    var,
                    SymbolKindLite::Local,
                    Span {
                        start: var.span.start,
                        end: stmt.span.end,
                    },
                    "for variable".to_string(),
                    String::new(),
                    parent,
                );
                for inner in body {
                    self.walk_stmt(inner, Span { start: stmt.span.start, end: stmt.span.end }, parent);
                }
                self.scopes.pop();
            }
            StmtKind::Return { values } => {
                for value in values {
                    self.walk_expr(value, false);
                }
            }
            StmtKind::Break => {}
            StmtKind::Expr(expr) => {
                self.walk_expr(expr, false);
            }
        }
    }

    fn walk_expr(&mut self, expr: &Expr, is_write: bool) {
        match &expr.kind {
            ExprKind::Name(name) => {
                let def_id = self.resolve_name(&name.text, expr.span.start);
                if def_id.is_none() && !self.is_builtin(&name.text) {
                    self.diagnostics.push(LintDiagnostic {
                        span: name.span,
                        message: format!("unresolved symbol '{}'", name.text),
                        severity: DiagnosticSeverityLite::Warning,
                    });
                }
                self.refs.push(SymbolRef {
                    name: name.text.clone(),
                    span: name.span,
                    def_id,
                    is_write,
                });
            }
            ExprKind::Table(fields) => {
                for field in fields {
                    self.walk_expr(&field.value, false);
                }
            }
            ExprKind::Unary { expr, .. } => self.walk_expr(expr, false),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.walk_expr(lhs, false);
                self.walk_expr(rhs, false);
            }
            ExprKind::Member { base, name } => {
                self.walk_expr(base, false);
                let owner_def = self.resolve_owner(base, expr.span.start);
                let owner_module = self.resolve_owner_module(base, expr.span.start);
                let def_id = if let Some(owner) = owner_def {
                    self.find_child(owner, &name.text)
                } else if let Some(module_name) = owner_module.as_ref() {
                    self.find_module_child(module_name, &name.text)
                } else {
                    None
                };
                self.member_accesses.push(MemberAccess {
                    owner_def,
                    owner_module,
                    name: name.text.clone(),
                    span: name.span,
                    def_id,
                });
            }
            ExprKind::Index { base, index } => {
                self.walk_expr(base, false);
                self.walk_expr(index, false);
            }
            ExprKind::Call { callee, args } => {
                self.walk_expr(callee, false);
                for arg in args {
                    self.walk_expr(arg, false);
                }
            }
            ExprKind::Function { params, body } => {
                self.scopes.push(HashMap::new());
                for param in params {
                    self.define_in_current_scope(
                        param,
                        SymbolKindLite::Parameter,
                        Span {
                            start: param.span.start,
                            end: expr.span.end,
                        },
                        "parameter".to_string(),
                        String::new(),
                        None,
                    );
                }
                for inner in body {
                    self.walk_stmt(inner, expr.span, None);
                }
                self.scopes.pop();
            }
            ExprKind::Nil | ExprKind::Bool(_) | ExprKind::Number(_) | ExprKind::String(_) => {}
        }
    }

    fn define_function_name(
        &mut self,
        name: &FuncName,
        is_local: bool,
        stmt_span: Span,
        visible: Span,
        parent: Option<usize>,
    ) -> usize {
        if name.parts.len() == 1 {
            let part = &name.parts[0];
            if is_local {
                self.define_in_current_scope(
                    part,
                    SymbolKindLite::Function,
                    Span {
                        start: part.span.start,
                        end: visible.end,
                    },
                    "function".to_string(),
                    String::new(),
                    parent,
                )
            } else {
                self.define_global(
                    part,
                    SymbolKindLite::Function,
                    Span {
                        start: part.span.start,
                        end: self.program.eof,
                    },
                    "function".to_string(),
                    String::new(),
                    parent,
                )
            }
        } else {
            let owner_name = &name.parts[0].text;
            let owner_def = self.resolve_name(owner_name, name.parts[0].span.start);
            if owner_def.is_none() {
                self.diagnostics.push(LintDiagnostic {
                    span: name.parts[0].span,
                    message: format!("owner '{}' is not defined for dotted function declaration", owner_name),
                    severity: DiagnosticSeverityLite::Warning,
                });
            }
            let mut current_owner = owner_def;
            for part in &name.parts[1..] {
                let owner = current_owner.unwrap_or_else(|| {
                    self.alloc_symbol(
                        owner_name.clone(),
                        SymbolKindLite::Global,
                        name.parts[0].span,
                        Span { start: 0, end: self.program.eof },
                        "synthetic owner".to_string(),
                        String::new(),
                        None,
                    )
                });
                let existing = self.find_child(owner, &part.text);
                current_owner = Some(existing.unwrap_or_else(|| {
                    self.alloc_child_symbol(
                        owner,
                        part.clone(),
                        SymbolKindLite::Function,
                        Span {
                            start: part.span.start,
                            end: stmt_span.end,
                        },
                        "field function".to_string(),
                        String::new(),
                    )
                }));
            }
            current_owner.unwrap()
        }
    }

    fn define_global(
        &mut self,
        name: &Name,
        kind: SymbolKindLite,
        visible: Span,
        detail: String,
        documentation: String,
        parent: Option<usize>,
    ) -> usize {
        let existing = self.scopes[0].get(&name.text).copied();
        if let Some(existing) = existing {
            self.emit_duplicate(existing, name, "global");
            return existing;
        }
        let id = self.alloc_symbol(name.text.clone(), kind, name.span, visible, detail, documentation, parent);
        self.scopes[0].insert(name.text.clone(), id);
        id
    }

    fn define_in_current_scope(
        &mut self,
        name: &Name,
        kind: SymbolKindLite,
        visible: Span,
        detail: String,
        documentation: String,
        parent: Option<usize>,
    ) -> usize {
        let existing = self.scopes.last().and_then(|scope| scope.get(&name.text).copied());
        if let Some(existing) = existing {
            self.emit_duplicate(existing, name, "symbol");
            return existing;
        }
        let id = self.alloc_symbol(name.text.clone(), kind, name.span, visible, detail, documentation, parent);
        self.scopes.last_mut().unwrap().insert(name.text.clone(), id);
        id
    }

    fn alloc_symbol(
        &mut self,
        name: String,
        kind: SymbolKindLite,
        span: Span,
        visible: Span,
        detail: String,
        documentation: String,
        parent: Option<usize>,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.defs.push(SymbolDef {
            id,
            name,
            kind,
            span,
            visible,
            detail,
            documentation,
            parent,
            children: Vec::new(),
        });
        if let Some(parent_id) = parent {
            if let Some(def) = self.defs.iter_mut().find(|d| d.id == parent_id) {
                def.children.push(id);
            }
        }
        id
    }

    fn alloc_child_symbol(
        &mut self,
        owner: usize,
        name: Name,
        kind: SymbolKindLite,
        visible: Span,
        detail: String,
        documentation: String,
    ) -> usize {
        if let Some(existing) = self.find_child(owner, &name.text) {
            return existing;
        }
        self.alloc_symbol(name.text, kind, name.span, visible, detail, documentation, Some(owner))
    }

    fn emit_duplicate(&mut self, existing: usize, name: &Name, label: &str) {
        if self.emitted_duplicate.insert((name.text.clone(), name.span.start)) {
            self.diagnostics.push(LintDiagnostic {
                span: name.span,
                message: format!("duplicate {} '{}'", label, name.text),
                severity: DiagnosticSeverityLite::Warning,
            });
        }
        let _ = existing;
    }

    fn resolve_name(&self, name: &str, offset: usize) -> Option<usize> {
        for scope in self.scopes.iter().rev() {
            if let Some(id) = scope.get(name).copied() {
                if let Some(def) = self.defs.iter().find(|d| d.id == id) {
                    if contains(def.visible, offset) {
                        return Some(id);
                    }
                }
            }
        }
        self.defs
            .iter()
            .find(|def| def.name == name && contains(def.visible, offset))
            .map(|def| def.id)
    }

    fn resolve_owner(&self, expr: &Expr, offset: usize) -> Option<usize> {
        match &expr.kind {
            ExprKind::Name(name) => self.resolve_name(&name.text, offset),
            ExprKind::Member { base, name } => {
                let owner = self.resolve_owner(base, offset)?;
                self.find_child(owner, &name.text)
            }
            _ => None,
        }
    }

    fn resolve_owner_module(&self, expr: &Expr, offset: usize) -> Option<String> {
        match &expr.kind {
            ExprKind::Name(name) => self
                .aliases
                .iter()
                .find(|a| a.name == name.text && contains(a.visible, offset))
                .map(|a| a.module_name.clone()),
            ExprKind::Member { base, name } => {
                let base_module = self.resolve_owner_module(base, offset)?;
                Some(format!("{}.{}", base_module, name.text))
            }
            _ => None,
        }
    }

    fn find_child(&self, owner: usize, name: &str) -> Option<usize> {
        let owner_def = self.defs.iter().find(|d| d.id == owner)?;
        owner_def
            .children
            .iter()
            .filter_map(|id| self.defs.iter().find(|d| d.id == *id))
            .find(|child| child.name == name)
            .map(|child| child.id)
    }

    fn find_module_child(&self, _module: &str, _name: &str) -> Option<usize> {
        None
    }

    fn register_assignment_shape(&mut self, target: &Expr, value: &Expr, visible_end: usize) {
        match &target.kind {
            ExprKind::Name(name) => {
                if let Some(id) = self.resolve_name(&name.text, target.span.start) {
                    self.register_value_shape(id, value, visible_end);
                }
            }
            ExprKind::Member { base, name } => {
                if let Some(owner) = self.resolve_owner(base, target.span.start) {
                    let kind = match value.kind {
                        ExprKind::Function { .. } => SymbolKindLite::Function,
                        _ => SymbolKindLite::Field,
                    };
                    let field_id = self.alloc_child_symbol(
                        owner,
                        name.clone(),
                        kind,
                        Span {
                            start: name.span.start,
                            end: visible_end,
                        },
                        if kind == SymbolKindLite::Function {
                            "field function".to_string()
                        } else {
                            "field".to_string()
                        },
                        String::new(),
                    );
                    self.register_value_shape(field_id, value, visible_end);
                }
            }
            _ => {}
        }
    }

    fn register_value_shape(&mut self, owner: usize, value: &Expr, visible_end: usize) {
        match &value.kind {
            ExprKind::Table(fields) => self.register_table_fields(owner, fields, visible_end),
            ExprKind::Function { params, body } => {
                self.scopes.push(HashMap::new());
                for param in params {
                    self.define_in_current_scope(
                        param,
                        SymbolKindLite::Parameter,
                        Span {
                            start: param.span.start,
                            end: value.span.end,
                        },
                        "parameter".to_string(),
                        String::new(),
                        Some(owner),
                    );
                }
                for stmt in body {
                    self.walk_stmt(stmt, value.span, Some(owner));
                }
                self.scopes.pop();
            }
            _ => {}
        }
    }

    fn register_table_fields(&mut self, owner: usize, fields: &[TableField], visible_end: usize) {
        for field in fields {
            if let Some(key) = &field.key {
                let kind = match field.value.kind {
                    ExprKind::Function { .. } => SymbolKindLite::Function,
                    _ => SymbolKindLite::Field,
                };
                let id = self.alloc_child_symbol(
                    owner,
                    key.clone(),
                    kind,
                    Span {
                        start: key.span.start,
                        end: visible_end,
                    },
                    if kind == SymbolKindLite::Function {
                        "table function".to_string()
                    } else {
                        "table field".to_string()
                    },
                    String::new(),
                );
                self.register_value_shape(id, &field.value, visible_end);
            } else {
                self.walk_expr(&field.value, false);
            }
        }
    }

    fn capture_require_alias(&mut self, name: &Name, expr: &Expr, visible_end: usize) {
        if let ExprKind::Call { callee, args } = &expr.kind {
            if let ExprKind::Name(callee_name) = &callee.kind {
                if callee_name.text == "require" {
                    if let Some(first) = args.first() {
                        if let ExprKind::String(s) = &first.kind {
                            let module_name = s.trim_matches('"').trim_matches('\'').to_string();
                            self.aliases.push(AliasModule {
                                name: name.text.clone(),
                                module_name,
                                visible: Span {
                                    start: name.span.start,
                                    end: visible_end,
                                },
                            });
                        }
                    }
                }
            }
        }
    }

    fn is_builtin(&self, name: &str) -> bool {
        matches!(
            name,
            "require" | "assert" | "type" | "tostring" | "tonumber" | "pairs" | "ipairs" | "print"
        ) || self.config_globals.contains_key(name)
            || self.config_syscalls.contains_key(name)
    }
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}
