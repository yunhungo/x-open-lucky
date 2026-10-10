// @created 2026-10-11
// @description 用供应商响应夹具验证 DNS 幂等、创建与错误边界。
// @author yunhungo
use crate::models::{Credential, Ddns};
use serde_json::json;
use std::collections::BTreeMap;
use wiremock::{
    matchers::{body_json, header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

fn fixture() -> (reqwest::Client, Credential, Ddns) {
    let credential = Credential {
        id: "credential".into(),
        name: "fixture".into(),
        provider: "cloudflare".into(),
        secrets: BTreeMap::from([
            ("token".into(), "test-token".into()),
            ("key_id".into(), "test-id".into()),
            ("key_secret".into(), "test-key".into()),
        ]),
        has_secret: true,
    };
    let rule = Ddns {
        id: "rule".into(),
        name: "fixture".into(),
        enabled: true,
        credential_id: "credential".into(),
        zone: "example.com".into(),
        domain: "nas.example.com".into(),
        record_type: "A".into(),
        source: "static".into(),
        source_value: "203.0.113.5".into(),
        interval: 300,
        ttl: 600,
        proxied: false,
    };
    (
        reqwest::Client::builder().no_proxy().build().unwrap(),
        credential,
        rule,
    )
}
#[tokio::test]
async fn cloudflare_creates_record_and_uses_auto_ttl_when_proxied() {
    let server = MockServer::start().await;
    let (client, c, mut rule) = fixture();
    rule.proxied = true;
    Mock::given(method("GET"))
        .and(path("/zones"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"success":true,"result":[{"id":"zone"}]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/zones/zone/dns_records"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"success":true,"result":[]})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST")).and(path("/zones/zone/dns_records")).and(body_json(json!({"name":"nas.example.com","type":"A","content":"203.0.113.5","ttl":1,"proxied":true}))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"success":true,"result":{"id":"record"}}))).expect(1).mount(&server).await;
    assert!(
        super::cloudflare::update_at(&client, &c, &rule, "203.0.113.5", &server.uri())
            .await
            .unwrap()
    );
}
#[tokio::test]
async fn cloudflare_does_not_write_unchanged_or_ambiguous_records() {
    let server = MockServer::start().await;
    let (client, c, rule) = fixture();
    Mock::given(path("/zones"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"success":true,"result":[{"id":"zone"}]})),
        )
        .mount(&server)
        .await;
    let record = json!({"id":"record","content":"203.0.113.5","ttl":600,"proxied":false});
    let query = || Mock::given(method("GET")).and(path("/zones/zone/dns_records"));
    query()
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"success":true,"result":[record.clone()]})),
        )
        .mount(&server)
        .await;
    assert!(
        !super::cloudflare::update_at(&client, &c, &rule, "203.0.113.5", &server.uri())
            .await
            .unwrap()
    );
    server.reset().await;
    Mock::given(path("/zones"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"success":true,"result":[{"id":"zone"}]})),
        )
        .mount(&server)
        .await;
    query()
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"success":true,"result":[record.clone(),record]})),
        )
        .mount(&server)
        .await;
    assert!(
        super::cloudflare::update_at(&client, &c, &rule, "203.0.113.5", &server.uri())
            .await
            .is_err()
    );
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method == "GET"));
}
#[tokio::test]
async fn aliyun_creates_signed_exact_domain_record() {
    let server = MockServer::start().await;
    let (client, c, rule) = fixture();
    Mock::given(method("GET"))
        .and(query_param("Action", "DescribeDomainRecords"))
        .and(query_param("SearchMode", "EXACT"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"DomainRecords":{"Record":[]}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(query_param("Action", "AddDomainRecord"))
        .and(query_param("DomainName", "example.com"))
        .and(query_param("RR", "nas"))
        .and(query_param("Value", "203.0.113.5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"RecordId":"record"})))
        .expect(1)
        .mount(&server)
        .await;
    assert!(
        super::alidns::update_at(&client, &c, &rule, "203.0.113.5", &server.uri())
            .await
            .unwrap()
    );
    for r in server.received_requests().await.unwrap() {
        assert!(r
            .url
            .query_pairs()
            .any(|(k, v)| k == "Signature" && !v.is_empty()));
    }
}
#[tokio::test]
async fn dnspod_creates_missing_record_and_rejects_provider_errors() {
    let server = MockServer::start().await;
    let (client, c, rule) = fixture();
    Mock::given(method("POST"))
        .and(header("X-TC-Action", "DescribeRecordList"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"Response":{"Error":{"Code":"ResourceNotFound.NoDataOfRecord"}}}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST")).and(header("X-TC-Action","CreateRecord")).and(body_json(json!({"Domain":"example.com","SubDomain":"nas","RecordType":"A","RecordLine":"默认","Value":"203.0.113.5","TTL":600}))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"Response":{"RecordId":123}}))).expect(1).mount(&server).await;
    assert!(
        super::tencentcloud::update_at(&client, &c, &rule, "203.0.113.5", &server.uri())
            .await
            .unwrap()
    );
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"Response":{"Error":{"Code":"AuthFailure.SignatureFailure"}}}),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(
        super::tencentcloud::update_at(&client, &c, &rule, "203.0.113.5", &server.uri())
            .await
            .is_err()
    );
}
