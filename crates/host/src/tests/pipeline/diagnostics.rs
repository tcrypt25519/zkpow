use std::sync::{Mutex, OnceLock};

use tracing_subscriber::prelude::*;

use crate::pipeline::diagnostics::{
    chip_name, clear_shard_areas, conflicting_chips, shard_area, shard_areas, ShardAreas, ShardKind,
    RECURSION_CHIPS,
};

fn _shard_area_test_mutex() -> &'static Mutex<()> {
    static AREA_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    AREA_MUTEX.get_or_init(|| Mutex::new(()))
}

/// A chip statistics line exactly as `sp1_hypercube` formats it.
fn chip_line(name: &str, cells: &str) -> String {
    format!("{name:<15} | Prep Cols = 7     | Main Cols = 6     | Rows = 65_536 | Cells = {cells:<10}")
}

fn total_line(cells: &str, variables: u32) -> String {
    format!("Total number of cells: {cells}, number of variables: {variables}")
}

#[test]
fn reads_the_cell_total_out_of_sp1s_shard_line() {
    let cases = [
        (total_line("208_123_456", 28), Some(208_123_456)),
        (total_line("0", 0), Some(0)),
        (total_line("402_653_184", 29), Some(402_653_184)),
    ];

    for (message, expected) in cases {
        assert_eq!(shard_area(&message), expected, "message={message:?}");
    }
}

#[test]
fn ignores_every_other_line_on_the_same_target() {
    let cases = [
        "Proving shard".to_string(),
        chip_line("Byte", "851_968"),
        "Total number of cells".to_string(),
        "Total number of cells: , number of variables: 28".to_string(),
        "Total number of cells: many, number of variables: 28".to_string(),
        "total number of cells: 208, number of variables: 28".to_string(),
        "shard boundary".to_string(),
        String::new(),
    ];

    for message in cases {
        assert_eq!(shard_area(&message), None, "message={message:?}");
    }
}

#[test]
fn reads_the_chip_name_off_a_statistics_line_only() {
    assert_eq!(chip_name(&chip_line("Byte", "851_968")), Some("Byte"));
    assert_eq!(chip_name(&chip_line("Poseidon2WideDeg3", "36_107_904")), Some("Poseidon2WideDeg3"));
    assert_eq!(chip_name(&total_line("208_123_456", 28)), None);
    assert_eq!(chip_name("Proving shard"), None);
    assert_eq!(chip_name("setup | took 4s"), None);
    assert_eq!(chip_name("| Prep Cols = 7"), Some(""));
}

/// SP1 proves RISC-V shards and recursion shards through the same function, under the same target,
/// and names neither. Only the chip list tells them apart, so the layer has to read it. The two
/// chip name sets are disjoint, so one chip outside the recursion set settles a shard; no
/// particular RISC-V chip has to appear.
#[test]
fn separates_core_shards_from_recursion_shards_by_chip_list() {
    let _guard = _shard_area_test_mutex().lock().unwrap();
    clear_shard_areas();

    let subscriber = tracing_subscriber::registry().with(ShardAreas);
    tracing::subscriber::with_default(subscriber, || {
        // A RISC-V shard.
        tracing::info_span!("prove shard with data").in_scope(|| {
            tracing::debug!("Proving shard");
            tracing::debug!("{}", chip_line("Add", "89_760"));
            tracing::debug!("{}", chip_line("Byte", "851_968"));
            tracing::debug!("{}", total_line("4_750_480", 23));
        });
        // A compress shard: every chip of it is one a recursion machine has.
        tracing::info_span!("prove shard with data").in_scope(|| {
            tracing::debug!("Proving shard");
            tracing::debug!("{}", chip_line("MemoryVar", "14_680_064"));
            tracing::debug!("{}", chip_line("Poseidon2WideDeg3", "36_107_904"));
            tracing::debug!("{}", chip_line("PrefixSumChecks", "2_097_152"));
            tracing::debug!("{}", total_line("89_451_728", 27));
        });
        // A wrap shard, which swaps in the other Poseidon2 helpers.
        tracing::info_span!("prove shard with data").in_scope(|| {
            tracing::debug!("{}", chip_line("Poseidon2LinearLayer", "8_388_608"));
            tracing::debug!("{}", chip_line("Poseidon2SBox", "4_194_304"));
            tracing::debug!("{}", chip_line("ExtFeltConvert", "1_048_576"));
            tracing::debug!("{}", total_line("31_457_280", 25));
        });
        // The global memory shard: a RISC-V shard holding no `Byte` at all.
        tracing::info_span!("prove shard with data").in_scope(|| {
            tracing::debug!("{}", chip_line("MemoryGlobalInit", "6_291_456"));
            tracing::debug!("{}", chip_line("Global", "1_650_976"));
            tracing::debug!("{}", total_line("7_942_432", 23));
        });
    });

    let areas = shard_areas();
    assert_eq!(
        areas.iter().map(|area| (area.cells, area.kind)).collect::<Vec<_>>(),
        vec![
            (89_451_728, ShardKind::Recursion),
            (31_457_280, ShardKind::Recursion),
            (7_942_432, ShardKind::Core),
            (4_750_480, ShardKind::Core),
        ],
        "fullest first, each classified by its own chip list"
    );

    clear_shard_areas();
    assert!(shard_areas().is_empty());
}

/// A line that merely holds a pipe is not a chip statistics line, and must not settle a shard as
/// one of the RISC-V machine.
#[test]
fn a_piped_line_that_is_not_chip_statistics_does_not_classify_a_shard() {
    let _guard = _shard_area_test_mutex().lock().unwrap();
    clear_shard_areas();

    let subscriber = tracing_subscriber::registry().with(ShardAreas);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info_span!("prove shard with data").in_scope(|| {
            tracing::debug!("permit | acquired after 12ms");
            tracing::debug!("{}", chip_line("Select", "4_194_304"));
            tracing::debug!("{}", total_line("89_451_728", 27));
        });
    });

    assert_eq!(
        shard_areas().iter().map(|area| area.kind).collect::<Vec<_>>(),
        vec![ShardKind::Recursion]
    );
    clear_shard_areas();
}

/// A total logged outside a shard span has no chip list to classify it, so it is dropped rather
/// than counted as a shard of unknown kind.
#[test]
fn ignores_a_total_with_no_shard_span() {
    let _guard = _shard_area_test_mutex().lock().unwrap();
    clear_shard_areas();

    let subscriber = tracing_subscriber::registry().with(ShardAreas);
    tracing::subscriber::with_default(subscriber, || {
        tracing::debug!("{}", total_line("4_750_480", 23));
    });

    assert!(shard_areas().is_empty());
}

/// A chip name neither machine had when [`RECURSION_CHIPS`] was written reads as a chip of the
/// RISC-V machine, whose set is the open-ended one. On its own it classifies the shard; alongside a
/// recursion chip it leaves the shard unattributed instead of filing it under either machine.
#[test]
fn a_chip_outside_both_sets_settles_a_core_shard_but_spoils_a_recursion_one() {
    let _guard = _shard_area_test_mutex().lock().unwrap();
    clear_shard_areas();

    let subscriber = tracing_subscriber::registry().with(ShardAreas);
    tracing::subscriber::with_default(subscriber, || {
        // A core shard using a chip SP1 added since: still core, nothing else points away.
        tracing::info_span!("prove shard with data").in_scope(|| {
            tracing::debug!("{}", chip_line("SomeNewRiscvChip", "1_048_576"));
            tracing::debug!("{}", total_line("1_900_544", 21));
        });
        // A recursion shard that also holds one: the two sets have drifted, so attribute neither.
        tracing::info_span!("prove shard with data").in_scope(|| {
            tracing::debug!("{}", chip_line("Poseidon2WideDeg3", "36_107_904"));
            tracing::debug!("{}", chip_line("SomeNewRecursionChip", "2_097_152"));
            tracing::debug!("{}", total_line("89_451_728", 27));
        });
    });

    assert_eq!(
        shard_areas().iter().map(|area| (area.cells, area.kind)).collect::<Vec<_>>(),
        vec![(89_451_728, ShardKind::Unknown), (1_900_544, ShardKind::Core)]
    );
    assert_eq!(
        conflicting_chips(),
        vec!["SomeNewRecursionChip".to_string()],
        "the chip that put the shard on both machines is named for the warning"
    );

    clear_shard_areas();
    assert!(conflicting_chips().is_empty());
}

/// A name in both sets at once would make every shard holding it ambiguous, so the two sets must
/// stay disjoint. `scripts/check-sp1-chip-sets.sh` checks them against SP1's own machines.
#[test]
fn the_recursion_set_holds_no_duplicates() {
    let mut sorted = RECURSION_CHIPS.to_vec();
    sorted.sort_unstable();
    let mut deduped = sorted.clone();
    deduped.dedup();
    assert_eq!(sorted, deduped, "RECURSION_CHIPS has a repeated name");
    assert_eq!(sorted, RECURSION_CHIPS.to_vec(), "RECURSION_CHIPS should be sorted");
}
