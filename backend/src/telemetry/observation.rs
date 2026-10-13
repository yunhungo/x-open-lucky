// @created 2026-10-13
// @description 保留请求关联上下文，仅在明细获准采集时构造和清洗日志字段。
// @author yunhungo
use super::{Access, Telemetry};
use axum::http::Method;
use std::net::IpAddr;

#[derive(Clone)]
pub struct Observation {
    pub time: i64,
    pub request_id: String,
    pub service_id: String,
    pub host: String,
    pub listener: String,
    pub client_ip: IpAddr,
    pub method: Method,
    pub path: String,
    pub status: u16,
    pub duration_ms: f64,
    pub error: String,
    pub phase: &'static str,
    pub tls: bool,
}
impl Observation {
    pub fn record(&self, telemetry: &Telemetry) {
        telemetry.collect(
            super::Summary {
                time: self.time,
                service: &self.service_id,
                status: self.status,
                duration_ms: self.duration_ms,
                stream: self.phase != "request",
                failed: self.status >= 400 || !self.error.is_empty(),
            },
            || Access {
                time: self.time,
                request_id: self.request_id.clone(),
                service_id: self.service_id.clone(),
                host: self.host.clone(),
                listener: self.listener.clone(),
                client_ip: self.client_ip.to_string(),
                method: self.method.to_string(),
                path: self.path.clone(),
                status: self.status,
                duration_ms: self.duration_ms,
                error: self.error.clone(),
                phase: self.phase.into(),
                tls: self.tls,
            },
        );
    }
}
