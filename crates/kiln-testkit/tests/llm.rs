//! The LLM adapter against a local mock of the Messages API (M10 plan L6).
//! Nothing here calls the real API.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use kiln_engine::{CardPool, Game};
use kiln_policy::{GreedyPolicy, Policy};
use kiln_sim::llm::{LlmConfig, LlmPolicy};
use kiln_testkit::scenario;
use kiln_types::PlayerId;

/// A server that answers each request with the next canned body and keeps
/// what it received (headers lowercased, then the body).
fn mock(replies: Vec<&'static str>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("{e}"));
    let url = format!(
        "http://{}/v1/messages",
        listener
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_default()
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for reply in replies {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap_or_else(|e| panic!("{e}")));
            let mut request = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                let lower = line.to_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                request.push_str(&lower);
            }
            let mut body = vec![0u8; length];
            reader
                .read_exact(&mut body)
                .unwrap_or_else(|e| panic!("{e}"));
            request.push_str(&String::from_utf8_lossy(&body));
            log.lock().unwrap_or_else(|e| panic!("{e}")).push(request);
            let mut out = stream;
            let _ = write!(
                out,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    (url, seen)
}

fn bolt_board() -> Game {
    let pool = Arc::new(CardPool::m3_test_set());
    let s = scenario::parse(
        "scenario: kiln-scenario/1
name: llm
players:
  - battlefield: [Test Mountain]
    hand: [Test Bolt]
  - life: 3
turn: { number: 3, active: P1, step: precombat_main }
script: []
",
        "llm",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    scenario::run(&s, &pool).unwrap_or_else(|e| panic!("{e}"))
}

fn policy(url: String) -> LlmPolicy {
    let mut p = LlmPolicy::new(
        LlmConfig {
            endpoint: url,
            api_key: Some("test-key".into()),
            ..LlmConfig::default()
        },
        Arc::new(|n: &str| {
            (n == "Test Bolt").then(|| kiln_sim::live::CardText {
                mana_cost: Some("{R}".into()),
                type_line: "Instant".into(),
                oracle_text: "Test Bolt deals 3 damage to any target.".into(),
            })
        }),
        vec!["a".into(), "b".into()],
        Box::new(GreedyPolicy::default()),
    );
    p.begin_game(PlayerId(0), 1);
    p
}

const ANSWER: &str = r#"{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"{\"option\": 1, \"reason\": \"Bolt is lethal.\"}"}],"stop_reason":"end_turn"}"#;
const REFUSAL: &str =
    r#"{"content":[],"stop_reason":"refusal","stop_details":{"type":"refusal","category":null}}"#;
const BAD: &str = r#"{"content":[{"type":"text","text":"{\"option\": 9, \"reason\": \"?\"}"}],"stop_reason":"end_turn"}"#;

#[test]
fn it_asks_with_the_decision_and_card_text_and_uses_the_answer() {
    let (url, seen) = mock(vec![ANSWER]);
    let mut p = policy(url);
    let g = bolt_board();
    let d = g.pending().expect("a decision").clone();
    let i = p.decide(&g.view(PlayerId(0)), &d);
    assert_eq!(
        g.option_label(i as usize).as_deref(),
        Some("Cast Test Bolt")
    );
    assert_eq!(
        p.rationale().as_deref(),
        Some("claude-opus-5-5: Bolt is lethal.")
    );
    let req = seen.lock().unwrap_or_else(|e| panic!("{e}"))[0].clone();
    for want in [
        "x-api-key: test-key",
        "anthropic-version: 2026-10-21",
        "anthropic-beta: server-side-fallback-2026-07-01",
        "\"model\":\"claude-opus-5-5\"",
        "\"fallbacks\":\"default\"",
        "json_schema",
        "Cast Test Bolt",
        "deals 3 damage to any target",
    ] {
        assert!(req.contains(want), "request lacks {want:?}:\n{req}");
    }
    assert_eq!(p.stats()["llm:answered"], 1);
}

#[test]
fn refusals_and_bad_answers_fall_back() {
    let (url, seen) = mock(vec![REFUSAL, BAD]);
    let mut p = policy(url);
    let g = bolt_board();
    let d = g.pending().expect("a decision").clone();
    let greedy = GreedyPolicy::default().decide(&g.view(PlayerId(0)), &d);
    assert_eq!(p.decide(&g.view(PlayerId(0)), &d), greedy);
    assert_eq!(p.decide(&g.view(PlayerId(0)), &d), greedy);
    assert_eq!(p.stats()["llm:failed"], 2);
    let errors = p.take_errors();
    assert!(errors[0].contains("declined"), "{errors:?}");
    assert!(errors[1].contains("no valid option"), "{errors:?}");
    assert_eq!(seen.lock().unwrap_or_else(|e| panic!("{e}")).len(), 2);
}

#[test]
fn trivial_decisions_never_call_the_api() {
    let (url, seen) = mock(vec![]);
    let mut p = policy(url);
    let g = bolt_board();
    let mut g = g;
    // Cast the Bolt with greedy's help up to the payment, a trivial decision.
    g.apply_option(1).unwrap_or_else(|e| panic!("{e}"));
    while let Some(d) = g.pending().cloned() {
        if d.option_count() > 1 && !kiln_policy::is_key_decision(&d) {
            let _ = p.decide(&g.view(d.player), &d);
            break;
        }
        let i = GreedyPolicy::default().decide(&g.view(d.player), &d);
        g.apply_option(i).unwrap_or_else(|e| panic!("{e}"));
    }
    assert!(seen.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    assert_eq!(p.stats()["llm:answered"], 0);
}
