pub trait Variant {
    fn display(&self) -> String;
}

pub enum ConstVariant {
    Nil,
    True,
    Int(i32),
    Float(f32),
    String(String),
}

impl ConstVariant {
    pub fn is_nil(&self) -> bool {
        matches!(self, ConstVariant::Nil)
    }

    pub fn is_true(&self) -> bool {
        matches!(self, ConstVariant::True)
    }

    pub fn is_int(&self) -> bool {
        matches!(self, ConstVariant::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, ConstVariant::Float(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, ConstVariant::String(_))
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            ConstVariant::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            ConstVariant::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            ConstVariant::String(v) => Some(v),
            _ => None,
        }
    }
}

impl Variant for ConstVariant {
    fn display(&self) -> String {
        match self {
            ConstVariant::Nil => "nil".to_string(),
            ConstVariant::True => "true".to_string(),
            ConstVariant::Int(v) => v.to_string(),
            ConstVariant::Float(v) => v.to_string(),
            ConstVariant::String(v) => v.clone(),
        }
    }
}

pub struct FnArgVarint {
    pub id: u32,
}

impl FnArgVarint {
    pub fn new(id: u32) -> Self {
        Self { id }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }
}

impl Variant for FnArgVarint {
    fn display(&self) -> String {
        format!("arg_{}", self.id)
    }
}

pub struct NormalVarint {
    pub id: u32,
}

impl NormalVarint {
    pub fn new(id: u32) -> Self {
        Self { id }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }
}

impl Variant for NormalVarint {
    fn display(&self) -> String {
        format!("var_{}", self.id)
    }
}

pub struct GlobalVarint {
    pub id: u32,
}

impl GlobalVarint {
    pub fn new(id: u32) -> Self {
        Self { id }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }
}

impl Variant for GlobalVarint {
    fn display(&self) -> String {
        format!("global.{}", self.id)
    }
}

pub enum Expression {
    Const(ConstVariant),
    FnArg(FnArgVarint),
    NormalVar(NormalVarint),
    GlobalVar(GlobalVarint),
    DirectAccess(Box<Expression>, Box<Expression>),
    Neg(Box<Expression>),
    Add(Box<Expression>, Box<Expression>),
    Sub(Box<Expression>, Box<Expression>),
    Mul(Box<Expression>, Box<Expression>),
    Div(Box<Expression>, Box<Expression>),
    Mod(Box<Expression>, Box<Expression>),
    BitTest(Box<Expression>, Box<Expression>),
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    SetE(Box<Expression>, Box<Expression>),
    SetNE(Box<Expression>, Box<Expression>),
    SetG(Box<Expression>, Box<Expression>),
    SetLE(Box<Expression>, Box<Expression>),
    SetL(Box<Expression>, Box<Expression>),
    SetGE(Box<Expression>, Box<Expression>),
}

impl Expression {
    pub fn init_int(v: i32) -> Self {
        Expression::Const(ConstVariant::Int(v))
    }

    pub fn init_float(v: f32) -> Self {
        Expression::Const(ConstVariant::Float(v))
    }

    pub fn init_string(v: String) -> Self {
        Expression::Const(ConstVariant::String(v))
    }

    pub fn init_nil() -> Self {
        Expression::Const(ConstVariant::Nil)
    }

    pub fn init_true() -> Self {
        Expression::Const(ConstVariant::True)
    }

    pub fn init_fn_arg(id: u32) -> Self {
        Expression::FnArg(FnArgVarint::new(id))
    }

    pub fn init_normal_var(id: u32) -> Self {
        Expression::NormalVar(NormalVarint::new(id))
    }

    pub fn init_global_var(id: u32) -> Self {
        Expression::GlobalVar(GlobalVarint::new(id))
    }

    /// access with 'var[varb]'
    pub fn init_direct_access(lhs: Expression, rhs: Expression) -> Self {
        Expression::DirectAccess(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_neg(expr: Expression) -> Self {
        Expression::Neg(Box::new(expr))
    }

    pub fn init_add(lhs: Expression, rhs: Expression) -> Self {
        Expression::Add(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_sub(lhs: Expression, rhs: Expression) -> Self {
        Expression::Sub(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_mul(lhs: Expression, rhs: Expression) -> Self {
        Expression::Mul(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_div(lhs: Expression, rhs: Expression) -> Self {
        Expression::Div(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_mod(lhs: Expression, rhs: Expression) -> Self {
        Expression::Mod(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_bit_test(lhs: Expression, rhs: Expression) -> Self {
        Expression::BitTest(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_and(lhs: Expression, rhs: Expression) -> Self {
        Expression::And(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_or(lhs: Expression, rhs: Expression) -> Self {
        Expression::Or(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_set_e(lhs: Expression, rhs: Expression) -> Self {
        Expression::SetE(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_set_ne(lhs: Expression, rhs: Expression) -> Self {
        Expression::SetNE(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_set_g(lhs: Expression, rhs: Expression) -> Self {
        Expression::SetG(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_set_le(lhs: Expression, rhs: Expression) -> Self {
        Expression::SetLE(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_set_l(lhs: Expression, rhs: Expression) -> Self {
        Expression::SetL(Box::new(lhs), Box::new(rhs))
    }

    pub fn init_set_ge(lhs: Expression, rhs: Expression) -> Self {
        Expression::SetGE(Box::new(lhs), Box::new(rhs))
    }

    pub fn is_number(&self) -> bool {
        matches!(
            self,
            Expression::Const(ConstVariant::Int(_)) | Expression::Const(ConstVariant::Float(_))
        )
    }

    // simplify expression for optimization
    // avoid const expression calculation for int and float
    pub fn simplify(&mut self) {
        // eliminate const expression calculation from the leaf node
        match self {
            Expression::Add(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
                if let Expression::Const(ConstVariant::Int(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Int(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Int(l + r));
                    }
                } else if let Expression::Const(ConstVariant::Float(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Float(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(l + r));
                    }
                }
            }
            Expression::Sub(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
                if let Expression::Const(ConstVariant::Int(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Int(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Int(l - r));
                    }
                } else if let Expression::Const(ConstVariant::Float(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Float(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(l - r));
                    }
                } else if let Expression::Const(ConstVariant::Float(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Int(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(l - *r as f32));
                    }
                } else if let Expression::Const(ConstVariant::Int(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Float(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(*l as f32 - r));
                    }
                }
            }
            Expression::Mul(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
                if let Expression::Const(ConstVariant::Int(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Int(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Int(l * r));
                    }
                } else if let Expression::Const(ConstVariant::Float(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Float(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(l * r));
                    }
                } else if let Expression::Const(ConstVariant::Float(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Int(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(l * *r as f32));
                    }
                } else if let Expression::Const(ConstVariant::Int(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Float(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(*l as f32 * r));
                    }
                }
            }
            Expression::Div(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
                if let Expression::Const(ConstVariant::Int(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Int(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Int(l / r));
                    }
                } else if let Expression::Const(ConstVariant::Float(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Float(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(l / r));
                    }
                } else if let Expression::Const(ConstVariant::Float(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Int(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(l / *r as f32));
                    }
                } else if let Expression::Const(ConstVariant::Int(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Float(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Float(*l as f32 / r));
                    }
                }
            }
            Expression::Mod(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
                if let Expression::Const(ConstVariant::Int(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Int(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Int(l % r));
                    }
                }
            }
            Expression::Neg(expr) => {
                expr.simplify();
                if let Expression::Const(ConstVariant::Int(v)) = expr.as_ref() {
                    *self = Expression::Const(ConstVariant::Int(-v));
                } else if let Expression::Const(ConstVariant::Float(v)) = expr.as_ref() {
                    *self = Expression::Const(ConstVariant::Float(-v));
                }
            }
            Expression::BitTest(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
                if let Expression::Const(ConstVariant::Int(l)) = lhs.as_ref() {
                    if let Expression::Const(ConstVariant::Int(r)) = rhs.as_ref() {
                        *self = Expression::Const(ConstVariant::Int(l & r));
                    }
                }
            }
            Expression::And(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();

                match (lhs.as_ref(), rhs.as_ref()) {
                    (Expression::Const(ConstVariant::Nil), Expression::Const(ConstVariant::Nil)) => {
                        *self = Expression::Const(ConstVariant::Nil);
                    }
                    (Expression::Const(ConstVariant::Nil), Expression::Const(_)) => {
                        *self = Expression::Const(ConstVariant::Nil);
                    }
                    (Expression::Const(_), Expression::Const(ConstVariant::Nil)) => {
                        *self = Expression::Const(ConstVariant::Nil);
                    }
                    (Expression::Const(_), Expression::Const(_)) => {
                        *self = Expression::Const(ConstVariant::True);
                    }
                    _ => {}
                }
            }
            Expression::Or(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();

                match (lhs.as_ref(), rhs.as_ref()) {
                    (Expression::Const(ConstVariant::Nil), Expression::Const(ConstVariant::Nil)) => {
                        *self = Expression::Const(ConstVariant::Nil);
                    }
                    (Expression::Const(ConstVariant::Nil), Expression::Const(_)) => {
                        *self = Expression::Const(ConstVariant::True);
                    }
                    (Expression::Const(_), Expression::Const(ConstVariant::Nil)) => {
                        *self = Expression::Const(ConstVariant::True);
                    }
                    (Expression::Const(_), Expression::Const(_)) => {
                        *self = Expression::Const(ConstVariant::True);
                    }
                    _ => {}
                }
            }
            Expression::SetE(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();

                match (lhs.as_ref(), rhs.as_ref()) {
                    (Expression::Const(ConstVariant::Int(l)), Expression::Const(ConstVariant::Int(r)) ) => {
                        if l == r {
                            *self = Expression::Const(ConstVariant::True);
                        }
                    }
                    (Expression::Const(ConstVariant::Float(l)), Expression::Const(ConstVariant::Float(r)) ) => {
                        if l == r {
                            *self = Expression::Const(ConstVariant::True);
                        }
                    }
                    _ => {}
                }
            }
            Expression::SetNE(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
            }
            Expression::SetG(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
            }
            Expression::SetLE(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
            }
            Expression::SetL(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
            }
            Expression::SetGE(lhs, rhs) => {
                lhs.simplify();
                rhs.simplify();
            }
            _ => {}
        }
    }
}
