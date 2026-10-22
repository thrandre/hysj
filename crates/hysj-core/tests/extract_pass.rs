use hysj_core::{BackendError, Changes, MailBackend, Outgoing, RawHeaders, Store, sync};
use hysj_proto::{Account, Bucket};

/// Serves fixed raw bodies; counts fetches.
struct Server {
    bodies: Vec<(String, String)>,
    fetched: usize,
}

impl MailBackend for Server {
    async fn search(&mut self, _: &str, _: usize) -> Result<Vec<RawHeaders>, BackendError> {
        Ok(Vec::new())
    }

    async fn sync_since(&mut self, _: Option<&str>) -> Result<Changes, BackendError> {
        unreachable!()
    }
    async fn fetch(&mut self, _: &str) -> Result<Vec<u8>, BackendError> {
        unreachable!()
    }
    async fn fetch_many(
        &mut self,
        ids: &[String],
        max: u32,
    ) -> Result<Vec<(String, Vec<u8>)>, BackendError> {
        self.fetched += ids.len();
        Ok(self
            .bodies
            .iter()
            .filter(|(id, _)| ids.contains(id))
            .map(|(id, raw)| {
                (
                    id.clone(),
                    raw.as_bytes()[..raw.len().min(max as usize)].to_vec(),
                )
            })
            .collect())
    }
    async fn send(&mut self, _: &Outgoing) -> Result<String, BackendError> {
        unreachable!()
    }
}

const CODE_MAIL: &str = "Authentication-Results: mx.google.com; dmarc=pass header.from=svc.example\r\nFrom: Svc <no-reply@svc.example>\r\nTo: me@gmail.com\r\nSubject: Your sign-in code\r\nMessage-ID: <c1@svc>\r\nDate: Mon, 19 Oct 2026 10:00:00 +0000\r\n\r\nYour code is 482913.\r\n";

#[tokio::test]
async fn extracts_after_sync() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&Account {
            name: "gmail".into(),
            address: "me@gmail.com".into(),
            host: "imap.gmail.com".into(),
            port: 993,
            username: "me@gmail.com".into(),
            smtp_host: "smtp.gmail.com".into(),
            smtp_port: 465,
            label: None,
        })
        .unwrap();
    let headers = |raw: &str| raw.split("\r\n\r\n").next().unwrap().as_bytes().to_vec();
    let gone =
        "From: Shop <o@shop.example>\r\nSubject: Takk for kjøpet\r\nMessage-ID: <o1@shop>\r\n\r\nx";
    let new = vec![
        RawHeaders {
            remote_id: "1:1".into(),
            headers: headers(CODE_MAIL),
            ..Default::default()
        },
        RawHeaders {
            remote_id: "1:2".into(),
            headers: headers(gone),
            ..Default::default()
        },
    ];
    store
        .apply(
            "gmail",
            &Changes {
                new,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        store.list(Bucket::Digest, None).unwrap().len(),
        2,
        "the no-reply code mail and the shop's receipt start in the Digest"
    );

    let mut server = Server {
        bodies: vec![("1:1".into(), CODE_MAIL.into())],
        fetched: 0,
    };
    assert_eq!(
        sync::extract_pending(&mut store, &mut server, "gmail")
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        store.list(Bucket::Codes, None).unwrap().len(),
        1,
        "the code mail moved to Codes"
    );
    assert_eq!(
        store.list(Bucket::Digest, None).unwrap().len(),
        1,
        "the receipt stays"
    );
    let codes = store.codes(0).unwrap();
    assert_eq!((codes.len(), codes[0].kind.as_str()), (1, "Sign-in code"));

    // Nothing is fetched twice, including the message that vanished.
    assert_eq!(
        sync::extract_pending(&mut store, &mut server, "gmail")
            .await
            .unwrap(),
        0
    );
    assert_eq!(server.fetched, 2);
}
