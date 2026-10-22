use std::time::Duration;

use hysj_core::{Client, Store};
use hysj_proto::{Decision, ScreenerDecide, method};
use hysjd::{Daemon, Hub, socket};

#[tokio::test]
async fn clients_over_the_socket_get_pushes() {
    let dir = std::env::temp_dir().join(format!("hysjd-sock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hysjd.sock");
    let hub = Hub::new(Daemon::new(Store::open_in_memory().unwrap()));
    let serve_path = path.clone();
    let serve_hub = hub.clone();
    tokio::spawn(async move { socket::listen(&serve_path, serve_hub).await });
    let path2 = path.clone();
    let connect = || async {
        for _ in 0..50 {
            if let Ok(t) = socket::connect(&path2).await {
                return t;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("daemon did not listen");
    };

    let first = connect().await; // waits until the daemon listens
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "owner-only socket");
    }
    // A second daemon on the same socket refuses to start.
    let second = Hub::new(Daemon::new(Store::open_in_memory().unwrap()));
    assert!(socket::listen(&path, second).await.is_err());

    let (mut app, _) = Client::connect_as(first, Some("app")).await.unwrap();
    let (mut listener, _) = Client::connect(connect().await).await.unwrap();
    assert!(hub.app_connected());

    let decide = ScreenerDecide {
        sender: "x@y.example".into(),
        decision: Decision::Block,
    };
    let _: serde_json::Value = app.call(method::SCREENER_DECIDE, decide).await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), listener.next_event())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.kind, "mail");

    drop(app);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !hub.app_connected(),
        "app count drops when the window closes"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
