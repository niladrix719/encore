use std::fs;
use std::path::Path;
use std::rc::Rc;

use anyhow::Result;
use common::js_runtime_path;
use insta::glob;
use swc_common::errors::{Handler, HANDLER};
use swc_common::{Globals, SourceMap, GLOBALS};
use tempfile::TempDir;

use encore_tsparser::builder::Builder;
use encore_tsparser::parser::parser::ParseContext;
use encore_tsparser::{app, builder};

mod common;

#[test]
fn test_parser() {
    env_logger::init();
    glob!("testdata/*.txt", |path| {
        let input = fs::read_to_string(path).unwrap();
        let ar = txtar::from_str(&input);
        let tmp_dir = TempDir::with_prefix("parse").unwrap();
        ar.materialize(&tmp_dir).unwrap();
        match parse_txtar(tmp_dir.path()) {
            Ok(_) => {}
            Err(e) => {
                panic!("{:#?}\n{}", e, e.backtrace());
            }
        }
    });
}

#[test]
fn test_api_tags() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/api_tags.txt");
    let ar = txtar::from_str(&fs::read_to_string(path).unwrap());
    let tmp_dir = TempDir::with_prefix("parse").unwrap();
    ar.materialize(&tmp_dir).unwrap();
    let app = parse_txtar(tmp_dir.path()).unwrap();

    let tags = |name: &str| -> Vec<String> {
        let rpc = app
            .meta
            .svcs
            .iter()
            .flat_map(|svc| &svc.rpcs)
            .find(|rpc| rpc.name == name)
            .unwrap_or_else(|| panic!("endpoint {name} not found"));
        rpc.tags.iter().map(|t| t.value.clone()).collect()
    };
    assert_eq!(tags("literal"), ["plain"]);
    assert_eq!(tags("imported"), ["premium", "beta"]);
    assert_eq!(tags("mixed"), ["plain", "local", "premium"]);
    assert_eq!(tags("reexported"), ["premium", "beta"]);
    assert!(tags("none").is_empty());
}

#[test]
fn test_api_tags_rejects_non_constants() {
    for (case, tag) in [
        ("let", "MUTABLE"),
        ("annotated", "WIDE"),
        ("call", "make()"),
        ("template", "`a${1}`"),
        ("member", "OBJ.x"),
    ] {
        let src = format!(
            r#"-- foo/foo.ts --
import {{ api }} from "encore.dev/api";
let MUTABLE = "x";
const WIDE: string = "x";
const make = () => "x";
const OBJ = {{ x: "x" }};
export const t = api({{ tags: [{tag}] }}, async (): Promise<void> => {{}});
-- package.json --
{{"name": "foo", "type": "module", "dependencies": {{"encore.dev": "^1.35.0"}}}}
"#
        );
        let tmp_dir = TempDir::with_prefix("parse").unwrap();
        txtar::from_str(&src).materialize(&tmp_dir).unwrap();
        assert!(
            parse_txtar(tmp_dir.path()).is_err(),
            "{case}: tag {tag} should be rejected"
        );
    }
}

fn parse_txtar(app_root: &Path) -> Result<app::AppDesc> {
    let globals = Globals::new();
    let cm: Rc<SourceMap> = Default::default();
    let errs = Rc::new(Handler::with_tty_emitter(
        swc_common::errors::ColorConfig::Auto,
        true,
        false,
        Some(cm.clone()),
    ));

    GLOBALS.set(&globals, || -> Result<app::AppDesc> {
        HANDLER.set(&errs, || -> Result<app::AppDesc> {
            let builder = Builder::new()?;
            let pc = ParseContext::new(
                app_root.to_path_buf(),
                Some(js_runtime_path()),
                cm,
                errs.clone(),
            )?;

            let app = builder::App {
                root: app_root.to_path_buf(),
                platform_id: None,
                local_id: "test".to_string(),
            };
            let pp = builder::ParseParams {
                app: &app,
                pc: &pc,
                working_dir: app_root,
                parse_tests: false,
            };

            builder.parse(&pp).ok_or(anyhow::anyhow!("parse failed"))
        })
    })
}
