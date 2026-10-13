// @created 2026-10-13
// @description 校验 MiniJinja 请求模板的可用上下文与有界表达式子集。
// @author yunhungo
use anyhow::{bail, ensure, Result};
use axum::http::HeaderName;
use minijinja::machinery::{
    ast::{BinOpKind, CallArg, Expr, Stmt, UnaryOpKind},
    parse,
};
use std::collections::HashSet;

pub fn validate(source: &str, response: bool) -> Result<HashSet<HeaderName>> {
    let ast = parse(source, "header", Default::default(), Default::default())
        .map_err(|e| anyhow::anyhow!("模板语法错误（第 {} 行）", e.line().unwrap_or(1)))?;
    let mut policy = Policy {
        remaining: 256,
        headers: HashSet::new(),
        response,
    };
    policy.stmt(&ast, 0)?;
    Ok(policy.headers)
}
struct Policy {
    remaining: usize,
    headers: HashSet<HeaderName>,
    response: bool,
}
fn lookup(expr: &Expr<'_>) -> Option<Vec<String>> {
    match expr {
        Expr::Var(v) => Some(vec![v.id.into()]),
        Expr::GetAttr(v) => {
            let mut path = lookup(&v.expr)?;
            path.push(v.name.into());
            Some(path)
        }
        Expr::GetItem(v) => {
            let Expr::Const(key) = &v.subscript_expr else {
                return None;
            };
            let mut path = lookup(&v.expr)?;
            path.push(key.value.as_str()?.into());
            Some(path)
        }
        _ => None,
    }
}
impl Policy {
    fn step(&mut self, depth: usize) -> Result<()> {
        ensure!(depth <= 24 && self.remaining > 0, "模板表达式过于复杂");
        self.remaining -= 1;
        Ok(())
    }
    fn stmt(&mut self, stmt: &Stmt<'_>, depth: usize) -> Result<()> {
        self.step(depth)?;
        match stmt {
            Stmt::Template(v) => {
                for item in &v.children {
                    self.stmt(item, depth + 1)?;
                }
            }
            Stmt::EmitExpr(v) => self.expr(&v.expr, depth + 1)?,
            Stmt::EmitRaw(_) => {}
            Stmt::IfCond(v) => {
                self.expr(&v.expr, depth + 1)?;
                for item in v.true_body.iter().chain(&v.false_body) {
                    self.stmt(item, depth + 1)?;
                }
            }
            _ => bail!("请求头模板支持变量和条件，不支持循环、赋值或模板导入"),
        }
        Ok(())
    }
    fn expr(&mut self, expr: &Expr<'_>, depth: usize) -> Result<()> {
        self.step(depth)?;
        match expr {
            Expr::Var(_) | Expr::GetAttr(_) | Expr::GetItem(_) => {
                let path = lookup(expr).ok_or_else(|| anyhow::anyhow!("模板字段名称必须为常量"))?;
                let names: Vec<_> = path.iter().map(String::as_str).collect();
                match names.as_slice() {
                    ["request", "host" | "authority" | "scheme" | "method" | "path" | "uri" | "id"]
                    | ["client", "ip"]
                    | ["upstream", "host"] => {}
                    ["response", "status"] if self.response => {}
                    ["request", "headers", name] => {
                        let parsed = HeaderName::from_bytes(name.as_bytes())?;
                        ensure!(*name == parsed.as_str(), "模板中的请求头名称请使用小写");
                        self.headers.insert(parsed);
                    }
                    _ => bail!("模板使用了未知字段或当前阶段不可用的字段"),
                }
            }
            Expr::Const(_) => {}
            Expr::UnaryOp(v) if matches!(v.op, UnaryOpKind::Not) => {
                self.expr(&v.expr, depth + 1)?
            }
            Expr::BinOp(v)
                if matches!(
                    v.op,
                    BinOpKind::ScAnd
                        | BinOpKind::ScOr
                        | BinOpKind::Eq
                        | BinOpKind::Ne
                        | BinOpKind::Lt
                        | BinOpKind::Lte
                        | BinOpKind::Gt
                        | BinOpKind::Gte
                        | BinOpKind::In
                ) =>
            {
                self.expr(&v.left, depth + 1)?;
                self.expr(&v.right, depth + 1)?;
            }
            Expr::Compare(v) => {
                self.expr(&v.expr, depth + 1)?;
                for op in &v.ops {
                    self.expr(&op.expr, depth + 1)?;
                }
            }
            Expr::IfExpr(v) => {
                self.expr(&v.test_expr, depth + 1)?;
                self.expr(&v.true_expr, depth + 1)?;
                if let Some(expr) = &v.false_expr {
                    self.expr(expr, depth + 1)?;
                }
            }
            Expr::Filter(v) => {
                ensure!(
                    ["default", "lower", "upper", "trim", "replace"].contains(&v.name),
                    "模板不支持此过滤器"
                );
                if let Some(expr) = &v.expr {
                    self.expr(expr, depth + 1)?;
                }
                for arg in &v.args {
                    match arg {
                        CallArg::Pos(expr) => self.expr(expr, depth + 1)?,
                        _ => bail!("过滤器仅支持位置参数"),
                    }
                }
            }
            _ => bail!("模板支持字段、比较、条件和字符串过滤器，不支持函数调用或算术扩展"),
        }
        Ok(())
    }
}
