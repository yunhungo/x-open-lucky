// @created 2026-10-13
// @description 预编译并执行有界动态请求头规则，固定值直接复用。
// @author yunhungo
use anyhow::{ensure, Result};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use minijinja::{context, Environment, UndefinedBehavior, Value};
use std::{
    collections::{BTreeMap, HashSet},
    io::{self, Write},
};

enum Operation {
    Remove,
    Static(HeaderValue),
    Template(String),
}
pub struct Rules {
    operations: Vec<(HeaderName, Operation)>,
    environment: Option<Environment<'static>>,
    pub inputs: HashSet<HeaderName>,
}
struct Output(Vec<u8>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len() + bytes.len() > 8192 {
            return Err(io::Error::other("Header size limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn replace(value: &str, from: &str, to: &str) -> Result<String, minijinja::Error> {
    let count = value.matches(from).count();
    let size = value
        .len()
        .saturating_sub(count.saturating_mul(from.len()))
        .saturating_add(count.saturating_mul(to.len()));
    if size > 8192 {
        return Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "Header size limit",
        ));
    }
    Ok(value.replace(from, to))
}
fn environment() -> Environment<'static> {
    let mut env = Environment::empty();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_fuel(Some(2048));
    env.set_recursion_limit(24);
    env.add_filter("default", minijinja::filters::default);
    env.add_filter("lower", minijinja::filters::lower);
    env.add_filter("upper", minijinja::filters::upper);
    env.add_filter("trim", minijinja::filters::trim);
    env.add_filter("replace", replace);
    env
}
impl Rules {
    pub fn compile(values: &BTreeMap<String, Option<String>>, response: bool) -> Result<Self> {
        let mut rules = Self {
            operations: Vec::new(),
            environment: None,
            inputs: HashSet::new(),
        };
        for (name, value) in values {
            let name = HeaderName::from_bytes(name.as_bytes())?;
            let operation = match value {
                None => Operation::Remove,
                Some(value) if value.contains("{{") || value.contains("{%") => {
                    rules
                        .inputs
                        .extend(crate::template_policy::validate(value, response)?);
                    let key = name.as_str().to_owned();
                    rules
                        .environment
                        .get_or_insert_with(environment)
                        .add_template_owned(key.clone(), value.clone())
                        .map_err(|_| anyhow::anyhow!("请求头模板编译失败"))?;
                    Operation::Template(key)
                }
                Some(value) => Operation::Static(HeaderValue::from_str(value)?),
            };
            rules.operations.push((name, operation));
        }
        Ok(rules)
    }
    pub fn dynamic(&self) -> bool {
        self.environment.is_some()
    }
    pub fn apply(&self, headers: &mut HeaderMap, context: &Value) -> Result<()> {
        let mut total = 0;
        for (name, operation) in &self.operations {
            let value = match operation {
                Operation::Remove => {
                    headers.remove(name);
                    continue;
                }
                Operation::Static(value) => value.clone(),
                Operation::Template(key) => {
                    let mut output = Output(Vec::with_capacity(64));
                    let template = self
                        .environment
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("Missing template"))?
                        .get_template(key)?;
                    template
                        .render_captured_to(context, &mut output)
                        .map_err(|_| anyhow::anyhow!("动态请求头求值失败，请检查变量及默认值"))?;
                    HeaderValue::from_bytes(&output.0)
                        .map_err(|_| anyhow::anyhow!("动态请求头包含无效字符"))?
                }
            };
            total += value.len();
            ensure!(total <= 32768, "动态请求头超过大小限制");
            headers.insert(name.clone(), value);
        }
        Ok(())
    }
}
pub struct RequestContext<'a> {
    pub request: &'a axum::extract::Request,
    pub peer: std::net::SocketAddr,
    pub authority: &'a axum::http::uri::Authority,
    pub tls: bool,
    pub upstream: &'a str,
    pub id: &'a str,
}
pub fn context(input: RequestContext<'_>, request_rules: &Rules, response_rules: &Rules) -> Value {
    if !request_rules.dynamic() && !response_rules.dynamic() {
        return Value::UNDEFINED;
    }
    let headers = Value::from_iter(
        request_rules
            .inputs
            .union(&response_rules.inputs)
            .filter_map(|name| {
                input
                    .request
                    .headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .map(|value| (name.as_str(), value))
            }),
    );
    context! {
        request => context! {
            host => input.authority.host(), authority => input.authority.as_str(),
            scheme => if input.tls { "https" } else { "http" }, method => input.request.method().as_str(),
            path => input.request.uri().path(), uri => input.request.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/"),
            id => input.id, headers => headers,
        },
        client => context! { ip => input.peer.ip().to_string() },
        upstream => context! { host => input.upstream },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compile(value: &str, response: bool) -> Result<Rules> {
        Rules::compile(
            &BTreeMap::from([("x-test".into(), Some(value.into()))]),
            response,
        )
    }
    #[test]
    fn templates_support_request_defaults_conditions_and_response() {
        let rules = compile("{{ request.host | upper }}-{{ request.headers['x-absent'] | default('fallback') }}-{{ 'ok' if response.status == 200 else 'fail' }}", true).unwrap();
        let context = context! { request => context! {host => "app.test", headers => context!{}}, response => context! {status => 200} };
        let mut headers = HeaderMap::new();
        rules.apply(&mut headers, &context).unwrap();
        assert_eq!(headers["x-test"], "APP.TEST-fallback-ok");
        assert!(!compile("plain", false).unwrap().dynamic());
        assert!(compile("{{ response.status }}", false).is_err());
    }
    #[test]
    fn unbounded_or_unknown_templates_are_rejected_before_compilation() {
        for source in [
            "{{ request.typo }}",
            "{{ 'x' * 100000000 }}",
            "{% for x in request.headers %}x{% endfor %}",
            "{{ range(100000) }}",
            "{{ request.host | indent(100000) }}",
            "{{ request.headers[request.host] }}",
            "{{ request.host + request.host }}",
        ] {
            assert!(compile(source, false).is_err(), "{source}");
        }
    }
    #[test]
    fn runtime_headers_cannot_inject_or_expand_without_bound() {
        let mut headers = HeaderMap::new();
        let rules = compile("{{ request.host }}", false).unwrap();
        assert!(rules
            .apply(
                &mut headers,
                &context! { request => context! {host => "bad\r\nInjected: yes"} }
            )
            .is_err());
        assert!(rules
            .apply(
                &mut headers,
                &context! { request => context! {host => "x".repeat(8193)} }
            )
            .is_err());
        let rules = compile("{{ request.host | replace('x', 'yyyy') }}", false).unwrap();
        assert!(rules
            .apply(
                &mut headers,
                &context! { request => context! {host => "x".repeat(8192)} }
            )
            .is_err());
    }
}
