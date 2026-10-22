//! Loads a model and runs each task on a made-up mail.
//! `cargo run --release -p hysj-ai --example smoke -- qwen3|gemma3`

use hysj_ai::{Choice, Llm, MessageText, timed};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let choice = std::env::args()
        .nth(1)
        .and_then(|a| Choice::parse(&a))
        .expect("qwen3 or gemma3");
    let (llm, load) = timed(Llm::load(choice)).await;
    let llm = llm?;
    println!("{}: loaded in {load:.1}s", choice.version());
    let m = MessageText {
        from: "Ingrid Solheim",
        subject: "Styremøte 28. oktober",
        body: "Hei!\n\nJeg er bortreist den 28. Kan du ta budsjettpunktet på styremøtet? Det er mest vedlikehold av taket som må forklares. Gi beskjed innen fredag.\n\nHilsen Ingrid",
    };
    let (g, t) = timed(llm.gist(&m)).await;
    println!("gist ({t:.1}s): {g:?}");
    let (r, t) = timed(llm.replies(&m)).await;
    println!("replies ({t:.1}s): {r:?}");
    let facts = "Headline: Nothing here needs you. FINN found you 2 homes and 1 job.\nLede: 41 emails came in since 08:00. 3 newsletters are worth reading.\n\nListings: FINN found you 2 homes and 1 job.\n- Moderne 4-roms med utsikt\n\nReading: 3 newsletters.\n- Async closures land on stable\n";
    let (d, t) = timed(llm.digest(facts)).await;
    println!("digest ({t:.1}s): {d:?}");
    Ok(())
}
