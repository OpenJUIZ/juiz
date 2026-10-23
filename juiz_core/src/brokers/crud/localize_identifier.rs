/// 自分自身のブローカーを指す識別子 ({broker_type}://<host>:<port>/...) を core://core/... に書き換える。
///
/// process/list などは core のオブジェクトを {broker_type}://<アクセスされた住所>/... の形で返すので、
/// それをそのまま同じサーバーに渡されたときは、自分宛てに再リクエストせず
/// ローカルのオブジェクトとして扱う。そうしないと処理中のリクエストが持つロックを
/// 2 本目のリクエストが待ち続けてタイムアウトする (HTTP は issue #43、QUIC も同じ)。
pub fn localize_identifier(
    identifier: &str,
    broker_type_name: &str,
    accessed_broker_name: Option<&str>,
) -> String {
    let Some(host) = accessed_broker_name else {
        return identifier.to_owned();
    };
    let Some(rest) = identifier
        .strip_prefix(broker_type_name)
        .and_then(|s| s.strip_prefix("://"))
    else {
        return identifier.to_owned();
    };
    let Some((broker_name, path)) = rest.split_once('/') else {
        return identifier.to_owned();
    };
    if is_same_broker(broker_name, host) {
        format!("core://core/{path}")
    } else {
        identifier.to_owned()
    }
}

/// broker_name (host:port) が、アクセスされた住所 (host[:port]) のサーバー自身を指すか。
/// localhost / 127.0.0.1 / 0.0.0.0 / ::1 はポートが同じなら同一とみなす。
pub fn is_same_broker(broker_name: &str, host: &str) -> bool {
    if broker_name == host {
        return true;
    }
    fn split_host_port(s: &str) -> (&str, &str) {
        match s.rsplit_once(':') {
            Some((h, p)) if !p.contains(']') => (h, p),
            _ => (s, "80"),
        }
    }
    fn is_loopback(h: &str) -> bool {
        matches!(h, "localhost" | "127.0.0.1" | "0.0.0.0" | "[::1]" | "[::]")
    }
    let (b_host, b_port) = split_host_port(broker_name);
    let (h_host, h_port) = split_host_port(host);
    b_port == h_port && (b_host == h_host || (is_loopback(b_host) && is_loopback(h_host)))
}

#[cfg(test)]
mod tests {
    use super::localize_identifier;

    #[test]
    fn localize_identifier_rewrites_own_http_identifier() {
        let id = "http://127.0.0.1:8000/process/inc0::inc";
        assert_eq!(
            localize_identifier(id, "http", Some("127.0.0.1:8000")),
            "core://core/process/inc0::inc"
        );
        assert_eq!(
            localize_identifier(id, "http", Some("localhost:8000")),
            "core://core/process/inc0::inc"
        );
        assert_eq!(
            localize_identifier(
                "http://192.168.0.10:8000/container_process/get0::get:counter0",
                "http",
                Some("192.168.0.10:8000")
            ),
            "core://core/container_process/get0::get:counter0"
        );
    }

    #[test]
    fn localize_identifier_rewrites_own_qmp_identifier() {
        assert_eq!(
            localize_identifier(
                "qmp://127.0.0.1:5000/process/inc0::inc",
                "qmp",
                Some("localhost:5000")
            ),
            "core://core/process/inc0::inc"
        );
        // ブローカーの種類が違えば同じ住所でも書き換えない
        let http = "http://127.0.0.1:5000/process/inc0::inc";
        assert_eq!(
            localize_identifier(http, "qmp", Some("127.0.0.1:5000")),
            http
        );
    }

    #[test]
    fn localize_identifier_keeps_namespace() {
        // ネームスペース付きの識別子でも住所の部分だけを見て書き換える (issue #150)
        assert_eq!(
            localize_identifier(
                "http://127.0.0.1:8000/process/robot1/arm/inc0::inc",
                "http",
                Some("localhost:8000")
            ),
            "core://core/process/robot1/arm/inc0::inc"
        );
        assert_eq!(
            localize_identifier(
                "qmp://127.0.0.1:5000/container_process/robot1/get0::get:counter0",
                "qmp",
                Some("127.0.0.1:5000")
            ),
            "core://core/container_process/robot1/get0::get:counter0"
        );
        let other = "http://127.0.0.1:8001/process/robot1/inc0::inc";
        assert_eq!(
            localize_identifier(other, "http", Some("127.0.0.1:8000")),
            other
        );
    }

    #[test]
    fn localize_identifier_keeps_other_identifiers() {
        let other_port = "http://127.0.0.1:8001/process/inc0::inc";
        assert_eq!(
            localize_identifier(other_port, "http", Some("127.0.0.1:8000")),
            other_port
        );
        let other_host = "http://192.168.0.11:8000/process/inc0::inc";
        assert_eq!(
            localize_identifier(other_host, "http", Some("192.168.0.10:8000")),
            other_host
        );
        let core = "core://core/process/inc0::inc";
        assert_eq!(
            localize_identifier(core, "http", Some("127.0.0.1:8000")),
            core
        );
        let id = "http://127.0.0.1:8000/process/inc0::inc";
        assert_eq!(localize_identifier(id, "http", None), id);
    }
}
