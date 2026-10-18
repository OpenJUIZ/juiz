pub(crate) fn default_conf() -> &'static str {
    return r#"

"name": "test_system"
"plugins":
    "broker_factories": 
        "http_broker":
            "path": "${HOME}/plugins/brokers/"
    "ec_factories":
        "timer_ec":
            "path": "./target/debug"

"brokers":
    - "type_name": "http"
      "name": "localhost_server"
      "host": "0.0.0.0"
      "port": 8000
"#;
}
