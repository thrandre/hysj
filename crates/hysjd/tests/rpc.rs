use hysj_core::{Changes, ChannelTransport, Client, ClientError, RawHeaders, Store, Transport};
use hysj_proto::{Bucket, ListParams, Message, MessageSummary, RpcError, ShowParams, method};
use serde_json::json;

fn start() -> ChannelTransport {
    let (client, daemon) = ChannelTransport::pair();
    let mut store = Store::open_in_memory().unwrap();
    for name in ["work", "personal"] {
        store
            .add_account(&hysj_proto::Account {
                name: name.into(),
                address: format!("me@{name}.example"),
                host: "imap.example".into(),
                port: 993,
                username: "me".into(),
                smtp_host: "smtp.example".into(),
                smtp_port: 465,
                label: None,
            })
            .unwrap();
        let headers = format!("From: Someone <s@{name}.example>\r\nSubject: Hi from {name}\r\n");
        let new = vec![RawHeaders {
            remote_id: "1".into(),
            headers: headers.into_bytes(),
            ..Default::default()
        }];
        let changes = Changes {
            cursor: "1".into(),
            new,
            ..Default::default()
        };
        store.apply(name, &changes).unwrap();
    }
    // Bodies are normally fetched lazily from the server; seed one so `show` needs no network.
    store.set_body("1", "hello").unwrap();
    tokio::spawn(hysjd::serve(
        daemon,
        hysjd::Hub::new(hysjd::Daemon::new(store)),
    ));
    client
}

#[tokio::test]
async fn handshake_list_show() {
    let (mut c, hello) = Client::connect(start()).await.unwrap();
    assert_eq!(hello.version, hysj_proto::PROTOCOL_VERSION);

    let people: Vec<MessageSummary> = c
        .call(
            method::LIST,
            ListParams {
                bucket: Bucket::Screener,
                account: None,
            },
        )
        .await
        .unwrap();
    assert!(!people.is_empty());
    assert!(people.iter().all(|m| m.bucket == Bucket::Screener));

    let work: Vec<MessageSummary> = c
        .call(
            method::LIST,
            ListParams {
                bucket: Bucket::Screener,
                account: Some("work".into()),
            },
        )
        .await
        .unwrap();
    assert!(!work.is_empty() && work.len() < people.len());
    assert!(work.iter().all(|m| m.account == "work"));

    let msg: Message = c
        .call(
            method::SHOW,
            ShowParams {
                id: people[0].id.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(msg.summary, people[0]);

    let missing = c
        .call::<Message>(method::SHOW, ShowParams { id: "nope".into() })
        .await;
    assert!(matches!(missing, Err(ClientError::Rpc(e)) if e.code == RpcError::NOT_FOUND));
}

#[tokio::test]
async fn rejects_wrong_version_and_calls_before_hello() {
    let mut t = start();
    let mut call = async |body: serde_json::Value| {
        t.send(body.to_string()).await.unwrap();
        serde_json::from_str::<serde_json::Value>(&t.recv().await.unwrap()).unwrap()
    };

    let r =
        call(json!({"jsonrpc":"2.0","id":1,"method":"list","params":{"bucket":"people"}})).await;
    assert_eq!(r["error"]["code"], RpcError::HELLO_REQUIRED);

    let r = call(json!({"jsonrpc":"2.0","id":2,"method":"hello","params":{"version":999}})).await;
    assert_eq!(r["error"]["code"], RpcError::VERSION_MISMATCH);

    let r = call(json!("not a request")).await;
    assert_eq!(r["error"]["code"], RpcError::PARSE);
}
