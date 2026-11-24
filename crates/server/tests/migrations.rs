//! Migrations the production database has applied must never change: the
//! server refuses to start when one does ("migration N was previously
//! applied but has been modified"), even for a comment. To change the
//! schema, add a new migration file and its line below.

use sha2::{Digest, Sha256};

/// SHA-256 of every migration, as applied in production.
const APPLIED: &[(&str, &str)] = &[
    ("0001_initial.sql", "2a2a412e5d300fe7f26a381bd24d26206cd6e3c983948f8d00b30c5fbf297ae1"),
    ("0002_drop_founder.sql", "0e78d6a89eca791519d09db06131491c798624f3573ee0ed512176f81f5a45a9"),
    ("0003_shop.sql", "f1afdfcd99ed91cfab8232eb8d3ee3dbc1a95832b1501c84ac5ef4c6640b947b"),
    (
        "0004_accounts_replays_stats.sql",
        "80f780c14dc752c522e2f5f49e17763d09cf062bf048e5f30c33845f2b963708",
    ),
    ("0005_missions_board.sql", "f0035662896f3ed9e38f377c9bdd48cf481855c60d71f30ff077203360204d85"),
    ("0006_correspondence.sql", "2cc3447332fe8ccb9fb24f184b39eb667fb82aee7109cdb763bef3936594a438"),
    ("0007_social.sql", "5e7da4eb56a2cefcdcc009850d2b3ae354474faa1ca4ee7f0f81fa2ef90b3639"),
    ("0008_cue_wins.sql", "c1611ee249580ccdb2bafaa77c27a143f9f1a396d7264a452d96086ef813ed1a"),
    (
        "0009_analysis_reports_metrics.sql",
        "dae527dd3e2b0771de471633291d4c98daef1f2047d8ec7706890b693bb5967a",
    ),
    (
        "0010_computer_ladder.sql",
        "0c128ebef825b75734f7102eebd87f6a13e5159ec83236a4ca681aa42f190842",
    ),
    (
        "0011_cue_stats_daily.sql",
        "bcf9a4b6ead472b0d1d3140ec53c9fc574223c9234cdd72f9be8c17b98cda6f1",
    ),
    ("0012_name_changes.sql", "068d8947c160821a6ef153933c32784a5d5ad2ea7b7b7b9b774d8a73d64484ce"),
    ("0013_msstore.sql", "77fa92f18e167e9cd96fcd8739b523a35a58244a2aae5587b9128506379ce5c6"),
    ("0014_rulesets.sql", "aa6297e1d94fc1c402a81499e0222bc2a6c30d8c7c47c8cd2e3a3f93dd044093"),
    ("0015_avatars.sql", "2ebd93210dee8b75570c0f07d4472c5ff765c39c0d48c778b619fbd6f33f4580"),
    ("0016_lookups.sql", "fd952d28a33556a3a87fc73dc67254273ecfef023a104d2dd3f3c1915c90638f"),
    ("0017_ladder_tickets.sql", "664e609f6c124fd7ccc0c76aff42b1c1815c1760cbe180dd60d4c93dc9adbb30"),
    ("0018_staff_notes.sql", "a41dbe65678193f240e9dec998b236a974af7c0ede416fa8d67b5236d51648bb"),
    ("0019_events.sql", "cdb52fd1b01a4ab815eeca17f86ca66028cf06c6fead821bf32cd969eb7e9258"),
    ("0020_cup_medals.sql", "47d4e193975d6d82913ae0866575a973d9c59943a67b99baa19c4a510e90fd56"),
    ("0021_age.sql", "951d1b19c08fbe2019db126db4d8790080c1eeed01558d54dd6431a14851c7bd"),
    ("0022_official_rules.sql", "42f0892b73d5a0e6c647809678835f399b1f27518275c7a602f8fd8f1c08acf5"),
    ("0023_indexes.sql", "7382fc9792409f716bae7587143655d0b1ea1fb514972c994cc48c42270cf1c7"),
    ("0024_match_rooms.sql", "df4e9458e2e9922d55f505ae3ae8570c176290f4a8bf2790c11a70ca0e2b6f67"),
    ("0025_daily_time.sql", "28b481dfd9ff63dfb352b9267c3010513c6765633955b408ac94d6107b2ec246"),
    ("0026_official_calls.sql", "fb6160ecd7534115fc277a8de3ebe2da42037d4522fd9f03d0bf6978b354905c"),
];

#[test]
fn applied_migrations_are_unchanged() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for (name, expected) in APPLIED {
        let bytes = std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        // As committed (LF): a Windows checkout may have turned the line
        // endings into CRLF, which production never sees.
        let text = String::from_utf8(bytes).expect("UTF-8").replace("\r\n", "\n");
        let bytes = text.into_bytes();
        let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            &got, expected,
            "{name} was edited: applied migrations must not change (add a new one instead)"
        );
    }
    let on_disk = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).count();
    assert_eq!(
        on_disk,
        APPLIED.len(),
        "a new migration: add its name and hash to APPLIED once it is released"
    );
}
