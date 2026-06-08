use chrono::Datelike;
use papyrus_lib::filters::FilterArgs;

/// Regression test: --last-months=3 in December should correctly subtract months across year boundary.
/// Before the fix, subtracting from month=12 gave month=9 of the *same* year.
#[test]
fn test_last_months_december_boundary() {
    // We can't control what "today" is in the test, but we can check the logic is correct
    // by parsing directly using the same function the production code uses.
    let today = chrono::Local::now().date_naive();
    let args = FilterArgs {
        last_months: Some(3),
        ..Default::default()
    };
    let fs = args.into_filter_set().unwrap();
    let from = fs.date_from.unwrap();

    // The `from` date must be exactly 3 months before today
    let expected_year = if today.month() <= 3 {
        today.year() - 1
    } else {
        today.year()
    };
    let expected_month = if today.month() <= 3 {
        today.month() + 12 - 3
    } else {
        today.month() - 3
    };
    assert_eq!(from.year(), expected_year, "Year boundary wrong");
    assert_eq!(from.month(), expected_month, "Month wrong: today={:?}", today);
}

/// Regression: --last-months=13 should work (more than a year back)
#[test]
fn test_last_months_more_than_a_year() {
    let args = FilterArgs {
        last_months: Some(13),
        ..Default::default()
    };
    let fs = args.into_filter_set().unwrap();
    let from = fs.date_from.unwrap();
    let today = chrono::Local::now().date_naive();

    // from must be at least 13 months before today
    let diff_days = (today - from).num_days();
    assert!(diff_days >= 390, "Expected >390 days back, got {}", diff_days);
    assert!(diff_days <= 400, "Expected ~396 days back, got {}", diff_days);
}

/// --last-days should work correctly
#[test]
fn test_last_days() {
    let args = FilterArgs {
        last_days: Some(30),
        ..Default::default()
    };
    let fs = args.into_filter_set().unwrap();
    let from = fs.date_from.unwrap();
    let today = chrono::Local::now().date_naive();
    let diff = (today - from).num_days();
    assert_eq!(diff, 30);
}

/// --year flag should set date range for whole year
#[test]
fn test_year_filter() {
    let args = FilterArgs {
        year: Some(2023),
        ..Default::default()
    };
    let fs = args.into_filter_set().unwrap();
    assert_eq!(fs.date_from.unwrap().year(), 2023);
    assert_eq!(fs.date_from.unwrap().month(), 1);
    assert_eq!(fs.date_to.unwrap().year(), 2023);
    assert_eq!(fs.date_to.unwrap().month(), 12);
}
