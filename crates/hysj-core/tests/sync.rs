use hysj_core::{BackendError, Changes, MailBackend, Outgoing, RawHeaders, Store, sync};
use hysj_proto::{Account, Bucket};

/// Serves a fixed mailbox; the cursor is the number of messages already seen.
struct Fake {
    mailbox: Vec<&'static str>,
    reset: bool,
    fetches: usize,
}

const A: &str = "From: Aisha Rahman <aisha@example.com>\r\nSubject: Q3 numbers\r\nDate: Mon, 19 Oct 2026 09:00:00 +0000\r\nMessage-ID: <a@x>\r\n\r\nCould you check the Q3 numbers?\r\n";
const B: &str = "From: marcus@example.com\r\nSubject: Dinner\r\nDate: Tue, 20 Oct 2026 09:00:00 +0000\r\n\r\nSaturday?\r\n";

impl MailBackend for Fake {
    async fn search(&mut self, _: &str, _: usize) -> Result<Vec<RawHeaders>, BackendError> {
        Ok(Vec::new())
    }

    async fn sync_since(&mut self, cursor: Option<&str>) -> Result<Changes, BackendError> {
        let seen: usize = cursor.map_or(0, |c| c.parse().unwrap());
        let seen = if self.reset { 0 } else { seen };
        let new = self.mailbox[seen..]
            .iter()
            .enumerate()
            .map(|(i, m)| RawHeaders {
                remote_id: (seen + i).to_string(),
                headers: m.split("\r\n\r\n").next().unwrap().as_bytes().to_vec(),
                ..Default::default()
            })
            .collect();
        Ok(Changes {
            cursor: self.mailbox.len().to_string(),
            reset: self.reset,
            new,
            ..Default::default()
        })
    }

    async fn fetch_many(
        &mut self,
        _: &[String],
        _: u32,
    ) -> Result<Vec<(String, Vec<u8>)>, BackendError> {
        unreachable!("sync tests don't extract")
    }

    async fn send(&mut self, _: &Outgoing) -> Result<String, BackendError> {
        unreachable!("sync tests don't send")
    }

    async fn fetch(&mut self, remote_id: &str) -> Result<Vec<u8>, BackendError> {
        self.fetches += 1;
        Ok(self.mailbox[remote_id.parse::<usize>()?]
            .as_bytes()
            .to_vec())
    }
}

#[tokio::test]
async fn sync_then_lazy_body() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&Account {
            name: "work".into(),
            address: "me@example.com".into(),
            host: "imap.example.com".into(),
            port: 993,
            username: "me@example.com".into(),
            smtp_host: "smtp.example.com".into(),
            smtp_port: 465,
            label: None,
        })
        .unwrap();
    let mut fake = Fake {
        mailbox: vec![A],
        reset: false,
        fetches: 0,
    };

    assert_eq!(
        sync::sync_account(&mut store, &mut fake, "work")
            .await
            .unwrap(),
        1
    );
    fake.mailbox.push(B);
    assert_eq!(
        sync::sync_account(&mut store, &mut fake, "work")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sync::sync_account(&mut store, &mut fake, "work")
            .await
            .unwrap(),
        0
    );

    let list = store.list(Bucket::Screener, None).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(
        list[0].from, "marcus@example.com",
        "newest first, address when no name"
    );
    assert_eq!(list[1].from, "Aisha Rahman");
    assert_eq!(list[1].from_addr, "aisha@example.com");
    assert!(
        list[1]
            .date
            .as_deref()
            .unwrap()
            .starts_with("2026-10-19T09:00:00")
    );
    assert!(
        store
            .list(Bucket::Screener, Some("other"))
            .unwrap()
            .is_empty()
    );

    let id = &list[1].id;
    assert!(
        store.get(id).unwrap().unwrap().body.is_none(),
        "headers only after sync"
    );
    let msg = sync::load(&mut store, &mut fake, id)
        .await
        .unwrap()
        .unwrap();
    assert!(msg.body.unwrap().contains("Q3 numbers?"));
    sync::load(&mut store, &mut fake, id).await.unwrap();
    assert_eq!(fake.fetches, 1, "body cached after first view");

    fake.reset = true;
    fake.mailbox = vec![B];
    sync::sync_account(&mut store, &mut fake, "work")
        .await
        .unwrap();
    assert_eq!(
        store.list(Bucket::Screener, None).unwrap().len(),
        1,
        "reset drops stale messages"
    );
}
