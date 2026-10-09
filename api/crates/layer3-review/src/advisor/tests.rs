use super::*;

fn input() -> AdvisorInput {
    AdvisorInput {
        title: "T".into(),
        abstract_text: "A".into(),
        authors: vec!["A. Author".into()],
        statements: vec![],
        referee: RefereeOut::default(),
        text: Some("paper".into()),
        source_dir: None,
        main_file: Some("main.tex".into()),
    }
}

#[test]
fn local_environment_is_an_allowlist() {
    let command = local_command(Path::new("codex"));
    assert!(
        command
            .as_std()
            .get_envs()
            .all(|(key, _)| key == "HOME" || key == "PATH")
    );
}

#[test]
fn prompts_carry_evidence_consent_and_letter_rules() {
    let advice = referee_prompts::advice(&input());
    for requirement in [
        "./scratch",
        "Do not modify ./source",
        "evidence",
        "author's consent",
        "Lean 4",
        "main.tex",
    ] {
        assert!(advice.contains(requirement), "{requirement}");
    }
    let letter = referee_prompts::letter(&input(), None, None);
    for requirement in [
        "No process narration",
        "no links in the body",
        "note",
        "Do not ask for a meeting",
        "English",
    ] {
        assert!(letter.contains(requirement), "{requirement}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn codex_uses_a_fresh_copy_and_parses_answer_file() {
    use std::os::unix::fs::PermissionsExt;
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("original");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("main.tex"), "original paper").unwrap();
    let program = work.path().join("fake-codex");
    std::fs::write(
        &program,
        r#"#!/bin/sh
while [ "$#" -gt 0 ]; do
  case "$1" in
    -C) shift; task_dir="$1";;
    -o) shift; answer_path="$1";;
  esac
  shift
done
[ -f "$task_dir/TASK.md" ] || exit 2
[ -d "$task_dir/scratch" ] || exit 3
[ -f "$task_dir/source/main.tex" ] || exit 4
printf '%s' '{"summary":"useful advice"}' > "$answer_path"
"#,
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let advisor = CodexCli {
        program,
        model: Some("test".into()),
        timeout: Duration::from_secs(5),
    };
    let mut input = input();
    input.source_dir = Some(source.clone());
    assert_eq!(
        advisor.advise(&input).await.unwrap().summary,
        "useful advice"
    );
    assert_eq!(
        std::fs::read_to_string(source.join("main.tex")).unwrap(),
        "original paper"
    );
    assert!(!source.join("TASK.md").exists());
}

#[tokio::test]
async fn chat_uses_source_text_and_shared_prompts() {
    use axum::{Json, Router, routing::post};
    use serde_json::{Value, json};
    let token = tempfile::tempdir()
        .unwrap()
        .path()
        .to_string_lossy()
        .to_string();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new().route("/chat/completions", post(|Json(body): Json<Value>| async move {
        let user = body["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("paper"));
        assert!(user.contains("evidence"));
        Json(json!({"choices":[{"message":{"content":"```json\n{\"summary\":\"A written argument\"}\n```"}}]}))
    }));
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let chat = ChatModel::new(&format!("http://{address}"), token, "advisor-test".into()).unwrap();
    assert_eq!(
        Advisor::advise(&chat, &input()).await.unwrap().summary,
        "A written argument"
    );
}
